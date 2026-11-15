"""Offline tests of the review gate: decisions, parsing and the write order. No network, no `gh`."""
import os
import subprocess
import sys
import unittest
from unittest import mock

sys.path.insert(0, os.path.join(os.path.dirname(__file__), '..'))
import review_gate as gate  # noqa: E402

REPO = 'owner/repo'
HEAD = 'a' * 40
OLD = 'b' * 40
REQUIRED = {'Tests', 'Analyze (rust)'}


def user(login):
    return {'login': login + '[bot]', 'type': 'Bot'}


def review(login, state='COMMENTED', commit=HEAD, ident=1):
    return {'id': ident, 'user': user(login), 'state': state, 'commit_id': commit}


def thread(login, resolved=False):
    return {'isResolved': resolved, 'comments': {'nodes': [
        {'author': {'__typename': 'Bot', 'login': login}, 'url': 'https://example.test/t'}]}}


def facts(**overrides):
    values = dict(head=HEAD, state='OPEN', draft=False, labels=set(),
                  reviews=[review(gate.GREPTILE)], threads=[],
                  checks={'Tests': 'pass', 'Analyze (rust)': 'pass', gate.GREPTILE_CHECK: 'pass'},
                  required=set(REQUIRED))
    values.update(overrides)
    return gate.Facts(**values)


class Decisions(unittest.TestCase):
    def test_clean_greptile_review_and_green_ci_request_coderabbit(self):
        self.assertEqual(gate.evaluate(facts(), automatic=True).action, 'request')

    def test_closed_or_draft_is_skipped(self):
        self.assertEqual(gate.evaluate(facts(state='CLOSED'), automatic=True).action, 'skip')
        self.assertEqual(gate.evaluate(facts(draft=True), automatic=True).action, 'skip')

    def test_greptile_review_of_an_older_commit_waits(self):
        decision = gate.evaluate(facts(reviews=[review(gate.GREPTILE, commit=OLD)]), automatic=True)
        self.assertEqual(decision.action, 'wait')
        self.assertIn('Greptile must review the current commit.', decision.reasons)

    def test_summary_footer_for_the_head_with_a_passing_check_counts_as_current(self):
        decision = gate.evaluate(facts(reviews=[review(gate.GREPTILE, commit=OLD)], summary_head=HEAD),
                                 automatic=True)
        self.assertEqual(decision.action, 'request')

    def test_summary_without_a_passing_greptile_check_is_not_enough(self):
        checks = {'Tests': 'pass', 'Analyze (rust)': 'pass', gate.GREPTILE_CHECK: 'pending'}
        decision = gate.evaluate(facts(reviews=[], summary_head=HEAD, checks=checks), automatic=True)
        self.assertEqual(decision.action, 'wait')

    def test_no_greptile_review_waits(self):
        decision = gate.evaluate(facts(reviews=[]), automatic=True)
        self.assertIn('Greptile has not reviewed this pull request yet.', decision.reasons)

    def test_changes_requested_and_unresolved_findings_wait(self):
        decision = gate.evaluate(facts(reviews=[review(gate.GREPTILE, 'CHANGES_REQUESTED')],
                                       threads=[thread('greptile-apps'), thread('greptile-apps', resolved=True),
                                                thread('someone-else')]), automatic=True)
        self.assertEqual(decision.action, 'wait')
        self.assertIn('Greptile still requests changes.', decision.reasons)
        self.assertEqual(sum(r.startswith('Unresolved Greptile finding') for r in decision.reasons), 1)

    def test_request_label_alone_does_not_bypass_greptile(self):
        # CodeRabbit finding on PR #49: a label from an earlier commit is a request, not a review.
        stale = facts(labels={gate.REVIEW_LABEL}, reviews=[review(gate.GREPTILE, 'CHANGES_REQUESTED')])
        self.assertEqual(gate.evaluate(stale, automatic=False).action, 'wait')
        self.assertEqual(gate.evaluate(stale, automatic=True).action, 'wait')

    def test_published_coderabbit_review_ends_automatic_requests(self):
        started = facts(reviews=[review(gate.GREPTILE, commit=OLD), review(gate.CODERABBIT, commit=OLD, ident=2)])
        self.assertEqual(gate.evaluate(started, automatic=True).action, 'done')

    def test_manual_repeat_after_coderabbit_skips_greptile_but_not_ci(self):
        started = facts(reviews=[review(gate.GREPTILE, 'CHANGES_REQUESTED', OLD),
                                 review(gate.CODERABBIT, commit=OLD, ident=2)], threads=[thread('greptile-apps')])
        self.assertEqual(gate.evaluate(started, automatic=False).action, 'request')
        failing = facts(reviews=started.reviews, checks={'Tests': 'fail', 'Analyze (rust)': 'pass'})
        self.assertIn('Required check Tests is fail.', gate.evaluate(failing, automatic=False).reasons)

    def test_pending_or_dismissed_coderabbit_review_is_not_a_review(self):
        for state in ('PENDING', 'DISMISSED'):
            pending = facts(reviews=[review(gate.GREPTILE), review(gate.CODERABBIT, state, ident=2)])
            self.assertEqual(gate.evaluate(pending, automatic=True).action, 'request')

    def test_a_human_named_like_the_bot_is_not_the_bot(self):
        impostor = {'id': 3, 'user': {'login': 'coderabbitai', 'type': 'User'}, 'state': 'COMMENTED',
                    'commit_id': HEAD}
        self.assertEqual(gate.evaluate(facts(reviews=[review(gate.GREPTILE), impostor]), automatic=True).action,
                         'request')

    def test_request_already_recorded_for_this_commit_is_not_repeated_automatically(self):
        recorded = facts(gate_status=gate.REQUESTED)
        self.assertEqual(gate.evaluate(recorded, automatic=True).action, 'done')
        self.assertEqual(gate.evaluate(recorded, automatic=False).action, 'request')

    def test_missing_failing_and_unconfigured_checks_wait(self):
        decision = gate.evaluate(facts(checks={'Tests': 'pending', gate.GREPTILE_CHECK: 'pass'}), automatic=True)
        self.assertIn('Required check Tests is pending.', decision.reasons)
        self.assertIn('Required check has not reported: Analyze (rust)', decision.reasons)
        self.assertIn('No required CI checks are configured; readiness cannot be confirmed.',
                      gate.evaluate(facts(required=set()), automatic=True).reasons)

    def test_running_coderabbit_waits(self):
        checks = dict(facts().checks, CodeRabbit='pending')
        self.assertIn('CodeRabbit is already running.', gate.evaluate(facts(checks=checks), automatic=True).reasons)

    def test_status_description_fits_github_limit(self):
        state, text = gate.status_for(gate.Decision('wait', ['x' * 500]))
        self.assertEqual(state, 'pending')
        self.assertEqual(len(text), gate.STATUS_LIMIT)


class Parsing(unittest.TestCase):
    def comment(self, body, ident=1, updated='2026-11-15T10:00:00Z', login='greptile-apps'):
        return {'id': ident, 'updated_at': updated, 'user': user(login), 'body': body}

    def test_summary_head_reads_only_the_footer_of_the_latest_summary(self):
        footer = f'{gate.GREPTILE_SUMMARY_MARKER}\nsee https://github.com/{REPO}/commit/{OLD}\n' \
                 f'Last reviewed commit: [fix](https://github.com/{REPO}/commit/{HEAD})'
        older = f'{gate.GREPTILE_SUMMARY_MARKER}\nLast reviewed commit: [x](https://github.com/{REPO}/commit/{OLD})'
        comments = [self.comment(older, 1, '2026-11-14T10:00:00Z'), self.comment(footer, 2)]
        self.assertEqual(gate.greptile_summary_head(comments, REPO), HEAD)

    def test_summary_from_another_author_or_repository_is_ignored(self):
        body = f'{gate.GREPTILE_SUMMARY_MARKER}\nLast reviewed commit: [x](https://github.com/{REPO}/commit/{HEAD})'
        self.assertIsNone(gate.greptile_summary_head([self.comment(body, login='someone')], REPO))
        self.assertIsNone(gate.greptile_summary_head([self.comment(body)], 'other/repo'))

    def test_no_required_checks_reply_is_an_empty_list(self):
        done = subprocess.CompletedProcess([], 1, '', "no required checks reported on the 'x' branch\n")
        with mock.patch.object(gate.subprocess, 'run', return_value=done):
            self.assertEqual(gate.gh_json(['pr', 'checks', '1', '--required'], allowed_codes=(0, 1, 8)), [])

    def test_unexpected_gh_failure_is_an_error(self):
        failed = subprocess.CompletedProcess([], 4, '', 'HTTP 403')
        with mock.patch.object(gate.subprocess, 'run', return_value=failed):
            with self.assertRaisesRegex(RuntimeError, 'HTTP 403'):
                gate.gh_json(['api', 'x'])


class FakeGitHub:
    def __init__(self, current, heads):
        self.current, self.heads, self.calls = current, list(heads), []

    def facts(self, number):
        return self.current

    def pull(self, number):
        return {'headRefOid': self.heads.pop(0)}

    def relabel(self, number, present):
        self.calls.append(('relabel', present))

    def set_status(self, sha, state, description):
        self.calls.append(('status', sha, state, description))


class Reconcile(unittest.TestCase):
    def test_ready_pull_request_is_labelled_then_marked(self):
        github = FakeGitHub(facts(labels={gate.REVIEW_LABEL}), [HEAD])
        self.assertEqual(gate.reconcile(github, 7, write=True, automatic=True).action, 'request')
        self.assertEqual(github.calls, [('relabel', True), ('status', HEAD, 'success', gate.REQUESTED)])

    def test_push_during_the_check_writes_nothing(self):
        github = FakeGitHub(facts(), [OLD])
        self.assertEqual(gate.reconcile(github, 7, write=True, automatic=True).action, 'wait')
        self.assertEqual(github.calls, [])

    def test_read_only_run_never_writes(self):
        github = FakeGitHub(facts(), [HEAD])
        gate.reconcile(github, 7, write=False, automatic=True)
        self.assertEqual(github.calls, [])

    def test_waiting_status_is_written_once(self):
        waiting = facts(reviews=[])
        github = FakeGitHub(waiting, [])
        gate.reconcile(github, 7, write=True, automatic=True)
        description = github.calls[-1][3]
        waiting.gate_status = description
        gate.reconcile(github, 7, write=True, automatic=True)
        self.assertEqual(len(github.calls), 1)
        self.assertEqual(github.calls[0][2], 'pending')


if __name__ == '__main__':
    unittest.main()

"""Review gate: request CodeRabbit only after Greptile and the required CI checks are clean.

Order per pull request: Greptile reviews the current commit, its findings are resolved and the required checks
pass; then the gate adds the CodeRabbit opt-in label. Once CodeRabbit has published a review, Greptile no longer
gates the PR (it reviews only on open), and further CodeRabbit reviews are requested manually.

The gate is level-triggered: every run re-reads the facts of each open pull request and acts on the current
state, so a delayed, dropped or duplicated trigger loses nothing. All writes are idempotent.
"""
from __future__ import annotations

import argparse
import json
import re
import subprocess
import sys
from dataclasses import dataclass, field
from urllib.parse import quote

BASES = ('main', 'santacita-beta')
# Used only when a target branch reports no required checks (no protection rule or ruleset), so an
# unprotected branch can never pass the gate without CI.
FALLBACK_REQUIRED_CHECKS = frozenset({
    'Tests', 'Analyze (rust)', 'Analyze (javascript-typescript)', 'Analyze (actions)'})
GREPTILE = 'greptile-apps'
GREPTILE_CHECK = 'Greptile Review'
GREPTILE_SUMMARY_MARKER = '<!-- greptile_summary -->'
CODERABBIT = 'coderabbitai'
REVIEW_LABEL = 'coderabbit-ready'
# Commit statuses belong to the commit, not the PR: two PRs with the same head (e.g. into main and into
# santacita-beta) must not see or overwrite each other's marker, so each PR has its own context.
STATUS_CONTEXT = 'review-gate (#{number})'
REQUESTED = 'CodeRabbit requested for this commit'
REVIEWED = 'CodeRabbit has reviewed; request repeats manually'
STATUS_LIMIT = 140  # GitHub truncates commit status descriptions.


class NotReady(RuntimeError):
    """An expected waiting state, unlike an API or configuration failure."""


# ---- facts --------------------------------------------------------------------------------------------------

def bot(login: str, user: dict) -> bool:
    return user.get('type') == 'Bot' and user.get('login', '').removesuffix('[bot]') == login


def published(reviews: list[dict], login: str) -> list[dict]:
    return [r for r in reviews if bot(login, r['user']) and r['state'] not in {'PENDING', 'DISMISSED'}]


def greptile_summary_head(comments: list[dict], repository: str) -> str | None:
    """The commit named in the footer of Greptile's latest summary comment, if any."""
    summaries = [c for c in comments
                 if bot(GREPTILE, c['user']) and GREPTILE_SUMMARY_MARKER in (c.get('body') or '')]
    latest = max(summaries, key=lambda c: (c['updated_at'], c['id']), default=None)
    if latest is None:
        return None
    match = re.search(r'Last reviewed commit:\s*\[[^\n]*?\]\(https://github\.com/'
                      + re.escape(repository) + r'/commit/([0-9a-f]{40})\)', latest['body'])
    return match[1] if match else None


@dataclass
class Facts:
    head: str
    state: str
    draft: bool
    labels: set[str]
    reviews: list[dict]
    threads: list[dict]
    checks: dict[str, str]          # check name -> bucket (pass/fail/pending/skipping/cancel)
    required: set[str]
    summary_head: str | None = None
    gate_status: str | None = None  # description of this gate's latest status on the head commit


@dataclass
class Decision:
    action: str                     # 'skip' | 'wait' | 'request' | 'done'
    reasons: list[str] = field(default_factory=list)


def evaluate(f: Facts, *, automatic: bool) -> Decision:
    """Pure decision. `automatic` runs never repeat a CodeRabbit request; a manual run may."""
    if f.state != 'OPEN' or f.draft:
        return Decision('skip', ['The pull request must be open and ready for review.'])
    # Only a published CodeRabbit review starts the second stage: a label is a request, not a review.
    started = bool(published(f.reviews, CODERABBIT))
    if automatic and started:
        return Decision('done', [REVIEWED])
    if automatic and f.gate_status == REQUESTED:
        return Decision('done', [REQUESTED])

    reasons: list[str] = []
    if not started:
        greptile = max(published(f.reviews, GREPTILE), key=lambda r: r['id'], default=None)
        summary_ready = f.summary_head == f.head and f.checks.get(GREPTILE_CHECK) == 'pass'
        if greptile is not None and greptile['state'] == 'CHANGES_REQUESTED':
            reasons.append('Greptile still requests changes.')
        elif greptile is None and not summary_ready:
            reasons.append('Greptile has not reviewed this pull request yet.')
        elif greptile is not None and greptile['commit_id'] != f.head and not summary_ready:
            reasons.append('Greptile must review the current commit.')
        for thread in f.threads:
            first = (thread['comments']['nodes'] or [None])[0]
            author = (first or {}).get('author') or {}
            if (not thread['isResolved'] and author.get('__typename') == 'Bot'
                    and author.get('login', '').removesuffix('[bot]') == GREPTILE):
                reasons.append('Unresolved Greptile finding: ' + first['url'])
        for name, bucket in sorted(f.checks.items()):
            if name.lower().startswith('greptile') and bucket != 'pass':
                reasons.append(f'Greptile check is {bucket}.')

    if not f.required:
        reasons.append('No required CI checks are configured; readiness cannot be confirmed.')
    for name in sorted(f.required):
        bucket = f.checks.get(name)
        if bucket is None:
            reasons.append(f'Required check has not reported: {name}')
        elif bucket != 'pass':
            reasons.append(f'Required check {name} is {bucket}.')
    if any(name.lower().startswith('coderabbit') and bucket == 'pending' for name, bucket in f.checks.items()):
        reasons.append('CodeRabbit is already running.')
    return Decision('wait', reasons) if reasons else Decision('request')


def status_for(decision: Decision) -> tuple[str, str]:
    if decision.action == 'request':
        return 'success', REQUESTED
    if decision.action == 'done':
        return 'success', decision.reasons[0]
    text = 'Waiting: ' + decision.reasons[0]
    return 'pending', text if len(text) <= STATUS_LIMIT else text[:STATUS_LIMIT - 1] + '…'


# ---- GitHub -------------------------------------------------------------------------------------------------

def gh_json(arguments: list[str], *, allowed_codes=(0,)):
    result = subprocess.run(['gh', *arguments], capture_output=True, text=True, check=False)
    # `gh pr checks --required` exits 1 with this message when the branch requires nothing.
    if (result.returncode == 1 and arguments[:2] == ['pr', 'checks'] and '--required' in arguments
            and not result.stdout.strip()
            and result.stderr.strip().startswith('no required checks reported on the ')):
        return []
    if result.returncode not in allowed_codes:
        raise RuntimeError(f"{' '.join(arguments[:2])}: {result.stderr.strip() or 'GitHub request failed'}")
    if not result.stdout.strip():
        return None
    try:
        return json.loads(result.stdout)
    except json.JSONDecodeError as exc:
        raise RuntimeError(f"{' '.join(arguments[:2])}: invalid JSON response") from exc


def paged(path: str) -> list[dict]:
    return [item for page in gh_json(['api', '--paginate', '--slurp', path]) or [] for item in page]


THREADS_QUERY = '''query($owner:String!,$name:String!,$number:Int!,$cursor:String) {
  repository(owner:$owner,name:$name) { pullRequest(number:$number) {
    reviewThreads(first:100,after:$cursor) {
      nodes { isResolved comments(first:1) { nodes { author { __typename login } url } } }
      pageInfo { hasNextPage endCursor } } } } }'''


class GitHub:
    def __init__(self, repository: str):
        if len(repository.split('/')) != 2:
            raise ValueError('Use owner/repository')
        self.repository = repository

    def pull(self, number: int) -> dict:
        return gh_json(['pr', 'view', str(number), '--repo', self.repository,
                        '--json', 'number,headRefOid,state,isDraft,baseRefName,labels'])

    def open_pulls(self, base: str) -> list[int]:
        pulls = paged(f'repos/{self.repository}/pulls?state=open&base={quote(base, safe="")}&per_page=100')
        return [p['number'] for p in pulls if not p['draft']]

    def threads(self, number: int) -> list[dict]:
        owner, name = self.repository.split('/')
        threads, cursor = [], None
        while True:
            arguments = ['api', 'graphql', '-f', f'query={THREADS_QUERY}', '-f', f'owner={owner}',
                         '-f', f'name={name}', '-F', f'number={number}']
            if cursor:
                arguments += ['-f', f'cursor={cursor}']
            result = gh_json(arguments)
            if result.get('errors'):
                raise RuntimeError('Could not read all review discussions')
            page = result['data']['repository']['pullRequest']['reviewThreads']
            threads += page['nodes']
            if not page['pageInfo']['hasNextPage']:
                return threads
            cursor = page['pageInfo']['endCursor']

    def checks(self, number: int) -> dict[str, str]:
        arguments = ['pr', 'checks', str(number), '--repo', self.repository, '--json', 'name,bucket']
        rows = (gh_json([*arguments, '--required'], allowed_codes=(0, 1, 8)) or []) + \
            (gh_json(arguments, allowed_codes=(0, 1, 8)) or [])
        return {row['name']: row['bucket'] for row in rows}

    def required(self, base: str) -> set[str]:
        branch = gh_json(['api', f'repos/{self.repository}/branches/{quote(base, safe="")}'])
        protection = branch.get('protection')
        if branch['protected'] and not isinstance(protection, dict):
            raise RuntimeError('The protected branch protection summary is unavailable.')
        rule = (protection or {}).get('required_status_checks') or {}
        names = set(rule.get('contexts', ())) | {c['context'] for c in rule.get('checks', ())}
        for entry in paged(f'repos/{self.repository}/rules/branches/{quote(base, safe="")}'):
            if entry['type'] == 'required_status_checks':
                names |= {c['context'] for c in entry['parameters']['required_status_checks']}
        return names or set(FALLBACK_REQUIRED_CHECKS)

    def gate_status(self, sha: str, number: int) -> str | None:
        context = STATUS_CONTEXT.format(number=number)
        statuses = paged(f'repos/{self.repository}/commits/{sha}/statuses?per_page=100')
        mine = [s for s in statuses if s['context'] == context]
        return max(mine, key=lambda s: s['id'])['description'] if mine else None

    def set_status(self, sha: str, number: int, state: str, description: str) -> None:
        gh_json(['api', '--method', 'POST', f'repos/{self.repository}/statuses/{sha}', '-f', f'state={state}',
                 '-f', f'context={STATUS_CONTEXT.format(number=number)}', '-f', f'description={description}'])

    def relabel(self, number: int, present: bool) -> None:
        # Removing first makes CodeRabbit see a fresh `labeled` event for the current commit.
        # Any other removal failure must stop here: adding a label that is still present produces no event, and
        # the request would be recorded without CodeRabbit ever seeing it.
        endpoint = f'repos/{self.repository}/issues/{number}/labels'
        if present:
            try:
                gh_json(['api', '--method', 'DELETE', f'{endpoint}/{REVIEW_LABEL}'])
            except RuntimeError as exc:
                if '(HTTP 404)' not in str(exc):  # only "already removed" is harmless
                    raise
        gh_json(['api', '--method', 'POST', endpoint, '-f', f'labels[]={REVIEW_LABEL}'])

    def facts(self, number: int) -> Facts:
        pull = self.pull(number)
        return Facts(
            head=pull['headRefOid'], state=pull['state'], draft=pull['isDraft'],
            labels={label['name'] for label in pull['labels']},
            reviews=paged(f'repos/{self.repository}/pulls/{number}/reviews?per_page=100'),
            threads=self.threads(number), checks=self.checks(number), required=self.required(pull['baseRefName']),
            summary_head=greptile_summary_head(
                paged(f'repos/{self.repository}/issues/{number}/comments?per_page=100'), self.repository),
            gate_status=self.gate_status(pull['headRefOid'], number))


# ---- reconcile ----------------------------------------------------------------------------------------------

def reconcile(github, number: int, *, write: bool, automatic: bool) -> Decision:
    facts = github.facts(number)
    decision = evaluate(facts, automatic=automatic)
    if decision.action == 'request':
        # A push while the facts were read invalidates them: never label a commit that was not checked.
        if github.pull(number)['headRefOid'] != facts.head:
            return Decision('wait', ['The pull request changed while it was checked; the next run rechecks it.'])
        if write:
            github.relabel(number, REVIEW_LABEL in facts.labels)
    if write and decision.action != 'skip':
        state, description = status_for(decision)
        if facts.gate_status != description:
            github.set_status(facts.head, number, state, description)
    return decision


def report(number: int, decision: Decision, *, write: bool) -> str:
    verb = {'request': 'CodeRabbit requested' if write else 'ready for CodeRabbit',
            'done': 'nothing to do', 'skip': 'skipped', 'wait': 'waiting'}[decision.action]
    lines = [f'PR #{number}: {verb}'] + [f'  - {reason}' for reason in decision.reasons]
    return '\n'.join(lines)


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument('--repo', required=True, help='owner/repository')
    target = parser.add_mutually_exclusive_group(required=True)
    target.add_argument('--pr', type=int, help='One pull request; a manual request may repeat CodeRabbit')
    target.add_argument('--sweep', action='store_true', help='All open, ready pull requests on the gated bases')
    parser.add_argument('--write', action='store_true', help='Apply the label and the commit status')
    args = parser.parse_args(argv)
    if args.pr is not None and args.pr < 1:
        parser.error('Use a positive pull request number without #')
    github = GitHub(args.repo)
    numbers = [args.pr] if args.pr else sorted({n for base in BASES for n in github.open_pulls(base)})
    errors = []
    for number in numbers:
        try:
            print(report(number, reconcile(github, number, write=args.write, automatic=args.sweep),
                         write=args.write))
        except (RuntimeError, KeyError, ValueError) as exc:
            errors.append(f'PR #{number}: {exc}')
    if not numbers:
        print('No open pull requests on ' + ', '.join(BASES))
    for error in errors:
        print(error, file=sys.stderr)
    return 1 if errors else 0


if __name__ == '__main__':
    sys.exit(main())

// Offline tests of the review gate: decisions, parsing, the Octokit adapter and the write order. No network.
import assert from 'node:assert/strict'
import {describe, test} from 'node:test'
import * as gate from './review-gate.mjs'

const REPO = 'owner/repo'
const HEAD = 'a'.repeat(40)
const OLD = 'b'.repeat(40)
const REQUIRED = ['Tests', 'Analyze (rust)']

const bot = login => ({login: `${login}[bot]`, type: 'Bot'})
const review = (login, state = 'COMMENTED', commit = HEAD, id = 1) => ({id, user: bot(login), state, commit_id: commit})
const thread = (login, resolved = false) => ({
  isResolved: resolved,
  comments: {nodes: [{author: {__typename: 'Bot', login}, url: 'https://example.test/t'}]},
})
const facts = overrides => ({
  head: HEAD, state: 'open', draft: false, author: 'itwormz', labels: new Set(),
  reviews: [review(gate.GREPTILE)], threads: [],
  checks: new Map([['Tests', 'pass'], ['Analyze (rust)', 'pass'], [gate.GREPTILE_CHECK, 'pass']]),
  required: new Set(REQUIRED), summaryHead: null, gateStatus: null,
  ...overrides,
})
const decide = (overrides, automatic = true) => gate.evaluate(facts(overrides), {automatic})

describe('decisions', () => {
  test('a clean Greptile review and green CI request CodeRabbit', () => {
    assert.equal(decide({}).action, 'request')
  })

  test('closed or draft pull requests are skipped', () => {
    assert.equal(decide({state: 'closed'}).action, 'skip')
    assert.equal(decide({draft: true}).action, 'skip')
  })

  test('a Greptile review of an older commit waits', () => {
    const decision = decide({reviews: [review(gate.GREPTILE, 'COMMENTED', OLD)]})
    assert.equal(decision.action, 'wait')
    assert.ok(decision.reasons.includes('Greptile must review the current commit.'))
  })

  test('the summary footer for the head with a passing Greptile check counts as current', () => {
    assert.equal(decide({reviews: [review(gate.GREPTILE, 'COMMENTED', OLD)], summaryHead: HEAD}).action, 'request')
  })

  test('a summary without a passing Greptile check is not enough', () => {
    const checks = new Map([['Tests', 'pass'], ['Analyze (rust)', 'pass'], [gate.GREPTILE_CHECK, 'pending']])
    assert.equal(decide({reviews: [], summaryHead: HEAD, checks}).action, 'wait')
  })

  test('no Greptile review waits', () => {
    assert.ok(decide({reviews: []}).reasons.includes('Greptile has not reviewed this pull request yet.'))
  })

  test('changes requested and unresolved Greptile findings wait', () => {
    const decision = decide({
      reviews: [review(gate.GREPTILE, 'CHANGES_REQUESTED')],
      threads: [thread('greptile-apps'), thread('greptile-apps', true), thread('someone-else')],
    })
    assert.equal(decision.action, 'wait')
    assert.ok(decision.reasons.includes('Greptile still requests changes.'))
    assert.equal(decision.reasons.filter(r => r.startsWith('Unresolved Greptile finding')).length, 1)
  })

  test('a request label alone does not bypass Greptile', () => {
    // CodeRabbit finding on PR #49: a label from an earlier commit is a request, not a review.
    const stale = {labels: new Set([gate.REVIEW_LABEL]), reviews: [review(gate.GREPTILE, 'CHANGES_REQUESTED')]}
    assert.equal(decide(stale, false).action, 'wait')
    assert.equal(decide(stale, true).action, 'wait')
  })

  test('a published CodeRabbit review ends automatic requests', () => {
    const reviews = [review(gate.GREPTILE, 'COMMENTED', OLD), review(gate.CODERABBIT, 'COMMENTED', OLD, 2)]
    assert.equal(decide({reviews}).action, 'done')
  })

  test('a manual repeat after CodeRabbit skips Greptile but not CI', () => {
    const reviews = [review(gate.GREPTILE, 'CHANGES_REQUESTED', OLD), review(gate.CODERABBIT, 'COMMENTED', OLD, 2)]
    assert.equal(decide({reviews, threads: [thread('greptile-apps')]}, false).action, 'request')
    const checks = new Map([['Tests', 'fail'], ['Analyze (rust)', 'pass']])
    assert.ok(decide({reviews, checks}, false).reasons.includes('Required check Tests is fail.'))
  })

  test('a pending or dismissed CodeRabbit review is not a review', () => {
    for (const state of ['PENDING', 'DISMISSED']) {
      assert.equal(decide({reviews: [review(gate.GREPTILE), review(gate.CODERABBIT, state, HEAD, 2)]}).action, 'request')
    }
  })

  test('a clean CodeRabbit run without a review object still starts the second stage', () => {
    // A clean CodeRabbit run posts only its walkthrough comment: no pull-request review exists.
    const greptileOnOld = [review(gate.GREPTILE, 'COMMENTED', OLD)]
    assert.equal(decide({reviews: greptileOnOld, coderabbitSummary: true}).action, 'done')
    assert.equal(decide({reviews: greptileOnOld, coderabbitSummary: true}, false).action, 'request')
  })

  test("a passing CodeRabbit check is not this PR's review", () => {
    // The check belongs to the commit: a PR with the same head into another base would see it too.
    const passed = new Map([...facts().checks, ['CodeRabbit', 'pass']])
    const decision = decide({reviews: [review(gate.GREPTILE, 'COMMENTED', OLD)], checks: passed})
    assert.equal(decision.action, 'wait')
    assert.ok(decision.reasons.includes('Greptile must review the current commit.'))
  })

  test('a skipped CodeRabbit run or a label is not a review', () => {
    const skipped = new Map([...facts().checks, ['CodeRabbit', 'skipping']])
    const stale = {reviews: [review(gate.GREPTILE, 'COMMENTED', OLD)], checks: skipped, labels: new Set([gate.REVIEW_LABEL])}
    assert.equal(decide(stale).action, 'wait')
    assert.ok(decide(stale).reasons.includes('Greptile must review the current commit.'))
  })

  test('a person named like the bot is not the bot', () => {
    const impostor = {id: 3, user: {login: 'coderabbitai', type: 'User'}, state: 'COMMENTED', commit_id: HEAD}
    assert.equal(decide({reviews: [review(gate.GREPTILE), impostor]}).action, 'request')
  })

  test('a request recorded for this commit is not repeated automatically', () => {
    assert.equal(decide({gateStatus: gate.REQUESTED}).action, 'done')
    assert.equal(decide({gateStatus: gate.REQUESTED}, false).action, 'request')
  })

  test('missing, failing and unconfigured checks wait', () => {
    const decision = decide({checks: new Map([['Tests', 'pending'], [gate.GREPTILE_CHECK, 'pass']])})
    assert.ok(decision.reasons.includes('Required check Tests is pending.'))
    assert.ok(decision.reasons.includes('Required check has not reported: Analyze (rust)'))
    assert.ok(decide({required: new Set()}).reasons
      .includes('No required CI checks are configured; readiness cannot be confirmed.'))
  })

  test('a running CodeRabbit waits', () => {
    const checks = new Map([...facts().checks, ['CodeRabbit', 'pending']])
    assert.ok(decide({checks}).reasons.includes('CodeRabbit is already running.'))
  })

  test('the status description fits the GitHub limit', () => {
    const [state, text] = gate.statusFor({action: 'wait', reasons: ['x'.repeat(500)]})
    assert.equal(state, 'pending')
    assert.equal(text.length, gate.STATUS_LIMIT)
  })
})

describe('bot authors', () => {
  test('dependency bot pull requests are not gated', () => {
    for (const author of ['dependabot[bot]', 'app/dependabot', 'dependabot', 'renovate[bot]', 'github-actions[bot]']) {
      assert.equal(decide({author, reviews: []}).action, 'exempt', author)
    }
  })

  test('people and other apps are gated', () => {
    for (const author of ['itwormz', 'some-other-bot[bot]']) assert.equal(decide({author, reviews: []}).action, 'wait')
  })

  test('an old waiting status on a bot pull request is cleared once', async () => {
    const stuck = facts({author: 'dependabot[bot]', reviews: [], gateStatus: 'Waiting: Greptile has not reviewed this pull request yet.'})
    const client = fakeClient(stuck)
    await gate.reconcile(client, 54, {write: true, automatic: true})
    assert.deepEqual(client.calls, [['status', HEAD, 54, 'success', gate.EXEMPT]])
    stuck.gateStatus = gate.EXEMPT
    await gate.reconcile(client, 54, {write: true, automatic: true})
    assert.equal(client.calls.length, 1)
  })

  test('a bot pull request without a status gets none', async () => {
    const client = fakeClient(facts({author: 'dependabot[bot]', reviews: []}))
    await gate.reconcile(client, 55, {write: true, automatic: true})
    assert.deepEqual(client.calls, [])
  })
})

describe('parsing', () => {
  const comment = (body, id = 1, updated = '2026-11-15T10:00:00Z', login = 'greptile-apps') =>
    ({id, updated_at: updated, user: bot(login), body})

  test('the summary head is read only from the footer of the latest summary', () => {
    const footer = `${gate.GREPTILE_SUMMARY_MARKER}\nsee https://github.com/${REPO}/commit/${OLD}\n`
      + `Last reviewed commit: [fix](https://github.com/${REPO}/commit/${HEAD})`
    const older = `${gate.GREPTILE_SUMMARY_MARKER}\nLast reviewed commit: [x](https://github.com/${REPO}/commit/${OLD})`
    assert.equal(gate.greptileSummaryHead([comment(older, 1, '2026-11-14T10:00:00Z'), comment(footer, 2)], REPO), HEAD)
  })

  test('a CodeRabbit walkthrough counts as a finished review, a skip notice does not', () => {
    const walkthrough = '<!-- This is an auto-generated comment: summarize by coderabbit.ai -->\n<!-- walkthrough_start -->\nx'
    const skipped = '<!-- This is an auto-generated comment: skip review by coderabbit.ai -->\nReview skipped'
    assert.equal(gate.coderabbitSummary([comment(walkthrough, 1, undefined, 'coderabbitai')]), true)
    assert.equal(gate.coderabbitSummary([comment(skipped, 1, undefined, 'coderabbitai')]), false)
    const impostor = {id: 2, user: {login: 'coderabbitai', type: 'User'}, body: walkthrough}
    assert.equal(gate.coderabbitSummary([impostor, comment(walkthrough, 3, undefined, 'greptile-apps')]), false)
  })

  test('a summary from another author or repository is ignored', () => {
    const body = `${gate.GREPTILE_SUMMARY_MARKER}\nLast reviewed commit: [x](https://github.com/${REPO}/commit/${HEAD})`
    assert.equal(gate.greptileSummaryHead([comment(body, 1, undefined, 'someone')], REPO), null)
    assert.equal(gate.greptileSummaryHead([comment(body)], 'other/repo'), null)
  })

  test('check runs and statuses map to the buckets gh pr checks shows', () => {
    const run = (status, conclusion = null) => ({id: 1, name: 'x', status, conclusion})
    assert.equal(gate.bucketOf(run('in_progress')), 'pending')
    assert.equal(gate.bucketOf(run('queued')), 'pending')
    assert.equal(gate.bucketOf(run('completed', 'success')), 'pass')
    assert.equal(gate.bucketOf(run('completed', 'neutral')), 'skipping')
    assert.equal(gate.bucketOf(run('completed', 'skipped')), 'skipping')
    assert.equal(gate.bucketOf(run('completed', 'cancelled')), 'cancel')
    for (const conclusion of ['failure', 'timed_out', 'action_required', 'startup_failure', 'stale']) {
      assert.equal(gate.bucketOf(run('completed', conclusion)), 'fail', conclusion)
    }
    assert.equal(gate.bucketOf({id: 1, context: 'x', state: 'success'}), 'pass')
    assert.equal(gate.bucketOf({id: 1, context: 'x', state: 'pending'}), 'pending')
    assert.equal(gate.bucketOf({id: 1, context: 'x', state: 'failure'}), 'fail')
    assert.equal(gate.bucketOf({id: 1, context: 'x', state: 'error'}), 'fail')
  })
})

// A stand-in for the Octokit client actions/github-script provides: responses keyed by REST method.
function fakeOctokit(responses = {}) {
  const calls = []
  const endpoint = name => {
    const fn = async params => {
      calls.push([name, params])
      const response = responses[name]
      if (response instanceof Error) throw response
      return {data: typeof response === 'function' ? response(params) : response}
    }
    fn.endpointName = name
    return fn
  }
  const names = ['pulls.get', 'pulls.list', 'pulls.listReviews', 'issues.listComments', 'issues.removeLabel',
    'issues.addLabels', 'checks.listForRef', 'repos.listCommitStatusesForRef', 'repos.getBranch',
    'repos.getBranchRules', 'repos.createCommitStatus']
  const rest = {}
  for (const name of names) {
    const [group, method] = name.split('.')
    rest[group] ??= {}
    rest[group][method] = endpoint(name)
  }
  const unwrap = data => data?.check_runs ?? data ?? []
  return {
    calls, rest,
    paginate: async (fn, params) => unwrap((await fn(params)).data),
    graphql: async () => responses.graphql,
  }
}
const httpError = status => Object.assign(new Error(`HTTP ${status}`), {status})

describe('Octokit adapter', () => {
  test('a label already gone is harmless', async () => {
    const github = fakeOctokit({'issues.removeLabel': httpError(404), 'issues.addLabels': []})
    await gate.createClient(github, 'owner', 'repo').relabel(7, true)
    assert.deepEqual(github.calls.map(([name]) => name), ['issues.removeLabel', 'issues.addLabels'])
  })

  test('a failed removal stops before the label is added', async () => {
    const github = fakeOctokit({'issues.removeLabel': httpError(500), 'issues.addLabels': []})
    await assert.rejects(gate.createClient(github, 'owner', 'repo').relabel(7, true), /HTTP 500/)
    assert.deepEqual(github.calls.map(([name]) => name), ['issues.removeLabel'])
  })

  test('each pull request reads and writes only its own status on a shared commit', async () => {
    const github = fakeOctokit({
      'repos.listCommitStatusesForRef': [
        {id: 1, context: 'review-gate (#7)', description: gate.REQUESTED},
        {id: 3, context: 'review-gate (#8)', description: 'Waiting: newer'},
        {id: 2, context: 'review-gate (#8)', description: 'Waiting: older'},
      ],
      'repos.createCommitStatus': {},
    })
    const client = gate.createClient(github, 'owner', 'repo')
    assert.equal(await client.gateStatus(HEAD, 7), gate.REQUESTED)
    assert.equal(await client.gateStatus(HEAD, 8), 'Waiting: newer')
    assert.equal(await client.gateStatus(HEAD, 9), null)
    await client.setStatus(HEAD, 8, 'success', gate.REQUESTED)
    assert.equal(github.calls.at(-1)[1].context, 'review-gate (#8)')
  })

  test('checks keep the latest run per name and ignore the gate statuses', async () => {
    const github = fakeOctokit({
      'checks.listForRef': {check_runs: [
        {id: 1, name: 'Tests', status: 'completed', conclusion: 'failure'},
        {id: 2, name: 'Tests', status: 'completed', conclusion: 'success'},
        {id: 3, name: 'CodeRabbit', status: 'in_progress', conclusion: null},
      ]},
      'repos.listCommitStatusesForRef': [
        {id: 5, context: 'Greptile Review', state: 'success'},
        {id: 6, context: 'review-gate (#7)', state: 'pending'},
      ],
    })
    const checks = await gate.createClient(github, 'owner', 'repo').checks(HEAD)
    assert.deepEqual(Object.fromEntries(checks), {Tests: 'pass', CodeRabbit: 'pending', 'Greptile Review': 'pass'})
  })

  test('required checks come from protection and rulesets, with a fallback for an unprotected branch', async () => {
    const protectedBranch = fakeOctokit({
      'repos.getBranch': {protected: true, protection: {required_status_checks: {contexts: ['Tests'], checks: [{context: 'Lint'}]}}},
      'repos.getBranchRules': [{type: 'required_status_checks', parameters: {required_status_checks: [{context: 'Extra'}]}}],
    })
    assert.deepEqual([...await gate.createClient(protectedBranch, 'o', 'r').required('main')].sort(), ['Extra', 'Lint', 'Tests'])
    const bare = fakeOctokit({'repos.getBranch': {protected: false}, 'repos.getBranchRules': []})
    assert.deepEqual([...await gate.createClient(bare, 'o', 'r').required('dev')].sort(), [...gate.FALLBACK_REQUIRED_CHECKS].sort())
  })
})

function fakeClient(current, heads = [current.head]) {
  const pending = [...heads]
  return {
    calls: [],
    async facts() { return current },
    async pull() { return {head: {sha: pending.shift()}} },
    async relabel(number, present) { this.calls.push(['relabel', present]) },
    async setStatus(sha, number, state, description) { this.calls.push(['status', sha, number, state, description]) },
  }
}

describe('reconcile', () => {
  test('a ready pull request is labelled, then marked', async () => {
    const client = fakeClient(facts({labels: new Set([gate.REVIEW_LABEL])}))
    assert.equal((await gate.reconcile(client, 7, {write: true, automatic: true})).action, 'request')
    assert.deepEqual(client.calls, [['relabel', true], ['status', HEAD, 7, 'success', gate.REQUESTED]])
  })

  test('a push during the check writes nothing', async () => {
    const client = fakeClient(facts(), [OLD])
    assert.equal((await gate.reconcile(client, 7, {write: true, automatic: true})).action, 'wait')
    assert.deepEqual(client.calls, [])
  })

  test('a read-only run never writes', async () => {
    const client = fakeClient(facts())
    await gate.reconcile(client, 7, {write: false, automatic: true})
    assert.deepEqual(client.calls, [])
  })

  test('a waiting status is written once', async () => {
    const waiting = facts({reviews: []})
    const client = fakeClient(waiting, [])
    await gate.reconcile(client, 7, {write: true, automatic: true})
    waiting.gateStatus = client.calls.at(-1)[4]
    await gate.reconcile(client, 7, {write: true, automatic: true})
    assert.equal(client.calls.length, 1)
    assert.equal(client.calls[0][3], 'pending')
  })

  test('a failed relabel records no request', async () => {
    const client = fakeClient(facts({labels: new Set([gate.REVIEW_LABEL])}))
    client.relabel = async () => { throw httpError(500) }
    await assert.rejects(gate.reconcile(client, 7, {write: true, automatic: true}), /HTTP 500/)
    assert.deepEqual(client.calls, [])
  })
})

describe('run', () => {
  const core = () => {
    const out = {info: [], failed: [], summary: []}
    const summary = {addHeading: () => summary, addCodeBlock: text => { out.summary.push(text); return summary }, write: async () => {}}
    return {out, info: text => out.info.push(text), setFailed: text => out.failed.push(text), summary}
  }
  const context = {repo: {owner: 'owner', repo: 'repo'}}

  test('a malformed PR number fails without touching GitHub', async () => {
    const c = core()
    await gate.run({context, core: c, pr: '#56', client: {}})
    assert.deepEqual(c.out.failed, ['Use a positive pull request number without #'])
  })

  test('a sweep reconciles each open PR once, automatically', async () => {
    const seen = []
    const client = {
      openPulls: async branch => (branch === 'main' ? [5, 3] : [3]),
      facts: async number => { seen.push(number); return facts({state: 'closed'}) },
    }
    const c = core()
    await gate.run({context, core: c, client})
    assert.deepEqual(seen, [3, 5])
    assert.deepEqual(c.out.failed, [])
  })

  test('an error on one PR fails the run after the others are reconciled', async () => {
    const client = {
      openPulls: async () => [1, 2],
      facts: async number => { if (number === 1) throw new Error('boom'); return facts({state: 'closed'}) },
    }
    const c = core()
    await gate.run({context, core: c, client})
    assert.match(c.out.info.join('\n'), /PR #2: skipped/)
    assert.deepEqual(c.out.failed, ['PR #1: boom'])
  })
})

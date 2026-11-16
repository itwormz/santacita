// Review gate: request CodeRabbit only after Greptile and the required CI checks are clean.
//
// Order per pull request: Greptile reviews the current commit, its findings are resolved and the required checks
// pass; then the gate adds the CodeRabbit opt-in label. Once CodeRabbit has published a review, Greptile no longer
// gates the PR (it reviews only on open), and further CodeRabbit reviews are requested manually.
//
// The gate is level-triggered: every run re-reads the facts of each open pull request and acts on the current
// state, so a delayed, dropped or duplicated trigger loses nothing. All writes are idempotent.
//
// Run by actions/github-script (.github/workflows/review-gate.yml):
//   const { run } = await import(`${process.env.GITHUB_WORKSPACE}/scripts/review-gate.mjs`)
//   await run({ github, context, core, pr, write })

export const BASES = ['main', 'santacita-beta']
// Used only when a target branch reports no required checks (no protection rule or ruleset), so an unprotected
// branch can never pass the gate without CI.
export const FALLBACK_REQUIRED_CHECKS = ['Tests', 'Analyze (rust)', 'Analyze (javascript-typescript)', 'Analyze (actions)']
export const GREPTILE = 'greptile-apps'
export const GREPTILE_CHECK = 'Greptile Review'
export const GREPTILE_SUMMARY_MARKER = '<!-- greptile_summary -->'
export const CODERABBIT = 'coderabbitai'
export const CODERABBIT_CHECK = 'CodeRabbit'
export const CODERABBIT_WALKTHROUGH_MARKER = '<!-- walkthrough_start -->'
export const REVIEW_LABEL = 'coderabbit-ready'
// Neither reviewer looks at these authors' PRs: Greptile does not review them and CodeRabbit ignores them
// (`ignore_usernames` in .coderabbit.yaml). Waiting for a review would leave the PR pending forever.
export const EXEMPT_AUTHORS = new Set(['dependabot', 'renovate', 'github-actions'])
export const EXEMPT = 'Not gated: dependency and automation PRs get no bot review'
export const REQUESTED = 'CodeRabbit requested for this commit'
export const REVIEWED = 'CodeRabbit has reviewed; request repeats manually'
export const STATUS_LIMIT = 140 // GitHub truncates commit status descriptions.
// Commit statuses belong to the commit, not the PR: two PRs with the same head (e.g. into main and into
// santacita-beta) must not see or overwrite each other's marker, so each PR has its own context.
export const statusContext = number => `review-gate (#${number})`

export class NotReady extends Error {}

// ---- facts --------------------------------------------------------------------------------------------------

const botName = login => (login ?? '').replace(/^app\//, '').replace(/\[bot\]$/, '')

export function isBot(name, user) {
  return user?.type === 'Bot' && botName(user.login) === name
}

export function published(reviews, name) {
  return reviews.filter(review => isBot(name, review.user) && !['PENDING', 'DISMISSED'].includes(review.state))
}

export function isExemptAuthor(login) {
  return EXEMPT_AUTHORS.has(botName(login))
}

function escapeRegExp(text) {
  return text.replace(/[.*+?^${}()|[\]\\]/g, '\\$&')
}

/** The commit named in the footer of Greptile's latest summary comment, if any. */
export function greptileSummaryHead(comments, repository) {
  const summaries = comments.filter(c => isBot(GREPTILE, c.user) && (c.body ?? '').includes(GREPTILE_SUMMARY_MARKER))
  const latest = summaries.reduce((best, c) =>
    !best || c.updated_at > best.updated_at || (c.updated_at === best.updated_at && c.id > best.id) ? c : best, null)
  if (!latest) return null
  const footer = new RegExp(String.raw`Last reviewed commit:\s*\[[^\n]*?\]\(https://github\.com/`
    + escapeRegExp(repository) + String.raw`/commit/([0-9a-f]{40})\)`)
  return latest.body.match(footer)?.[1] ?? null
}

/**
 * Whether CodeRabbit has finished a review of this PR. A clean run publishes no pull-request review, only its
 * walkthrough comment; a skipped run (base not admitted, incremental review off) posts no walkthrough.
 */
export function coderabbitSummary(comments) {
  return comments.some(c => isBot(CODERABBIT, c.user) && (c.body ?? '').includes(CODERABBIT_WALKTHROUGH_MARKER))
}

/** The `gh pr checks` bucket of a check run or a commit status. */
export function bucketOf(item) {
  if ('conclusion' in item || 'status' in item && !('state' in item)) {
    if (item.status !== 'completed') return 'pending'
    return {success: 'pass', neutral: 'skipping', skipped: 'skipping', cancelled: 'cancel'}[item.conclusion] ?? 'fail'
  }
  return {success: 'pass', pending: 'pending'}[item.state] ?? 'fail'
}

/**
 * Facts of one pull request.
 * @typedef {{head: string, state: string, draft: boolean, author: string, labels: Set<string>, reviews: object[],
 *   threads: object[], checks: Map<string, string>, required: Set<string>, summaryHead?: string|null,
 *   coderabbitSummary?: boolean,
 *   gateStatus?: string|null}} Facts
 */

/**
 * Pure decision. `automatic` runs never repeat a CodeRabbit request; a manual run may.
 * @returns {{action: 'skip'|'exempt'|'wait'|'request'|'done', reasons: string[]}}
 */
export function evaluate(f, {automatic}) {
  if (f.state !== 'open' || f.draft) return {action: 'skip', reasons: ['The pull request must be open and ready for review.']}
  if (isExemptAuthor(f.author)) return {action: 'exempt', reasons: [EXEMPT]}
  // Only a finished CodeRabbit review starts the second stage: a published review, its walkthrough comment
  // (a clean run publishes nothing else) or its passing check. A label is a request, not a review.
  const started = published(f.reviews, CODERABBIT).length > 0 || Boolean(f.coderabbitSummary)
    || f.checks.get(CODERABBIT_CHECK) === 'pass'
  if (automatic && started) return {action: 'done', reasons: [REVIEWED]}
  if (automatic && f.gateStatus === REQUESTED) return {action: 'done', reasons: [REQUESTED]}

  const reasons = []
  if (!started) {
    const greptile = published(f.reviews, GREPTILE).reduce((best, r) => (!best || r.id > best.id ? r : best), null)
    const summaryReady = f.summaryHead === f.head && f.checks.get(GREPTILE_CHECK) === 'pass'
    if (greptile?.state === 'CHANGES_REQUESTED') reasons.push('Greptile still requests changes.')
    else if (!greptile && !summaryReady) reasons.push('Greptile has not reviewed this pull request yet.')
    else if (greptile && greptile.commit_id !== f.head && !summaryReady) reasons.push('Greptile must review the current commit.')
    for (const thread of f.threads) {
      const first = thread.comments.nodes[0]
      const author = first?.author ?? {}
      if (!thread.isResolved && author.__typename === 'Bot' && botName(author.login) === GREPTILE) {
        reasons.push('Unresolved Greptile finding: ' + first.url)
      }
    }
    for (const [name, bucket] of [...f.checks].sort(([a], [b]) => a.localeCompare(b))) {
      if (name.toLowerCase().startsWith('greptile') && bucket !== 'pass') reasons.push(`Greptile check is ${bucket}.`)
    }
  }

  if (f.required.size === 0) reasons.push('No required CI checks are configured; readiness cannot be confirmed.')
  for (const name of [...f.required].sort()) {
    const bucket = f.checks.get(name)
    if (bucket === undefined) reasons.push(`Required check has not reported: ${name}`)
    else if (bucket !== 'pass') reasons.push(`Required check ${name} is ${bucket}.`)
  }
  if ([...f.checks].some(([name, bucket]) => name.toLowerCase().startsWith('coderabbit') && bucket === 'pending')) {
    reasons.push('CodeRabbit is already running.')
  }
  return reasons.length ? {action: 'wait', reasons} : {action: 'request', reasons: []}
}

export function statusFor(decision) {
  if (decision.action === 'request') return ['success', REQUESTED]
  if (decision.action === 'done' || decision.action === 'exempt') return ['success', decision.reasons[0]]
  const text = 'Waiting: ' + decision.reasons[0]
  return ['pending', text.length <= STATUS_LIMIT ? text : text.slice(0, STATUS_LIMIT - 1) + '…']
}

// ---- GitHub -------------------------------------------------------------------------------------------------

const THREADS_QUERY = `query($owner:String!,$name:String!,$number:Int!,$cursor:String) {
  repository(owner:$owner,name:$name) { pullRequest(number:$number) {
    reviewThreads(first:100,after:$cursor) {
      nodes { isResolved comments(first:1) { nodes { author { __typename login } url } } }
      pageInfo { hasNextPage endCursor } } } } }`

/** Thin Octokit adapter: the only place that talks to GitHub. `github` is the client actions/github-script gives. */
export function createClient(github, owner, repo) {
  const repository = `${owner}/${repo}`
  const base = {owner, repo}
  const latest = (items, key) => {
    const byName = new Map()
    for (const item of items) {
      const known = byName.get(key(item))
      if (!known || item.id > known.id) byName.set(key(item), item)
    }
    return [...byName.values()]
  }
  const client = {
    repository,
    async pull(number) {
      return (await github.rest.pulls.get({...base, pull_number: number})).data
    },
    async openPulls(branch) {
      const pulls = await github.paginate(github.rest.pulls.list, {...base, state: 'open', base: branch, per_page: 100})
      return pulls.filter(p => !p.draft).map(p => p.number)
    },
    async threads(number) {
      const threads = []
      let cursor = null
      for (;;) {
        const result = await github.graphql(THREADS_QUERY, {owner, name: repo, number, cursor})
        const page = result.repository.pullRequest.reviewThreads
        threads.push(...page.nodes)
        if (!page.pageInfo.hasNextPage) return threads
        cursor = page.pageInfo.endCursor
      }
    },
    /** Latest check run per name and latest status per context of the commit, as `gh pr checks` shows them. */
    async checks(sha) {
      const runs = await github.paginate(github.rest.checks.listForRef, {...base, ref: sha, filter: 'latest', per_page: 100})
      const statuses = await github.paginate(github.rest.repos.listCommitStatusesForRef, {...base, ref: sha, per_page: 100})
      const checks = new Map()
      for (const run of latest(runs, r => r.name)) checks.set(run.name, bucketOf(run))
      for (const status of latest(statuses.filter(s => !s.context.startsWith('review-gate')), s => s.context)) {
        checks.set(status.context, bucketOf(status))
      }
      return checks
    },
    async required(branch) {
      const {data} = await github.rest.repos.getBranch({...base, branch})
      if (data.protected && typeof data.protection !== 'object') {
        throw new Error('The protected branch protection summary is unavailable.')
      }
      const rule = data.protection?.required_status_checks ?? {}
      const names = new Set([...(rule.contexts ?? []), ...(rule.checks ?? []).map(c => c.context)])
      const rules = await github.paginate(github.rest.repos.getBranchRules, {...base, branch, per_page: 100})
      for (const entry of rules) {
        if (entry.type === 'required_status_checks') {
          for (const check of entry.parameters.required_status_checks) names.add(check.context)
        }
      }
      return names.size ? names : new Set(FALLBACK_REQUIRED_CHECKS)
    },
    async gateStatus(sha, number) {
      const context = statusContext(number)
      const statuses = await github.paginate(github.rest.repos.listCommitStatusesForRef, {...base, ref: sha, per_page: 100})
      const mine = latest(statuses.filter(s => s.context === context), () => context)
      return mine[0]?.description ?? null
    },
    async setStatus(sha, number, state, description) {
      await github.rest.repos.createCommitStatus({...base, sha, state, context: statusContext(number), description})
    },
    async relabel(number, present) {
      // Removing first makes CodeRabbit see a fresh `labeled` event for the current commit. Any other removal
      // failure must stop here: adding a label that is still present produces no event, and the request would be
      // recorded without CodeRabbit ever seeing it.
      if (present) {
        try {
          await github.rest.issues.removeLabel({...base, issue_number: number, name: REVIEW_LABEL})
        } catch (error) {
          if (error.status !== 404) throw error // only "already removed" is harmless
        }
      }
      await github.rest.issues.addLabels({...base, issue_number: number, labels: [REVIEW_LABEL]})
    },
    async facts(number) {
      const pull = await client.pull(number)
      const head = pull.head.sha
      const comments = await github.paginate(github.rest.issues.listComments, {...base, issue_number: number, per_page: 100})
      return {
        head, state: pull.state, draft: pull.draft, author: pull.user?.login ?? '',
        labels: new Set(pull.labels.map(label => label.name)),
        reviews: await github.paginate(github.rest.pulls.listReviews, {...base, pull_number: number, per_page: 100}),
        threads: await client.threads(number),
        checks: await client.checks(head),
        required: await client.required(pull.base.ref),
        summaryHead: greptileSummaryHead(comments, repository),
        coderabbitSummary: coderabbitSummary(comments),
        gateStatus: await client.gateStatus(head, number),
      }
    },
  }
  return client
}

// ---- reconcile ----------------------------------------------------------------------------------------------

export async function reconcile(client, number, {write, automatic}) {
  const facts = await client.facts(number)
  const decision = evaluate(facts, {automatic})
  if (decision.action === 'request') {
    // A push while the facts were read invalidates them: never label a commit that was not checked.
    if ((await client.pull(number)).head.sha !== facts.head) {
      return {action: 'wait', reasons: ['The pull request changed while it was checked; the next run rechecks it.']}
    }
    if (write) await client.relabel(number, facts.labels.has(REVIEW_LABEL))
  }
  // An exempt PR gets a status only to clear an older waiting one; it never needs a new one.
  if (write && decision.action !== 'skip' && (decision.action !== 'exempt' || facts.gateStatus != null)) {
    const [state, description] = statusFor(decision)
    if (facts.gateStatus !== description) await client.setStatus(facts.head, number, state, description)
  }
  return decision
}

export function report(number, decision, {write}) {
  const verb = {
    request: write ? 'CodeRabbit requested' : 'ready for CodeRabbit', done: 'nothing to do',
    exempt: 'not gated', skip: 'skipped', wait: 'waiting',
  }[decision.action]
  return [`PR #${number}: ${verb}`, ...decision.reasons.map(reason => `  - ${reason}`)].join('\n')
}

/**
 * Entry point for actions/github-script. `pr` empty = every open, ready PR on the gated bases (automatic: never
 * repeats a CodeRabbit request); a number = that PR (manual: may repeat it). `write` applies label and status.
 */
export async function run({github, context, core, pr = '', write = false, client = null}) {
  const text = String(pr ?? '').trim()
  if (text && !/^[1-9][0-9]*$/.test(text)) {
    core.setFailed('Use a positive pull request number without #')
    return
  }
  client ??= createClient(github, context.repo.owner, context.repo.repo)
  const numbers = text
    ? [Number(text)]
    : [...new Set((await Promise.all(BASES.map(branch => client.openPulls(branch)))).flat())].sort((a, b) => a - b)
  const lines = []
  const errors = []
  for (const number of numbers) {
    try {
      lines.push(report(number, await reconcile(client, number, {write, automatic: !text}), {write}))
    } catch (error) {
      errors.push(`PR #${number}: ${error.message}`)
    }
  }
  if (!numbers.length) lines.push(`No open pull requests on ${BASES.join(', ')}`)
  core.info(lines.join('\n'))
  await core.summary.addHeading('Review gate', 3).addCodeBlock([...lines, ...errors].join('\n')).write()
  if (errors.length) core.setFailed(errors.join('\n'))
}

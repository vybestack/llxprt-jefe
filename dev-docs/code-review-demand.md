# CodeRabbit review-demand policy

Jefe uses the root [`.coderabbit.yaml`](../.coderabbit.yaml) as its authoritative
repository-level CodeRabbit configuration. Organization or workspace global
overrides can take precedence. On a pull request, use
`@coderabbitai configuration` to inspect the resolved values and the source of
each value rather than assuming the root file won.

This policy controls when Jefe asks for a review and how it measures the
result. It does not alter the vendor's organization-level allowance, guarantee
zero throttling, or replace the separately owned cross-repository measurement
ledger.

## Deliberate ready-for-review lifecycle

1. Keep the pull request in draft while implementation, refactoring, or known
   remediation is still in progress. If draft state is unavailable, keep
   `[WIP]`, `DO NOT MERGE`, or `[skip review]` in the title, or apply the `wip`
   or `do-not-review` label.
2. Complete the issue's acceptance evidence and run the repository's required
   exact-head local gate. For Jefe this is normally `make ci-check`.
3. Push the verified commit and confirm that the pull request's current head SHA
   is the commit that passed the gate.
4. Remove every WIP title marker or exclusion label, mark the pull request ready,
   and add the `review-ready` label. Add that label only after the implementation
   and required local evidence are ready for external review. The positive label
   is the explicit vendor-documented trigger for the first automatic review.
5. Treat the automatic review as coverage of the head CodeRabbit reports, not
   automatically as coverage of every later push. After remediation, rerun the
   required local gate, push the final commit, and compare the current head SHA
   with the most recent successfully reviewed head SHA.

Automatic review is disabled until the positive `review-ready` label opts the
pull request in. After opt-in, automatic incremental review remains enabled but
pauses after two reviewed commits. Two is the vendor-documented early-pause
range for active branches and preserves follow-up review without reviewing
every small push. Drafts, title markers, and negative labels override the ready
label so active development does not consume review allowance.

Do not infer coverage from the absence of a throttle message. A review is
covered only when a successful completion identifies the relevant reviewed head
SHA.

## Manual review requests and allowance cost

Both manual commands cost one PR review from the allowance when the review runs:

- `@coderabbitai review` requests an incremental review of commits added since
  the last review. Use it after the automatic pause when uncovered commits are
  ready for a focused pass.
- `@coderabbitai full review` requests a complete review from scratch. Reserve
  it for a broad architectural rewrite, a large rebase or conflict resolution,
  or a case where the prior review boundary is uncertain. It is not a routine
  retry command.

Do not request a review when the reviewed head already equals the current head
SHA. Do not send repeated commands after a throttle response. Preserve the
response as evidence, finish any other required review gates, and issue one
deliberate request only when uncovered work remains and allowance is available.
A manual request supplements the two-commit automatic cap; it does not prove
that the request completed or that the requested head was reviewed.

## Immutable measurement events

The publication repository owns the cross-repository measurement ledger and it
is not available in this repository. When that ledger is available to Jefe
automation, append the events below. Until then, retain GitHub comments, check
runs, reviews, and throttle responses as source evidence; do not create a
mutable local counter as a substitute.

All ledger entries are append-only. Every entry includes a schema version,
unique event ID, repository and pull-request identity, UTC observation time,
the source URL or source event identity, and the effective resolved
configuration fingerprint or the literal value `unknown`. Events that observe
review eligibility also include an eligibility snapshot and reason, such as
ready, draft, title-excluded, label-excluded, or otherwise ineligible.
Corrections append a replacement or invalidation event that references the
prior event; they never rewrite it. Request and outcome events remain separate
so a request cannot be mutated into a successful completion.

### `review_requested`

Append for every automatic or manual request, including a request later
throttled or failed. Include:

- a unique request ID;
- automatic, manual incremental, or manual full trigger;
- requester or automation identity;
- requested head SHA and base SHA;
- request timestamp; and
- the ready-state eligibility snapshot that authorized the request.

### `review_completed`

Append only when the vendor reports successful completion. Include the request
ID, completion timestamp, reviewed head SHA, review identity or URL, and review
kind. Never copy the requested head SHA into the reviewed head field without a
completion artifact that identifies that head.

### `review_throttled`

Append when the request or review receives an explicit allowance/rate-limit
response. Include the request ID, requested head SHA, response timestamp,
vendor response classification, and source URL or immutable message identity.
A throttle event is not a completion event.

### `review_coverage_observed`

Append at ready-for-review, after each completion or throttle response, after a
push, and at the terminal PR state. Include the current head SHA, the most
recent successfully reviewed head SHA (or null), the completion event ID used
as evidence (or null), an exact-head-covered boolean, and the eligibility
snapshot at observation. For cohorting, the terminal state is `merged` or
`closed`; include that state and its terminal timestamp.

Create one terminal denominator record per PR whose lifecycle contains at least
one qualifying ready/opt-in observation. Once qualified, the PR remains in the
cohort even if `review-ready` is later removed, the PR returns to draft, or an
exclusion marker is added. Multiple ready/unready cycles still produce one
terminal denominator record; immutable observations preserve each transition.
This captures stale coverage without inflating the metric by dropping previously
eligible PRs or assuming coverage from a missing throttle message.

Collectors must be idempotent on source identity. Duplicate ingestion may point
to the same source event but must not double-count a request, completion, or
throttle.

## Complete rolling-window evaluation

Define the Monday 00:00 UTC measurement cutoff `T` and publish at publication
time `P = T + 7d`, after the seven-day outcome-settling period. The report's
ingestion as-of boundary is `P`. Compare the adjacent complete current window
`[T-28d, T)` with the prior window `[T-56d, T-28d)`. Never tune from a partial
window. Requests join a window by request timestamp; their linked completion or
throttle outcomes may arrive through `P`. Terminal coverage observations join
by terminal timestamp. Events arriving after `P` remain append-only and produce
a referenced correction in the next publication.

Evaluate throttle rate and exact-head review coverage together:

- **Throttle rate:** requests in the window with a linked `review_throttled`
  event divided by all distinct `review_requested` requests in the window.
- **Exact-head review coverage:** eligible ready PR terminal observations whose
  current head SHA equals a successfully reviewed head SHA, divided by all
  eligible ready PR terminal observations in the window.

Also report automatic, manual incremental, and manual full requests separately
so manual retries cannot hide demand. Publish event counts and missing-evidence
counts with each ratio. Report a zero denominator as `not applicable`, never as
zero percent. Late-arriving events remain append-only and are identified as a
correction in the next publication rather than rewriting a prior result.

Group results by the effective resolved configuration fingerprint. A window
with mixed or `unknown` fingerprints is non-comparable for tuning and must be
published as such. A lower throttle rate does not justify a change if exact-head
coverage fell, and a higher coverage rate does not justify unbounded requests.

Do not change the two-commit cap, ready-label behavior, or WIP controls inside a
measured window. A future tuning change must cite both adjacent complete windows
for both metrics, record the configuration commit and resolved fingerprint, and
take effect after the next cutoff. Missing throttle messages never count as
successful reviews.

## Vendor references

- [CodeRabbit configuration schema](https://coderabbit.ai/integrations/schema.v2.json)
- [Automatic review controls](https://docs.coderabbit.ai/configuration/auto-review)
- [Code review commands](https://docs.coderabbit.ai/reference/review-commands)

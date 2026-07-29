# Issue #500 — Bound automatic post-open OCR reviews

## Problem and root cause

`.github/workflows/ocr-review.yml` admits every `pull_request_target` event for
`opened`, `reopened`, `synchronize`, and `ready_for_review`. Its per-PR
concurrency cancels obsolete runs but does not count completed automatic OCR
runs. PR #498 therefore received four completed automatic OCR reviews, exceeding
the project's post-open limit of two.

The current workflow already has explicit manual review paths (`/ocr`,
`/open-code-review`, and `workflow_dispatch`). Those paths are authorized and
must remain available after automatic reviews are exhausted.

## Decision (accepted approach)

Add a budget decision as the second step of the existing serialized
`code-review` job, immediately after PR context resolution and before checkout,
installation, provider preflight, or OCR execution. Keeping the decision inside
the existing per-PR concurrency boundary prevents two workflow runs from reading
the same count and both spending the final slot.

Automatic review state is persisted in one dedicated bot comment marked
`<!-- jefe-ocr-budget -->`. This records a hidden count for all automatic runs
after the change, including runs with no inline findings. For migration of PRs
that predate the state marker, the gate also queries both the PR review list and
review comments, counting unique completed review commits that own
`<!-- jefe-ocr-inline -->` comments from the exact GitHub Actions bot identity.
Once a state comment exists, its maximum valid persisted count is authoritative;
this prevents later manual reviews from inflating the automatic count.

`OCR_MAX_REVIEWS_POST_OPEN` configures the limit and defaults to `2` only when
unset. Zero disables automatic OCR. A malformed or negative value fails the
budget step before OCR instead of silently applying a different policy.

Every later step in `code-review` is conditioned on the gate's `should_run`
output. After setup, connectivity preflight, and scope validation succeed, an
automatic-only reservation step persists exactly one additional count before
invoking OCR. The invocation requires that reservation output, so cancellation
or an API failure cannot spend an unrecorded provider attempt. Setup,
configuration, connectivity-preflight, and scope-policy failures that prevent
reservation do not consume budget. Manual triggers bypass reservation and never
increment or reset automatic state. If no state comment exists, a manual gate
initializes the historical migration baseline before the manual review posts.

When an automatic trigger is over budget, the gate creates or updates the one
budget comment with: `OCR skipped: post-open review budget (N) reached for this
PR.` The comment also points to `/ocr` and `/open-code-review` for an explicit
on-demand review. Repeated pushes update the same comment rather than producing
comment spam.

The llxprt-code implementation for issue 2666 was inspected. Its persistent
comment counter, configurable default of two, manual-trigger bypass, and sticky
suspension notice inform this bounded design. Its checkbox reset, `/review`
alias, and severity-routing work from issue 2672 are not copied because they
expand behavior beyond issue #500 and conflict with this issue's posting-logic
non-goal.

## Acceptance matrix

| ID | Actor / launch path | Inputs and boundary cases | Target | Observable success | Observable failure and diagnostics | Side effects before failure | Persistence / compatibility | Behavioral evidence |
| --- | --- | --- | --- | --- | --- | --- | --- | --- |
| A1 | GitHub Actions receives an automatic `pull_request_target` event (`opened`, `reopened`, `synchronize`, or `ready_for_review`) | Effective automatic count is below the configured limit | Hosted Linux workflow runner; same-repo or fork PR under the existing trusted-base model | Gate permits setup; after preflight/scope checks, the reservation persists one slot and OCR runs only after that write succeeds | A GitHub API failure or invalid PR context fails before checkout/install/OCR; setup/preflight/scope failures before reservation do not consume budget and retain existing diagnostics | Authenticated budget-comment reads always; legacy review/comment reads only while state is absent; one reservation write immediately before OCR | Existing PRs without state seed from marker-backed completed reviews; new runs persist count across workflow runs | Contract test scopes the gate before install, verifies migration and downstream conditions, and requires reservation before OCR invocation |
| A2 | Same automatic paths | Effective count equals or exceeds the limit; includes limit `0` | Same | Checkout, install, LLM preflight, OCR, posting, manifests, artifact upload, and classification do not run; one sticky notice says the post-open budget was reached | Failure to read or write budget state fails closed before OCR and appears in the budget step log | May create/update only the dedicated budget notice; no OCR/tool/provider side effects | Repeated over-budget pushes update the same marked comment | Contract test proves `should_run=false` at `count >= limit`, skip text/marker, and every later step is gated |
| A3 | Repository administrator configures the budget | `OCR_MAX_REVIEWS_POST_OPEN` unset, `0`, positive integer, malformed text, or negative integer | Repository Actions variables | Unset resolves to 2; nonnegative integer is honored without workflow edits | Malformed/negative configuration calls `core.setFailed` and no OCR steps run | No provider or PR-review posting side effect before failure | Variable changes apply on the next trigger; no migration rewrite is required | Contract test checks variable/default wiring and strict validation signals |
| A4 | Authorized collaborator requests `/ocr` or `/open-code-review`, or launches `workflow_dispatch` | Automatic count below, at, or above cap | Existing manual workflow paths | Review runs regardless of automatic count | Existing authorization, PR resolution, and infrastructure diagnostics are unchanged | Existing manual-review side effects; if state is absent, one baseline state comment is created before the review | Manual review never increments or resets automatic count; baseline initialization prevents a first manual run from entering later migration history | Contract test checks manual baseline initialization, explicit bypass, and automatic-only count-record condition |
| A5 | Existing PR contains prior OCR inline reviews but no new budget-state comment | Zero, one, two, or more marker-backed bot review objects; fallback comments may create multiple review objects for one commit | GitHub REST pagination | Unique completed OCR commit IDs seed the effective count; duplicate review objects on one commit count once | Review-list/comment-list API failures fail closed before OCR | Read-only API calls before decision | Once state exists it is authoritative. Historical trigger provenance is unavailable, so migration can include old manual marker-backed reviews and cannot count completed runs that left no inline marker (clean, lineless-only, failed-publication, or fully deduplicated runs). | Contract plus exact-script fixtures check paginated review APIs, exact bot identity, inline marker filtering, unique commit IDs, and authoritative persisted semantics |
| A6 | Existing fork-safe workflow executes either allowed or skipped path | Fork PR head contains workflow/config changes | `pull_request_target` trusted base | Existing trusted-base checkout, non-execution of PR code, trigger types, permissions, and per-PR cancellation remain intact | Existing fork-safety diagnostics remain unchanged | No new execution of PR-supplied files | Current marker and posting behavior remain compatible | Existing preservation contract tests plus focused regression assertions |

## Explicit non-goals

- No pre-open local OCR accounting. Local pre-PR reviews do not exist on a PR and
  remain governed by the delivery process; this workflow enforces the post-open
  phase only.
- No single total cap of four; the accepted implementation enforces the stronger
  explicit post-open default of two.
- No throttling or counting of `/ocr`, `/open-code-review`, or
  `workflow_dispatch` manual reviews.
- No new `/review` alias, checkbox reset, resume command, label, or automatic
  budget reset. The existing manual commands already provide an explicit
  trigger after suspension.
- No severity/category routing for trivial, nit, style, test, or
  maintainability findings. That is a separate posting-policy feature (as in
  llxprt-code issue 2672) and directly conflicts with issue #500's “no changes
  to posting logic” scope.
- No changes to trigger types, fork-safety, OCR tooling/version/provider,
  finding posting/deduplication, manifest/redaction, or artifact behavior.
  Intentional budget skips suppress the infrastructure notifier; genuine budget
  gate and OCR infrastructure failures retain existing notification behavior.
- No new production module, script, dependency, Cargo manifest/lockfile change,
  reusable workflow, public abstraction, lint suppression, quality threshold,
  `.llxprt/`, or `.code_puppy/` change.

## Bounded vertical slices

### Slice 1 — Automatic budget decision and observable skip (A1–A6)

- **Owner / boundary:** GitHub workflow orchestration only; the existing
  `code-review` per-PR concurrency boundary remains the serialization owner.
- **Allowed files:** `.github/workflows/ocr-review.yml`,
  `tests/core/ocr_workflow_contracts.rs`, and this plan.
- **RED:** add focused workflow contract tests for decision placement,
  historical count sources, strict variable handling, manual bypass, automatic
  state persistence, skip notice, and downstream gating. Prove they fail against
  the current workflow because no budget step/state exists.
- **GREEN:** add the budget step, conditions, and automatic pre-invocation
  reservation step; preserve all unrelated workflow behavior.
- **Refactor:** only within the two accepted workflow script blocks when needed
  for clarity; do not extract a new script/module.
- **Verification:** focused `cargo test --test integration ocr_budget`, then
  `cargo xtask quick`, then exact-head `cargo xtask ci`.
- **Stop for approval:** any need for a new workflow/script/dependency, a trigger
  or permission change, a posting/routing change, a reset subsystem, an unlisted
  path, or behavior outside A1–A6.

## Expected paths and scope budget

| File | Layer / reason | Expected magnitude |
| --- | --- | --- |
| `.github/workflows/ocr-review.yml` | Accepted workflow gate, state comment, and step conditions for A1–A6 | about 180–260 changed lines |
| `tests/core/ocr_workflow_contracts.rs` | RED/GREEN structural workflow behavior contracts | about 100–160 added lines |
| `project-plans/issue500-plan.md` | Required issue-delivery acceptance and scope record | planning evidence |

Target: 3 files and well below 1,500 net changed lines. No mandatory scope-review
trigger is expected. Stop without approval before 40 files or 2,500 net lines;
perform a mandatory scope review above 25 files or 1,500 net lines.

## Scope ledger

| Item / file | Disposition | Acceptance mapping / rationale |
| --- | --- | --- |
| `.github/workflows/ocr-review.yml` | Accepted (explicitly authorized by issue request) | A1–A6; issue's requested workflow fix |
| `tests/core/ocr_workflow_contracts.rs` | Accepted | Behavioral contract evidence for A1–A6 |
| `project-plans/issue500-plan.md` | Accepted | Canonical delivery-policy requirement |
| Dedicated marked budget-state comment | Accepted | Durable no-finding count and non-spamming skip observability |
| Historical marker-backed review migration | Accepted with compatibility limit | A5 and issue's requested review-list query; pre-state trigger provenance and markerless runs cannot be reconstructed |
| Intentional-skip notifier suppression | Accepted | A2; budget exhaustion is a successful no-op, while actual gate/OCR failures remain diagnosable |
| `/review` alias and checkbox reset | Defer | Existing manual commands satisfy override; separate interaction scope |
| Severity routing for trivial/nit findings | Defer | Separate posting-policy feature; conflicts with explicit non-goal |
| Pre-open phase reconstruction | Reject for this workflow | No pre-open PR review objects exist to classify |

No newly discovered work. Every changed file must be added to this ledger before
editing.

## Review counters and triage policy

- Local Open Code Review before PR: 1 / 2 used (successful detached review;
  2 files, 3 findings).
- Pull-request Open Code Review after PR: 0 / 2 used.
- Independent review/remediation cycles: 2 / 2 used. The second cycle included
  DeepThinker and RustReviewer; no additional independent review cycle is
  permitted for this effort.

Every finding is recorded as **Blocker-Fix**, **In-scope-Fix**, **Reject**, or
**Defer**. Reviewer output does not authorize a new path, subsystem, public
abstraction, workflow behavior, dependency, or posting policy.

### Review finding triage

| Finding | Disposition | Resolution |
| --- | --- | --- |
| Automatic slot was recorded after OCR, so cancellation or a failed state write could overspend the cap | Blocker-Fix | Replaced the post-result recorder with an awaited reservation immediately before automatic OCR invocation |
| Setup/preflight failure could reserve a slot although OCR could not run | Blocker-Fix | Reservation now returns `reserved=false` before any state write when `ocr-exit-code.txt` records a prior failure |
| Persisted state still queried legacy review APIs | In-scope-Fix | Valid authenticated persisted state is authoritative and bypasses migration APIs |
| Limit parsing accepted whitespace or unsafe integers | In-scope-Fix | Exact decimal nonnegative safe-integer parsing now fails closed on malformed values |
| Duplicate, malformed, spoofed, or unsafe persisted state was ambiguous | Blocker-Fix | State requires exactly one exact GitHub Actions bot comment and one safe count marker; controlled corruption fails fast |
| Raw workflow-dispatch PR spellings could split the per-PR concurrency group | Blocker-Fix | PR resolution rejects noncanonical dispatch spellings before any budget-state access |
| Over-budget migration notice omitted manual commands | In-scope-Fix | Every exhausted notice now names `/ocr` and `/open-code-review` |
| Budget-gate failures should be hidden from the infrastructure notifier | Reject | Genuine gate failures must remain observable; only intentional `review_skipped=true` no-ops suppress notification |
| Legacy migration cannot count markerless completed runs | Defer | Historical data is unavailable; A5 now documents clean, lineless-only, failed-publication, and fully deduplicated undercount |
| Legacy migration cannot distinguish pre-state manual from automatic marker-backed reviews | Defer | Historical trigger provenance is unavailable without a new Actions-query/provenance subsystem; A5 documents the compatibility limit and all future manual runs remain excluded |
| Permanent Rust tests do not execute the embedded JavaScript | Reject / bounded evidence | Adding a Node-backed Cargo test subsystem/runtime dependency was outside the approved paths; an exact-script temporary harness executed 13 gate and 4 reservation scenarios, while committed contracts prevent wiring drift |
| Severity routing, `/review` alias, and checkbox reset should accompany the cap | Defer | Separate posting/interaction behavior explicitly excluded from issue #500 |

## Verification evidence

Candidate working tree after the final review remediation:

- RED captured before production changes: 5 of 6 issue #500 contracts failed.
- GREEN: `cargo test --test integration ocr_budget -- --nocapture` — 6 passed.
- Exact embedded-script harness — 13 gate scenarios and 4 reservation scenarios
  passed, covering strict limits, cap boundaries, persisted-state authority,
  migration commit deduplication, manual baseline/bypass, identity spoofing,
  duplicate/malformed state, setup failure, reservation success, and state drift.
- `actionlint .github/workflows/ocr-review.yml` — passed.
- YAML parse and `git diff --check` — passed.
- `RUST_TEST_THREADS=1 cargo xtask quick` — passed.
- Final `RUST_TEST_THREADS=1 cargo xtask ci` — passed: format, allow policy,
  source size, architecture, strict and complexity Clippy, 71.62% line coverage,
  locked all-feature build, full test suite, and doctests.
- OCR preview reviewed `.github/workflows/ocr-review.yml` and
  `tests/core/ocr_workflow_contracts.rs`; Markdown plan exclusion was the OCR
  tool's unsupported-extension behavior.
- Local OCR completed successfully (2 files, 3 findings), and all findings were
  triaged above.
- Two independent review cycles completed; all in-scope correctness findings
  were remediated or explicitly dispositioned above.
- Scope: 3 files, below the 25-file / 1,500-net-line target; no unapproved path,
  permission, trigger, dependency, quality-tool, or posting-policy expansion.
- PR CI, ancestry, and conflict checks remain pending until the candidate is
  committed, pushed, and opened as a PR.

## Completion and stopping rules

Stop for approval if implementation requires any unplanned workflow route,
permission, file, subsystem, abstraction, dependency, quality-tool change,
posting policy, or hard-budget breach. Stop successfully when A1–A6 are proven,
all exact-head gates pass, required reviews are triaged, the PR is conflict-free,
and this scope ledger is clean. Do not continue optional hardening or cleanup.

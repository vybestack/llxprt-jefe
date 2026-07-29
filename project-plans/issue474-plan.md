# Issue 474 delivery plan

## Problem statement

The `LLxprt PR Review` workflow (`.github/workflows/pr-review.yml`) checks out
`github.event.pull_request.base.sha` and then runs `node scripts/ci-quota-check.mjs`
(workflow line ~210) and `node scripts/pr-review-walkthrough.mjs`
(workflow line ~396) against that working tree. Both scripts (and their three
helpers) were introduced by commit `8d0c45b` (PR #466). For any PR whose
recorded `pull_request.base.sha` is a strict ancestor of `8d0c45b`, the
checked-out tree lacks those scripts and the job fails with
`Cannot find module .../scripts/ci-quota-check.mjs` (exit 1).

This is a CI/workflow infrastructure defect, not a product bug. The same PR
fails identically across reruns until `base.sha` advances past `8d0c45b`.

## Root cause (verified)

- All `git diff` operations in the workflow use explicit SHAs
  (`${MERGE_BASE}` and `${PR_HEAD_SHA}`), never the working tree. The
  checked-out ref's sole purpose is to provide the workflow-supporting scripts
  (`scripts/ci-quota-*.mjs`, `scripts/pr-review-*.mjs`).
- `base.sha` is also read into `base_sha` (line ~108) and consumed only by
  `git merge-base` (line ~117) for diff scoping. That computation is correct
  and must be preserved exactly; it does not depend on the checked-out ref.
- Therefore checking out the base-branch **tip** (guaranteed to contain the
  scripts) instead of the possibly-stale `base.sha` fixes the failure without
  altering diff semantics, merge-base accuracy, or fork safety.

## Acceptance matrix

| # | Actor / path | Input / boundary | Observable success | Observable failure / diagnostic | Behavioral test |
|---|---|---|---|---|---|
| A1 | `LLxprt PR Review` job, checkout step | `pull_request.base.sha` older than workflow/script introduction commit `8d0c45b` | Checkout ref resolves to the base branch tip (or a ref guaranteed to contain the scripts), not the stale `base.sha` | (pre-fix) `Cannot find module .../scripts/ci-quota-check.mjs`, exit 1 | Structural assertion: checkout step does not use `github.event.pull_request.base.sha` as its `ref` |
| A2 | `LLxprt PR Review` job, script steps | Workflow references `scripts/ci-quota-check.mjs` and `scripts/pr-review-walkthrough.mjs` | The checked-out ref contains every workflow-referenced script at the base-branch tip | (pre-fix) module resolution failure | Structural assertion: checkout ref targets the base branch / tip; all five `scripts/*.mjs` referenced or imported exist in the workflow's expected tree |
| A3 | `LLxprt PR Review` job, diff steps | Any PR | Diff scoping unchanged: `git merge-base` still uses the event's `base.sha`; diffs still use `${MERGE_BASE}`..`${PR_HEAD_SHA}` | (regression) diffs would change | Structural assertion: `base_sha` is still read from `github.event.pull_request.base.sha` and still feeds `git merge-base` |
| A4 | Regression guard | PR whose `base.sha` predates `8d0c45b` | Workflow proceeds past the quota step | (pre-fix) hard failure | `tests/core/pr_review_workflow_contracts.rs` proves the fix structurally |

## Non-goals

- Changes to product code, dependencies, or `ci-quota-check.mjs` script logic.
- Changes to required product CI (`ci.yml`) — it passed for PR #444 and is unaffected.
- Hardening of `release.yml` (tracked separately in #471).
- OCR reproducibility-manifest completeness (#464).
- OCR config/secrets failures (#158).
- Changing `pull_request_target` semantics, fork-safety model, or the
  mergeability gate. The fix must preserve `pull_request_target` and all
  existing fork-safety and concurrency controls.

## Planned vertical slices

### Slice 1 (only slice): checkout strategy fix + regression test

- **Acceptance rows:** A1, A2, A3, A4
- **Architecture owner:** CI/review automation (`.github/workflows/pr-review.yml`).
  Integration boundary: the workflow's checkout `ref` and the event context
  consumed by `git merge-base`.
- **Allowed files:**
  - `.github/workflows/pr-review.yml` (checkout step only)
  - `tests/core/pr_review_workflow_contracts.rs` (new)
  - `tests/core/mod.rs` (module wire-up line only)
- **RED:** new contract test fails on current main because the checkout step
  still uses `github.event.pull_request.base.sha` as its `ref`.
- **GREEN completion criteria:** contract test passes; checkout step resolves
  scripts from the base-branch tip; `base.sha` still feeds `git merge-base`
  unchanged.
- **Non-goals for this slice:** everything in the issue-level non-goals above.
- **Verification commands:**
  - `cargo test --test integration pr_review_workflow_contracts`
  - `make quick-check`
  - `make ci-check` (fmt, clippy, build, coverage, test)
- **Stopping conditions:** any requirement to touch `ci.yml`, `ci-quota-check.mjs`
  logic, fork-safety model, or product code requires user approval.

## Scope ledger

- `.github/workflows/pr-review.yml` — checkout step `ref` change (in scope, A1/A2).
- `tests/core/pr_review_workflow_contracts.rs` — new regression test (in scope, A4).
- `tests/core/mod.rs` — one module declaration line (in scope, test wire-up).

No newly discovered out-of-scope work. No dependency, agent-memory, or
quality-tool changes.

## Review counters

- Open Code Review (local, before PR): 0 / 2
- Open Code Review (after PR): 0 / 2
- CodeRabbit: pending PR

# Issue #439 — CI does not run on main, leaving no post-merge signal or flake baseline

## Problem

The `CI` workflow (`.github/workflows/ci.yml`) only triggers on
`pull_request` against `main` and on `workflow_dispatch`. After a PR merges,
nothing re-runs the same required jobs on the resulting `main` commit.

Consequences:

1. **No post-merge signal** — a merge-only defect or platform-specific
   failure is not detected after merge. `main` is only inferred green.
2. **No flake baseline** — when a platform job (e.g.
   `Native Windows (MSVC + psmux)`) fails on a PR, there is no comparable
   `main` run to attribute the failure as pre-existing.
3. **Merge-commit gaps** — CI tests the PR merge result, but the actual
   first-parent commit on `main` is never re-verified by execution.

## Decision (accepted approach)

Add a `push` trigger scoped to `main` to the `CI` workflow so every commit
landing on `main` runs the same required jobs as PRs. This is the exact
suggested fix in the issue.

**Scope boundary from issue thread (acoliver):** "good with running ci on main
-- but not ocr that's to noisy and intensive."

Therefore OCR is explicitly excluded:

- The `OCR Review` workflow (`ocr-review.yml`) is **not** given a `push`
  trigger. It remains PR-only.
- No new required-status gate or branch-protection change is introduced by
  this PR (that is a repository-settings concern, not a workflow-file
  change, and is recorded as a non-goal).

This change does not alter job definitions, steps, env, permissions, or
concurrency for the `CI` workflow; it only extends the events that start it.

## Non-goals

- Adding OCR on `push` to `main` (explicitly rejected by maintainer).
- Changing `release.yml` (tag-driven; already works).
- Changing required status checks / branch protection rules (repo settings,
  not a workflow file change).
- Changing CI job contents, dependencies, runner OS, cache keys, or step
  ordering.
- Adding new jobs or workflows.
- Anything touching `make ci-check`, coverage thresholds, lint, or
  complexity policy.
- Modifying `.llxprt/`, `.code_puppy/`, or unrelated tests/docs.

## Acceptance matrix

| # | Behavior | Evidence |
|---|----------|----------|
| AC1 | The `CI` workflow runs on every push to `main`. | `ci.yml` `on:` includes `push: branches: [main]`. |
| AC2 | The `CI` workflow still runs on PRs against `main` and via `workflow_dispatch` (no regression of existing triggers). | `on:` retains `pull_request: branches: [main]` and `workflow_dispatch`. |
| AC3 | OCR (`ocr-review.yml`) does **not** run on push to `main` (stays PR-only). | `ocr-review.yml` `on:` is unchanged (no `push` key added). |
| AC4 | The change is scoped to the CI trigger; job definitions, steps, env, permissions, and concurrency are unchanged. | Diff inspection + `actionlint` validation. |
| AC5 | Workflow YAML is valid and parses under `actionlint`. | `actionlint` returns clean. |
| AC6 | No production source, tests, dependencies, or quality-gate config are modified. | `git diff` contains only `.github/workflows/ci.yml` (+ this plan). |
| AC7 | The `tui_smoke` optional job remains gated to `workflow_dispatch` only and is not spuriously enabled on push. | The existing `if:` guard on `tui_smoke` keys on `github.event_name == 'workflow_dispatch'`, so it stays skipped on push. Documented, not changed. |

## Vertical slices

1. **Trigger extension** — add `push: branches: [main]` to `ci.yml`'s `on:`
   block. Single file, single coherent change. Confirms AC1, AC2, AC4.

This is a single-slice change (one workflow trigger edit). It does not cross
multiple architectural ownership layers and requires no new modules.

## Expected paths / files

- `.github/workflows/ci.yml` — add `push` trigger (the only production change).
- `project-plans/issue439-plan.md` — this plan (documentation only).

## Verification

- `actionlint .github/workflows/ci.yml` (workflow validity)
- `make quick-check` is not meaningfully affected (no Rust change), but the
  YAML edit is validated with actionlint and a diff review.
- Final CI gate is the CI workflow itself running on the PR (and, after
  merge, on `main` — which is the whole point of the issue).

## Scope ledger

| Date | Item | Disposition |
|------|------|-------------|
| 2026-07-26 | Initial scope: add `push: [main]` to CI trigger only. | Accepted |
| 2026-07-26 | OCR on push to main (proposed by issue text). | Rejected — maintainer (acoliver) explicitly excluded in issue thread: "not ocr that's to noisy and intensive". |
| 2026-07-26 | Required status checks / branch protection. | Deferred — repository settings, not a workflow-file change. Separate concern. |

## Review counters (OCR)

- Local OCR runs before PR: 0 / 2
- OCR runs after PR opened: 0 / 2

(Cap: two local + two PR per issue/PR effort. This change is a ~2-line
workflow trigger edit; OCR runs will be spent only if the PR review surfaces
blockers.)

## Verification evidence

- Local: `actionlint .github/workflows/ci.yml` → exit 0, no warnings.
- Local: `actionlint .github/workflows/ocr-review.yml` → exit 0 (unchanged,
  confirms OCR stays PR-only).
- Diff scope: `.github/workflows/ci.yml` (+2 lines) and this plan only.
- PR #443 CI (exact head `789d447`), all required checks SUCCESS:
  - Format (rustfmt) [OK]
  - Lint (clippy) [OK]
  - Clippy allow policy [OK]
  - Source file length checks [OK]
  - Complexity checks [OK]
  - Coverage gate [OK]
  - Build [OK]
  - Test [OK]
  - Native Windows (MSVC + psmux) [OK] (5m22s)
  - Optional TUI smoke (tmux) → SKIPPED (workflow_dispatch-only, by design)
- OCR (PR): Phase `review`, exit 0, **No findings.**
- CodeRabbit: excluded by label configuration (no comments).
- PR state: `mergeable: MERGEABLE`, `mergeStateStatus: CLEAN` (no conflicts).

## Deferred findings / follow-ups

- Branch protection / required status checks for the new `push: [main]` runs
  is a repository-settings concern (not a workflow-file change) and is
  intentionally out of scope for this PR.

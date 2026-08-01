# Issue #514 — PR checks show failed when superseded runs have successful replacements

## Summary

`parse_checks_rollup` aggregates every `statusCheckRollup` node, so a stale
`CANCELLED` CheckRun (mapped to `Failure`) poisons the rollup even when a newer
attempt of the same check succeeded. GitHub's effective rollup (and `gh pr
checks`) reports no failure for that PR.

## Acceptance matrix

| ID | Behavior | Proof |
|----|----------|-------|
| A1 | For multiple `CheckRun` nodes sharing an effective identity, only the latest attempt (greatest `startedAt`, tie-broken by array position) contributes to the aggregate `checks_status`. | `effective_check_nodes` + aggregate unit test |
| A2 | The PR #509 shape (older canceled LLxprt checks, successful replacements, all other effective checks successful) projects to `PrCheckStatus::Success`. | canceled-then-successful rollup fixture |
| A3 | A latest attempt with a genuine failure conclusion still projects to `Failure`. | successful-then-failed fixture |
| A4 | Pending latest attempts continue to project as `Pending`. | pending-latest fixture |
| A5 | The PR detail check list omits superseded attempts so its rows cannot disagree with its aggregate glyph. | `parse_pull_request_detail_json` checks-vec length/status assertion |
| A6 | Independent `StatusContext` entries continue to aggregate correctly. | status-context-only fixture |
| A7 | PR list and PR detail projections use the same effective-check selection logic. | list + detail both route through `parse_checks_rollup`/`effective_check_nodes` |
| A8 | Mergeability parsing and display are unchanged. | no edits to mergeable code; existing mergeable tests untouched |
| A9 | Regression tests cover canceled-then-successful, failed-then-successful, successful-then-failed, and status-context-only rollups. | four fixtures in `parse_pr_tests.rs` |

## Effective identity & ordering

- **Identity**: `__typename` + `name`(`context`) + disambiguator. The
  disambiguator is `workflowName` (the `gh pr view --json statusCheckRollup`
  transport) or `checkSuite.app.slug` (the raw GraphQL transport), falling back
  to empty. This avoids conflating unrelated apps/workflows with identical job
  names while grouping re-runs of the same job.
- **Ordering**: greatest `startedAt` wins (fallback `completedAt`, then array
  position) — the most recently started attempt is the effective one, matching
  GitHub's own latest-run projection.

## Non-goals

- Do not change mergeability semantics or glyphs.
- Do not add a new `PrCheckStatus` variant.
- Do not change `parse_check_status` token mapping (a lone/latest `CANCELLED`
  stays `Failure`; this issue removes *superseded* attempts from the rollup).
- Do not change refresh cadence, pagination, persistence, or cache.
- Do not broadly rewrite GitHub parsing outside the PR check-rollup path.
- Do not redesign the Actions/runs screen.

## Vertical slices

1. **Dedup core** — new `src/github/pr_check_rollup.rs` owns
   `parse_check_status`, `parse_checks_rollup`, `effective_check_nodes`, and the
   identity/ordering helpers (moved from `parse_pr.rs` to keep that file under
   the 1000-line source-size gate while adding new logic).
2. **Wire-up** — `parse_pr.rs` imports the moved/new functions; the detail
   `checks` vec is built from `effective_check_nodes` so list and detail share
   one selection path.
3. **List query fields** — add `startedAt completedAt checkSuite { app { slug } }`
   to the CheckRun fragment in both PR-search GraphQL variants so the list path
   can order and disambiguate attempts.

## Expected files

- `src/github/pr_check_rollup.rs` (NEW, internal module)
- `src/github/parse_pr.rs` (remove 2 fns, import them, update detail + query)
- `src/github/mod.rs` (module decl + public re-export)
- `src/github/parse_pr_tests.rs` (regression tests)

## Scope ledger

| Discovery | Disposition |
|-----------|-------------|
| `parse_pr.rs` is 999/1000 lines; adding dedup logic in place would breach the source-size hard gate | In-scope-Fix: move the cohesive check-rollup concern (`parse_check_status` + `parse_checks_rollup`) into the new internal `pr_check_rollup.rs` module alongside the new dedup logic — the repo's standard source-size extraction pattern, no behavior change beyond the accepted fix |

## Review counters

- OCR (pre-PR): 0 / 2
- OCR (post-PR): 0 / 2

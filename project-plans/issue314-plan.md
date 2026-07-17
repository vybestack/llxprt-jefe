# Issue #314 — PR mergeable/merge-conflict status indicators

**Repository:** vybestack/llxprt-jefe
**Issue:** PR headup should display if the PR is mergable or not (conflicts or
whatever)

## Requirement analysis

The issue requests four display improvements. The current state of the
codebase already covers two of them, so this delivery focuses on the gaps:

| # | Requirement | Current state | Action |
|---|-------------|---------------|--------|
| 1 | PR list shows a workflow-error indicator (x/check) | DONE — `checks_glyph` renders `✓checks`/`✗checks` | none |
| 2 | PR list shows a mergeable/has-conflicts indicator | MISSING — list query + struct lack `mergeable` | **add** |
| 3 | PR detail header shows mergeable status | MISSING — header omits mergeable | **add** |
| 4 | Detail header shows target branch (main or otherwise) | DONE — branches row `head --> base` | none |
| 5 | Detail header shows approvals-needed | PARTIAL — `review_decision` shown only in Reviews content section, not in the fixed header | **add to header** |

## Acceptance matrix

| Row | Layer | Behavior | Test |
|-----|-------|----------|------|
| A1 | parse | GraphQL search node with `mergeable: "MERGEABLE"` → list item `mergeable = Some(true)` | `tests_pr.rs` |
| A2 | parse | `mergeable: "CONFLICTING"` → `Some(false)`; missing/`UNKNOWN` → `None` | `tests_pr.rs` |
| A3 | list-view | meta line emits a merge glyph: `✓merge` / `✗conflict` / `-merge` | `pr_list.rs` |
| A4 | detail-header | state row includes mergeable + checks + review status glyphs | `pr_render_tests.rs` |

## Non-goals

- Changing the number of fixed header rows (keeps `PR_DETAIL_HEADER_ROWS`
  layout math stable).
- Merging the detail `mergeable: Option<bool>` with a new enum — keep the
  existing detail field representations untouched.
- New GraphQL queries beyond adding `mergeable` to the existing search field
  selection.
- Branch-protection rule fetching (only the PR-scoped `mergeable` signal).

## Vertical slices

1. **Domain + parse** — add `mergeable: Option<bool>` to the list `PullRequest`
   struct; add `mergeable` to the GraphQL search queries; parse the GraphQL
   enum into the bool.
2. **List glyph** — add a shared `mergeable_glyph` and render it in the list
   meta line.
3. **Detail header** — surface mergeable + checks-rollup + review-decision
   status in the fixed header state row.

## Verification

`make ci-check` (fmt, clippy, coverage ≥ 30, build, test).

## Scope ledger

- Files touched: domain/mod.rs, github/parse_pr.rs, github/tests_pr.rs,
  pr_detail_content.rs, ui/components/pr_list.rs, ui/components/pr_detail.rs,
  ui/components/pr_render_tests.rs, + test fixtures updated for the new
  `PullRequest` field.

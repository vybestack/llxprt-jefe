# Issue #579 — Filtered issue pagination must not overrun the page or skip issues

> When an issue-type filter has to be applied client-side,
> `fetch_issue_search_filtered_pages` accumulates matches across several raw
> search pages. It breaks once `collected.len() > page_size` without truncating,
> so the caller receives more issues than it asked for, and it returns the end
> cursor of the last raw page fetched even though the trailing matches of that
> page were never emitted. Resuming from that cursor skips every issue between
> the last emitted item and the cursor, leaving holes in the list.

## Acceptance matrix

| # | Actor / launch path | Input / boundary | Targets | Observable success | Observable failure / diagnostic | Side effects before failure | Persistence / compatibility | Proof |
|---|---|---|---|---|---|---|---|---|
| A1 | `GhClient::list_issues` with an issue-type filter that requires client-side narrowing | Matches from several raw pages sum to more than `page_size` | All platforms | The response carries at most `page_size` issues | n/a — no new diagnostic | Only the raw search fetches already performed | `IssueListResponse` shape unchanged | Unit test on the filtered-pagination loop asserting the emitted count |
| A2 | Same path, caller resumes with the returned cursor | The loop stopped because the next raw page would overflow the requested page | All platforms | The returned cursor is the end cursor of the last raw page whose matches were all emitted, so the follow-up request re-reads no emitted issue and skips none | n/a | As above | Cursor stays an opaque server cursor | Unit test asserting the cursor and a two-request continuation test proving the concatenated issues have no gap and no duplicate |
| A3 | Same path | Accumulated matches reach exactly `page_size` | All platforms | Exactly `page_size` issues, cursor is that page's end cursor, `has_more` follows the raw page | n/a | As above | unchanged | Unit test for the exact-fill boundary |
| A4 | Same path | The raw pages run out (`has_more == false`) before `page_size` matches accumulate | All platforms | Every match found is emitted, `has_more` is `false`, cursor is the last raw page's end cursor | n/a | As above | unchanged | Unit test for exhaustion |
| A5 | Same path | The server returns an unchanged end cursor (no forward progress) | All platforms | The loop stops and reports `has_more == false` instead of spinning | n/a | As above | unchanged | Unit test for the non-advancing cursor guard |
| A6 | Same path | No matches at all in a page that still reports `has_more` | All platforms | The loop keeps fetching and does not return early with a stale cursor | n/a | As above | unchanged | Unit test for the skip-empty-page case |
| A7 | `GhClient::list_issues` without a client-side issue-type filter | Any filter/sort | All platforms | Unchanged single raw-page behavior | Unchanged `GhError` mapping | Unchanged | unchanged | Existing `github_client` tests remain green |

## Non-goals

- Changing the GraphQL selection to request per-node cursors (`edges { cursor }`)
  so a page could resume mid-page. The available `pageInfo.endCursor` is
  page-granular, and page-granular resume is enough to satisfy A1/A2.
- Changing the server-side sort, filter, or query construction (issue #573/#578).
- Changing `IssueListResponse`, the reducer's page-append/dedup behavior, or any
  UI surface. This is a client-boundary fix with no rendered-output change, so no
  TUI harness scenario is added.
- Adding retry, caching, or prefetch behavior to the pagination loop.
- Reworking `fetch_issue_search_raw_page` or its error mapping.

## Vertical slices

### Slice 1 — Bounded page and correct resume cursor

- **Rows:** A1–A7.
- **Owner / boundary:** `src/github` GitHub-client boundary; the raw fetch stays
  the only side-effecting call.
- **Allowed paths:** `src/github/issue_pages.rs` (production loop plus its
  in-module unit tests), `project-plans/issue579-plan.md`.
- **RED:** unit tests drive the accumulate-and-stop loop with a scripted page
  sequence and fail today because the emitted page exceeds `page_size` and the
  returned cursor belongs to a page whose matches were not all emitted.
- **GREEN:** a raw page is either consumed whole or deferred whole. When the next
  raw page would push the total past `page_size`, the loop returns what it has
  with the previous page's end cursor and `has_more == true`. A raw page can
  never contribute more than `page_size` matches (it is fetched with
  `first = page_size` and filtering only removes items), so a deferral always
  leaves at least one emitted issue and the caller always makes progress.
- **Non-goals:** as above.
- **Verification:** `cargo test --lib github::issue_pages`, `cargo xtask quick`,
  then the full `cargo xtask ci` gate on the candidate head.
- **Stop for approval:** any need to change the GraphQL query shape, the response
  type, reducer behavior, dependencies, or quality tooling.

## Expected paths / architectural layers

- `src/github/issue_pages.rs` — the client-side filtering loop and its unit tests.
- `project-plans/issue579-plan.md` — this plan.

No new subsystem, public abstraction, dependency, workflow, quality-tool change,
or unrelated refactor is authorized.

## Scope ledger

| Entry | Status | Reason |
|---|---|---|
| Cap the filtered page at `page_size` | In scope | A1 |
| Return the resume cursor of the last fully emitted raw page | In scope | A2 |
| Test seam: the loop takes the raw-page fetch as a parameter | In scope | Required to prove A1–A6 without a network or `gh` process; stays private to the module |
| Preserve exhaustion / non-advancing-cursor / empty-page behavior | In scope | A4–A6 |
| Per-node search cursors for mid-page resume | Rejected | Explicit non-goal; query-shape change beyond the issue |
| Reducer-level duplicate suppression | Rejected | Different ownership; the fix removes both gaps and duplicates at the source |

## Review and verification ledger

- Local OCR: `0 / 2`
- PR OCR: `0 / 2`
- rustreviewer / DeepThinker: pending
- RED evidence: with the pre-fix loop,
  `filtered_page_never_returns_more_issues_than_requested` failed with "a page of
  four must not carry 6 issues: [1, 3, 4, 5, 6, 8]",
  `filtered_page_resumes_at_the_last_fully_emitted_page` failed with
  `left: [1, 3, 4, 5, 6, 8] / right: [1, 3, 4]`, and
  `resuming_from_the_returned_cursor_skips_and_repeats_nothing` failed with
  "every emitted page must respect the requested size". The four boundary tests
  (exact fill, exhaustion, empty page, non-advancing cursor) passed before and
  after, proving the fix preserved them.
- Exact-head verification: pending
- Deferred findings: none

## Completion contract

Complete only when every row has behavioral evidence, the filtered loop provably
returns at most `page_size` issues with a resume cursor that neither skips nor
repeats an issue, exact-head local verification and required CI pass, reviews are
triaged within their counters, the PR is conflict-free with correct ancestry, and
every changed file maps to this ledger.

# Issue #579 — Filtered issue pagination must not overrun the page or skip issues

> When an issue-type filter has to be applied client-side,
> `fetch_issue_search_filtered_pages` accumulates matches across several raw
> search pages. It breaks once `collected.len() > page_size` without truncating,
> so the caller receives more issues than it asked for. The continuation cursor
> is the end cursor of the last raw page fetched, which stays consistent only
> because nothing is truncated: capping the page without moving the cursor back
> would silently drop every match between the last emitted issue and that
> cursor. The page bound and the resume point therefore have to be fixed
> together.

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
| A8 | Same filtered path | A raw page carries more matches than the whole requested page, so it can be neither emitted nor deferred | All platforms | n/a | `GhError::ApiError` naming the oversized page, instead of an empty page whose cursor invites the identical request forever | Only the raw fetches performed | unchanged | Unit test asserting the typed error |
| A9 | Same filtered path | A raw page reports `has_more` but carries no end cursor | All platforms | The loop stops with what it has and `has_more == false` rather than restarting from the first page | n/a | As above | unchanged | Unit test asserting one fetch and no continuation |
| A10 | Same filtered path | A raw fetch fails after earlier pages already contributed matches | All platforms | n/a | The underlying `GhError` propagates instead of being reported as a short page | Only the raw fetches performed | unchanged | Unit test asserting the propagated error |

## Non-goals

- Changing the GraphQL selection to request per-node cursors (`edges { cursor }`)
  so a page could resume mid-page. The available `pageInfo.endCursor` is
  page-granular, and page-granular resume is enough to satisfy A1/A2.
- Changing the server-side sort, filter, or query construction (issue #573/#578).
- Changing `IssueListResponse`, the reducer's page-append behavior
  (`PaginatedList::accept_page` concatenates and does not de-duplicate), or any
  UI surface. This is a client-boundary fix with no rendered-output change, so no
  TUI harness scenario is added.
- Introducing a page-size domain type (`NonZeroU32`) or otherwise changing the
  `GhClient::list_issues` signature.
- Tracking every cursor visited during an accumulation to detect a multi-step
  cursor cycle. The loop refuses to follow a cursor it cannot advance past; a
  server that hands back an already-consumed cursor several pages later
  contradicts the connection contract and is out of this issue's scope.
- Adding retry, caching, or prefetch behavior to the pagination loop.
- Reworking `fetch_issue_search_raw_page` or its error mapping.

## Vertical slices

### Slice 1 — Bounded page and correct resume cursor

- **Rows:** A1–A10.
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
| Typed error when a page can be neither emitted nor deferred | In scope | A8; without it the new deferral could return an empty page with an unchanged cursor |
| Stop on a `has_more` page that carries no end cursor | In scope | A9; the same "cannot advance" guard the loop already applies to a repeated cursor |
| Per-node search cursors for mid-page resume | Rejected | Explicit non-goal; query-shape change beyond the issue |
| `NonZeroU32` page-size domain type | Rejected | Public client-signature change; A8 already removes the non-progress exit |
| Visited-cursor cycle detection | Rejected | Pre-existing, contradicts the connection contract, and needs a new mechanism |
| Reducer-level duplicate suppression | Rejected | Different ownership; the fix keeps a stable traversal free of gaps and duplicates at the source |

## Review and verification ledger

- Local OCR: `2 / 2` — both runs over the committed range reported zero findings;
  the second ran after review remediation.
- PR OCR: `1 / 2` — the repository's automatic OpenCodeReview job reviewed head
  `7c1fa84b` against merge base `a88d4d9f` and reported no findings.
- rustreviewer: one full review of the committed range; six findings triaged.
  Fixed: the deferral could return an empty page with an unchanged cursor when a
  raw page could be neither emitted nor deferred (now a typed `GhError::ApiError`,
  A8); a `has_more` page with no end cursor restarted pagination (now stops, A9);
  the doc comment overstated the no-gap guarantee (now scoped to stable results);
  the exact-fill fixture returned more raw nodes than the requested page size
  (now production-realistic); the plan's problem statement and its reducer
  de-duplication claim were inaccurate (both corrected). Added tests for A8, A9,
  A10, and for deferring the final page. Rejected: a `NonZeroU32` page-size
  domain type and visited-cursor cycle detection, both recorded as non-goals.
- RED evidence: with the pre-fix loop,
  `filtered_page_never_returns_more_issues_than_requested` failed with "a page of
  four must not carry 6 issues: [1, 3, 4, 5, 6, 8]",
  `filtered_page_resumes_at_the_last_fully_emitted_page` failed with
  `left: [1, 3, 4, 5, 6, 8] / right: [1, 3, 4]`, and
  `resuming_from_the_returned_cursor_skips_and_repeats_nothing` failed with
  "every emitted page must respect the requested size". The four boundary tests
  (exact fill, exhaustion, empty page, non-advancing cursor) passed before and
  after, proving the fix preserved them.
- State-layer check: `PageToken::from_cursor` (`src/domain/pagination.rs:74`)
  already collapses `has_more` without a cursor to `Done`, and
  `PaginatedList::should_load_more` (`src/domain/paginated_list.rs:384`) only
  requests another page when the selection sits on the last row, so a page that
  is shorter than `page_size` costs one extra scroll rather than spinning.
- Exact-head verification: `cargo xtask ci` passes at head `7c1fa84b` (fmt,
  clippy-allow policy, source size, architecture, multiplexer surface, strict
  Clippy, complexity, coverage, build, test); line coverage 69.83%.
- CI: all 19 required checks on PR #606 pass (2 optional jobs skip).
- Deferred findings: none. A second independent reviewer pass could not be
  obtained — both review subagent providers returned usage-limit errors — so the
  second cycle was spent on the local OCR run above.

## Completion contract

Complete only when every row has behavioral evidence, the filtered loop provably
returns at most `page_size` issues with a resume cursor that neither skips nor
repeats an issue, exact-head local verification and required CI pass, reviews are
triaged within their counters, the PR is conflict-free with correct ancestry, and
every changed file maps to this ledger.

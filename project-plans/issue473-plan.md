# Issue #473 — Add user-selectable list sorting (number, created, updated, priority) to Issues, PRs, and Actions

## Problem

All three list screens (Issues, PRs, Actions) hardwire sort at fetch time
(`updated_at DESC, number ASC`) with no user control. Users cannot re-order
the already-loaded list by number, created date, updated date, or (Issues
only) priority. There is no sort concept in the keybind bar, help modal, or
domain state.

## Decision (accepted approach)

Sort is a **projection-time view transform** on the already-loaded
`PaginatedList`, not a fetch-time concern. It reuses the existing
`PaginatedList::sort_by` helper and preserves selection by identity (issue
number / PR number / run id). Filter stays fetch-time; sort composes on top.

Sort lives as a **second section in the existing filter dialog** (a row below
the filter fields), implemented via two additional `Cycle` editor-kind
`FilterFieldView` entries (`by` and `order`) appended to each domain's filter
field list. Cycling is driven by the existing `resolve_filter_control_key`
machinery — no new key resolver. The fetch-time `sort_issues` /
`sort_pull_requests` / `cmp_workflow_runs_newest_first` calls remain as a
defensive sane-order guarantee; the active sort re-projects after every
load/append.

**Priority is a native GitHub issue property** (not a label), fetched by
adding a `priority` GraphQL subfield to both issue queries, parsed into a new
`Issue.priority` field, and exposed as a sort key on Issues only.

**Single PR, sequenced internally by phase.** The issue text suggests five
PRs, but the bounded-delivery workflow targets ≤25 files / 1,500 net lines
per PR. The full feature is estimated at ~18-22 files and well under 1,500
net lines, so it ships as ONE PR with internally sequenced phases (each phase
is a green commit). This keeps the scope ledger in one place and the
acceptance matrix provable in one CI run.

## Architectural decisions (locked)

1. **Sort config storage location.** A new per-domain sort config lives on
   `IssuesState` / `PullRequestsState` / `ActionsState` directly (not inside
   the `filter_ui` struct, not inside the domain filter). Rationale: sort is
   not a fetch constraint, so it must not be part of the
   `IssueListIdentity` / `PrListIdentity` / `ActionsListIdentity` (which
   drive stale-rejection). Putting it on the filter would entangle sort
   changes with reload identity. A dedicated `SortConfig { by, order }`
   domain type (with per-domain `SortBy` enums) keeps the comparators typed.

2. **Sort fields extend the filter field count.** `ISSUE_FILTER_FIELD_COUNT`
   becomes 10 (8 filters + `sort_by` + `sort_order`); `PR_FILTER_FIELD_COUNT`
   becomes 10; `ACTIONS_FILTER_FIELD_COUNT` becomes 5 (3 filters + 2 sort).
   The wrap-around cycle already keys off these constants, so Tab navigation
   naturally flows into the sort row. The `by`/`order` fields render on a new
   row below the filter rows (a `Sort:` prefix row). The active-field
   highlight and the existing `←/→ cycle` / `Enter apply` / `Esc cancel`
   hints already cover the interaction — no new keybind hints are needed
   beyond documenting sort in the help modal text.

3. **Re-projection trigger.** A single private helper
   `resort_issues_preserving_selection` (and PR/Actions siblings) is called
   inside the three load-result apply paths (`apply_issue_list_loaded`,
   `apply_issue_list_silent_refreshed`, `apply_issue_list_page_loaded`) and
   whenever the sort config changes. Mirrors the existing
   `resort_actions_runs_preserving_selection` pattern.

4. **`created_at` is missing from both `Issue` and `PullRequest` list
   structs** — both GraphQL list queries omit `createdAt`. Phase 1 adds
   `created_at: String` to `Issue` + fetches `createdAt` in both issue
   queries; Phase 3 does the same for `PullRequest` + the PR query. This is
   required, not optional, for sort-by-created.

5. **Priority GraphQL shape.** GitHub's GraphQL exposes issue priority via a
   `priority` subfield. Following the `issueType { name }` precedent, parse
   `priority` into `Issue.priority: Option<String>` (None when absent/legacy).
   The comparator orders by the parsed value; missing priority sorts last
   (desc) / first (asc) deterministically. Implementation must verify the
   exact field name returned by `gh api graphql` and adjust if it needs a
   nested object (`priority { value }`) — this is the one external-shape risk
   and is isolated to Phase 2.

6. **Defaults preserve current behavior.** Issues/PRs default
   `UpdatedAt/Desc`; Actions defaults `CreatedAt/Desc` (matches the current
   `cmp_workflow_runs_newest_first` which sorts by `created_at` desc).

## Acceptance matrix

| ID | Domain | Behavior | Evidence |
|----|--------|----------|----------|
| A1 | Issues | Sort by number (asc/desc) re-orders the loaded list instantly without refetch. | Comparator unit test + state test asserting order after `IssueListLoaded`. |
| A2 | Issues | Sort by created date (asc/desc). Requires `created_at` fetched + parsed. | Comparator unit test + parse test proving `createdAt` round-trips. |
| A3 | Issues | Sort by updated date (asc/desc). Default `UpdatedAt/Desc`. | Comparator unit test. |
| A4 | Issues | Sort by priority (asc/desc). Requires `priority` GraphQL field fetched + parsed into `Issue.priority`. Missing priority sorts deterministically last (desc) / first (asc). | Comparator unit test + parse test proving `priority` round-trips from a GraphQL fixture. |
| A5 | Issues | Selection (highlighted issue) is preserved by identity (issue number) across re-sort, silent refresh, and page append. | State test: load list, select issue #N, change sort, assert #N still selected. |
| A6 | Issues | Sort row (`Sort: by:[updated] order:[desc]`) renders in the filter dialog below the filter fields; `by`/`order` are cycle fields reachable via Tab. | Filter-bar render snapshot test + key-routing test asserting Tab reaches the sort row and `←/→`/space cycles it. |
| A7 | Issues | Changing sort does NOT mutate `IssueListIdentity` (no refetch, no stale-rejection interaction). | State test asserting `list.identity()` filter is unchanged after a sort change. |
| A8 | Issues | Background silent-refresh appends/refreshes respect the active sort. | State test: silent refresh re-sorts and preserves selection by number. |
| A9 | PRs | Sort by number / created / updated (asc/desc). No priority. Default `UpdatedAt/Desc`. Requires `created_at` fetched on the PR list query. | Comparator unit tests + parse test. |
| A10 | PRs | Same dialog placement, projection-time behavior, selection preservation, no-refetch. | State + render tests mirroring A5–A8. |
| A11 | Actions | Sort by number (run number) / created / updated (asc/desc). No priority. Default `CreatedAt/Desc` (preserves `cmp_workflow_runs_newest_first`). | Comparator unit tests. |
| A12 | Actions | Same dialog placement and projection-time behavior. The existing fetch-time `cmp_workflow_runs_newest_first` stays as the defensive default; active sort re-projects. | State test mirroring A5–A8. |
| A13 | All | Sort config persists across restart per-domain, matching the existing filter-persistence pattern (per-repo for Issues/PRs; Actions is not per-repo so it persists at the domain level). | Durable projection round-trip test: set sort, project, restore, assert sort restored. |
| A14 | All | Help modal text documents that sort lives in the filter dialog (`f`). Keybind bar already advertises `f filter`. | Help-text assertion test. |
| A15 | All | Existing fetch-time sort (`sort_issues`, `sort_pull_requests`, `cmp_workflow_runs_newest_first`) remains as a defensive sane-order guarantee; it does not override the active sort. | Assert active sort wins after a load (covered by A1–A12). |

## Non-goals

- **No list-level sort hotkeys.** The single-letter namespace is exhausted
  and `s`/`S` collides with send-to-agent. Dialog-only sort. Follow-up if
  wanted (backtick key).
- **No sort-by-label / sort-by-assignee / sort-by-author / sort-by-state.**
  Only number, created, updated, priority.
- **No Slice B `FieldDescriptor` generalization.** The per-domain projections
  stay as-is with sort fields appended. Follow-up.
- **No priority on PRs/Actions.** Concept does not apply.
- **No server-side sort pushdown.** The GraphQL `orderBy` stays as the
  default-fetch ordering only; sort is projection-time.
- **No new public abstraction, process subsystem, dependency, or quality-tool
  change.**
- **No refactor of the filter-field-count machinery beyond extending the
  constants and field lists.**
- **No change to the durable schema version** (`STATE_SCHEMA_V2`). Sort
  config rides in `RepoPreferences` (Issues/PRs) and a new persisted Actions
  field, all `#[serde(default)]` so legacy documents restore to the current
  defaults.
- **No TUI scenario rewrite** beyond one new scenario proving the sort row
  renders and cycles (the issue lists a TUI scenario in acceptance).

## Vertical slices (each = one green commit)

### Phase 1 — Shared sort types + Issues sort (number/created/updated)

Acceptance rows: A1, A2, A3, A5, A6, A7, A8, A15 (Issues minus priority).

1. Domain: add `SortOrder` (`Asc`/`Desc`, default `Desc`) and an
   `IssueSortBy` enum (`Number`/`Created`/`Updated`/`Priority`, default
   `Updated`) + `IssueSortConfig { by, order }` (default `Updated/Desc`) in
   `src/domain/issues.rs`. Add `created_at: String` to `Issue`.
2. GraphQL: add `createdAt` to both issue list queries in
   `src/github/parse.rs`; parse it in `parse_issue_from_item`.
3. Comparator: add `issue_comparator(config)` returning a closure usable by
   `PaginatedList::sort_by`, with a `priority` arm that compares
   `Issue.priority` (None sorts last/first). Unit-test all 8 by×order
   combinations.
4. State: add `sort_config: IssueSortConfig` to `IssuesState`. Add private
   `resort_issues_preserving_selection`. Call it inside
   `apply_issue_list_loaded`, `apply_issue_list_silent_refreshed`, and
   `apply_issue_list_page_loaded` after accept. Add reducer arms for sort
   cycle/change events.
5. Filter UI: extend `ISSUE_FILTER_FIELD_COUNT` to 10; extend
   `issue_filter_fields` to append `sort_by` + `sort_order` `FilterFieldView`s
   on a `Sort:` row (handle the row prefix in the projection). Wire the two
   new fields to the `Cycle` editor kind in
   `src/app_input/issues_filter.rs`.
6. Events: add `IssueSortByCycle` / `IssueSortOrderCycle` (or reuse a generic
   cycle) + reducer. Key routing: when `field_index` is in the sort range,
   `←/→`/space cycles the sort.
7. RED→GREEN: comparator tests, parse test for `createdAt`, state tests for
   re-projection + selection preservation + no-identity-mutation, filter-bar
   render test for the sort row, key-routing test for Tab into sort.

### Phase 2 — Issues priority fetch + priority sort

Acceptance rows: A4.

1. GraphQL: add `priority` to both issue queries. Verify the exact field
   shape via `gh api graphql` against the real repo; parse into
   `Issue.priority: Option<String>`.
2. Comparator: the `IssueSortBy::Priority` arm (added in Phase 1 as a
   placeholder comparing None) now compares the parsed priority.
3. RED→GREEN: parse test from a GraphQL fixture including priority;
   comparator test for priority asc/desc with mixed None.

### Phase 3 — PRs sort (number/created/updated)

Acceptance rows: A9, A10.

1. Domain: add `PrSortBy` (`Number`/`Created`/`Updated`, default `Updated`) +
   `PrSortConfig`. Add `created_at: String` to `PullRequest`.
2. GraphQL: add `createdAt` to the PR search query in
   `src/github/parse_pr.rs`; parse it in `parse_pr_from_node`.
3. Comparator + state mirror of Phase 1 (PR variants).
4. Filter UI: extend `PR_FILTER_FIELD_COUNT` to 10; extend
   `pr_filter_field_views`; wire sort fields in `src/app_input/prs_filter.rs`.
5. RED→GREEN: mirror Phase 1 tests.

### Phase 4 — Actions sort (number/created/updated)

Acceptance rows: A11, A12.

1. Domain: add `ActionsSortBy` (`Number`/`Created`/`Updated`, default
   `Created`) + `ActionsSortConfig`. (`WorkflowRun` already has `created_at`
   and `run_number`.)
2. Comparator + state: replace the hardwired
   `cmp_workflow_runs_newest_first` usage in `actions_load_ops.rs` with the
   active sort (keeping the fetch-time call as the defensive default before
   accept, then re-projecting). The existing
   `resort_actions_runs_preserving_selection` is generalized to take the
   active sort config.
3. Filter UI: extend `ACTIONS_FILTER_FIELD_COUNT` to 5; extend
   `actions_filter_fields`; wire sort in `src/app_input/actions.rs`.
4. RED→GREEN: comparator tests, state tests for re-projection.

### Phase 5 — Persistence + help text + TUI scenario

Acceptance rows: A13, A14 + the TUI scenario acceptance row.

1. Persistence: add `issue_sort` / `pr_sort` (`#[serde(default)]`) to
   `RepoPreferences` in `src/domain/mod.rs`; restore in
   `reset_issues_for_repo_change` / `reset_prs_for_repo_change`; remember in
   `remember_issue_preferences_for` / `remember_pr_preferences_for`. Actions
   sort is not per-repo — persist on `ActionsState` via a new
   `#[serde(default)]` field carried through the durable projection (or, if
   the durable projection does not carry Actions state, document Actions sort
   as session-only and record the deviation in the scope ledger — to be
   resolved in Phase 5 after inspecting the projection).
2. Help modal: add a line documenting sort in the filter dialog.
3. TUI scenario: add `dev-docs/tmux-scenarios/issues-sort-cycle.json`
   proving the sort row renders and `←/→` cycles it.
4. RED→GREEN: durable round-trip test, help-text assertion, scenario.

## Expected files (by architectural layer)

| File | Phase | Change |
|------|-------|--------|
| `src/domain/issues.rs` | 1,2 | `SortOrder`, `IssueSortBy`, `IssueSortConfig`, `Issue.created_at`, `Issue.priority`, comparator |
| `src/github/parse.rs` | 1,2 | `createdAt` + `priority` in queries + parsing |
| `src/state/issues_types.rs` | 1 | `IssuesState.sort_config` |
| `src/state/issues_load_ops.rs` | 1 | re-projection calls |
| `src/state/issues_ops.rs` | 1 | sort cycle reducer arms |
| `src/state/issues_tests_filter.rs` (or new test file) | 1 | state tests |
| `src/ui/components/filter_controls.rs` | 1 | sort row in `issue_filter_fields`/props |
| `src/app_input/issues_filter.rs` | 1 | sort field wiring |
| `src/state/issues_types.rs` | 1 | `ISSUE_FILTER_FIELD_COUNT` → 10 |
| `src/domain/mod.rs` (PR struct) | 3 | `PullRequest.created_at` |
| `src/github/parse_pr.rs` | 3 | `createdAt` in query + parsing |
| `src/state/pr_types.rs` | 3 | `PullRequestsState.sort_config`, `PR_FILTER_FIELD_COUNT` |
| `src/state/prs_load_ops.rs` | 3 | re-projection |
| `src/state/prs_ops.rs` | 3 | sort cycle reducer |
| `src/ui/components/pr_filter_controls.rs` | 3 | sort row |
| `src/app_input/prs_filter.rs` | 3 | sort wiring |
| `src/domain/actions.rs` | 4 | `ActionsSortBy`, `ActionsSortConfig` |
| `src/state/actions_load_ops.rs` | 4 | generalize re-sort to active config |
| `src/state/actions_ops.rs` | 4 | sort cycle reducer, `ACTIONS_FILTER_FIELD_COUNT` → 5 |
| `src/ui/components/filter_controls.rs` | 4 | sort row in `actions_filter_fields` |
| `src/app_input/actions.rs` | 4 | sort wiring |
| `src/domain/mod.rs` (`RepoPreferences`) | 5 | `issue_sort`, `pr_sort` persisted fields |
| `src/state/preferences_ops.rs` | 5 | remember/restore sort |
| `src/state/issues_ops.rs` / `prs_ops.rs` | 5 | restore sort on mode enter |
| `src/ui/components/keybind_bar.rs` or help text | 5 | document sort in filter dialog |
| `dev-docs/tmux-scenarios/issues-sort-cycle.json` | 5 | TUI scenario |

**Estimated: ~22 files, ~900–1,200 net lines.** Under the 25-file /
1,500-line target; no mandatory scope-review trigger. Each phase is a green
commit well under the 15-file / 800-line commit budget.

## Scope ledger

| File | Change | Reason |
|------|--------|--------|
| (to be filled as each phase commits) | | |

No newly discovered work yet. No out-of-scope files.

## Review counters

- OCR before PR: 0 / 2
- OCR after PR: 0 / 2

## Verification

- `cargo xtask quick` (fmt + check + test) during iteration.
- Full `cargo xtask ci` (fmt check, clippy-allow policy, source-size,
  architecture, strict + complexity clippy, coverage ≥ 30%, locked
  all-feature build + test) on the green checkpoint before PR.
- TUI scenario `issues-sort-cycle.json` exercised via the harness.
- Existing filter/navigation/persistence tests remain green.

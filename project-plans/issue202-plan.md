# Issue #202 Plan: Unify list pagination and lazy data-loading

## Goal

Introduce a common deterministic state container (`PaginatedList<T, I>`) for
the list-loading lifecycle shared by Actions, Issues, and PRs. Each screen
re-implements the same lifecycle today: fetch a page of items from a `gh` data
source, track `has_more`/continuation/pending request, reject stale responses
by request id, and manage loading flags. Actions additionally has **no working
load-more path** — it models `page`/`has_more` but never fetches page 2.

This plan targets **PR #1**: ship the shared abstraction + migrate Actions
(which gains working load-more). Issues (PR #2) and PRs (PR #3) follow.

## Design (approved)

- **Abstraction**: a generic struct `PaginatedList<T, I>`, NOT a trait object,
  NOT `dyn Any`, NOT a `ListLoader` service. The shared value is deterministic
  state-transition policy, not interchangeable data providers. There is exactly
  one `gh` implementation per screen.
- **Pagination model**: a `PageToken` enum unifies the two backends:
  - `Cursor(String)` — GraphQL end-cursor (Issues, PRs).
  - `PageNumber(u32)` — REST next-page number (Actions). `PageNumber(n)` means
    the NEXT page to request.
  - `Done` — no more pages.
- `has_more` is **derived** from the token (`!matches!(token, Done)`), never
  stored, so contradictory state (`has_more=true, token=Done`) is impossible.
- **Layering** (DAG-respecting):
  - `src/domain/pagination.rs` — `PageToken`, `ListRequestId`. No
    project-internal dependencies.
  - `src/state/pagination.rs` — `PaginatedList<T, I>`, `PendingLoad<I>`,
    `AcceptOutcome`, `ReloadVisibility`, `LoadCorrelation`, `BeginOutcome`.
    No I/O.
  - `state/*_load_ops.rs` — thin screen adapters: construct identity/result,
    delegate to `PaginatedList`, apply screen-specific detail/error/scroll
    policy.
  - `app_input/*_list_dispatch.rs` — decide when to load, allocate/start
    request, convert `PageToken` into backend args, spawn the `gh` task.
- **Identity structs**: `ActionsListIdentity { scope_repo_id, filter }`
  (likewise `IssueListIdentity`, `PrListIdentity`). Explicit structs, not
  tuples.
- **Single pending**: `PendingLoad<I>` enum — `Reload { identity, request_id,
  visibility }` or `Page { identity, token, request_id }`. Makes illegal states
  unrepresentable (no separate reload/page pending slots to disagree).
- **`ReloadVisibility { Visible, Silent }`** models the PR silent-refresh path
  (no visible loading indicator). PR migration (PR #3) will use this.
- **`AcceptOutcome { Applied, Empty, Stale }`** is the return of every accept
  method. The reducer uses it to decide screen-specific side effects (clear
  error, reset detail, etc.).
- **REMOVE the `request_id == 0` legacy special-case**. `ListRequestId::default()`
  is "no ids allocated"; the first real request is 1. Stale rejection must be
  unconditional.
- **Zero bool fields** on `PaginatedList`; loading is derived from `pending`.

## PR #1 scope (this plan)

### New files
- `src/domain/pagination.rs` — `PageToken`, `ListRequestId`.
- `src/state/pagination.rs` — `PaginatedList<T, I>` + supporting enums.

### Wiring
- `src/domain/mod.rs` — `mod pagination; pub use pagination::*;`
- `src/state/mod.rs` — `pub mod pagination;`

### Actions migration (gains load-more)
- `src/state/types.rs` — replace Actions list fields
  (`runs`, `selected_run_index`, `page`, `has_more`, `list_reload_pending`,
  `next_list_request_id`, `loading.list`) with
  `pub list: PaginatedList<WorkflowRun, ActionsListIdentity>`. Add
  `ActionsListIdentity`. Keep detail loading separate. Drop `ActionsListReloadPending`.
- `src/state/actions_ops.rs` — thin the list-reload reducer to delegate to
  `PaginatedList`. **Delete the dead `page > 1` extend branch** in
  `reload_runs()`. Centralize request-id allocation through
  `list.next_request_id()`.
- `src/state/actions_load_tests.rs`, `src/state/actions_tests.rs` — update
  field accesses; add load-more reducer tests.
- `src/messages/actions.rs`, `src/messages/actions_conversion.rs`,
  `src/state/events.rs` — add an explicit `ActionsRunsPageLoaded` (append)
  variant alongside `ActionsRunsLoaded` (reload). Stop branching on `page == 1`.
- `src/app_input/actions_orchestration.rs`, `src/app_input/actions.rs`,
  `src/app_input/mod.rs` — add `load_more_runs_if_at_end()` and wire it after
  Actions list navigation, mirroring Issues/PRs.
- UI/selector files reading renamed Actions fields (e.g.
  `src/ui/screens/actions.rs`, `src/actions_view.rs`,
  `src/ui/components/actions_list.rs`) — update to read `list.items()` etc.

### TDD order
1. RED — `PageToken` normalization tests (`domain/pagination.rs`).
2. GREEN — `domain/pagination.rs`.
3. RED — `PaginatedList` state-machine tests (`state/pagination.rs`): reload
   replace/select, page append, stale rejection (id/request/token), empty
   page, failure clears pending but preserves rows+continuation, load-more
   predicate, request-id exhaustion, silent reload visibility, reload
   supersedes pending page.
4. GREEN — `state/pagination.rs`.
5. RED — Actions TUI scenario + reducer/dispatch load-more tests.
6. GREEN — Actions migration + load-more dispatch.
7. REFACTOR — remove duplicated request-id bumping; thin load ops.

### Non-goals (this PR)
- Issues migration (PR #2), PRs migration (PR #3).
- Comment pagination, detail loading, mutations, workflow dispatch.
- Changing the `gh` call/parse layer.

## Invariants
- A list result may mutate state only when it exactly matches the single
  pending operation by kind, identity, request id, and (for pages) requested
  token.
- `ActionsRunsLoaded` triggers first-row detail load; `ActionsRunsPageLoaded`
  (append) must NOT trigger a detail reload.
- `should_load_more` is true only when: items non-empty, selection at last
  index, `next_page != Done`, no pending request.

## Verification
- `make quick-check` during iteration.
- `make ci-check` before completion (fmt, clippy-allow policy, source-size,
  clippy `-D warnings`, complexity, coverage >=30%, build, test).

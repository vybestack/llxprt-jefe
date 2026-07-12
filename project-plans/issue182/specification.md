# Issue #182 — Issues screen: create / delete / close

GitHub issue: vybestack/llxprt-jefe#182
Branch: `issue182`

## Goal

Add full list-level lifecycle control over issues from within jefe:

1. **Create** — already exists (`OpenNewIssueComposer`). Verify the keybind is
   present and discoverable in `keybind_hints_for`. (Mostly verification; the
   `n` keybind + composer already work.)
2. **Close** — close an issue from the Issues list/detail.
3. **Delete** — delete an issue from the Issues list/detail via GraphQL
   `deleteIssue` (requires the GraphQL node id), behind a confirm overlay.

## Coordinate with #175

#175 (edit properties: labels, assignees, milestone, type, title, **state**) is
still OPEN. This lands first, so the **close** mutation must be structured so
#175 can extend it to the full property-edit set. There must be exactly **one**
code path for closing an issue.

Design decision: implement close as a typed mutation that goes through the
existing `IssueMutationPending` + `apply_issue_mutation_event` machinery (same
path as body-update and comment-mutations), delivered as a new
`IssueClosed` result event. This keeps a single mutation pipeline that #175 can
extend (it will add more result variants / a richer property-mutation target).

## Key facts discovered during research

- The GraphQL node id (`id` field) is **NOT** currently captured anywhere. The
  list GraphQL search query (`parse.rs` line ~799 / ~955-957) and the
  `gh issue view --json` detail fetch do NOT select the `id` node field. So:
  - **Delete** (which needs the node id) requires adding `node_id: String` to
    both `Issue` and `IssueDetail` domain types, selecting `id` in the GraphQL
    search queries, and adding `id` to the `gh issue view --json` field list.
- **VERIFIED**: `gh issue view NUMBER --json id` returns the GraphQL node id
  (e.g. `I_kwDORSOxIM7sXe5_`) — NOT the REST numeric id. So the detail node id
  comes for free by adding `id` to the existing `--json` field list in
  `get_issue_detail`. No extra GraphQL fetch needed. (Note: `--json nodeId` is
  rejected; the field name is `id`.)
- **Close** can use `gh issue close` (REST-style, by number) — no node id
  needed. Per the issue, `closeIssue` (GraphQL) OR `gh issue close` are both
  acceptable. We use `gh issue close` (simplest, by number) but route the
  result through the single typed-mutation pipeline.
- **Delete** has NO `gh issue delete` equivalent → must go through GraphQL
  `deleteIssue` mutation (mirrors the `resolveReviewThread` pattern in
  `src/github/pr_threads.rs`).
- The `*SilentRefreshed` machinery referenced in the issue exists ONLY for PRs,
  NOT for issues. Issues refresh post-mutation via `RefocusIssueList` (list
  reload) + `load_issue_detail_for_selection` (detail reload), exactly as
  `create_issue` does after `IssueCreated`. Close/delete reuse that pattern.

## Architectural layers to touch (all, respecting boundaries)

| Layer | File(s) | Change |
|-------|---------|--------|
| domain | `src/domain/mod.rs` | Add `node_id: String` to `Issue` + `IssueDetail`. |
| github client | `src/github/mod.rs` (new `issue_lifecycle.rs` submodule to stay under file-size limit) | `close_issue(owner,repo,number)` via `gh issue close`; `delete_issue(node_id)` via GraphQL `deleteIssue`. Add `pub use`. |
| github parse | `src/github/parse.rs` | Select `id` in both GraphQL search query strings; parse `node_id` in `parse_issue_from_item` + `parse_issue_detail_json`. Add `gh issue view --json` `id` field? — `gh issue view --json id` returns the REST numeric id, NOT the node id, so detail node_id must come from a GraphQL fetch. **Decision**: fetch detail node id via the existing GraphQL comments path is wrong; instead, the list already carries node_id, and for detail we add a GraphQL `nodeId` lookup OR derive from the list. Simplest correct approach: after close/delete we operate on the currently-focused issue; for delete we need the node id which the **list** row carries (and the detail preview inherits). So populate `IssueDetail.node_id` from the preview (list) path and from a dedicated GraphQL `repository.issue(number:){ id }` fetch in `get_issue_detail`. |
| state types | `src/state/types.rs` | New `AppEvent` variants: `IssueCloseRequested`, `IssueClosed`, `IssueDeleteRequested`, `IssueDeleteConfirm`, `IssueDeleteCancel`, `IssueDeleted`. New state: `IssueCloseMutationPending`? — reuse `IssueMutationPending` shape. Add `delete_confirm: Option<IssueDeleteConfirmState>` + `close_pending`/`delete_pending` to `IssuesState`. |
| messages | `src/messages.rs` + `src/messages/issues_conversion.rs` | Mirror the new AppEvent variants as `IssuesMessage` variants + `name()` + conversions. |
| state ops | new `src/state/issues_lifecycle_ops.rs` (to keep under file-size/complexity limits) | Reducer: open/close confirm overlay, apply `IssueClosed`/`IssueDeleted` (update list row + detail state, clear pending), apply failures. |
| app_input dispatch | `src/app_input/mod.rs` (new arms), new `src/app_input/issues_lifecycle.rs` | Route the new messages → spawn gh tasks (close/delete) off-thread via `spawn_gh_task_with_panic`; post-success reload list + detail. |
| keybinds | `src/app_input/issues.rs` | Wire `c`→close? `c` is already "new comment" in detail. Use **`C` (shift-c) → close**, **`D` (shift-d) → delete** in BOTH list and detail focus. Confirm overlay: `Enter`→confirm, `Esc`→cancel (two-step arm like merge chooser). |
| UI overlay | new `src/ui/components/issue_delete_confirm.rs` + mount in `src/ui/screens/issues.rs` | Confirm overlay for delete (mirrors `merge_chooser.rs`). |
| keybind hints | `src/ui/components/keybind_bar.rs` | Update `DashboardIssues` hint string to include close + delete. |
| TUI scenario | `dev-docs/tmux-scenarios/issues-close-delete.json` | End-to-end harness scenario. |

## Keybind design (emoji-free, consistent with existing footer)

Issues list + detail:
- `C` (Shift-c) → close the focused issue (no confirm — closing is reversible;
  but route through the mutation pipeline). If already closed, show a read-only
  notice.
- `D` (Shift-d) → open the delete confirm overlay (destructive → two-step
  confirm like merge chooser: first `D` arms, `Enter` confirms, `Esc` cancels).

Footer update (`DashboardIssues`):
add `C close | D delete` to the hint string.

## Single code path for close (coordinate #175)

`IssueCloseRequested` → reducer sets `IssueMutationPending` (target =
`InlineState::None`-equivalent lifecycle marker) → dispatch spawns
`GhClient::close_issue` → `IssueClosed` event → reducer updates the list row +
detail `state = Closed`, clears pending → dispatch reloads list+detail.

When #175 lands, it extends this same pipeline: `IssueCloseRequested` becomes
one of several property-mutation requests sharing `IssueMutationPending` + the
`apply_issue_mutation_event` reducer + the off-thread dispatch spawn. No
parallel close path is introduced.

## TDD plan (RED → GREEN → REFACTOR)

Per `dev-docs/standards/testing-and-quality.md`:
1. GraphQL command construction is unit-tested (parse `node_id`, build
   `deleteIssue`/`close` args) — pure functions, no I/O.
2. State-transition reducer tests: confirm overlay open/cancel/confirm,
   `IssueClosed` updates list+detail, `IssueDeleted` removes from list + clears
   detail, failure clears pending + sets scoped error, idempotency guards.
3. Key-routing tests (`issues_key_tests.rs`): `C`/`D` resolve to the new events
   in list + detail focus; confirm overlay intercepts keys.
4. Message-conversion round-trip tests for every new variant.
5. TUI harness scenario (`issues-close-delete.json`) proves end-to-end focus +
   overlay rendering.

# Issue #175 Implementation Plan — Edit issue/PR properties from within jefe

## Goal

Allow editing an issue's or PR's properties (labels, assignees, milestone,
title, state, and issue type) from within jefe's Issues and Pull Requests
modes — closing the read/write gap so triage does not require dropping to a
terminal.

## Scope (this PR)

Editing the following properties for the focused issue/PR, from the detail view:

| Property   | Issues | PRs | Mechanism                            |
|------------|--------|-----|--------------------------------------|
| Labels     | yes    | yes | `gh issue edit`/`gh pr edit` (add/remove) |
| Assignees  | yes    | yes | `gh issue edit`/`gh pr edit` (add/remove) |
| Milestone  | yes    | yes | `gh issue edit`/`gh pr edit` (`--milestone`/`--remove-milestone`) |
| Title      | yes    | yes | `gh issue edit`/`gh pr edit` (`--title`) |
| State      | yes (open/close) | yes (close/reopen) | GraphQL `updateIssue`/`closeIssue`/`reopenIssue` (issues); `gh pr close`/`gh pr ready`/GraphQL `updatePullRequest` for reopen (PRs) |
| Issue Type | yes    | n/a | GraphQL `updateIssue` with `issueTypeId` |

## UX approach

Extend the existing `e` edit key into an **edit menu** when pressed in the
detail view (Body subfocus). The menu lists: `Body`, `Labels`, `Assignees`,
`Milestone`, `Title`, `State`, `Type` (issues only). Selecting opens the
appropriate modal/selector:

- **Body** → existing inline editor (`OpenInlineEditor` IssueBody) — unchanged.
- **Title** → existing inline editor with a new `EditorTarget::IssueTitle` /
  `PrTitle` variant (reuses the inline composer text-editing machinery).
- **Labels / Assignees / Milestone / Type** → a reusable **property chooser**
  overlay (mirrors the merge chooser): a selectable list populated from the
  repo's label/assignee/milestone/type sets, with multi-select for labels and
  assignees (toggle) and single-select for milestone/type. Space toggles,
  Enter submits the diff.
- **State** → a small single-select chooser: Open/Closed (issues),
  Close/Reopen (PRs).

All overlays follow the established precedence chain (inline > chooser > merge
chooser > search > filter), mirror the merge-chooser key handling (Up/Down/
Enter/Esc), and surface failures via the existing `MutationFailed` /
`PrMutationFailed` non-blocking warning path. On success, the detail + list
are silently refreshed so the change is reflected without a spinner flash.

## Architecture & module boundaries (from dev-docs/standards/architecture.md)

- `src/github/` — pure command/argument construction + `gh`/GraphQL calls +
  JSON parsing. New file `src/github/edit_properties.rs` for the
  property-edit builders + client methods. Unit-test arg construction and
  add/remove diffing here (no network).
- `src/messages.rs` — add new `IssuesMessage` / `PullRequestsMessage`
  variants + `message_names!` entries.
- `src/state/` — reducer transitions (open/close chooser, navigate, confirm,
  cancel, apply-success/failure). New ops files: `state/issues_property_ops.rs`,
  `state/prs_property_ops.rs`. New state types in `state/types.rs` /
  `state/pr_types.rs` (`PropertyEditorState`, `IssuePropertyKind`,
  `PrPropertyKind`).
- `src/app_input/` — dispatch helpers spawning the gh task off the UI thread
  (mirror `issues_mutation.rs` / `prs_mutation.rs`). New:
  `app_input/issues_property_edit.rs`, `app_input/prs_property_edit.rs`.
  Wire into `issues_orchestration` / `prs_orchestration` route functions and
  the `issues.rs` / `prs.rs` key routers (add a precedence tier for the
  property editor).
- `src/ui/components/` — new `property_editor.rs` overlay component (iocraft),
  mirrors `merge_chooser.rs`. Wire into `ui/screens/issues.rs` and
  `ui/screens/pull_requests.rs`.
- `src/ui/components/keybind_bar.rs` — keep `e edit` hint; it now opens the
  edit menu. No change needed (the hint text already says `e edit`).
- `src/selection/` — add a `PropertyEditor` selectable pane for mouse
  selection support (mirror `MergeChooser` wiring in `selection/`).

## gh / GraphQL command construction (the core of the github layer)

### Labels (issues + PRs) — `gh issue edit` / `gh pr edit`

Compute add/remove diffs from current vs. desired sets, then build:

- `gh issue edit --repo OWNER/REPO NUMBER --add-label A,B --remove-label X`
- `gh pr edit --repo OWNER/REPO NUMBER --add-label A,B --remove-label X`

### Assignees (issues + PRs) — `gh issue edit` / `gh pr edit`

- `--add-assignee USER` / `--remove-assignee USER` (repeatable).

### Milestone (issues + PRs) — `gh issue edit` / `gh pr edit`

- `--milestone "NAME"` to set, `--remove-milestone` to clear.

### Title (issues + PRs) — `gh issue edit` / `gh pr edit`

- `--title "NEW TITLE"`.

### Issue Type — GraphQL `updateIssue`

`gh api graphql -f query='mutation($id:ID!,$type:ID!){updateIssue(input:{id:$id,issueTypeId:$type}){issue{...}}}' -F id=NODE_ID -F type=TYPE_ID`. To clear, pass `null`. Requires fetching the repo's
issue types + the issue's node id first. Reuse the existing issue-type query
infrastructure in `parse.rs`.

### State

- Issues: GraphQL `closeIssue` / `reopenIssue` (and `updateIssue` for
  state-only). Use `gh issue close` / `gh issue reopen` for simplicity and
  consistency with the CLI-based edit path.
- PRs: `gh pr close` / `gh pr reopen`.

## TDD plan (RED → GREEN → REFACTOR)

Per dev-docs/standards/testing-and-quality.md: unit-test the pure layers;
keep tmux/network out of unit tests.

1. **github arg builders** (`edit_properties.rs`):
   - labels add/remove diff computation.
   - assignees add/remove diff.
   - milestone set/clear arg shape.
   - title arg shape.
   - issue-type GraphQL mutation shape (set + clear).
   - state command shape (issue close/reopen, pr close/reopen).
2. **state reducers** (`issues_property_ops` / `prs_property_ops`):
   - open property editor from detail (preconditions: detail loaded, no other
     overlay active).
   - navigate selection wraps among enabled rows.
   - toggle (labels/assignees) marks a row selected/unselected.
   - confirm emits the pending-property state; cancel closes cleanly.
   - apply-success updates detail fields + clears overlay.
   - apply-failure sets scoped error, preserves overlay selection (no wedge).
3. **key router** (`issues.rs` / `prs.rs`):
   - `e` in detail opens the edit menu.
   - edit-menu navigation/confirm/cancel key routing.
   - precedence: property editor consumes keys while open.
4. **TUI scenario** (`dev-docs/tmux-scenarios/`): add
   `edit-properties-menu.json` documenting the `e` → menu → select → submit
   flow (proves the scenario fails before impl, passes after).

## Files (non-exhaustive)

New:
- `src/github/edit_properties.rs` (+ tests inline or `src/github/tests/edit_properties.rs`)
- `src/state/issues_property_ops.rs`
- `src/state/prs_property_ops.rs`
- `src/app_input/issues_property_edit.rs`
- `src/app_input/prs_property_edit.rs`
- `src/ui/components/property_editor.rs`
- `dev-docs/tmux-scenarios/edit-properties-menu.json`

Modified:
- `src/github/mod.rs` (pub re-exports, client method wrappers)
- `src/messages.rs` (new variants + names)
- `src/state/events.rs` (new AppEvent variants)
- `src/state/types.rs` (property-editor state for issues)
- `src/state/pr_types.rs` (property-editor state for PRs)
- `src/state/mod.rs` (wire new ops files)
- `src/app_input/mod.rs` (declare new modules)
- `src/app_input/issues.rs` / `prs.rs` (key routing)
- `src/app_input/issues_orchestration.rs` / `prs_orchestration.rs` (dispatch)
- `src/messages/issues_conversion.rs` / `prs_conversion.rs` (event ↔ message)
- `src/ui/screens/issues.rs` / `pull_requests.rs` (render overlay)
- `src/ui/components/mod.rs` (export new component)
- `src/selection/*` (selectable pane for mouse selection)

## Verification

`make ci-check` (fmt check + clippy gates + coverage ≥30% + build + test).

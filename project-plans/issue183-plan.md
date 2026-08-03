# Issue #183 — PR screen: closing, deleting, and creating pull requests

> The Pull Requests mode can read a pull request and, since #175, edit its
> properties. It cannot start one, and it cannot finish one: creating a PR and
> retiring a merged branch both still require dropping to a terminal. This
> change closes that gap and keeps exactly one code path for the state
> transition #175 already owns.

## What already exists, and what it forces

| Capability | Where it lives today | Consequence for this issue |
|---|---|---|
| Close / reopen a PR | `PrPropertyKind::State` → `GhClient::close_item` / `reopen_item` (`src/app_input/prs_property_edit.rs`) | The close path exists and must not be duplicated. Only its **reachability** is missing: the editor refuses to open unless `PrFocus::PrDetail` owns focus, so a PR cannot be closed from the list |
| Confirm-destructive overlay | `IssueDeleteConfirmState` + `IssueDeleteConfirmOverlay` (issue #182) | The two-step arm/confirm shape is settled; the PR overlay mirrors it rather than inventing a second idiom |
| Post-mutation refresh | `PostMutationRefresh` + `PrListSilentRefreshed` / `PrDetailSilentRefreshed` | Every mutation here refreshes through that machinery; no bespoke reload |
| Head branch of a PR | `PullRequest::head_ref` (list row) and `PullRequestDetail::head_ref` | Branch deletion needs no extra read query for the branch name |
| Inline creation form | `NewIssueFormState` (issue #407) in `IssuesState` | The New PR composer mirrors it: PR-mode-owned draft state, not a `ModalState` |

### The constraint that shapes the event design

`src/state/events.rs` is **exactly 1000 lines**, which is the hard source-size
limit (`cargo xtask check source-size`, `DEFAULT_HARD_LIMIT = 1000`). Verified
empirically: appending a single blank line produces
`ERROR: src/state/events.rs has 1001 lines (max 1000)`. `src/messages/prs_conversion.rs`
is at 999 and has the same problem. Every new behavior in this issue needs
reducer events, so `AppEvent` cannot grow one variant at a time.

`AppEvent` already carries wrapped sub-enums for exactly this reason —
`Observation(ObservationEvent)` and `Keys(KeysEditorMessage)`. This change
follows that precedent: a single `AppEvent::PrLifecycle(Box<PrLifecycleEvent>)`
variant, mirrored by `PullRequestsMessage::Lifecycle(Box<PrLifecycleEvent>)`,
carries the whole PR mutation-lifecycle family. The existing PR **merge**
lifecycle events move into that family so the cutover is complete rather than
half-done, which is what frees the room both capped files need. Loosening or
raising any limit is not an option and is not done.

## Acceptance matrix

| # | Actor / launch path | Input / boundary | Observable success | Observable failure / diagnostic | Side effects before failure | Persistence / compatibility | Proof |
|---|---|---|---|---|---|---|---|
| A1 | User presses `Shift+W` with the PR **list** focused | A PR is selected, so the list preview populated `pr_detail` | The existing State property editor opens on the selected PR; confirming runs the same `close_item` / `reopen_item` path as the detail view | No PR selected, or an overlay/mutation already active: nothing opens | None | No new close code path; #175 remains the only one | Reducer test opening from `PrFocus::PrList`; key test that `Shift+W` in `prs.list` yields `PrOpenPropertyEditor { State }` |
| A2 | User presses `Shift+D` with the PR list or detail focused | A PR is focused | A confirm overlay names the PR number and the head branch and shows the unarmed hint | No PR focused: a non-blocking notice, no overlay | None | Overlay is transient state, never persisted | Reducer tests for open/arm/cancel; render test for the overlay text |
| A3 | Same overlay, first `Enter` | Overlay open, unarmed | The overlay arms and the hint changes to the confirm wording | n/a | None | n/a | Reducer test |
| A4 | Same overlay, second `Enter` | Overlay armed, PR still open | The PR is closed through `close_item`, then its head branch is deleted through GraphQL `deleteRef`; the row and detail show `Closed`; list and detail silently refresh | Any step fails: the pending clears, the overlay is gone, and a scoped error names the PR | The close may already have happened when the branch delete fails; the error says so | Optimistic close is reconciled by the silent refresh | Argument-construction tests, dispatch outcome tests, reducer result tests |
| A5 | Same, PR already merged or closed | Armed confirm | No close is attempted; only the branch is deleted | Branch delete failure surfaces as in A4 | None | n/a | Dispatch test over both PR states |
| A6 | Same, head branch is the PR's own base branch | Armed confirm | The delete is refused before any request, with a message naming the branch | Same message | None | n/a | Reducer test |
| A7 | Same, head branch name is empty (list row never carried one) | Armed confirm | The delete is refused before any request with a "head branch unavailable" message | Same | None | n/a | Reducer test |
| A8 | Same, PR head lives in a fork | Armed confirm | `deleteRef` cannot resolve a ref that is not in the base repository, and the surfaced error says the branch was not found | Same | The close already happened | n/a | Ref-resolution parse test for the `null` ref response |
| A9 | User presses `Esc` on the overlay | Any armed state | The overlay closes and nothing is dispatched | n/a | None | n/a | Reducer test |
| A10 | User presses `n` with the PR list focused | PR mode active, no overlay | A New PR composer opens with Head, Base, Title and Body; the repository's branches load in the background and Base is seeded from the repository's default branch | An overlay or in-flight mutation is already active: nothing opens | None | Draft is transient; a repository change discards it with a notice | Reducer test; key test |
| A11 | Composer, branch load completes | Branch list and default branch returned | Head and Base become selectable lists; Base preselects the default branch; Head preselects the first branch that is not the default | Load failure shows the error in the composer and blocks submit | None | n/a | Reducer tests for both outcomes; parse tests for the GraphQL page |
| A12 | Composer, `Tab` / `BackTab` | Any field | Focus cycles Head → Base → Title → Body and back | n/a | None | n/a | Reducer test |
| A13 | Composer, `Up` / `Down` | Head or Base focused | The branch selection moves within the loaded list; on Title or Body it is inert | n/a | None | n/a | Reducer test |
| A14 | Composer, typing | Title or Body focused | Characters, backspace, delete, cursor motion and (Body only) newline edit the draft | n/a | None | n/a | Reducer tests |
| A15 | Composer, `Ctrl+Enter` / `Alt+Enter` | Title non-empty, Head ≠ Base, branches loaded | `gh pr create --repo --head --base --title --body` runs; on success a notice names the new PR number and the list reloads | Empty title, Head = Base, or branches still loading / failed: a composer error and no request | None | The composer stays open on validation failure and closes on success | Argument-construction test, validation reducer tests, dispatch test |
| A16 | Composer, `Esc` | Any | The composer closes and the draft is discarded | n/a | None | n/a | Reducer test |
| A17 | Any user on the PR screen | Footer and help | The `DashboardPullRequests` footer advertises new PR, close, and delete in parallel with the Issues footer, with no emoji | n/a | None | Footer projection stays registry-driven | Footer projection test; help projection test |
| A18 | Existing merge chooser | `m`, navigate, confirm, cancel, and every async merge result | Behavior is byte-for-byte what it was before the event family moved | Unchanged | Unchanged | Unchanged | The existing merge reducer, dispatch, and integration tests pass unmodified in behavior |

## Non-goals

- Deleting the pull request record itself. GitHub exposes `deleteIssue` for
  issues and nothing equivalent for pull requests; "delete" here is close plus
  head-branch removal, which is what the issue's implementation notes describe.
- A second close/reopen path. The `state` transition delivered by #175 stays
  the only one; this change only makes it reachable from the list.
- Creating branches, pushing, or any local git work as part of PR creation.
- Reviewers, labels, assignees, milestone, draft status, or templates at
  creation time. Properties are #175's surface and remain editable after create.
- Cross-repository (fork) head selection in the composer. The branch list is
  the base repository's own branches.
- Deleting a branch outside the delete-PR flow.

## Vertical slices

### Slice 1 — One PR lifecycle event family (A18, and the precondition for everything else)

Introduce `src/state/pr_lifecycle_events.rs` with `PrLifecycleEvent`, wire
`AppEvent::PrLifecycle(Box<PrLifecycleEvent>)` and
`PullRequestsMessage::Lifecycle(Box<PrLifecycleEvent>)`, and move the existing
merge-chooser lifecycle events into it. Conversions move to
`src/messages/prs_lifecycle_conversion.rs`.

- RED: existing merge tests rewritten against the new constructors fail to compile/run first.
- GREEN: merge behavior unchanged; `cargo xtask check source-size` reports headroom in `events.rs` and `prs_conversion.rs`.
- Non-goals: no behavior change of any kind.

### Slice 2 — Close from the list (A1)

Relax the property editor's focus precondition to accept a previewed PR under
list focus, and register `Shift+W` in the `prs.list` context.

### Slice 3 — Delete: close plus head-branch removal (A2–A9)

`src/github/pr_lifecycle.rs` gains the ref-resolution query, its parser, and the
`deleteRef` mutation. `src/state/prs_delete_ops.rs` owns the overlay and result
transitions. `src/app_input/prs_lifecycle.rs` performs close-then-delete off the
UI thread. `src/ui/components/pr_delete_confirm.rs` renders the overlay.

### Slice 4 — Create from a branch (A10–A16)

`src/github/pr_lifecycle.rs` gains the branch-page query, its parser, and the
`gh pr create` argument builder. `src/state/new_pr_form_ops.rs` owns the draft.
`src/app_input/new_pr_submit.rs` loads branches and submits.

### Slice 5 — Footer, help, and scenarios (A17)

Registry hints for the new actions, plus TUI scenarios for the delete overlay
and the composer.

## Expected files by layer

- Domain / inventory: `default_action_inventory_s4.rs`, `default_action_inventory_display.rs`, `action_registry.rs` (no new handler keys planned).
- GitHub boundary: `src/github/pr_lifecycle.rs`, `src/github/mod.rs`.
- State: `pr_lifecycle_events.rs`, `pr_types.rs`, `prs_delete_ops.rs`, `new_pr_form_ops.rs`, `prs_property_ops.rs`, `prs_merge_ops.rs`, `mod.rs`.
- Messages: `prs.rs`, `prs_conversion.rs`, `prs_lifecycle_conversion.rs`, `names.rs`, `message_names.rs`.
- Input: `action_handlers_s4.rs`, `action_context.rs`, `prs.rs`, `prs_orchestration.rs`, `prs_lifecycle.rs`, `new_pr_submit.rs`.
- UI: `pr_delete_confirm.rs`, `new_pr_form.rs`, `screens/pull_requests.rs`.
- Scenarios: `dev-docs/tmux-scenarios/pr-delete-confirm.json`, `dev-docs/tmux-scenarios/pr-new-composer.json`.

## Scope ledger

| Entry | Justification | Status |
|---|---|---|
| Moving the PR merge lifecycle events into `PrLifecycleEvent` | `events.rs` and `prs_conversion.rs` are both at the hard source-size cap; the family cannot grow without this, and leaving merge behind would be a half-finished cutover | Accepted, required by the gate |
| Relaxing the property editor's focus precondition | A1 requires closing from the list, and #175 forbids a second close path | Accepted |
| Showing `prs_state.draft_notice` in the PR banner | A4 and A15 require observable success. PR mode recorded notices and rendered none, so a completed delete or create was silent. The viewport arithmetic reads the same predicate as the render, so the banner row cannot be double-counted | Accepted, required by acceptance |
| Sharing the GraphQL error-envelope check (`graphql_errors`) | Every new GraphQL call needs the check `closeIssue` already had; duplicating it three more times was the alternative | Accepted |
| Ordering the new footer hints next to `merge` | The PR footer is already wider than a 200-column terminal; placing the lifecycle hints ahead of the long property hint keeps them reachable | Accepted |

## Review counters

- Local OCR runs: 1 of 2.
- PR OCR runs: 0 of 2.

## Verification evidence

Run on the candidate head (`issue183`):

| Gate | Result |
|---|---|
| `cargo xtask fmt` | pass |
| `cargo xtask check clippy-allows` | pass |
| `cargo xtask check source-size` | pass (`events.rs` 976, `prs_conversion.rs` 881 — both back under the cap) |
| `cargo xtask check architecture` | pass |
| `cargo xtask check multiplexer-surface` | pass |
| `cargo xtask lint` | pass |
| `cargo xtask complexity` | pass |
| `cargo xtask build` | pass |
| `cargo xtask test` | pass (whole workspace, all features) |
| TUI `pr-new-composer.json` | pass (13 steps) |

`cargo xtask coverage` aborted locally on
`harness_v1_fixtures::llxprt_continue_field_fixture_sends_one_exact_issue_prompt`.
That fixture drives a real multiplexer and fails the same way on `origin/main`
in this working environment (verified in a clean worktree at `1aa6ba0c`), and it
passes in the workspace test run above, so it is environmental flake rather than
a regression. CI runs coverage on a clean runner.

## Review triage

Local Open Code Review, run 1: eleven findings.

| Finding | Disposition | Action |
|---|---|---|
| The delete success callback was reused, under its delete name, for create | In-scope—Fix | Renamed to `apply_mutation_outcome`; both paths now read as what they are |
| A delete whose close succeeded but whose branch removal failed left the row showing "open" | Blocker—Fix | `DeleteFailed` now carries `closed`, the reducer applies the close and requests the reconciling refresh, and `execute_pr_delete` returns a typed outcome rather than a flat `Result` |
| The delete-confirm and New PR overlays could both be open, stacked at the same coordinates | Blocker—Fix | `pr_delete_blocked` now also refuses while the composer is open, with a test |
| `new_pr_form_blocked` ignored an in-flight merge | In-scope—Fix | Now refuses while any PR mutation is in flight, with a test |
| With no default branch, head and base both landed on the first branch | In-scope—Fix | The head now starts on the first index that is not the base, with a test |
| `mark_pull_request_closed` cloned the whole list to change one row | In-scope—Fix | Uses `PaginatedList::iter_mut` |
| The branch-list collapse test asserted on indentation | In-scope—Fix | Asserts the branch names are absent instead |
| `dev-docs/tmux-scenarios/pr-delete-confirm.json` asserted the overlay was absent right after asking for it, so it passed whatever happened | Blocker—Fix | Deleted. The overlay cannot open without a live pull request, so the scenario could only ever be a tautology; the overlay is covered by `pr_delete_confirm` render tests, the reducer tests, and the key tests |
| The State editor "will never open from the list" because `pr_detail` is cleared on list load | Reject | The list-load dispatch previews the selected row immediately afterwards (`prs_list_dispatch` line 347), so `pr_detail` is populated; the no-preview case is already covered by `no_editor_opens_when_no_pull_request_is_previewed` |
| `flag_value` uses an empty prefix for `--method` | Reject | The finding itself concludes no change is needed |

## Deferred findings

None.

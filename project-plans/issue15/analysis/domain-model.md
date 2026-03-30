# Domain Model Analysis — Issues Mode

Plan ID: `PLAN-20260329-ISSUES-MODE`

## Scope

Defines canonical domain objects, invariants, state transitions, integration touchpoints, and existing code modification map for the Issues Mode feature.

---

## New Domain Entities

### IssueState (enum)
- `Open`
- `Closed`

Invariants:
- Maps directly to GitHub issue states.
- Default filter shows `Open` issues.

### Issue (list representation)
- `number: u64`
- `title: String`
- `state: IssueState`
- `author_login: String`
- `updated_at: String`
- `assignee_summary: String`
- `labels_summary: String`
- `comment_count: u64`

Invariants:
- Always scoped to a single `RepositoryId`.
- Number is unique within repository scope.

### IssueDetail
- `repo_owner_name: String`
- `number: u64`
- `title: String`
- `state: IssueState`
- `author_login: String`
- `created_at: String`
- `updated_at: String`
- `labels: Vec<String>`
- `assignees: Vec<String>`
- `milestone: Option<String>`
- `body: String`
- `external_url: String`
- `comments: Vec<IssueComment>`
- `has_more_comments: bool`
- `comments_cursor: Option<String>`

Invariants:
- Must match current `selected_repository_id` scope.
- Stale detail from prior scope must never be displayed.

### IssueComment
- `comment_id: u64`
- `author_login: String`
- `created_at: String`
- `edited_at: Option<String>`
- `body: String`

### IssueFilter
- `query_text: String`
- `state: Option<IssueFilterState>` (open | closed | all)
- `author: String`
- `assignee: String`
- `labels: Vec<String>`
- `mentioned: String`
- `updated_before: String`
- `updated_after: String`

Invariants:
- Structured filters are AND-composed.
- Labels use AND across selected labels.
- Text query AND-composed with structured filters.
- Default committed state: all criteria unset/empty.

### IssueFilterState (enum)
- `Open`
- `Closed`
- `All`

---

## Extended Existing Entities

### Repository (extended)
New field:
- `issue_base_prompt: String`

Invariants:
- Empty value is valid.
- Persisted via existing repository configuration persistence path.
- Included in send-to-agent payload composition.

---

## Issues State Aggregate

### IssuesState (new sub-state of AppState)
- `active: bool` — whether Issues Mode is active
- `issues: Vec<Issue>` — current scoped list
- `selected_issue_index: Option<usize>`
- `issue_detail: Option<IssueDetail>`
- `committed_filter: IssueFilter`
- `draft_filter: IssueFilter`
- `search_query: String`
- `list_loading: bool`
- `detail_loading: bool`
- `comments_loading: bool`
- `list_page_cursor: Option<String>`
- `has_more_issues: bool`
- `error: Option<String>`
- `issue_focus: IssueFocus` — current focus domain within issues mode
- `detail_subfocus: DetailSubfocus` — subfocus within detail pane
- `inline_state: InlineState` — mutable control state
- `agent_chooser: Option<AgentChooserState>`
- `filter_controls_open: bool`
- `search_input_focused: bool`
- `prior_agent_focus: Option<PriorAgentFocus>` — saved before entering issues mode
- `draft_notice: Option<String>` — transient notice for discarded drafts

### IssueFocus (enum)
- `RepoList`
- `IssueList`
- `IssueDetail`

### DetailSubfocus (enum)
- `Body`
- `Comment(usize)` — index into comments vec
- `NewComment`

### InlineState (enum)
- `None`
- `Composer { target: ComposerTarget, text: String, cursor: usize }`
- `Editor { target: EditorTarget, text: String, cursor: usize }`

### ComposerTarget (enum)
- `NewComment`
- `Reply { comment_index: usize, author: String }`

### EditorTarget (enum)
- `IssueBody`
- `Comment { comment_index: usize }`

### AgentChooserState
- `selected_index: usize`
- `agents: Vec<(AgentId, String)>` — id + display name

### PriorAgentFocus
- `pane_focus: PaneFocus`
- `selected_repository_index: Option<usize>`
- `selected_agent_index: Option<usize>`

---

## Event Taxonomy (New Events)

### Issues Mode Lifecycle
- `EnterIssuesMode`
- `ExitIssuesMode`
- `RefocusIssueList`

### Issues Navigation
- `IssuesNavigateUp`
- `IssuesNavigateDown`
- `IssuesPageUp`
- `IssuesPageDown`
- `IssuesHome`
- `IssuesEnd`
- `IssuesEnter`
- `IssuesCycleFocus` (Tab)
- `IssuesCycleFocusReverse` (Shift+Tab)
- `IssuesScrollDetailUp`
- `IssuesScrollDetailDown`

### Issue Data Loading
- `IssueListLoaded { issues: Vec<Issue>, cursor: Option<String>, has_more: bool }`
- `IssueListLoadFailed { error: String }`
- `IssueListPageLoaded { issues: Vec<Issue>, cursor: Option<String>, has_more: bool }`
- `IssueDetailLoaded { detail: IssueDetail }`
- `IssueDetailLoadFailed { error: String }`
- `IssueCommentsPageLoaded { comments: Vec<IssueComment>, cursor: Option<String>, has_more: bool }`
- `IssueCommentsPageFailed { error: String }`

### Filter/Search
- `OpenFilterControls`
- `CloseFilterControls`
- `ApplyFilter`
- `ClearFilter`
- `FocusSearchInput`
- `BlurSearchInput`
- `SetSearchQuery { query: String }`
- `ApplySearch`
- `ClearSearch`
- `UpdateDraftFilter { field: FilterField, value: String }`

### Inline Mutation
- `OpenNewCommentComposer`
- `OpenReplyComposer { comment_index: usize }`
- `OpenInlineEditor { target: EditorTarget }`
- `InlineChar(char)`
- `InlineBackspace`
- `InlineSubmit`
- `InlineCancelOrEsc`
- `CommentCreated { comment: IssueComment }`
- `CommentCreateFailed { error: String }`
- `IssueBodyUpdated { body: String }`
- `CommentUpdated { comment_index: usize, body: String }`
- `MutationFailed { error: String }`

### Send-to-Agent
- `OpenAgentChooser`
- `AgentChooserNavigateUp`
- `AgentChooserNavigateDown`
- `AgentChooserConfirm`
- `AgentChooserCancel`
- `SendToAgentCompleted`
- `SendToAgentFailed { error: String }`

---

## Transition and Side-Effect Ownership

- **State reducer** mutates `IssuesState` deterministically via events.
- **GitHub client boundary** owns `gh` CLI subprocess execution for list/detail/comment/mutation API calls.
- **Key routing** layer resolves issues-mode precedence and emits typed events.
- **UI layer** renders from `IssuesState` and emits user intents.
- **Persistence boundary** persists `issue_base_prompt` through existing repository config path.

---

## Edge/Error Model

- `gh` CLI not found → block operations, show install guidance.
- `gh` CLI not authenticated → block operations, show "Run: gh auth login".
- API 404 on issue detail (issue deleted) → scoped error message, keep mode stable.
- API rate limit → scoped retry affordance.
- Network failure → scoped error in affected pane, keep other panes functional.
- Repository scope change during in-flight API call → discard stale response (scope guard via request ID or scope token).
- Reply to comment while not focused on comment → no-op with non-blocking hint.
- Edit non-editable target → no-op with non-blocking hint.
- Send-to-agent with no agents → disable action, show message.
- Empty issue list → scoped empty state display.
- Empty comments → "No comments yet" display.

---

## Existing Code to Modify

### `src/domain/mod.rs`
- Add `Issue`, `IssueDetail`, `IssueComment`, `IssueState`, `IssueFilter`, `IssueFilterState` types.
- Add `issue_base_prompt: String` field to `Repository`.

### `src/state/types.rs` (types, re-exported via `src/state/mod.rs`)
- Add `IssuesState` and related sub-types to `AppState`.
- Add `dashboard_issues` variant to `ScreenMode`.
- Add issues-specific focus domains.
- Add issue events to `AppEvent` enum.

### `src/state/mod.rs` (behavior)
- Implement `apply()` cases for all new events.

### `src/input.rs`
- Extend `InputMode` and `input_mode_for_state()` for issues mode routing.

### `src/app_input/mod.rs` (main crate)
- Add issues-mode key dispatch handler.
- Add suppression rules for dashboard keys in issues mode.
- Wire GitHub client calls for data loading/mutation.

### `src/persistence/mod.rs`
- Add `issue_base_prompt` to persisted `Repository`/`State` structs.
- Ensure backward-compatible deserialization (serde default).

### `src/lib.rs`
- Add `pub mod github;` module declaration.

### `src/ui/`
- New components: issue list, issue detail, inline composer, inline editor, filter controls, agent chooser.
- Extend dashboard to render issues mode layout.
- Add `issue_base_prompt` field to repository config form.

---

## New Code to Create

### `src/github/mod.rs`
GitHub client boundary module.
- `GhClient` struct wrapping `gh` CLI subprocess calls.
- List issues (with filter/pagination params).
- Get issue detail.
- Get issue comments (paginated).
- Create comment.
- Update comment.
- Update issue body.
- Auth check.

### `src/ui/screens/issues.rs`
Issues mode screen layout component.

### `src/ui/components/issue_list.rs`
Issue list pane component.

### `src/ui/components/issue_detail.rs`
Issue detail pane component (includes comments timeline, inline controls).

### `src/ui/components/filter_controls.rs`
Issue filter controls component.

### `src/ui/components/agent_chooser.rs`
Send-to-agent chooser component.

---

## Integration Touchpoints

1. **Mode toggle**: `src/app_input/normal.rs` handles `i` key → emits `EnterIssuesMode` → state reducer activates issues mode → UI renders issues layout.
2. **Repository scope**: existing repo selection in `repo_list` → on change while in issues mode → emit scope invalidation → reload issues.
3. **Send-to-agent**: agent chooser → compose payload with `issue_base_prompt` from repository config → deliver to selected agent runtime.
4. **Persistence**: `issue_base_prompt` persisted alongside other repository fields in `state.json`.
5. **Help modal**: extend help content to include Issues Mode keybindings when in issues mode.

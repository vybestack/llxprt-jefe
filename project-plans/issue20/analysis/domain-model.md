# Domain Model Analysis — Pull Requests Mode

Plan ID: `PLAN-20260624-PR-MODE`

## Scope

Identifies the new domain entities, extended entities, state aggregate, event taxonomy,
message-bus surface, transition/side-effect ownership, edge/error model, and integration
touchpoints for repository-scoped PR Mode. Mirrors the Issues Mode domain model, adapted for
pull requests (review + check summaries, branch info) with deferred complex operations.

## New Domain Entities (`src/domain/mod.rs`)

### `PrState` (enum)
- Variants: `Open`, `Closed`, `Merged`.
- `Copy`; non-serde (transient, loaded from `gh`).
- Invariant: derived from `gh pr` JSON `state` + `mergedAt` (a merged PR maps to `Merged`, not
  `Closed`).

### `PrReviewState` (enum)
- Variants: `Approved`, `ChangesRequested`, `Commented`, `Pending`, `Dismissed`, `ReviewRequired`,
  `None`.
- `Copy`; used both for per-review `state` and aggregate `review_decision`.
- Invariant: aggregate `review_decision` parsed from `reviewDecision`; per-review `state` from the
  `reviews[].state` field.

### `PrCheckStatus` (enum)
- Variants: `Pending`, `Success`, `Failure`, `Neutral`, `None`.
- `Copy`; used both for per-check `status` and aggregate `checks_status` rollup.
- Invariant: rollup derived from `statusCheckRollup`; `None` when no checks reported.

### `PullRequest` (list-row entity)
- Fields: `number: u64`, `title: String`, `state: PrState`, `author_login: String`,
  `updated_at: String`, `head_ref: String`, `base_ref: String`, `is_draft: bool`,
  `review_decision: Option<PrReviewState>`, `checks_status: PrCheckStatus`,
  `assignee_summary: String`, `labels_summary: String`, `comment_count: u64`.
- Non-serde transient (mirrors `Issue`).
- Invariants: `number > 0`; summaries are display-ready joined strings; `title` may be empty but
  is rendered truncated by pane width.

### `PrReview` (review summary item)
- Fields: `author_login: String`, `state: PrReviewState`, `submitted_at: String`,
  `body: Option<String>`.
- Non-serde transient.
- Invariant: read-only; never mutated in-app.

### `PrCheck` (CI/check summary item)
- Fields: `name: String`, `status: PrCheckStatus`, `conclusion: String`, `url: Option<String>`.
- Non-serde transient.
- Invariant: read-only; `url` is display-only.

### `PullRequestDetail` (detail entity)
- Fields: `repo_owner_name: String`, `number: u64`, `title: String`, `state: PrState`,
  `is_draft: bool`, `author_login: String`, `created_at: String`, `updated_at: String`,
  `head_ref: String`, `base_ref: String`, `labels: Vec<String>`, `assignees: Vec<String>`,
  `milestone: Option<String>`, `body: String`, `external_url: String`,
  `review_decision: Option<PrReviewState>`, `checks_status: PrCheckStatus`,
  `reviews: Vec<PrReview>`, `checks: Vec<PrCheck>`, `comments: Vec<IssueComment>`,
  `has_more_comments: bool`, `comments_cursor: Option<String>`.
- Non-serde transient (mirrors `IssueDetail`).
- Invariants: `comments` ordered oldest→newest stable; `external_url` display-only;
  `reviews`/`checks` read-only.

### `PrFilterState` (enum)
- Variants: `Open`, `Closed`, `Merged`, `All`; `#[default] Open`; `Copy`.

### `ReviewDecisionFilter` (enum) — review-signal filter (issue #20 "review signals")
- Variants: `Any`, `Approved`, `ChangesRequested`, `ReviewRequired`, `None`; `#[default] Any`;
  `Copy`.
- Distinct from `PrReviewState` (which models per-review and aggregate `reviewDecision` DATA): this
  is the user's filter CHOICE. `Any` means "do not filter by review decision" (emits no qualifier).
- Invariant: each non-`Any` variant maps to exactly one GitHub search qualifier in the
  `search(type: ISSUE, query: ...)` string — `Approved`→`review:approved`,
  `ChangesRequested`→`review:changes_requested`, `ReviewRequired`→`review:required`,
  `None`→`review:none` (P00A §2c-verified server-side qualifiers; pagination-preserving).

### `ChecksFilter` (enum) — CI/check-rollup filter (issue #20 "workflow signals")
- Variants: `Any`, `Success`, `Failing`, `Pending`; `#[default] Any`; `Copy`.
- Distinct from `PrCheckStatus` (which models per-check and aggregate rollup DATA): this is the
  user's filter CHOICE over the CI rollup. `Any` means "do not filter by checks" (emits no qualifier).
- Invariant: each non-`Any` variant maps to exactly one GitHub search qualifier in the
  `search(type: ISSUE, query: ...)` string — `Success`→`status:success`, `Failing`→`status:failure`,
  `Pending`→`status:pending` (P00A §2c-verified server-side qualifiers; pagination-preserving).

### `PrFilter` (filter criteria)
- Fields: `query_text: String`, `state: Option<PrFilterState>`, `author: String`,
  `assignee: String`, `reviewer: String`, `is_draft: Option<bool>`, `labels: Vec<String>`,
  `review_decision: ReviewDecisionFilter`, `checks_status: ChecksFilter`.
- Invariants: structured criteria AND-composed; labels AND-composed; query AND-composed with
  structured criteria; `is_draft == None` means "any"; `review_decision == Any` and
  `checks_status == Any` each mean "do not filter on that signal" (no qualifier emitted). The
  `review_decision` and `checks_status` criteria satisfy issue #20's requirement to filter PRs by
  "common review/workflow signals"; both compile to SERVER-SIDE search qualifiers so cursor
  pagination (`endCursor`/`hasNextPage`) is preserved.

### Reused entity
- `IssueComment` (existing) is reused for PR comments — GitHub PRs are issues for the
  conversation-comment API. The comment NODE shape is identical, so the existing
  `parse_comments_json`/`parse_page_info`/`parse_created_comment_json` parsers are reused. Transport
  caveat (verified P00A §2d): PR comment FETCH goes through a NEW `list_pr_comments` querying
  GraphQL `repository.pullRequest(number:).comments` — NOT the issue `list_comments`
  (`repository.issue(number:)`), which is NULL for a PR number. PR comment CREATE uses the REST
  `/repos/{o}/{n}/issues/{number}/comments` endpoint, which DOES accept a PR number.

## Extended Existing Entities

### `Repository` (`src/domain/mod.rs`)
- No new persisted field required. PR Mode reuses:
  - `github_repo: String` (the `owner/name` slug; EMPTY string means unconfigured; validated for
    format — `src/domain/mod.rs` L205), and
  - `issue_base_prompt: String` (reused for the PR send-to-agent base prompt).
- Invariant: `github_repo` must be a non-empty `owner/name` slug before any PR request is issued;
  an empty/malformed slug yields `("","")` from `resolve_gh_repo` (mirrors
  `src/app_input/issues_dispatch.rs::resolve_gh_repo` L14-36) and surfaces a scoped configuration
  message — never a silent drop.

## State Aggregate (`src/state/types.rs`)

### `ScreenMode` (extend)
- Add `DashboardPullRequests`.
- Invariant: existing variants `Dashboard`, `Split`, `DashboardIssues` unchanged.

### `PrFocus` (new enum)
- Variants: `RepoList`, `PrList`, `PrDetail`.
- Mirrors `IssueFocus`; separate from `PaneFocus` (do not modify `PaneFocus`).

### `PrDetailSubfocus` (new enum)
- Variants: `Body`, `Review(usize)`, `Check(usize)`, `Comment(usize)`, `NewComment`.
- Invariant: indices are bounds-checked against loaded `reviews`/`checks`/`comments`; empty
  sections are skipped during `Tab`/`Shift+Tab` traversal.

### `ReadOnlyHintKind` (new enum) — CANONICAL DEFINITION

This is the single source of truth for the hint-kind variant set. Every other reference
(component-001/003/004, specification.md, and all phase docs) MUST match this exact set.

- Variants (Copy/Clone), complete and exhaustive:
  - `ReadOnlyReplyOnComment` — `r` pressed on body/review/check/new-comment (reply only valid on a
    comment).
  - `ReadOnlyNoComment` — `c` pressed on a review/check item (reviews and checks are read-only).
  - `ReadOnlyNotEditable` — `e` pressed anywhere in PR detail (body/reviews/checks not editable in
    v1).
  - `NoSelectionToOpen` — `o` pressed with no PR selected/loaded (nothing to open in browser;
    REQ-PR-012).
- Carried by `PrShowNotice(ReadOnlyHintKind)` to surface a non-blocking hint when an invalid
  `r`/`c`/`e`/`o` action is attempted, rather than silently dropping the key. The user-visible text
  for each variant is enumerated once in component-003 (No-op + Hint Mechanism table).

### `PullRequestsState` (new aggregate — mirrors `IssuesState`)
- `active: bool`
- `pull_requests: Vec<PullRequest>`
- `selected_pr_index: Option<usize>`
- `pr_detail: Option<PullRequestDetail>`
- `committed_filter: PrFilter`
- `draft_filter: PrFilter`
- `search_query: String`
- `loading: PrLoadingState` (`list`, `detail`, `comments` flags)
- `list_cursor: Option<String>`
- `has_more_prs: bool`
- `error: Option<String>`
- `pr_focus: PrFocus`
- `detail_subfocus: PrDetailSubfocus`
- `list_scroll_offset: usize` (first-visible PR-list row; driven by the NEW shared
  list-viewport / selection-follow helper — see component-001 lines 177-196 and REQ-PR-006.
  There is no existing list-scroll helper to reuse; this offset + helper are a fresh deliverable.)
- `list_viewport_rows: usize` (PR-list pane height in rows; set as a prop from the typed layout
  module — `prs_pane_rows(...)` — NOT read via `crossterm::size()` in the reducer. Used by the
  selection-follow helper to recompute `list_scroll_offset`; mirrors how `detail_viewport_rows` feeds
  detail scroll math.)
- `detail_scroll_offset: usize`
- `detail_viewport_rows: usize`
- `inline_state: InlineState` (reused `InlineState` DIRECTLY — NOT `Option<InlineState>`. The
  current `InlineState` enum (`src/state/types.rs` `enum InlineState` L307) already carries a
  `#[default] None` sentinel (the `#[default]` attribute + `None` variant at L308-309) and
  `IssuesState` stores it directly as `inline_state: InlineState` (`src/state/types.rs` L386). "No
  active composer" is `InlineState::None`; presence is tested via `inline_state != InlineState::None`,
  exactly like `src/state/issues_ops.rs` L47 and L152. Wrapping it in `Option` would double-encode the
  absent state and diverge from the Issues precedent.)
- `agent_chooser: Option<AgentChooserState>` (reused)
- `filter_ui: PrFilterUiState` (`controls_open`, `field_index`, `draft_labels_text`); `field_index`
  ranges over the EIGHT filter fields in cycling order — 0 state, 1 draft, 2 review-decision,
  3 checks-status, 4 author, 5 assignee, 6 reviewer, 7 labels (Tab/Shift+Tab wrap modulo 8)
- `search_input_focused: bool`
- `prior_agent_focus: Option<PriorAgentFocus>` (reused)
- `draft_notice: Option<String>`
- `mutation_pending: Option<PrMutationPending>`
- `next_mutation_id: u64`
- `list_reload_pending: Option<PrListReloadPending>`
- `next_pr_list_request_id: u64`
- `list_page_pending: Option<PrListPagePending>`
- `detail_pending: Option<PrDetailPending>`
- `next_pr_detail_request_id: u64`
- `comments_page_pending: Option<PrCommentsPagePending>`
- `next_comments_page_request_id: u64`

### Pending-request guards (new structs; mirror Issues guards)
- `PrListReloadPending { scope_repo_id, request_id }`
- `PrListPagePending { scope_repo_id, request_id, cursor }`
- `PrDetailPending { scope_repo_id, pr_number, request_id }`
- `PrCommentsPagePending { scope_repo_id, pr_number, request_id, cursor }`
- `PrMutationPending { scope_repo_id, mutation_id, target }`
- Invariant: every async response carries `scope_repo_id` + `request_id`; the reducer discards any
  response whose scope or request id no longer matches current state (staleness guard).

### Reused state sub-entities (no new variants needed)
- `InlineState`, `ComposerTarget`, `AgentChooserState`, `PriorAgentFocus` are reused as-is.
  (`ComposerTarget::Reply{comment_index, author}` and `NewComment` cover PR comment/reply; PR body
  editing is out of scope so no new `EditorTarget` variant is required.)

## Event Taxonomy (`src/state/types.rs` `AppEvent`)

New PR events (additive; existing `AppEvent` variants unchanged). Grouped:

### Lifecycle
- `EnterPrsMode`, `ExitPrsMode`, `RefocusPrList`.

### Navigation / Focus
- `PrNavigateUp/Down/PageUp/PageDown/Home/End`
- `PrListEnter`
- `PrCycleFocus`, `PrCycleFocusReverse`
- `PrScrollDetailUp/Down/PageUp/PageDown`
- `PrDetailSubfocusNext/Prev`

### Data Loading (carry `scope_repo_id` + `request_id`)
- `PrListLoaded { scope_repo_id, filter, request_id, pull_requests, cursor, has_more }`
- `PrListLoadFailed { scope_repo_id, request_id, error }`
- `PrListPageLoaded { scope_repo_id, request_id, pull_requests, cursor, has_more }`
- `PrDetailLoaded { scope_repo_id, pr_number, request_id, detail }`
- `PrDetailLoadFailed { scope_repo_id, pr_number, request_id, error }`
- `PrCommentsPageLoaded { scope_repo_id, pr_number, request_id, comments, cursor, has_more }`
- `PrCommentsPageFailed { scope_repo_id, pr_number, request_id, error }`

### Filter / Search
- `PrOpenFilterControls`, `PrCloseFilterControls`, `PrApplyFilter`, `PrClearFilter`
- `PrFilterNavigateNext/Prev`, `PrCycleFilterState`, `PrCycleDraftFilter`,
  `PrCycleReviewFilter`, `PrCycleChecksFilter` (Space cycles each enumerated field — state, draft,
  review-decision, checks-status; issue #20 review/workflow signal filters)
- `PrUpdateDraftFilter { field, value }`
- `PrFocusSearchInput`, `PrBlurSearchInput`, `PrSetSearchQuery { query }`, `PrApplySearch`,
  `PrClearSearch`

### Inline Mutation
- `PrOpenNewCommentComposer`, `PrOpenReplyComposer { comment_index }`
- `PrInline*` (Char/Newline/Backspace/Delete/Cursor*/Submit/CancelOrEsc) — reuse the inline
  pattern
- `PrCommentCreated { scope_repo_id, pr_number, mutation_id, comment }`
- `PrCommentCreateFailed { scope_repo_id, pr_number, mutation_id, error }`
- `PrMutationFailed { scope_repo_id, pr_number, mutation_id, error }`

### Read-Only No-op Notice
- `PrShowNotice(ReadOnlyHintKind)` — emitted by `handle_pr_detail_key`/`handle_pr_list_key` for
  invalid `r`/`c`/`e`/`o` actions. The key is CONSUMED (never leaks to dashboard/destructive
  handlers) and the reducer surfaces a non-blocking hint via `draft_notice` — never a silent
  `None`. The carried `ReadOnlyHintKind` is the CANONICAL enum defined above
  (`ReadOnlyReplyOnComment`, `ReadOnlyNoComment`, `ReadOnlyNotEditable`, `NoSelectionToOpen`;
  Copy/Clone) — see the canonical definition for the complete variant set. Reducer:
  `apply_pr_show_notice(kind)` sets `prs_state.draft_notice` (REQ-PR-010 read-only; REQ-PR-012
  nothing-to-open; REQ-PR-013 no-silent-drop).
- The `NoSelectionToOpen` variant is emitted when `o` is pressed with no PR selected/loaded
  (REQ-PR-012). The key is CONSUMED and surfaces a non-blocking notice — never a silent `None`.

### Open-in-Browser (REQ-PR-012)
- `PrOpenInBrowser` — emitted by `handle_pr_list_key`/`handle_pr_detail_key` when `o` is pressed and
  a PR is selected/loaded. The reducer half (`apply_pr_open_in_browser`) is PURE — it only sets a
  transient "opening in browser…" notice; the actual launch is a side effect performed by the
  dispatch layer (`dispatch_pr_open_in_browser`), which spawns
  `gh pr view <number> --repo <owner>/<name> --web` via `spawn_gh_task_with_panic` (off the UI
  thread). `gh --web` opens the platform default browser, so NO bespoke OS opener is introduced.
- `PrOpenedInBrowser { scope_repo_id, pr_number }` — success acknowledgement; clears the
  opening notice.
- `PrOpenInBrowserFailed { scope_repo_id, pr_number, error }` — failure (categorized `GhError`);
  `apply_pr_open_in_browser_failed` surfaces a scoped error notice (REQ-PR-013 no-silent-drop).
- `external_url` remains DISPLAY-ONLY; no in-app merge/approve/request-changes/review-submit
  mutation exists in v1.

### Send-to-Agent
- `PrOpenAgentChooser`, `PrAgentChooserNavigate{Up,Down}`, `PrAgentChooserConfirm`,
  `PrAgentChooserCancel`, `PrSendToAgentCompleted`, `PrSendToAgentFailed { error }`

## Message-Bus Surface (`src/messages.rs`, `src/messages/prs_conversion.rs`)

- Add `MessageDomain::PullRequests`.
- Add `PullRequestsMessage` enum mirroring `IssuesMessage` (lifecycle, navigation, data loading,
  filter/search, inline mutation, send-to-agent, open-in-browser), with `Box`-ed large payloads
  (`detail`, `filter`) as Issues does. The open-in-browser variants are `OpenInBrowser`,
  `OpenedInBrowser{..}`, `OpenInBrowserFailed{..}` (round-trip with the `PrOpenInBrowser*`
  `AppEvent`s; REQ-PR-012).
- Add `AppMessage::PullRequests(PullRequestsMessage)` plus `domain()`/`route()`/`name()` arms and
  a `message_names!` invocation.
- Add `From<AppEvent>` routing for PR events (`from_prs_event`) and
  `From<PullRequestsMessage> for AppEvent`, with `PullRequestsMessage::from_app_event` in
  `src/messages/prs_conversion.rs`.
- Invariant: the typed bus is a faithful bidirectional mapping of the PR `AppEvent` surface; the
  legacy `AppEvent` facade remains the source for reducer logic via `AppEvent::from(message)`.

## Transition and Side-Effect Ownership

| Concern | Owner |
|---------|-------|
| Mode enter/exit, focus, selection, filter/search draft+commit, subfocus, scroll, staleness discard, list/detail/comment merge, inline-state transitions | State reducer (`apply_prs_message` → `apply_prs_event` chain in `src/state/prs_ops.rs` + `prs_inline_ops.rs` + `prs_load_ops.rs` + `prs_mutation_ops.rs`) |
| `gh` CLI list/detail/review/check/comment fetch + comment create + send-payload build | GitHub client boundary (`src/github/mod.rs` + `parse_pr.rs`) |
| Spawning gh I/O off-thread, delivering data events, retry orchestration, open-in-browser launch (`dispatch_pr_open_in_browser` → `gh pr view <n> --repo <o>/<n> --web`) | Dispatch layer (`src/app_input/prs_dispatch.rs`, `prs_list_dispatch.rs`, `prs_mutation.rs`) via `spawn_gh_task_with_panic` |
| Key routing → `Option<AppEvent>` | Key handlers (`src/app_input/prs.rs`, `prs_filter.rs`, `normal.rs`) |
| Rendering panes/controls, emitting intent | UI (`src/ui/screens/pull_requests.rs`, `components/pr_*.rs`) |
| Repository config read, no new persisted field | Persistence (unchanged; `prs_state` excluded from persisted mapping) |

## Edge / Error Model

- `gh` not installed → `GhError::NotInstalled`; block + install guidance.
- `gh` not authenticated → `GhError::NotAuthenticated`; block + `gh auth login` guidance.
- Repo access denied / 404 → scoped pane error; mode/focus stable.
- Rate limit → scoped error + retry.
- Network failure → scoped error + retry.
- Invalid/missing `github_repo` slug → scoped configuration message; no malformed request.
- Scope change during in-flight request → response discarded by scope/request-id guard.
- Reply when no comment focused → `PrShowNotice(ReadOnlyReplyOnComment)` (consumed key + non-blocking
  hint via `draft_notice`); NOT a silent `None`.
- `c` on review/check → `PrShowNotice(ReadOnlyNoComment)`; `e` on body/review/check/new-comment →
  `PrShowNotice(ReadOnlyNotEditable)` (read-only / out of scope) — consumed key + non-blocking hint,
  never a silent `None`.
- `o` with no PR selected/loaded → `PrShowNotice(NoSelectionToOpen)` (consumed key + non-blocking
  hint); NOT a silent `None`. `o` open-in-browser launch failure → `PrOpenInBrowserFailed{error}`
  scoped notice (REQ-PR-012/013).
- Empty states: no repos / no PRs / no reviews / no checks / no comments / no agents.
- Comment pagination failure → keep loaded comments + scoped retry.
- Long content → scroll overflow from real rendered length; titles truncated by pane width.
- No silent `None`-drop of unavailable-context cases — surface message or log.

## Existing Code to Modify

- `src/domain/mod.rs` — add PR entities/enums; reuse `IssueComment`.
- `src/state/types.rs` — add `ScreenMode::DashboardPullRequests`, `PrFocus`, `PrDetailSubfocus`,
  `PullRequestsState`, PR pending guards, PR `AppEvent` variants; add `prs_state` to `AppState`.
- `src/state/mod.rs` — add `apply_prs_message` hub + `AppMessage::PullRequests` arm to
  `apply_message`; add `reset_prs_for_repo_change` hook in `select_repository_by_index`; declare
  `prs_*` ops modules + PR test modules. EXTRACT a SHARED repo-navigation helper
  `move_repo_selection(&mut self, direction) -> bool` (Finding 5) that performs the
  remember → bounded move within `visible_repository_indices` → restore sequence currently
  duplicated in `src/state/issues_ops.rs::navigate_repo_up_in_issues_mode` (L122-131) and
  `navigate_repo_down_in_issues_mode` (L137-148). Refactor BOTH Issues-mode functions to call it
  (`if self.move_repo_selection(dir) { self.reset_issues_for_repo_change() }`) so the logic is not
  copied; PR mode's `navigate_repo_{up,down}_in_prs_mode` call the SAME helper followed by
  `reset_prs_for_repo_change`. The verifier asserts PR mode defines no private copy of this nav
  logic. (See component-001 lines 125-153.)
- `src/input.rs` — add `InputMode::Prs*` variants; add `DashboardPullRequests` block to
  `input_mode_for_state`.
- `src/messages.rs` — add `MessageDomain::PullRequests`, `PullRequestsMessage`,
  `AppMessage::PullRequests` + routing/name/conversions.
- `src/github/mod.rs` — add PR client methods (`list_pull_requests`, `get_pull_request_detail`,
  `list_pr_comments` → GraphQL `repository.pullRequest(number:).comments`,
  `create_pr_comment`, `open_pull_request_in_browser` → `gh pr view <n> --repo <o>/<n> --web`,
  `build_pr_send_payload`) + `PrListResponse` + reuse `GhError` and the comment parsers
  (`parse_comments_json`/`parse_page_info`/`parse_created_comment_json`) + the `IssueComment` type.
  NOTE: PR comment FETCH uses the NEW `list_pr_comments` (`repository.pullRequest(number:)`), NOT the
  issue `list_comments` (`repository.issue(number:)`), because `repository.issue(number:)` is NULL
  for a PR number (silent-empty regression otherwise — verified P00A §2d). PR comment CREATE keeps
  the REST `/repos/{o}/{n}/issues/{number}/comments` endpoint (valid for PR numbers).
- `src/app_input/normal.rs` — `p`/`P` entry in `resolve_mode_key`; `handle_dashboard_prs_key`.
- `src/app_input/mod.rs` — `AppMessage::PullRequests` dispatch arms; register PR modules.
- `src/layout.rs` — PR layout constants + helper fns + `PRS_SIDEBAR_WIDTH` (reuse existing
  constants; no duplication). ALSO add the NEW shared list-row viewport / selection-follow helper
  pair (pure `list_first_visible_index` / `list_visible_window`; see component-001 lines 182-196).
  These live HERE — in the shared leaf module importable by both `state` and `ui` — and NOT in any
  `src/ui/components/list_viewport.rs` file (which does not exist; finding #2). No such list-row
  viewport helper exists today (`issue_list` renders all rows with no offset); this is a fresh build.
- `src/ui/orchestration.rs` — `DashboardPullRequests` arm in `build_screen_element`.
- `src/ui/screens/mod.rs`, `src/ui/components/mod.rs`, `src/ui/mod.rs` — register new screen +
  components.

## New Code to Create

- `src/state/prs_ops.rs` — enter/exit/focus/nav/subfocus/scroll/filter/search reducer + dispatch
  chain.
- `src/state/prs_load_ops.rs` — list/detail/page/comment loaded-event reducers (staleness guards).
- `src/state/prs_inline_ops.rs` — inline composer state reducers.
- `src/state/prs_mutation_ops.rs` — comment-create lifecycle + error reducers.
- `src/github/parse_pr.rs` — PR JSON parse helpers (`parse_pull_requests_json`,
  `parse_pull_request_detail_json`, `parse_pr_review`, `parse_pr_check`, arg builders, sort).
- `src/messages/prs_conversion.rs` — `AppEvent`↔`PullRequestsMessage` conversion.
- `src/app_input/prs.rs` — PR key routing (8-level precedence) → `Option<AppEvent>`.
- `src/app_input/prs_dispatch.rs` — detail/comment async loaders + prompt formatting + agent
  launch.
- `src/app_input/prs_list_dispatch.rs` — list reload/fetch async dispatch.
- `src/app_input/prs_filter.rs` — filter control key handling.
- `src/app_input/prs_mutation.rs` — inline submit → comment-create async dispatch.
- `src/ui/components/pr_list.rs` — scroll-aware PR list (renders only the visible window computed by
  the shared `crate::layout` selection-follow helpers; reuses `ScrollableText` only inside the
  detail view, never for list rows).

> NOTE (finding #2): the NEW shared list-row viewport / selection-follow helper pair (pure
> `list_first_visible_index` / `list_visible_window`; see component-001 lines 182-196) lives in
> `src/layout.rs` and is added under "Existing Code to Modify" below — it is NOT a UI file. There
> is NO `src/ui/components/list_viewport.rs` in this plan (the helpers must be importable by BOTH the
> `state` reducers and `ui`, so they live in the shared leaf `src/layout.rs`; see
> `plan/00-overview.md` lines 114-119).
- `src/ui/components/pr_detail.rs` — unified detail view (metadata + body + reviews + checks +
  comments + composer; the body/comments TEXT region reuses `ScrollableText`).
- `src/ui/components/pr_filter_controls.rs` — interactive PR filter controls (EIGHT fields in
  cycling order: state, draft, review-decision, checks-status, author, assignee, reviewer, labels;
  review-decision + checks-status are the issue #20 review/workflow signal filters).
- `src/ui/screens/pull_requests.rs` — `PullRequestsScreen`.
- PR test modules (`src/state/prs_tests*.rs`, etc.).

## Integration Touchpoints

1. **Mode toggle**: `p`/`P` from Dashboard → `EnterPrsMode` (via `resolve_mode_key`).
2. **Repo scope**: `select_repository_by_index` resets PR state when `prs_state.active`.
3. **Reducer hub**: `apply_message` routes `AppMessage::PullRequests` → `apply_prs_message`.
4. **Async I/O**: dispatch layer uses `spawn_gh_task_with_panic` for all gh calls.
5. **Send-to-agent**: reuses the agent chooser + writes `.jefe/pr-prompt.md`; reuses
   `issue_base_prompt`.
6. **Persistence**: no new persisted field; `prs_state` excluded from persisted mapping.
7. **Help modal**: PR-Mode bindings added to the help content.
8. **Render**: `build_screen_element` renders `PullRequestsScreen` for `DashboardPullRequests`.

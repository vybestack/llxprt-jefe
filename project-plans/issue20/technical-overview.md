# Technical Overview — Pull Requests Mode Contract Specification

Plan ID: `PLAN-20260624-PR-MODE`

## Scope

Defines acceptance-testable technical behavior for PR Mode.

No implementation sequencing is included (see `plan/`).

> **Citation discipline (finding #7):** wherever this document cites an existing integration point,
> the **SYMBOL NAME is authoritative** (the `module::name` form, e.g.
> `src/github/mod.rs::list_issues`) and any `LNNN`/line range is drift-prone EVIDENCE only. Resolve
> every cited symbol BY NAME and refresh stale lines during preflight; a symbol that cannot be found
> by name is a blocker (mirrors `00-overview.md` Critical Reminder #6).

## Top-Level Modes

- `dashboard_agents`
- `dashboard_issues`
- `dashboard_pull_requests`

### Transitions

- `p` from a non-PR dashboard context -> `dashboard_pull_requests`, `focus=pr_list`.
- `p` in `dashboard_pull_requests` -> keep mode, set `focus=pr_list`.
- `a` in `dashboard_pull_requests` -> `dashboard_agents` (Dashboard).
- `Esc` in `dashboard_pull_requests` -> precedence chain; exits mode only when no higher-priority
  cancel target exists.

## Key Routing Contract

Resolution order in `dashboard_pull_requests`:

1. `inline_composer`
2. `agent_chooser`
3. `search_input`
4. `filter_controls`
5. PR-global handlers (`p`, `a`, `Esc`, help)
6. Focus-domain handlers (`repo_list`, `pr_list`, `pr_detail`)
7. Pane-cycle (`Tab`, `Shift+Tab`) — cycles the three panes from EVERY pane (issue #46)
8. Suppressed dashboard/destructive keys consumed as no-op

### Required suppression/override while in PR Mode

- Suppress the dashboard `a` focus-agents handler.
- Suppress the dashboard `s/S` split-mode handler.
- Suppress the split-mode `Esc` handler.
- Suppress dashboard destructive lifecycle keys: `Ctrl-d`, `Ctrl-k`, `l`.
- Route `/` to PR-list search input only.
- Route `?`, `h`, `F1` to help with PR-Mode bindings included.
- Lowercase `s` is an explicit no-op in PR Mode.

## Focus-Domain Contract

### Domains

- `repo_list`
- `pr_list`
- `pr_detail`
- `comment_item`
- `review_item` (read-only)
- `check_item` (read-only)
- `search_input`
- `inline_composer`
- `filter_controls`
- `agent_chooser`

### Inter-pane focus

- In `dashboard_pull_requests`, this cycle supersedes dashboard pane-cycle behavior while the
  mode is active.
- `Tab`: `repo_list -> pr_list -> pr_detail -> repo_list`
- `Shift+Tab`: reverse

### Intra-pane navigation

- `repo_list`: `Up/Down` moves the active repository; selection change is immediately scoped.
  This movement is gated on `PrFocus::RepoList` and MUST NOT depend on the dashboard `pane_focus`
  field.
- `repo_list`: `Enter` is an explicit no-op (no commit action).
- `pr_list`: `Up/Down`, `PageUp/PageDown`, `Home/End`; selection-following keeps the selected row
  visible.
- `pr_detail`: `Up/Down` scroll the unified view.
- `pr_detail` subfocus (skipping empty sections), bound to `j`/`k`:
  - `j`: `body -> review_item(s) -> check_item(s) -> comment_item(s) -> new_comment_field -> body`
- `k` in `pr_detail` traverses the same subfocus graph in reverse.
- `Tab`/`Shift+Tab` in `pr_detail` cycle the three panes (NOT subfocus) — issue #46 requires Tab to
  cycle panes from every pane, so `Tab` exits the detail pane like any other. This DIVERGES from
  `resolve_issue_detail_key_event` (which maps `Tab -> IssueDetailSubfocusNext`); PR mode moves
  subfocus to `j`/`k` so `Tab` stays a pane-cycle everywhere.
- `Left` in `pr_detail` is an OPTIONAL reverse pane-cycle (-> `PrCycleFocusReverse`) for parity with
  the list pane; it is no longer the sole escape since `Tab`/`Shift+Tab` cycle out of detail.
- `c` opens the new-comment composer and sets subfocus to `new_comment_field` regardless of the
  current subfocus.
- `r` requires `focus=comment_item`; otherwise it is a no-op with a non-blocking hint.

## Exit-Focus Restoration Contract

On exit from `dashboard_pull_requests` to `dashboard_agents`, restore prior agent focus only if
all are true:

1. prior focus token exists,
2. referenced target still exists,
3. target is focusable in the current state.

Otherwise, fall back to default agent-list focus.

## Repository Scope Contract

### Source of truth

- `selected_repository_id` from `repo_list` is the authoritative PR scope key.

### Scope-change effects

On repository change (when `prs_state.active`):

- Discard/ignore in-flight list/detail/review/check/comment requests for the prior scope
  (request-id + scope-id invalidation).
- Start a list query for the new scope immediately.
- Ensure detail/reviews/checks/comments only show data from the current scope.
- If the prior selected PR is missing in the new scope, reseat the selection by selection rules.
- Discard unsent inline drafts from the prior repository scope and emit a non-blocking notice;
  drafts are not migrated across scope.

## Query, Sort, Filter, Search

### Minimum list fields

- `number`
- `title`
- `state`
- `author_login`
- `updated_at`
- `head_ref`
- `base_ref`
- `is_draft`
- `review_decision`
- `checks_status`
- `assignee_summary`
- `labels_summary`
- `comment_count`

### Default ordering

- `updated_at desc`, tie-breaker `number asc`.

### Filter/search inputs

- `query_text`
- `state in {open, closed, merged, all}`
- `author`
- `assignee`
- `reviewer`
- `is_draft in {any, drafts-only, ready-only}`
- `labels[]`

### Composition

- Structured filters AND-composed.
- Labels require all selected labels (AND).
- Text query AND-composed with structured filters.

### `f` list-only behavior

Precondition: `focus=pr_list`.

- Pass: open `filter_controls`.
- Fail: no-op.

`filter_controls` operations (fully interactive):

- Default committed state: `committed_filter.state = Some(PrFilterState::Open)` (the default scoped
  list shows OPEN pull requests); all other structured criteria (`author`, `assignee`, `reviewer`,
  `is_draft`, `labels`, `query_text`) are unset/empty. This matches `enter_prs_mode`
  (`analysis/pseudocode/component-001.md` L69: `committed_filter.state = Some(Open)`) and
  `PrFilterState`'s `#[default] Open` (`analysis/domain-model.md` L65).
- `Tab`/`Shift+Tab`: move field focus.
- `Space`: cycle enumerated fields (state, draft).
- Character entry: edit focused text field, updating the draft filter immediately.
- Apply: commit draft criteria and refresh the scoped list.
- Clear: reset committed criteria back to the default (`state = Some(PrFilterState::Open)`, all
  other criteria empty) and refresh the scoped list (`clear_committed_filter`, component-001
  L270-274).
- Cancel: close controls without committing draft criteria.

### Search behavior

- `/` focuses `search_input`.
- `Enter` in `search_input` applies (trimmed) `query_text`.
- `Esc` in `search_input` with non-empty query clears the query and remains focused.
- `Esc` in `search_input` with empty query blurs `search_input` to `pr_list`.

## Selection and Loading Rules

- First non-empty list load selects the first PR and loads detail/reviews/checks/comments.
- On filter/search/sort change:
  - keep the selected PR if still present (by number),
  - else select the first PR,
  - else no selection and a scoped detail empty state.
- Detail/reviews/checks/comments must never show stale prior-scope data.

### Loading-state contract

- Show a scoped loading state while list data is loading.
- Show scoped loading states while detail/reviews/checks/comments for the selected PR are loading.

### Pagination contract

- The PR list is lazy-loaded/paginated using REAL GraphQL cursor pagination, mirroring the issues
  list path: `gh api graphql` with `search(type: ISSUE, query, first, after)` and
  `pageInfo { hasNextPage endCursor }` (`src/github/parse.rs::build_issue_search_args` L594-621;
  `src/github/mod.rs::list_issues` L134-159). Page size = `PR_LIST_PAGE_SIZE` (30). No
  `gh pr list --limit` window heuristic; `endCursor`/`hasNextPage` are the cursor and has-more flag.
- Trigger: when selection reaches the last loaded row and `has_more` (`hasNextPage`) is true, fetch
  the next page using the stored `endCursor`.
- Preserve scope, sort, and committed filters/search across page loads.
- Repository switch invalidates the prior scope paging cursor (`endCursor`) and state.
- The comments timeline supports incremental loading/pagination via the SAME cursor mechanism, but
  through a NEW PR-specific fetcher `GhClient::list_pr_comments` querying the
  `repository.pullRequest(number:).comments(first, after)` + `pageInfo` GraphQL path (page size
  `PR_COMMENT_PAGE_SIZE` = 30). It does NOT reuse the issue `GhClient::list_comments`
  (`repository.issue(number:).comments`), because `repository.issue(number:)` is NULL for a PR number
  — reusing it would silently return zero comments (verified P00A §2d). `list_pr_comments` REUSES the
  existing `parse_comments_json`/`parse_page_info` helpers and the `IssueComment` node type (PR and
  issue comment nodes share the same shape). The detail fetch loads the first comment page via a
  SEPARATE `list_pr_comments` call, following the comments-sourcing precedent of `get_issue_detail`
  (which fetches comments via its own separate call, mod.rs L198-202, and OVERWRITES whatever the
  `gh issue view --json ...,comments` field returned, mod.rs L180/L197); the PR `gh pr view --json
  ...` set simply OMITS the `comments` field outright since it would only be overwritten. PR comment
  CREATE still uses the issues REST endpoint (`/repos/{o}/{n}/issues/{number}/comments`), which
  accepts a PR number.
- While loading additional comment pages, keep loaded comments visible and show an in-place
  loading affordance.
- Comment page merges append timeline items in stable order without replacing or reordering
  already-loaded comments.
- Pagination failure retains loaded comments and exposes retry without losing current detail focus.

## Detail, Review, Check, and Comment Data Contract

### Minimum detail fields

- `repo_owner_name`
- `number`
- `title`
- `state`
- `is_draft`
- `author_login`
- `created_at`
- `updated_at`
- `head_ref`
- `base_ref`
- `labels[]`
- `assignees[]`
- `milestone | null`
- `body`
- `external_url` (display-only)
- `review_decision`
- `checks_status`

### Minimum review fields

- `author_login`
- `state` (`approved`, `changes_requested`, `commented`, `pending`, `dismissed`)
- `submitted_at`
- `body | null`

### Minimum check fields

- `name`
- `status` (`pending`, `success`, `failure`, `neutral`)
- `conclusion`
- `url | null` (display-only)

### Minimum comment fields

- `comment_id`
- `author_login`
- `created_at`
- `edited_at | null`
- `body`

### Markdown display contract

- Body, comment, and review-body content is displayed as terminal-friendly rendered markdown text.
- Inline composer edits underlying markdown source text.

### Viewport and overflow contract

- The detail viewport height is provided to the detail component as a prop derived from the typed
  layout module; the component does not independently read `crossterm::size()` for scroll math.
- Maximum scroll offset is derived from the actual rendered content length, not a per-line
  heuristic estimate.

## Inline Mutation Contract

### Mutable-control exclusivity

- At most one mutable inline control (the comment composer) is active at any time.

### New comment/reply

- Inline composer only.
- The new-comment field is always present in PR detail.
- `c` opens the new-comment composer and sets subfocus to the new-comment field; the viewport
  auto-scrolls to reveal the composer.
- The reply field appears under a focused comment on `r`.
- Save: `Cmd+Enter` (macOS) / `Ctrl+Enter` (non-mac).
- Cancel: `Esc`.
- After successful create, the viewport follows so the new comment is visible.

### Reply semantics for GitHub pull requests

- A reply posts as a standard issue-style PR conversation comment (flat-thread model).
- The composer pre-fills the target mention (`@author`) for reply context.

### Read-only items

- Review and check items are not editable; `c`/`r`/`e` targeting them is a no-op with a hint.
- PR body editing is out of scope in v1.

## Send-to-Agent Contract

### Trigger

- `S` from `pr_detail`, only when no active inline composer.
- If `inline_composer` is active, `S` is consumed by inline-control precedence and does not
  trigger send-to-agent.

### Eligibility

- Existing agents only.
- No agent creation flow.

### Agent chooser interaction

- `Up/Down`: move target selection.
- `Enter`: confirm target and send.
- `Esc`: cancel chooser.

### Payload minimum

- repository identifier
- PR number/title/body
- branch info (`head_ref`, `base_ref`)
- review-state summary and check summary
- PR metadata needed for context
- focused comment body if comment-focused at trigger time
- `issue_base_prompt`

The launched agent is pointed at a written `.jefe/pr-prompt.md` file.

### No-agent condition

- Disable send controls.
- A trigger attempt shows a no-agent message.

## Repository Config Reuse

- PR Mode reuses the existing `github_repo` slug and `issue_base_prompt` repository config fields.
- No new persisted repository config field is introduced.
- `prs_state` is transient and excluded from persistence (the persisted-state mapping ignores it,
  equivalent to a serde default if it were ever serialized).

## Async I/O Contract

- All `gh` CLI calls run off the UI thread via the established async wrapper
  (`spawn_gh_task_with_panic`).
- Loaders set a loading flag, spawn the task, and deliver results back as `PullRequestsMessage`
  data events applied through the reducer.
- A panicking background task clears the relevant loading flag and surfaces an error.

## Auth and Failure Contract

### v1 auth source

- Active `gh` CLI auth context.

### Auth failure

- Block PR operations and show remediation guidance.

### Repository config failure

- Invalid/missing `github_repo` slug yields a scoped configuration message, not a request error.

### Non-auth failures

- For network/API/rate-limit/repo-access errors: show a scoped pane error, keep mode/focus
  stable, preserve drafts where feasible, expose retry, and never swallow the error silently.

## Empty-State Contract

- No accessible repositories.
- No PRs matching criteria.
- No reviews on the selected PR.
- No checks on the selected PR.
- No comments on the selected PR.
- No available agents for send.

## Open-in-Browser Contract

- Merge/approve/request-changes/review-submission are out of scope in-app (handoff to the browser).
- `external_url` is rendered display-only (never edited in-app).
- `o` is a real, fully-routed feature: key → `AppEvent::PrOpenInBrowser` → message-bus
  `PullRequestsMessage::OpenInBrowser` → `dispatch_pr_open_in_browser` spawns `gh pr view <number>
  --repo <owner>/<name> --web` via `spawn_gh_task_with_panic` (off the UI thread, through the
  `GhClient::open_pull_request_in_browser` boundary method). Success → `PrOpenedInBrowser` notice;
  failure → `PrOpenInBrowserFailed` scoped error; no selection → `NoSelectionToOpen` notice. The
  reducer half (`apply_pr_open_in_browser`) is PURE — it sets a transient notice only; all I/O is at
  the dispatch boundary. No in-app merge/approve mutation exists.

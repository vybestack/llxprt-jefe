# Technical Overview — Issues Mode Contract Specification

## Scope

Defines acceptance-testable technical behavior for Issues Mode.

No implementation sequencing is included.

## Top-Level Modes

- `dashboard_agents`
- `dashboard_issues`

### Transitions

- `i` from non-issues context -> `dashboard_issues`, `focus=issue_list`.
- `i` in `dashboard_issues` -> keep mode, set `focus=issue_list`.
- `a` in `dashboard_issues` -> `dashboard_agents`.
- `Esc` in `dashboard_issues` -> precedence chain; exits mode only when no higher-priority cancel target exists.

## Key Routing Contract

Resolution order in `dashboard_issues`:

1. `inline_editor` / `inline_composer`
2. Focus-domain handlers (`repo_list`, `issue_list`, `issue_detail`, `comment_item`, `search_input`, `filter_controls`, `agent_chooser`)
3. Issues-global handlers (`i`, `a`, `Esc`, `S`)
4. Dashboard-global handlers (if not consumed)

### Required suppression/override while in issues mode

- Suppress dashboard `a` focus-agents handler.
- Suppress dashboard `s/S` split-mode handler.
- Suppress split-mode `Esc` handler.
- Suppress dashboard destructive lifecycle keys: `Ctrl-d`, `Ctrl-k`, `l`.
- Route `/` to issue-list search input only.
- Route `?`, `h`, `F1` to help with Issues Mode bindings included.
- Lowercase `s` is an explicit no-op in Issues Mode.

## Focus-Domain Contract

### Domains

- `repo_list`
- `issue_list`
- `issue_detail`
- `comment_item`
- `search_input`
- `inline_editor`
- `inline_composer`
- `filter_controls`
- `agent_chooser`

### Inter-pane focus

- In `dashboard_issues`, this cycle supersedes dashboard pane-cycle behavior while mode is active.
- `Tab`: `repo_list -> issue_list -> issue_detail -> repo_list`
- `Shift+Tab`: reverse

### Intra-pane navigation

- `repo_list`: `Up/Down` moves active repository; selection change is immediately scoped.
- `repo_list`: `Enter` is explicit no-op (no commit action).
- `issue_list`: `Up/Down`, `PageUp/PageDown`, `Home/End`.
- `issue_detail`: `Up/Down` scroll.
- `issue_detail` subfocus:
  - with comments: `body -> comment_item(s) -> new_comment_field -> body`
  - no comments: `body -> new_comment_field -> body`
- `Shift+Tab` in `issue_detail` traverses the same subfocus graph in reverse order.
- `r` requires `focus=comment_item`; otherwise no-op with non-blocking hint.

## Exit-Focus Restoration Contract

On exit from `dashboard_issues` to `dashboard_agents`, restore prior agent focus only if all are true:

1. prior focus token exists,
2. referenced target still exists,
3. target is focusable in current state.

Otherwise, fall back to default agent-list focus.

## Repository Scope Contract

### Source of truth

- `selected_repository_id` from `repo_list` is authoritative issue scope key.

### Scope-change effects

On repository change:

- Discard/ignore in-flight list/detail/comment requests for prior scope.
- Start list query for new scope immediately.
- Ensure detail/comments only show data from current scope.
- If prior selected issue is missing in new scope, reseat selection by selection rules.
- Discard unsent inline drafts (`inline_composer`, `inline_editor`) from prior repository scope and emit non-blocking notice; drafts are not migrated across scope.

## Query, Sort, Filter, Search

### Minimum list fields

- `number`
- `title`
- `state`
- `author_login`
- `updated_at`
- `assignee_summary`
- `labels_summary`
- `comment_count`

### Default ordering

- `updated_at desc`, tie-breaker `number asc`.

### Filter/search inputs

- `query_text`
- `state in {open, closed, all}`
- `author`
- `assignee`
- `labels[]`
- `mentioned`
- `updated_before`
- `updated_after`

### Composition

- Structured filters AND-composed.
- Labels require all selected labels (AND).
- Text query AND-composed with structured filters.

### `f` list-only behavior

Precondition: `focus=issue_list`.

- Pass: open `filter_controls`.
- Fail: no-op.

`filter_controls` operations:

- Default committed state: no structured criteria set.
- Apply: commit draft criteria and refresh scoped list.
- Clear: remove committed criteria and refresh scoped list.
- Cancel: close controls without committing draft criteria.

### Search behavior

- `/` focuses `search_input`.
- `Enter` in `search_input` applies `query_text`.
- `Esc` in `search_input` with non-empty query clears query and remains focused.
- `Esc` in `search_input` with empty query blurs `search_input` to `issue_list`.

## Selection and Loading Rules

- First non-empty list load selects first issue and loads detail/comments.
- On filter/search/sort change:
  - keep selected issue if still present,
  - else select first issue,
  - else no selection and scoped detail empty state.
- Detail/comments must never show stale prior-scope data.

### Loading-state contract

- Show scoped loading state while list data is loading.
- Show scoped loading state while detail/comments for selected issue are loading.

### Pagination contract

- Issue list is lazy-loaded/paginated.
- Trigger: when selection reaches last loaded row and more pages are available, fetch next page.
- Preserve scope, sort, and committed filters/search across page loads.
- Repository switch invalidates prior scope paging cursor/state.
- Comments timeline supports incremental loading/pagination when additional comments exist.
- While loading additional comment pages, keep loaded comments visible and show in-place loading affordance.
- Comment page merges append timeline items in stable order without replacing or reordering already loaded comments.
- Pagination failure retains loaded comments and exposes retry without losing current detail focus.

## Detail and Comment Data Contract

### Minimum detail fields

- `repo_owner_name`
- `number`
- `title`
- `state`
- `author_login`
- `created_at`
- `updated_at`
- `labels[]`
- `assignees[]`
- `milestone | null`
- `body`
- `external_url` (display-only)

### Minimum comment fields

- `comment_id`
- `author_login`
- `created_at`
- `edited_at | null`
- `body`

### Markdown display contract

- Body and comment content is displayed as terminal-friendly rendered markdown text.
- Inline editing edits underlying markdown source text.

## Inline Mutation Contract

### Mutable-control exclusivity

- `inline_editor` and `inline_composer` are mutually exclusive.
- At most one mutable inline control is active at any time.

### New comment/reply

- Inline composer only.
- New-comment field is always present in issue detail.
- Reply field appears under focused comment on `r`.
- Save: `Cmd+Enter` (macOS) / `Ctrl+Enter` (non-mac).
- Cancel: `Esc`.

### Reply semantics for GitHub issues

- Reply posts as a standard issue comment in flat-thread model.
- Composer pre-fills target mention (`@author`) for reply context.

### Edit issue body/comment

- `e` on focused editable target opens `inline_editor`.
- Save: `Cmd+Enter` / `Ctrl+Enter`.
- Cancel: `Esc`.
- Non-editable target rejects `e` with non-blocking hint.

## Send-to-Agent Contract

### Trigger

- `S` from `issue_detail`, only when no active inline editor/composer.
- If `inline_editor` or `inline_composer` is active, `S` is consumed by inline-control precedence and does not trigger send-to-agent.

### Eligibility

- Existing agents only.
- No agent creation flow.

### Agent chooser interaction

- `Up/Down`: move target selection.
- `Enter`: confirm target and send.
- `Esc`: cancel chooser.

### Payload minimum

- repository identifier
- issue number/title/body
- issue metadata needed for context
- focused comment body if comment-focused at trigger time
- `issue_base_prompt`

### No-agent condition

- Disable send controls.
- Trigger attempt shows no-agent message.

## Repository Config: `issue_base_prompt`

- Canonical identifier: `issue_base_prompt`.
- Purpose: repository-scoped reusable base instruction text included in send payload composition.
- Editing surface: existing repository config screen as an added multiline field.
- Controls: Save and Reset.
- Reset behavior: restore last-saved persisted value.
- Empty value allowed.
- Persist via the existing repository configuration persistence path/mechanism (same store and lifecycle as other repository config fields).

## Auth and Failure Contract

### v1 auth source

- Active `gh` CLI auth context.

### Auth failure

- Block issue operations and show remediation guidance.

### Non-auth failures

- For network/API/rate-limit/repo-access errors: show scoped pane error, keep mode/focus stable, preserve drafts where feasible, and expose retry.

## Empty-State Contract

- No accessible repositories.
- No issues matching criteria.
- No comments on selected issue.
- No available agents for send.

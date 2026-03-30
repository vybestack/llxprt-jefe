# Functional Overview — Issues Mode Contract

## Purpose

Define acceptance-level user behavior for repository-scoped GitHub issues in Jefe.

This is a functional specification only. It does not include implementation planning.

## Core Invariants

- `i` enters Issues Mode.
- If already in Issues Mode, `i` refocuses the Issue List.
- Issues Mode persists until `a` or `Esc` exits to Agents Mode.
- Repositories remain visible and selectable on the left in Issues Mode.
- Issue list, detail, and comments are always scoped to the selected repository.
- Changing repository selection updates issue scope immediately.

## Agents Mode Definition

- Agents Mode is the existing dashboard agent workflow where issue-specific key behavior is inactive.
- Exiting Issues Mode returns to Agents Mode.
- Prior agent focus is restored only when all are true:
  - a pre-issues focus target exists,
  - that target still exists,
  - that target is currently focusable.
- If any validity check fails, focus falls back to the agent list.

## Key Routing and Conflict Resolution

While Issues Mode is active, key handling precedence is:

1. Inline editor/composer controls
2. Focused-pane controls
3. Issues Mode global controls
4. Dashboard-global controls (only if not claimed above)

### Explicit binding overrides in Issues Mode

- `a` exits Issues Mode (dashboard `a` pane-focus behavior is suppressed).
- `S` triggers send-to-agent from Issue Detail (dashboard `s/S` split binding is suppressed).
- `s` (lowercase) is a no-op in Issues Mode.
- `Esc` follows Issues Mode precedence first (split `Esc` behavior suppressed while Issues Mode is active).
- `Ctrl-d`, `Ctrl-k`, and `l` are disabled in Issues Mode to prevent destructive agent lifecycle actions.

### Help and search keys in Issues Mode

- `?`, `h`, `F1` open help and must include Issues Mode keybindings.
- `/` always targets issue-list search while in Issues Mode (dashboard search binding is not used in Issues Mode).

## Esc Precedence

When `Esc` is pressed in Issues Mode:

1. Cancel active inline edit/composer.
2. Else cancel active send-to-agent chooser.
3. Else if search input is focused and non-empty, clear search text and keep search focused.
4. Else if search input is focused and empty, blur search input and keep Issues Mode active.
5. Else close active transient controls (for example filter controls).
6. Else exit Issues Mode.

## Pane Focus and Navigation

### Inter-pane focus

- In Issues Mode, pane cycling supersedes dashboard pane cycling.
- `Tab`: Repository List -> Issue List -> Issue Detail -> Repository List.
- `Shift+Tab`: reverse order.

### Repository List focus

- `Up/Down`: move repository selection.
- Scope update occurs immediately on selection movement.
- `Enter`: explicit no-op; there is no additional commit step because selection is already active.

### Issue List focus

- `Up/Down`: move issue selection.
- `PageUp/PageDown`: scroll list page.
- `Home/End`: jump to start/end of loaded list.
- `Enter`: focus Issue Detail for selected issue.
- `f`: open filter controls (list-focus-only; no-op elsewhere).
- `/`: focus search input.

### Issue Detail focus

- `Up/Down`: scroll detail content.
- `Tab` subfocus order:
  - when comments exist: body -> comment items -> new-comment field -> body
  - when no comments exist: body -> new-comment field -> body
- `Shift+Tab` reverses the same subfocus order within Issue Detail.
- `r` on focused comment opens inline reply field for that comment.
- `r` when focus is not on a comment is a no-op with non-blocking hint.

## Issue List Contract

Each issue row displays:

- Number
- Title
- State (`open` or `closed`)
- Author login
- Updated timestamp
- Assignee summary
- Label summary
- Comment count

### Sorting

- Default sort: `updated desc`.
- Tie-breaker: `number asc`.

### Selection and detail loading

- On first non-empty load, first issue is selected and detail auto-loads.
- On filter/search/sort change, keep current selection if still present; else select first row.
- If list is empty, detail shows scoped empty state (never stale prior-repository data).

### Pagination and lazy loading

- Lists are paginated/lazy-loaded.
- When selection reaches the last loaded row and more results exist, next page loads automatically.
- Repository switch invalidates prior repository paging context.

### Loading states

- While list data is loading, show an in-scope list loading state.
- While detail/comments are loading for the selected issue, show an in-scope detail loading state.
- Comments timeline supports incremental loading/pagination when additional comments exist.
- While additional comment pages are loading, keep already loaded comments visible and show in-place loading affordance.
- Comment pagination appends older timeline items in stable order, without replacing or reordering already rendered loaded comments.
- If comment pagination fails, keep previously loaded comments and show scoped retry affordance.

## Filtering and Search Contract

### Supported criteria

- Text query (matches issue title and body)
- State (`open`, `closed`, `all`)
- Author
- Assignee (`me` supported)
- Labels (multi-select)
- Mentioned (`me`)
- Updated date bounds (`before`, `after`)

### Composition

- Structured filters are AND-composed.
- Label matching uses AND across selected labels.
- Text query is AND-composed with structured filters.

### Control behavior

- `f` opens filter controls only from Issue List focus.
- Default committed filter state on entry is unset/empty for all structured criteria.
- Apply commits criteria and refreshes scoped list.
- Clear removes committed criteria and refreshes scoped list.
- Cancel closes controls without committing draft edits.

### Search behavior

- `/` focuses issue-list search input.
- `Enter` applies current search query.
- Clearing query text restores list results constrained only by committed structured filters.

## Issue Detail and Comment Contract

Issue detail displays:

- Repository owner/name
- Number and title
- State
- Author
- Created/updated timestamps
- Labels
- Assignees
- Milestone (optional)
- Body/description
- Open-in-GitHub link (displayed as a URL only; not activatable via keybinding)
- Comments timeline (as a detail sub-region)

Each comment displays:

- Author
- Created timestamp
- Edited indicator/timestamp (if edited)
- Body

## Inline Create/Edit Contract

No comment modal flow is allowed.

### Mutable-control exclusivity

- At most one mutable inline control may be active at a time.
- Inline editor and inline composer cannot be active simultaneously.

### New comment and reply

- New-comment field is always available inline in Issue Detail.
- Reply field appears inline beneath focused comment when `r` is used.
- Save: `Cmd+Enter` (macOS) or `Ctrl+Enter` (non-mac).
- Cancel: `Esc`.

### Reply semantics for GitHub issues

- GitHub issues are flat comments.
- Reply composer pre-fills an `@author` mention for the target comment.
- Submitted comment body is exactly the composer text at submit time.

### Inline editing

- `e` edits focused issue body or focused comment.
- Save: `Cmd+Enter` or `Ctrl+Enter`.
- Cancel: `Esc`.
- Non-editable target + `e` results in no-op with non-blocking hint.

## Send-to-Agent Contract

### Trigger and eligibility

- Trigger: `S` from Issue Detail when no inline editor/composer is active.
- If inline editor/composer is active, `S` is handled by the active inline control context and does not trigger send-to-agent.
- Eligible targets: existing agents only.
- Creating a new agent is not part of this flow.

### Chooser interaction

- `Up/Down`: move agent selection.
- `Enter`: confirm selected agent and send.
- `Esc`: cancel send flow.

### Payload

- Repository identifier
- Issue number/title/body
- Relevant issue metadata for context
- Focused comment text if send is triggered while a comment is focused
- `issue_base_prompt`

### No-agent behavior

- Send action is disabled when there are no agents.
- Trigger attempt shows non-blocking no-agent message.

## `issue_base_prompt` Contract

- Canonical name: `issue_base_prompt`.
- Purpose: repository-specific reusable base instruction text included in send-to-agent payloads.
- Editing location: existing repository configuration screen as an additional multiline field.
- Editing form: multiline field with explicit Save and Reset controls.
- Reset restores last-saved persisted value.
- Empty value is valid.
- Persistence uses the existing repository configuration persistence path/mechanism; no separate storage location is introduced.

## Authentication and Error States

### Authentication (v1)

- Uses active `gh` CLI auth context.
- Missing/invalid auth blocks issue operations and shows explicit remediation guidance.

### Non-auth errors

For network/API/rate-limit/repository-access failures:

- Show scoped error in list/detail.
- Keep mode and focus stable.
- Preserve unsaved drafts where feasible.
- On repository scope change, unsent inline drafts (new comment/reply/edit) are not carried across repositories and are discarded with a non-blocking notice.
- Provide retry affordance.

## Empty States

- No repositories accessible in auth context.
- No issues matching current scoped criteria.
- No comments on selected issue.
- No available agents for send-to-agent.

## Out of Scope

- Creating a new agent from send-to-agent.
- Modal comment/reply flow.

# Issues Mode — Specification

Plan ID: `PLAN-20260329-ISSUES-MODE`

## Purpose

Add a GitHub Issues browsing, interaction, and send-to-agent workflow to Jefe, scoped per selected repository. Issues Mode introduces a new top-level dashboard mode (`dashboard_issues`) alongside the existing Agents Mode (`dashboard_agents`), with full key routing, inline mutation, filtering/search, pagination, and agent integration.

## Strategy Contract

1. **Extend existing architecture** — add `DashboardIssues` mode to `ScreenMode`/`AppState`, introduce a separate `IssueFocus` enum for issues-mode focus tracking (do NOT modify `PaneFocus`), and route keys through the established `InputMode` dispatch chain.
2. **Reuse existing patterns** — follow the domain/state/event/UI layering established in firstversion; no parallel architecture forks.
3. **GitHub API via `gh` CLI** — use the authenticated `gh` CLI as the GitHub API transport (no direct REST/GraphQL client library in v1).
4. **Modify, don't fork** — update `src/app_input/`, `input.rs`, `src/state/types.rs` + `src/state/mod.rs`, `domain/mod.rs`, and UI modules directly.

---

## Architectural Boundaries

| Layer | Ownership |
|-------|-----------|
| Domain | Issue/Comment entities, `IssueBasePrompt` field on Repository |
| State | `IssuesState` aggregate, issue-specific events/reducer, focus domains |
| GitHub Client | `gh` CLI wrapper boundary for list/detail/comment/mutation operations |
| UI | Issue list pane, issue detail pane, inline composer/editor, filter controls, agent chooser |
| Persistence | `issue_base_prompt` persisted in existing repository config path |

Forbidden couplings:
- UI must not call `gh` CLI directly; must go through GitHub client boundary.
- GitHub client boundary must not mutate `AppState` directly; must emit typed events.
- Inline editor/composer must not bypass state event reducer.

---

## Data Contracts and Invariants

### Issue (list row)
- `number: u64`
- `title: String`
- `state: IssueState` (open | closed)
- `author_login: String`
- `updated_at: String`
- `assignee_summary: String`
- `labels_summary: String`
- `comment_count: u64`

### Issue (detail)
- All list fields plus:
- `repo_owner_name: String`
- `created_at: String`
- `labels: Vec<String>`
- `assignees: Vec<String>`
- `milestone: Option<String>`
- `body: String`
- `external_url: String`

### Comment
- `comment_id: u64`
- `author_login: String`
- `created_at: String`
- `edited_at: Option<String>`
- `body: String`

### IssueBasePrompt
- `issue_base_prompt: String` field on `Repository`
- Empty value is valid
- Persisted via existing repository config persistence path

### Invariants
- Issue list and detail are always scoped to `selected_repository_id`.
- Repository scope change invalidates all prior issue list/detail/comment/pagination state.
- At most one inline mutable control (editor OR composer) active at a time.
- Detail/comments must never display data from a prior repository scope.
- Unsent inline drafts are discarded (with notice) on repository scope change.

---

## Integration Points with Existing Modules

| Module | Integration |
|--------|-------------|
| `src/state/types.rs` | Add `IssuesState`, issue events to `AppEvent`, `DashboardIssues` to `ScreenMode`, issue focus domains to `PaneFocus` (re-exported via `src/state/mod.rs`) |
| `src/domain/mod.rs` | Add `Issue`, `IssueComment`, `IssueState`, `IssueFilter` types; add `issue_base_prompt` to `Repository` |
| `src/input.rs` | Add `InputMode::Issues*` variants or extend routing for issues mode |
| `src/app_input/mod.rs` (main crate) | Add issues-mode key dispatch, `handle_issues_key`, suppression rules |
| `src/persistence/mod.rs` | Add `issue_base_prompt` to persisted `Repository`; no new persistence file |
| `src/ui/` | New issue list, issue detail, inline composer/editor, filter controls, agent chooser components |
| `src/lib.rs` | Add `pub mod github;` for GitHub client boundary |

---

## Functional Requirements

### REQ-ISS-001: Mode Entry and Exit
- `i` from non-issues context enters `dashboard_issues` with `focus=issue_list`.
- `i` while already in `dashboard_issues` refocuses `issue_list`.
- `a` from `dashboard_issues` exits to `dashboard_agents`.
- `Esc` follows issues-mode precedence chain; exits mode only when no higher-priority cancel target exists.

Behavior contract:
- GIVEN user is in Agents Mode
- WHEN `i` is pressed
- THEN mode transitions to `dashboard_issues` with issue list focused

- GIVEN user is in Issues Mode with no active inline controls
- WHEN `a` is pressed
- THEN mode transitions to `dashboard_agents` and prior agent focus is restored if valid

- GIVEN user is in Issues Mode with an active inline editor
- WHEN `Esc` is pressed
- THEN the inline editor is cancelled; mode remains `dashboard_issues`

### REQ-ISS-002: Key Routing and Suppression
- While in Issues Mode: suppress dashboard `a` focus-agents, `s/S` split-mode, split-mode `Esc`, destructive lifecycle keys (`Ctrl-d`, `Ctrl-k`, `l`).
- Route `/` to issue-list search; `?`/`h`/`F1` to help with Issues Mode bindings.
- Lowercase `s` is explicit no-op in Issues Mode.

Behavior contract:
- GIVEN user is in Issues Mode
- WHEN `Ctrl-d` is pressed
- THEN the key is consumed as no-op (agent destructive action suppressed)

- GIVEN user is in Issues Mode with issue list focused
- WHEN `/` is pressed
- THEN search input is focused for issue-list search

### REQ-ISS-003: Pane Focus and Navigation
- Issues Mode pane cycle: `repo_list -> issue_list -> issue_detail -> repo_list` (Tab/Shift+Tab).
- Repository list: `Up/Down` moves selection; scope updates immediately.
- Issue list: `Up/Down`, `PageUp/PageDown`, `Home/End`, `Enter` focuses detail.
- Issue detail: `Up/Down` scroll; Tab subfocus cycle through body/comments/new-comment.
- `r` on focused comment opens inline reply; `r` elsewhere is no-op with hint.

Behavior contract:
- GIVEN user is in Issues Mode with repo_list focused
- WHEN `Down` is pressed and next repository exists
- THEN repository selection moves down and issue list reloads for new scope

- GIVEN user is in Issues Mode with issue_detail focused on a comment
- WHEN `r` is pressed
- THEN inline reply composer opens pre-filled with `@author` mention

### REQ-ISS-004: Esc Precedence Chain
1. Cancel active inline edit/composer.
2. Cancel active send-to-agent chooser.
3. If search input focused and non-empty: clear search text, keep search focused.
4. If search input focused and empty: blur search input, keep Issues Mode.
5. Close active transient controls (filter controls).
6. Exit Issues Mode.

Behavior contract:
- GIVEN search input is focused with query text "bug"
- WHEN `Esc` is pressed
- THEN search text is cleared; search input remains focused; mode stays `dashboard_issues`

### REQ-ISS-005: Exit-Focus Restoration
- On exit from Issues Mode, restore prior agent focus only if: token exists, target still exists, target is focusable.
- Otherwise fall back to default agent-list focus.

Behavior contract:
- GIVEN user had agent "bot-1" selected before entering Issues Mode
- WHEN user exits Issues Mode and "bot-1" still exists
- THEN agent focus is restored to "bot-1"

### REQ-ISS-006: Issue List Display and Sorting
- Each row: number, title, state, author, updated timestamp, assignee summary, label summary, comment count.
- Default sort: `updated_at desc`, tie-breaker `number asc`.
- First non-empty load selects first issue and auto-loads detail.
- On filter/search change: keep selection if present; else select first.
- Empty list shows scoped empty state.

Behavior contract:
- GIVEN repository "acme/api" has 5 open issues
- WHEN Issues Mode enters with "acme/api" selected
- THEN issue list displays 5 issues sorted by updated desc; first issue is selected; detail loads

### REQ-ISS-007: Pagination and Lazy Loading
- Lists are paginated/lazy-loaded.
- When selection reaches last loaded row and more exist, next page loads automatically.
- Repository switch invalidates prior paging context.
- Comment timeline supports incremental loading/pagination.
- Comment pagination appends in stable order without reordering loaded comments.
- Comment pagination failure retains loaded comments and exposes retry.

Behavior contract:
- GIVEN issue list has 30 issues, page size is 20, user has loaded first page
- WHEN user navigates to issue at position 20
- THEN next page loads automatically; list grows to 40 items (or remaining)

### REQ-ISS-008: Filtering and Search
- Supported: text query, state, author, assignee, labels (multi AND), mentioned, updated date bounds.
- Structured filters AND-composed; text query AND-composed with structured filters.
- `f` opens filter controls (issue-list focus only).
- Apply/Clear/Cancel behavior for filter controls.
- `/` focuses search; Enter applies; Esc clears or blurs per precedence.

Behavior contract:
- GIVEN user opens filter controls and sets state=open and label=bug
- WHEN Apply is pressed
- THEN issue list refreshes showing only open issues with "bug" label

### REQ-ISS-009: Issue Detail and Comments
- Detail displays: repo owner/name, number, title, state, author, timestamps, labels, assignees, milestone, body, external URL (displayed as a URL only; not activatable via keybinding), comments timeline.
- Each comment: author, created, edited indicator, body.
- Markdown displayed as terminal-friendly rendered text.

Behavior contract:
- GIVEN issue #142 is selected with 5 comments
- WHEN issue detail loads
- THEN all detail fields and comments timeline are displayed; `external_url` is shown as a display-only URL

### REQ-ISS-010: Inline Create/Edit
- No modal flow; all inline.
- New-comment field always present in detail.
- Reply field appears under focused comment on `r` with `@author` pre-fill.
- `e` edits focused issue body or comment.
- Save: `Cmd+Enter`/`Ctrl+Enter`. Cancel: `Esc`.
- Mutable-control exclusivity: at most one active at a time.

Behavior contract:
- GIVEN user focuses a comment by @pat in issue detail
- WHEN `r` is pressed
- THEN inline reply composer opens with "@pat " pre-filled; save submits new comment

- GIVEN inline composer is active
- WHEN `e` is pressed on issue body
- THEN `e` is consumed by active control; inline editor does NOT open (exclusivity)

### REQ-ISS-011: Send-to-Agent
- `S` from issue detail when no inline control active opens agent chooser.
- Chooser: Up/Down, Enter to confirm, Esc to cancel.
- Payload: repo identifier, issue number/title/body, metadata, focused comment (if any), `issue_base_prompt`.
- No-agent state: disable send, show message.

Behavior contract:
- GIVEN user is viewing issue #142 with comment by @pat focused
- WHEN `S` is pressed and agents exist
- THEN agent chooser opens; on Enter with "backend-owner" selected, payload includes issue data + @pat comment + issue_base_prompt

### REQ-ISS-012: Repository Config `issue_base_prompt`
- Multiline field in existing repository config screen.
- Save and Reset controls.
- Reset restores last-saved value.
- Empty value valid.
- Persisted via existing repository config persistence path.

Behavior contract:
- GIVEN user opens repository config for "acme/api"
- WHEN `issue_base_prompt` field is edited and Save is pressed
- THEN value persists and is included in subsequent send-to-agent payloads

### REQ-ISS-013: Authentication and Error Handling
- v1: uses active `gh` CLI auth context.
- Missing/invalid auth blocks operations with remediation guidance.
- Non-auth errors: scoped error in list/detail, stable mode/focus, draft preservation, retry affordance.
- Repository scope change discards unsent drafts with non-blocking notice.

Behavior contract:
- GIVEN `gh` CLI is not authenticated
- WHEN user enters Issues Mode
- THEN issue operations are blocked; remediation guidance ("Run: gh auth login") is shown

- GIVEN user has unsent draft comment and switches repository
- WHEN repository scope changes
- THEN draft is discarded; non-blocking notice is shown

### REQ-ISS-014: Empty States
- No accessible repositories: explicit message.
- No issues matching criteria: scoped empty state.
- No comments: "No comments yet" display.
- No agents for send: disable send + message.

Behavior contract:
- GIVEN selected repository has no issues
- WHEN Issues Mode loads
- THEN issue list shows "No issues match current filters"; detail shows scoped empty state

---

## Non-Functional Requirements

### REQ-ISS-NFR-001: Responsiveness
- Issue list and detail loading must not block keyboard input.
- Loading states shown during API operations.

### REQ-ISS-NFR-002: Reliability
- API failures must not crash the application.
- Mode and focus remain stable through errors.

### REQ-ISS-NFR-003: Maintainability
- GitHub client boundary is isolated and testable.
- Issue state management follows existing event/reducer pattern.

---

## Testability Requirements

- All key routing paths testable via `AppState::apply()` event tests.
- GitHub client boundary mockable for unit tests.
- Issue list selection/pagination logic testable without API calls.
- Inline editor/composer exclusivity testable via state assertions.
- Filter composition testable as pure logic.

---

## Error/Edge Case Expectations

- `gh` CLI not installed: block operations, show install guidance.
- API rate limit: scoped retry affordance.
- Repository has no issues: empty state, not error.
- Issue deleted between list and detail load: handle gracefully with scoped message.
- Very long issue body/comments: scroll behavior, no truncation without affordance.
- Concurrent scope change during API call: discard stale response.

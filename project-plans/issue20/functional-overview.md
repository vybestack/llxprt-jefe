# Functional Overview — Pull Requests Mode Contract

Plan ID: `PLAN-20260624-PR-MODE`

## Purpose

Define acceptance-level user behavior for repository-scoped GitHub pull requests in Jefe.

This is a functional specification only. It does not include implementation planning. It mirrors
the Issues Mode functional contract, adapted for pull requests (review summary, CI/check summary,
branch info) and the deliberate deferral of complex operations (merge/approve/review submission)
to the browser via an external URL.

## Core Invariants

- `p` enters PR Mode.
- If already in PR Mode, `p` refocuses the PR List.
- PR Mode persists until `a` or `Esc` exits to Agents Mode.
- Repositories remain visible and selectable on the left in PR Mode.
- PR list, detail, reviews, checks, and comments are always scoped to the selected repository.
- Changing repository selection updates PR scope immediately.
- Review and check summaries are read-only.
- Comments are the only in-app PR mutation; merge/approve/review submission are deferred to the
  browser via a display-only external URL.

## Agents Mode Definition

- Agents Mode is the existing dashboard agent workflow where PR-specific key behavior is inactive.
- Exiting PR Mode returns to Agents Mode.
- Prior agent focus is restored only when all are true:
  - a pre-PR-mode focus target exists,
  - that target still exists,
  - that target is currently focusable.
- If any validity check fails, focus falls back to the agent list.

## Key Routing and Conflict Resolution

While PR Mode is active, key handling precedence is:

1. Inline comment composer controls
2. Send-to-agent chooser controls
3. Search input controls
4. Filter controls
5. PR-Mode global unwind/mode controls (`p`, `a`, `Esc`, help)
6. Focus-domain controls (repo list / PR list / PR detail)
7. Pane-cycle controls (`Tab`, `Shift+Tab`) — cycle the three panes from EVERY pane (issue #46)
8. Suppressed dashboard/destructive keys consumed as no-op

### Explicit binding overrides in PR Mode

- `a` exits PR Mode (dashboard `a` pane-focus behavior is suppressed).
- `S` triggers send-to-agent from PR Detail (dashboard `s/S` split binding is suppressed).
- `s` (lowercase) is a no-op in PR Mode.
- `Esc` follows PR-Mode precedence first (split `Esc` behavior suppressed while PR Mode is active).
- `Ctrl-d`, `Ctrl-k`, and `l` are disabled in PR Mode to prevent destructive agent lifecycle
  actions.

### Help and search keys in PR Mode

- `?`, `h`, `F1` open help and must include PR-Mode keybindings.
- `/` always targets PR-list search while in PR Mode (dashboard search binding is not used in PR
  Mode).

## Esc Precedence

When `Esc` is pressed in PR Mode:

1. Cancel the active inline comment composer.
2. Else cancel the active send-to-agent chooser.
3. Else if search input is focused and non-empty, clear search text and keep search focused.
4. Else if search input is focused and empty, blur search input and keep PR Mode active.
5. Else close active transient controls (for example filter controls).
6. Else exit PR Mode.

Transient notices and loading indicators do not consume `Esc`; if no higher-priority mutable
control is active, `Esc` exits PR Mode.

## Pane Focus and Navigation

### Inter-pane focus

- In PR Mode, pane cycling supersedes dashboard pane cycling.
- `Tab`: Repository List -> PR List -> PR Detail -> Repository List.
- `Shift+Tab`: reverse order.

### Repository List focus

- `Up/Down`: move repository selection.
- Scope update occurs immediately on selection movement.
- This navigation is driven by the PR-Mode focus model (`PrFocus::RepoList`), NOT by the
  dashboard `pane_focus` field; Up/Down MUST move the repository selection whenever the PR-Mode
  repo list is focused.
- `Enter`: explicit no-op; selection is already active.

### PR List focus

- `Up/Down`: move PR selection (selection-following keeps the selected row visible).
- `PageUp/PageDown`: move selection by page within the currently loaded list window.
- `Home/End`: jump selection to the start/end of the currently loaded list.
- Reaching the last loaded row while more results exist triggers cursor-pagination fetch; this is
  separate from viewport/page navigation.
- `Enter`: focus PR Detail for the selected PR.
- `f`: open filter controls (list-focus-only; no-op elsewhere).
- `/`: focus search input.

### PR Detail focus

- `Up/Down`: scroll the unified detail content.
- `j` subfocus-next order (skipping empty sections):
  - `body -> review items -> check items -> comment items -> new-comment field -> body`
- `k` reverses the same subfocus order.
- `Tab`/`Shift+Tab`: cycle the three panes (repo list / PR list / PR detail) from WITHIN PR Detail
  too — issue #46 requires Tab to cycle panes from every pane. They are NOT consumed for subfocus
  here (subfocus is on `j`/`k`); `Tab` is therefore the natural way to leave the detail pane.
- `Left`: OPTIONAL reverse pane-cycle back to the PR list (parity with the list pane); it is no
  longer the sole escape from detail since `Tab`/`Shift+Tab` already cycle out.
- `c`: open the new-comment composer and move subfocus to `new-comment` (consistent regardless of
  current subfocus); the viewport auto-scrolls to reveal the composer.
- `r` on a focused comment opens an inline reply field for that comment (pre-filled `@author`).
- `r` on body, a review item, a check item, or the new-comment field is a no-op with a
  non-blocking hint.
- `S`: send-to-agent chooser (only when no inline composer is active).

## PR List Contract

Each PR row displays:

- Number
- Title (truncated with an ellipsis to pane width)
- State (`open`, `closed`, `merged`) with a draft indicator when applicable
- Author login
- Updated timestamp
- Review status (`review_decision`)
- Check status (`checks_status`)
- Label summary
- Comment count

### Sorting

- Default sort: `updated desc`.
- Tie-breaker: `number asc`.

### Selection and detail loading

- On first non-empty load, the first PR is selected and detail auto-loads.
- On filter/search/sort change, keep the current selection if still present (by number); else
  select the first row.
- If the list is empty, detail shows a scoped empty state (never stale prior-repository data).

### Scrolling

- The PR list is scroll-aware. The selected row is always kept visible within the list viewport;
  selection never moves off-screen (no clipping). Selection-following is provided by a NEW shared
  list-viewport / selection-follow helper built for this feature: the issue list does NOT currently
  window its rows (it renders all rows with no offset), so there is no existing list-scroll
  infrastructure to reuse — the helper is a fresh deliverable with its own tests.
- Rendered rows match the loaded data exactly: loaded N PRs render N rows when they fit; no item
  is silently dropped from rendering, including the first and last positions.

### Pagination and lazy loading

- Lists are paginated/lazy-loaded.
- When selection reaches the last loaded row and more results exist, the next page loads
  automatically.
- Repository switch invalidates the prior repository paging context.

### Loading states

- While list data is loading, show an in-scope list loading state.
- While detail/reviews/checks/comments are loading for the selected PR, show in-scope detail
  loading states.
- Comment timeline supports incremental loading/pagination when additional comments exist.
- While additional comment pages load, keep already-loaded comments visible and show an in-place
  loading affordance.
- Comment pagination appends older timeline items in stable order, without replacing or reordering
  already-rendered comments.
- If comment pagination fails, keep previously loaded comments and show a scoped retry affordance.

## Filtering and Search Contract

### Supported criteria

- Text query (matches PR title and body)
- State (`open`, `closed`, `merged`, `all`)
- Author
- Assignee
- Reviewer
- Draft (any / drafts-only / ready-only)
- Labels (multi-select, AND across selected labels)

### Composition

- Structured filters are AND-composed.
- Label matching uses AND across selected labels.
- Text query is AND-composed with structured filters.

### Control behavior (fully interactive)

- `f` opens filter controls only from PR-List focus.
- Default committed filter state on entry is `state = open` (the default scoped PR list shows OPEN
  pull requests); all other structured criteria (author, assignee, reviewer, draft, labels, text
  query) are unset/empty. This is the same default the list loads with on mode entry.
- `Tab`/`Shift+Tab` move focus between fields.
- `Space` cycles enumerated fields (state, draft).
- Character entry edits the focused text field and updates the DRAFT filter immediately.
- Apply commits criteria and refreshes the scoped list.
- Clear resets committed criteria back to the default (`state = open`, all other criteria empty) and
  refreshes the scoped list.
- Cancel closes controls without committing draft edits.

### Search behavior

- `/` focuses the PR-list search input.
- `Enter` applies the current (trimmed) search query.
- Clearing query text restores list results constrained only by committed structured filters.

## PR Detail Contract

The PR detail is a single unified scrollable view containing, in order:

1. Metadata header:
   - Repository owner/name
   - Number and title
   - State (with draft indicator)
   - Author
   - Created/updated timestamps
   - Branch info: `head_ref -> base_ref`
   - Labels
   - Assignees
   - Milestone (optional)
   - Open-in-GitHub link (display-only URL; not activatable via keybinding)
2. Description body
3. Review-state summary:
   - Aggregate `review_decision` line
   - Per-review items: author, state, submitted timestamp, optional summary body
4. CI/check summary:
   - Aggregate `checks_status` rollup line
   - Per-check items: name, status, conclusion
5. Comments timeline (as a detail sub-region)
6. New-comment field (always present at the bottom of the unified scroll)

Each comment displays:

- Author
- Created timestamp
- Edited indicator/timestamp (if edited)
- Body

The detail viewport height is supplied as a prop derived from the typed layout module; scroll
overflow is derived from the actual rendered content length.

## Inline Comment/Reply Contract

No comment modal flow is allowed. Reviews and checks are read-only and not editable in-app.

### Mutable-control exclusivity

- At most one mutable inline control may be active at a time.

### New comment and reply

- The new-comment field is always available inline at the bottom of PR detail.
- `c` opens the new-comment composer and sets subfocus to the new-comment field; the viewport
  auto-scrolls so the composer is visible.
- A reply field appears beneath a focused comment when `r` is used.
- Save: `Cmd+Enter` (macOS) or `Ctrl+Enter` (non-mac).
- Cancel: `Esc`.
- After successful create, the viewport follows so the new comment is visible.

### Reply semantics for GitHub pull requests

- PR conversation comments are flat issue-style comments (issue-comment API).
- The reply composer pre-fills an `@author` mention for the target comment.
- The submitted comment body is exactly the composer text at submit time.

### Read-only items

- `c`, `r`, and `e` targeting a review item, a check item, or the body are no-ops with a
  non-blocking hint (PR body editing and review/check mutation are out of scope in v1).

## Send-to-Agent Contract

### Trigger and eligibility

- Trigger: `S` from PR Detail when no inline composer is active.
- If an inline composer is active, `S` is handled by the active inline control context and does
  not trigger send-to-agent.
- Eligible targets: existing agents only.
- Creating a new agent is not part of this flow.

### Chooser interaction

- `Up/Down`: move agent selection.
- `Enter`: confirm selected agent and send.
- `Esc`: cancel send flow.

### Payload

- Repository identifier
- PR number/title/body
- Branch info (`head_ref`, `base_ref`)
- Review-state summary and check summary
- Relevant PR metadata for context
- Focused comment text if send is triggered while a comment is focused
- `issue_base_prompt`

The launched agent is pointed at a written `.jefe/pr-prompt.md` file.

### No-agent behavior

- Send action is disabled when there are no agents.
- A trigger attempt shows a non-blocking no-agent message.

## Open-in-Browser Contract (Deferred Operations)

- Merge, approve, request-changes, and review submission are out of scope for in-app actions; they
  are the deliberate handoff to the browser (issue #20: "open in browser for complex actions").
- PR detail surfaces the `external_url` as a display-only URL (never edited in-app).
- `o` (from PR-list focus or PR-detail focus) opens the SELECTED pull request in the default browser
  by spawning `gh pr view <number> --repo <owner>/<name> --web` off the UI thread; `gh` opens the
  platform default browser, so no bespoke OS URL-opener is introduced. Success shows a non-blocking
  notice; failure shows a scoped error; with no PR selected, `o` is consumed and shows a
  "No pull request selected to open" notice (never a silent no-op).
- `o` performs NO in-app mutation — it only navigates to the PR page where deferred operations are
  completed. No keybinding performs an in-app merge/approve/request-changes/review-submit action.

## `issue_base_prompt` Reuse

- PR send-to-agent payloads reuse the existing repository `issue_base_prompt` field.
- No new repository config field is introduced for PR Mode.

## Authentication and Error States

### Authentication (v1)

- Uses the active `gh` CLI auth context.
- Missing/invalid auth blocks PR operations and shows explicit remediation guidance
  ("Run: gh auth login").

### Repository configuration

- The repository `github_repo` slug is validated for `owner/name` format before requests.
- Missing/invalid slug yields a scoped "configure repository" message, not a request error.

### Non-auth errors

For network/API/rate-limit/repository-access failures:

- Show a scoped error in the list/detail.
- Keep mode and focus stable.
- Preserve unsaved drafts where feasible.
- On repository scope change, unsent inline drafts are discarded with a non-blocking notice.
- Provide a retry affordance.
- No silent error swallowing: unavailable-context cases surface a message or log entry.

## Empty States

- No repositories accessible in the auth context.
- No PRs matching the current scoped criteria.
- No reviews on the selected PR.
- No checks on the selected PR.
- No comments yet on the selected PR.
- No available agents for send-to-agent.

## Out of Scope

- Full rich diff review parity (per-line review comments, diff hunks).
- In-app merge, approve, request-changes, or review submission.
- Advanced merge-queue / branch-protection automation.
- Editing the PR body or editing an existing PR comment in-app (the `e` key surfaces a read-only
  notice; inline edit is deferred to a future version).
- Creating a new agent from send-to-agent.
- Modal comment/reply flow.

### Scope adjudication: comment edit parity (FINAL)

This adjudication is FINAL and consistent with specification.md REQ-PR-010; no document in this plan
contradicts it.

- **NEW comment creation and reply ARE in scope (v1)** — the primary, non-gated PR interaction
  (parity-where-applicable with Issues Mode `c`/`r`).
- **EDIT of an existing PR comment OR the PR body is OUT of scope (v1, deferred).** `e` is CONSUMED
  and surfaces a read-only notice — never a silent no-op. No edit event/reducer/endpoint/dispatch/
  UI/test exists in v1.

Grounding:
- **Issue #20** names comments as the PRIMARY interaction and EXPLICITLY defers complex/advanced/
  mutation operations; editing a comment/body is a mutation operation and therefore deferred, while
  new comment/reply (the primary interaction) stays in scope.
- **Issue #46** requests the "same keybind patterns as issues mode WHERE APPLICABLE"; the "where
  applicable" qualifier keeps comment/reply parity but excludes the Issues-Mode `e`-driven inline
  EDIT for v1 PR mode (deferred per #20).

PR comment/body inline edit is a documented FOLLOW-UP, surfaced to the user via the consumed
read-only `e` notice — not a silent gap.

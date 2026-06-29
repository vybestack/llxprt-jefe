# Pull Requests Mode — Specification

Plan ID: `PLAN-20260624-PR-MODE`

GitHub Issue: #20 ("main github pr integration issue"); related closed design issue #46.

## Purpose

Add a GitHub Pull Request browsing, inspection, light-interaction, and send-to-agent
workflow to Jefe, scoped per selected repository. Pull Requests Mode introduces a new
top-level dashboard mode (`dashboard_pull_requests`) alongside the existing Agents Mode
(`dashboard_agents`) and Issues Mode (`dashboard_issues`). It mirrors the established
Issues Mode architecture — full key routing, inline comment mutation, filtering/search,
pagination, and agent integration — adapted for pull requests (review summary, CI/check
summary, branch info) and the deliberate deferral of complex operations (merge, approve,
review submission) to the browser via an external URL.

This specification is acceptance-level and testable. It defines WHAT the feature does, not
HOW it is built; the plan phases under `plan/` define the build sequence.

## Strategy Contract

1. **Extend existing architecture** — add `DashboardPullRequests` to `ScreenMode`/`AppState`,
   introduce a separate `PrFocus` enum for PR-mode focus tracking (do NOT modify `PaneFocus`
   or `IssueFocus`), add a `PullRequests` domain to the typed message bus
   (`MessageDomain::PullRequests` + `PullRequestsMessage` + `AppMessage::PullRequests`), and
   route keys through the established `InputMode` dispatch chain.
2. **Reuse existing patterns** — follow the domain/state/event/message-bus/UI layering and the
   multi-file module decomposition established by Issues Mode; no parallel architecture forks.
   PR comments reuse the `IssueComment` domain type and the issue-comment REST endpoint for
   COMMENT CREATION (`POST /repos/{owner}/{repo}/issues/{number}/comments` accepts a PR number,
   since GitHub PRs are issues for the REST comment API). However, the GraphQL per-PR comment FETCH
   must query `repository.pullRequest(number:).comments` via a PR-specific `list_pr_comments` method,
   NOT the issue `list_comments` (`repository.issue(number:)`), because `repository.issue(number:)`
   is NULL for a PR number (verified P00A §2d) and reusing it would silently return zero comments.
3. **GitHub API via `gh` CLI** — use the authenticated `gh` CLI as the GitHub API transport
   (no direct REST/GraphQL client library). All `gh` invocations occur off the UI thread via
   the established async wrapper (`src/app_input/gh_async.rs::spawn_gh_task_with_panic`).
4. **Modify, don't fork** — update `src/app_input/`, `src/input.rs`, `src/state/types.rs` +
   `src/state/mod.rs`, `src/domain/mod.rs`, `src/messages.rs` + `src/messages/`, `src/github/`,
   `src/layout.rs`, and `src/ui/` modules directly. No `*_v2`/`*_new`/`*_old` duplicates.
5. **Reuse shared infrastructure, never reimplement** — reuse layout constants (the typed
   `src/layout.rs` module — no duplicated `*_ROWS` constants), async gh I/O (the gh_async
   wrapper), the message-bus boundary, the `Sidebar`/`StatusBar`/`KeybindBar`/`AgentChooser`
   components, and `ScrollableText` for the PR-DETAIL text region (it is a TEXT-line scroller,
   the same way it is used by `issue_detail.rs`).
   NOTE: there is currently NO shared list-row viewport / selection-follow helper in the codebase —
   `src/ui/components/issue_list.rs` renders ALL rows via `props.issues.iter().enumerate()` (L137,
   verified) with no offset, and `ScrollableText` windows TEXT lines (not list rows). PR Mode
   therefore must BUILD a
   new, shared, reusable list-viewport / selection-follow helper (see REQ-PR-006) as an explicit
   deliverable with its own pseudocode + tests; `ScrollableText` is NOT a row-list scroll
   abstraction and must not be claimed as one.

---

## Architectural Boundaries

| Layer | Ownership |
|-------|-----------|
| Domain (`src/domain/mod.rs`) | `PullRequest`, `PullRequestDetail`, `PrReview`, `PrCheck`, `PrState`, `PrReviewState`, `PrCheckStatus`, `PrFilter`, `PrFilterState`, `ReviewDecisionFilter`, `ChecksFilter` entities; reuse `IssueComment` for PR comments |
| State (`src/state/`) | `PullRequestsState` aggregate, `PrFocus`, `PrDetailSubfocus`, PR-specific events, `apply_prs_message` reducer hub, focus domains, request-id staleness guards |
| Message Bus (`src/messages.rs`, `src/messages/prs_conversion.rs`) | `MessageDomain::PullRequests`, `PullRequestsMessage`, `AppMessage::PullRequests`, bidirectional `AppEvent`↔`PullRequestsMessage` conversion |
| GitHub Client (`src/github/`) | `gh` CLI wrapper boundary for PR list/detail/review/check/comment operations |
| UI (`src/ui/`) | PR list pane, unified PR detail view (metadata + body + review summary + check summary + comments), inline composer, filter controls, agent chooser, PR screen |
| Persistence (`src/persistence/`) | No new persisted Repository config field required; PR mode reuses the existing `github_repo` slug and `issue_base_prompt`. `prs_state` is transient and excluded from persistence. |

Forbidden couplings (acceptance-enforced in quality gate):
- UI must not call `gh` CLI directly; must go through the GitHub client boundary.
- The GitHub client boundary must not import `crate::state`, `crate::ui`, or `crate::app_input`
  and must not mutate `AppState`; it returns typed values/errors only.
- Inline composer must not bypass the state event reducer.
- Key handlers must not perform blocking `gh` I/O on the UI thread; all network I/O is spawned
  via `spawn_gh_task_with_panic` and delivered back as `PullRequestsMessage` data events.
- No `#[allow(clippy::...)]` attribute anywhere in first-party code; no raising of `clippy.toml`
  thresholds. (`scripts/check-clippy-allows.sh` is a CI gate.)

---

## Data Contracts and Invariants

### PullRequest (list row)
- `number: u64`
- `title: String`
- `state: PrState` (`Open` | `Closed` | `Merged`)
- `author_login: String`
- `updated_at: String`
- `head_ref: String` (source branch)
- `base_ref: String` (target branch)
- `is_draft: bool`
- `review_decision: Option<PrReviewState>` (aggregate review status: `Approved` | `ChangesRequested` | `ReviewRequired` | `Commented` | `None`)
- `checks_status: PrCheckStatus` (rollup: `Pending` | `Success` | `Failure` | `Neutral` | `None`)
- `assignee_summary: String`
- `labels_summary: String`
- `comment_count: u64`

### PullRequestDetail
- All list fields plus:
- `repo_owner_name: String`
- `created_at: String`
- `labels: Vec<String>`
- `assignees: Vec<String>`
- `milestone: Option<String>`
- `body: String`
- `external_url: String` (DISPLAY-ONLY; shown as the browser handoff target for deferred ops. The
  `o` keybinding opens the same PR page via `gh pr view --web` rather than parsing this string;
  no in-app keybinding edits or mutates it — see REQ-PR-012)
- `reviews: Vec<PrReview>` (review-thread summary, newest-first as returned)
- `checks: Vec<PrCheck>` (CI/check summary)
- `comments: Vec<IssueComment>` (reused issue-comment type; loaded via a SEPARATE first-page
  `list_pr_comments` call following the comments-sourcing precedent of `get_issue_detail`. NOTE:
  `get_issue_detail` DOES request `comments` in its `gh issue view --json` set and then OVERWRITES
  them from `list_comments`; PR detail instead OMITS the `comments` field from `gh pr view --json`
  entirely and sources comments solely from `list_pr_comments`. The fetch uses the PR-specific
  `list_pr_comments` (GraphQL `repository.pullRequest(number:).comments`), NOT the issue
  `list_comments` (`repository.issue(number:)`), since `repository.issue(number:)` is NULL for a PR
  number — see REQ-PR-007 and P00A §2d)
- `has_more_comments: bool` (from the comments `pageInfo.hasNextPage`)
- `comments_cursor: Option<String>` (the comments `pageInfo.endCursor`)

### PrReview (review-state summary item)
- `author_login: String`
- `state: PrReviewState` (`Approved` | `ChangesRequested` | `Commented` | `Pending` | `Dismissed`)
- `submitted_at: String`
- `body: Option<String>` (review summary body; may be empty)

### PrCheck (CI/check summary item)
- `name: String`
- `status: PrCheckStatus` (`Pending` | `Success` | `Failure` | `Neutral`)
- `conclusion: String` (raw conclusion text for display, e.g. "success", "failure", "skipped")
- `url: Option<String>` (display-only details link)

### PrFilter
- `query_text: String`
- `state: Option<PrFilterState>` (`Open` | `Closed` | `Merged` | `All`)
- `author: String`
- `assignee: String`
- `reviewer: String`
- `is_draft: Option<bool>` (None = any; Some(true) = drafts only; Some(false) = ready only)
- `labels: Vec<String>`
- `review_decision: ReviewDecisionFilter` (`Any` | `Approved` | `ChangesRequested` |
  `ReviewRequired` | `None`; issue #20 review-signal filter; non-`Any` → server-side `review:<x>`
  qualifier, `Any` emits none)
- `checks_status: ChecksFilter` (`Any` | `Success` | `Failing` | `Pending`; issue #20
  workflow-signal filter; non-`Any` → server-side `status:<x>` qualifier, `Any` emits none)

### Comment (reused `IssueComment`)
- `comment_id: u64`
- `author_login: String`
- `created_at: String`
- `edited_at: Option<String>`
- `body: String`

### Invariants
- PR list and detail are always scoped to `selected_repository_id`.
- Repository scope change invalidates all prior PR list/detail/review/check/comment/pagination
  state (request-id + scope-id staleness guards discard late responses).
- At most one inline mutable control (comment composer) active at a time.
- Review and check summaries are READ-ONLY: they are navigable for focus/scroll but never
  editable. `e`/`r`/`c` targeting a review/check subfocus are CONSUMED (so they never leak to
  dashboard handlers) and surface a non-blocking visible/logged NOTICE explaining the item is
  read-only — never a silent `None`/no-op.
- Detail/reviews/checks/comments must never display data from a prior repository scope.
- Unsent inline comment drafts are discarded (with non-blocking notice) on repository scope change.
- `review_decision` and `checks_status` are SUMMARIES; rich per-line diff review is out of scope.
- Deferred operations (merge, approve, request-changes, submit review) are NOT performed in-app;
  they are handed off to the browser. The PR page is opened with `o` (`gh pr view <n> --web`,
  off-thread) and `external_url` is also shown display-only as the same target (see REQ-PR-012).

---

## Integration Points with Existing Modules

> **Citation discipline (finding #7):** every cell below cites the **SYMBOL NAME first** (the
> authoritative anchor — type/fn/field/variant/module) and the line number second, in parentheses,
> purely as drift-prone EVIDENCE. Locate each integration point BY ITS SYMBOL NAME and refresh any
> stale `LNNN` during preflight; a symbol that cannot be found by name is a blocker (mirrors
> `00-overview.md` Critical Reminder #6).

| Module | Integration |
|--------|-------------|
| `src/state/types.rs` | Add `DashboardPullRequests` to `ScreenMode` (L227); add `prs_state: PullRequestsState` to `AppState` (after `issues_state` L277); add `PrFocus`, `PrDetailSubfocus`, `PullRequestsState`, and PR pending-request guard structs; add PR variants to `AppEvent` (do NOT modify existing) |
| `src/domain/mod.rs` | Add `PullRequest`, `PullRequestDetail`, `PrReview`, `PrCheck`, `PrState`, `PrReviewState`, `PrCheckStatus`, `PrFilter`, `PrFilterState`, `ReviewDecisionFilter`, `ChecksFilter`; reuse `IssueComment` |
| `src/input.rs` | Add `InputMode::Prs{Normal,Inline,Search,Filter,Chooser}` to the `InputMode` enum (enum at L9; insert after the last variant `IssuesChooser` at L30, before the closing brace at L31); add a `DashboardPullRequests` block to `input_mode_for_state()` (fn at L45; mirror the `DashboardIssues` block at L64) |
| `src/messages.rs` | Add `MessageDomain::PullRequests` (L18); add `PullRequestsMessage` enum; add `AppMessage::PullRequests` (L283) + `domain()`/`route()`/`name()` arms + `message_names!` invocation; add `From<AppEvent>` routing arm and `From<PullRequestsMessage> for AppEvent` |
| `src/messages/prs_conversion.rs` (new) | `PullRequestsMessage::from_app_event` + per-variant `AppEvent` conversions (mirror `messages/issues_conversion.rs`) |
| `src/state/mod.rs` | Add `AppMessage::PullRequests` arm to `apply_message()` (fn at L342; mirror the `AppMessage::Issues` arm at L370); add `apply_prs_message()` reducer hub; add `if self.prs_state.active { self.reset_prs_for_repo_change() }` to `select_repository_by_index()` (L488); declare `prs_*` ops modules |
| `src/state/prs_ops.rs`, `prs_inline_ops.rs`, `prs_load_ops.rs`, `prs_mutation_ops.rs` (new) | Reducer logic mirroring `issues_*` ops |
| `src/github/mod.rs` + `src/github/parse_pr.rs` (new) | Add `list_pull_requests()`, `get_pull_request_detail()`, `list_pr_comments()` (GraphQL `repository.pullRequest(number:).comments`), `create_pr_comment()`, `open_pull_request_in_browser()` (`gh pr view --web`), `build_pr_send_payload()`, `PrListResponse`, parse helpers; reuse `GhError`, `parse_comments_json`/`parse_page_info`/`IssueComment` (do NOT reuse the issue `list_comments` for PR comment FETCH — it queries `repository.issue(number:)`, which is NULL for a PR number) |
| `src/app_input/normal.rs` | Hook `p`/`P` in `resolve_mode_key()` (L296) when `screen_mode == Dashboard` → `EnterPrsMode`; add `handle_dashboard_prs_key` resolver (mirror `handle_dashboard_issues_key` L156) |
| `src/app_input/prs.rs`, `prs_dispatch.rs`, `prs_filter.rs`, `prs_list_dispatch.rs`, `prs_mutation.rs` (new) | PR key handling + side-effecting dispatch (async gh loaders), mirroring `issues_*` |
| `src/app_input/mod.rs` | Add `AppMessage::PullRequests` dispatch arms to `dispatch_app_message()` (L420); register new modules |
| `src/layout.rs` | Add PR layout constants + `prs_pane_rows`/`prs_detail_viewport_rows`/`pr_list_content_width` helpers and `PRS_SIDEBAR_WIDTH` (reuse `LEFT_COL_WIDTH`); NO duplicated constants |
| `src/ui/components/pr_list.rs`, `pr_detail.rs`, `pr_filter_controls.rs` (new) | PR list (scroll-aware), unified PR detail view, PR filter controls |
| `src/ui/screens/pull_requests.rs` (new) | `PullRequestsScreen` component (mirror `screens/issues.rs`) |
| `src/ui/orchestration.rs` | Add `ScreenMode::DashboardPullRequests` arm to `build_screen_element()` |
| `src/ui/screens/mod.rs`, `src/ui/components/mod.rs`, `src/ui/mod.rs` | Register and re-export new screen/components |
| `src/lib.rs` | No new top-level module needed (`pub mod github` already exists) |

---

## Essential PR Operations — Scope Classification

Issue #20 asks PR Mode to "support essential interaction operations" while explicitly placing
"advanced merge queue / branch-protection" handling OUT of scope. Issue #46 (the closed design
issue) frames PR Mode as a browse / inspect / light-interact / hand-off surface that mirrors Issues
Mode rather than a full GitHub PR-review client. The table below ENUMERATES every candidate
"essential PR operation" and classifies each as IN scope (v1) or OUT of scope (deferred), so the
narrowing in REQ-PR-012 is justified at the requirement level rather than asserted.

The unifying rationale for every OUT-of-scope row is identical: each deferred item is a **mutating /
state-changing GitHub operation** that overlaps the merge-queue / branch-protection / review-gate
machinery issue #20 explicitly excludes (a merge can be blocked by required checks/branch
protection; approve / request-changes / submit-review are review-gate mutations; close/reopen and
ready-for-review are PR-lifecycle mutations). Performing any of them in-app would require
reproducing GitHub's protection/permission/merge-method rules inside Jefe. v1 therefore HANDS THESE
OFF to the browser via the single `o` open-in-browser action (REQ-PR-012), which is the explicit v1
substitute for all deferred mutation operations: the user completes any deferred mutation on the
real GitHub PR page where GitHub's own gating applies. `external_url` is shown display-only as the
same handoff target.

| Candidate PR operation | Classification | Justification |
|------------------------|----------------|---------------|
| Browse / list PRs (scoped to repo) | IN (v1) | Core "essential interaction" of #20; mirrors Issues Mode list (REQ-PR-006/007) |
| Filter PRs (state/author/assignee/reviewer/draft/labels) | IN (v1) | Essential triage interaction; mirrors Issues filter (REQ-PR-008) |
| Search PRs (text query) | IN (v1) | Essential triage interaction; reuses `route_search_key` (REQ-PR-008) |
| View PR detail (metadata + body + branches) | IN (v1) | Core inspect operation of #20 (REQ-PR-009) |
| View reviews summary (read-only) | IN (v1) | Inspect-only; #46 review SUMMARY, not per-line review tooling (REQ-PR-009) |
| View checks/CI summary (read-only) | IN (v1) | Inspect-only rollup; not the merge-queue gate itself (REQ-PR-009) |
| Comment on a PR | IN (v1) | Essential non-gated interaction; reuses issue-comment endpoint (REQ-PR-010) |
| Reply to a comment | IN (v1) | Essential non-gated interaction (REQ-PR-010) |
| Send PR to agent | IN (v1) | Core Jefe value-add; mirrors Issues send-to-agent (REQ-PR-011) |
| Open PR in browser (`o`) | IN (v1) | The explicit v1 handoff substitute for all deferred mutations (REQ-PR-012) |
| **Edit own comment / edit PR body** | OUT (deferred) | v1 PR mode focuses on browse / comment / reply / send-to-agent / open-in-browser; inline edit of PR comments and the PR body is deferred. The `e` key surfaces a read-only notice (consumed, not silent — REQ-PR-010/013) and NO edit event/reducer/endpoint/dispatch/UI/test is implemented in v1. (Unlike Issues Mode, which DOES support `e`-driven inline edit via `OpenInlineEditor` — `src/app_input/issues.rs` L145-164, `src/app_input/issues_mutation.rs` — PR mode intentionally omits it for v1 to keep the surface minimal and consistent with the read-only `e` behavior specified throughout this plan.) |
| **Merge PR** | OUT (deferred → browser) | Gated by required checks / branch protection / merge-queue — the exact machinery #20 places out of scope; handed off via `o` |
| **Approve review** | OUT (deferred → browser) | Review-gate mutation tied to branch-protection approval rules excluded by #20; handed off via `o` |
| **Request changes** | OUT (deferred → browser) | Review-gate mutation (same rationale as approve); handed off via `o` |
| **Submit review** | OUT (deferred → browser) | Review-submission mutation feeding the review gate excluded by #20; handed off via `o` |
| **Close / reopen PR** | OUT (deferred → browser) | PR-lifecycle state mutation beyond "essential interaction"; handed off via `o` |
| **Ready-for-review (un-draft)** | OUT (deferred → browser) | PR-lifecycle state mutation; handed off via `o` |

REQ-PR-012 is the requirement that realizes the OUT-of-scope rows: it defers all listed mutation
operations to the browser and provides `o` (open-in-browser) as the single, fully-routed v1 handoff
action. No in-app keybinding performs any deferred mutation.

---

## Functional Requirements

### REQ-PR-001: Mode Entry and Exit
- `p` (or `P`) from the dashboard (`ScreenMode::Dashboard`) enters `dashboard_pull_requests`
  with `focus = pr_list`.
- `p` while already in `dashboard_pull_requests` refocuses `pr_list`.
- `a` from `dashboard_pull_requests` exits to `dashboard_agents` (Dashboard).
- `Esc` follows the PR-mode precedence chain; exits the mode only when no higher-priority
  cancel target exists.
- Entry is scoped to the currently selected repository; the PR list loads immediately for that
  scope.

Behavior contract:
- GIVEN user is in Agents Mode (Dashboard) with repository "acme/api" selected
- WHEN `p` is pressed
- THEN mode transitions to `dashboard_pull_requests`, PR list is focused, and a scoped PR list
  load for "acme/api" begins

- GIVEN user is in PR Mode with no active inline controls
- WHEN `a` is pressed
- THEN mode transitions back to Dashboard and prior agent focus is restored if valid

- GIVEN user is in PR Mode with an active inline comment composer
- WHEN `Esc` is pressed
- THEN the composer is cancelled; mode remains `dashboard_pull_requests`

### REQ-PR-002: Key Routing and Suppression
- While in PR Mode: suppress dashboard `a` focus-agents binding, dashboard `s` split binding
  (lowercase `s` is a no-op in PR Mode), split-mode `Esc`, and destructive lifecycle keys
  (`Ctrl-d`, `Ctrl-k`, `l`).
- Route `/` to PR-list search; `?`/`h`/`F1` open help and must include PR-Mode bindings.
- `S` triggers send-to-agent from PR Detail when no inline control is active; the dashboard
  split binding on `S` is suppressed in PR Mode.
- `o` (from PR-list or PR-detail focus, when no inline control is active) opens the selected PR in
  the browser (see REQ-PR-012); it is consumed in PR Mode and never leaks to dashboard handlers.
- Unhandled-but-claimed keys are consumed as no-ops so they never leak to dashboard handlers.

Behavior contract:
- GIVEN user is in PR Mode
- WHEN `Ctrl-d` is pressed
- THEN the key is consumed as a no-op (agent destructive action suppressed)

- GIVEN user is in PR Mode with PR list focused
- WHEN `/` is pressed
- THEN search input is focused for PR-list search

### REQ-PR-003: Pane Focus and Navigation
- PR-Mode pane cycle: `repo_list -> pr_list -> pr_detail -> repo_list` (`Tab`); reverse on
  `Shift+Tab`. `Tab`/`Shift+Tab` cycle the three panes from EVERY pane INCLUDING `pr_detail`. This
  is the explicit requirement of issue #46 ("Same focus cycling (Tab between repo list / PR list / PR
  detail)"): `Tab` ALWAYS advances `RepoList -> PrList -> PrDetail -> RepoList` and `Shift+Tab`
  reverses, from whichever pane currently holds focus. NOTE: this DIVERGES from Issues mode, where
  `Tab`/`BackTab` are consumed for detail subfocus inside the issue-detail pane
  (`resolve_issue_detail_key_event` maps `Tab -> IssueDetailSubfocusNext`); PR mode intentionally
  reserves `Tab`/`Shift+Tab` for inter-pane cycling everywhere to satisfy #46.
- Repository list focus: `Up/Down` moves repository selection AND immediately rescopes/reloads
  the PR list. This navigation MUST function whenever `PrFocus == RepoList`, independent of the
  dashboard `pane_focus` field (regression guard for #47-class bugs). `Right` also cycles to the
  next pane (mirrors `resolve_repo_list_key_event`).
- PR list focus: `Up/Down`, `PageUp/PageDown`, `Home/End`; `Enter` focuses PR detail; `Left`/`Right`
  cycle panes (mirrors `resolve_issue_list_key_event`).
- PR detail focus: `Up/Down` scroll the unified detail view; `PageUp/PageDown` page-scroll. Detail
  SUBFOCUS traversal (body → reviews → checks → comments → new-comment → body, skipping empty
  sections) is bound to `j` (subfocus next, → `PrDetailSubfocusNext`) and `k` (subfocus previous, →
  `PrDetailSubfocusPrev`). `j`/`k` are chosen because they are vim-style next/prev consistent with
  list navigation, are currently UNUSED in `src/app_input/` (only `Ctrl-k` is bound, at
  `issues.rs` L386, and it is suppressed in PR mode), and do NOT collide with the `Up/Down` scroll
  binding. `Tab`/`Shift+Tab` are NOT consumed inside `pr_detail` — they cycle panes (above). `Left`
  is NO LONGER required to leave the detail pane (`Tab` exits it); `Left` MAY remain bound as an
  optional reverse pane-cycle (→ `PrCycleFocusReverse`) for parity with the list pane, but it is not
  the sole escape. There is NO Tab-binding conflict: `Tab` means pane-cycle in every pane, and
  subfocus traversal lives on `j`/`k`.
- `r` on a focused comment opens an inline reply; `r` on body/review/check/new-comment is CONSUMED
  and surfaces a non-blocking visible/logged NOTICE (reviews/checks are read-only) — never a silent
  `None`/no-op.

Behavior contract:
- GIVEN user is in PR Mode with repo_list focused and a next repository exists
- WHEN `Down` is pressed
- THEN repository selection moves down and the PR list reloads for the new scope (selection
  change drives reload without depending on `pane_focus`)

- GIVEN user is in PR Mode with pr_list focused
- WHEN `Tab` is pressed
- THEN focus cycles to `pr_detail` (pane cycle), because the list focus handler does not consume
  `Tab` and it falls through to the pane-cycle fallback

- GIVEN user is in PR Mode with pr_detail focused
- WHEN `Tab` is pressed
- THEN focus cycles to `repo_list` (pane cycle wraps `PrDetail -> RepoList`); `Tab` is NOT consumed
  for subfocus inside `pr_detail` (issue #46 requirement)

- GIVEN user is in PR Mode with pr_detail focused on the body subfocus
- WHEN `j` is pressed
- THEN the detail subfocus advances to the next item (reviews) and focus stays in `pr_detail`;
  pressing `k` moves to the previous subfocus item

- GIVEN user is in PR Mode with pr_detail focused on a comment
- WHEN `r` is pressed
- THEN an inline reply composer opens pre-filled with the `@author` mention

- GIVEN user is in PR Mode with pr_detail focused on a review summary item
- WHEN `r` is pressed
- THEN the key is CONSUMED and a non-blocking visible/logged NOTICE is surfaced (reviews are
  read-only); the reducer never returns a silent `None`/no-op

### REQ-PR-004: Esc Precedence Chain
1. Cancel the active inline comment composer.
2. Else cancel the active send-to-agent chooser.
3. Else if search input is focused and non-empty: clear search text, keep search focused.
4. Else if search input is focused and empty: blur search input, keep PR Mode active.
5. Else close active transient controls (filter controls).
6. Else exit PR Mode.

Transient notices and loading indicators do NOT consume `Esc`.

Behavior contract:
- GIVEN search input is focused with query text "auth"
- WHEN `Esc` is pressed
- THEN search text is cleared, search input remains focused, mode stays `dashboard_pull_requests`

### REQ-PR-005: Exit-Focus Restoration
- On exit from PR Mode, restore the prior agent focus only if: the prior focus token exists, the
  referenced target still exists, and the target is focusable in the current state.
- Otherwise fall back to default agent-list focus.

Behavior contract:
- GIVEN user had agent "bot-1" selected before entering PR Mode
- WHEN user exits PR Mode and "bot-1" still exists and is focusable
- THEN agent focus is restored to "bot-1"; otherwise focus falls back to the agent list

### REQ-PR-006: PR List Display, Sorting, and Scroll-Following
- Each row displays: number, title (truncated with an ellipsis to the pane width), state
  (`open`/`closed`/`merged`, with a draft indicator when `is_draft`), author, updated timestamp,
  review status (`review_decision`), check status (`checks_status`), label summary, comment count.
- Default sort: `updated_at desc`, tie-breaker `number asc`.
- First non-empty load selects the first PR and auto-loads its detail.
- On filter/search change: keep the current selection if still present (by PR number); else
  select the first row; else no selection with a scoped empty state.
- The list is scroll-aware: the selected row is always kept visible within the list viewport
  (no clipping; selection-following is implemented by a NEW shared list-viewport / selection-follow
  helper built for this feature — there is no pre-existing list-row scroll helper to reuse — and is
  a regression guard for #55-class bugs). The helper computes a `first_visible_index` (scroll offset)
  from the selected index, the loaded-row count, and the viewport row count (from `src/layout.rs`),
  clamping so the selected row is always within `[first_visible, first_visible + viewport_rows)`.
  The number of rendered rows equals the number of loaded PRs that fit, and navigating to any
  position (including the last loaded row) keeps the selection on-screen.
- Loaded N PRs render exactly N rows when they fit; no item is silently dropped from rendering
  (regression guard for #54-class bugs).

Behavior contract:
- GIVEN repository "acme/api" has 5 open PRs
- WHEN PR Mode enters with "acme/api" selected
- THEN the PR list displays 5 PRs sorted by `updated desc`; the first PR is selected; detail loads

- GIVEN a PR list longer than the visible viewport
- WHEN the user navigates `Down` past the last visible row
- THEN the list scrolls to keep the newly selected row visible (selection never moves off-screen)

#### Scope adjudication: shared viewport helper (FINAL)

The selection-follow viewport helpers (`list_first_visible_index` / `list_visible_window`) are
implemented as a GENUINELY NEUTRAL, GENERIC shared abstraction in `src/layout.rs` — NOT a PR-specific
helper. Their signatures and unit tests are written against an INDEXED ROW LIST in the abstract
(parameters: `selected_index`, `loaded_len`, `viewport_rows` → first-visible index / visible window);
they make NO PR-only assumptions and contain NO reference to PR types, so they ARE reusable by any
indexed list in the codebase.

- They are fully unit-tested IN ISOLATION as pure logic in the P04 RED tests (`src/layout.rs`'s
  `#[cfg(test)] mod tests`), independent of any consumer.
- In this PR they are consumed by BOTH the PR list AND the PR detail (the two new consumers).
- **Migrating the EXISTING, already-shipped Issues list to adopt these helpers is OUT of scope for
  issue #20.** The Issues list (`src/ui/components/issue_list.rs`) currently renders all rows with no
  offset and works as shipped; refactoring it to route through the new helper is a SCOPE EXPANSION
  that risks regressing working Issues mode. It is explicitly recorded as a RECOMMENDED FOLLOW-UP
  (tracked against the relevant Issues-list scroll regression, issue #55) and is deliberately NOT
  performed in this PR. The helper is intentionally written generically so that follow-up adoption is
  a drop-in, not a rewrite.

### REQ-PR-007: Pagination and Lazy Loading
- The PR list is paginated/lazy-loaded using REAL GraphQL cursor pagination, mirroring the
  existing Issues list path (`gh api graphql` with `search(type: ISSUE, query, first, after)` and
  `pageInfo { hasNextPage endCursor }`; see `src/github/parse.rs::build_issue_search_args` and
  `src/github/mod.rs::list_issues`). The PR list page size is `PR_LIST_PAGE_SIZE` (= 30). There is
  no `gh pr list --limit` window heuristic and no client-invented cursor; `endCursor`/`hasNextPage`
  are the canonical paging cursor and has-more flag.
- When selection reaches the last loaded row and `has_more` is true, the next page loads
  automatically using the stored `endCursor`; loaded rows are preserved across page loads.
- Repository switch invalidates the prior scope's paging cursor (`endCursor`) and state.
- The comments timeline within PR detail supports incremental loading/pagination via the SAME
  cursor mechanism, but through a PR-SPECIFIC GraphQL comments method. PR comments are fetched via
  `GhClient::list_pr_comments`, which queries `repository.pullRequest(number:).comments(first, after)`
  + `pageInfo { hasNextPage endCursor } totalCount`, with comment page size `PR_COMMENT_PAGE_SIZE`
  (= 30). This method REUSES the existing `parse_comments_json` / `parse_page_info` helpers and the
  `IssueComment` type unchanged (the comment node shape is identical to the issue path). It MUST NOT
  reuse the issue `GhClient::list_comments` (which queries `repository.issue(number:).comments`):
  `repository.issue(number:)` is NULL for a PR number (verified P00A §2d), so reusing it would
  SILENTLY return zero comments for every PR. The PR detail fetch loads the first comment page via a
  SEPARATE `list_pr_comments` call, following the comments-sourcing precedent of `get_issue_detail`
  (which requests `comments` in its own `--json` set but then OVERWRITES them from `list_comments`,
  mod.rs L198-202); the PR `gh pr view --json` set OMITS `comments` outright since it would only be
  overwritten.
- Comment pagination appends older items in stable timeline order without reordering or replacing
  already loaded comments.
- Comment pagination failure retains loaded comments and exposes a scoped retry affordance.

Behavior contract:
- GIVEN the PR list has 60 PRs, `PR_LIST_PAGE_SIZE` is 30, and the first page (30 rows) is loaded
  with `has_more = true` and a stored `endCursor`
- WHEN the user navigates to the PR at the last loaded row (position 30) and more exist
- THEN the next page loads automatically using the stored `endCursor` and the list grows to 60
  (existing rows preserved)

### REQ-PR-008: Filtering and Search (Fully Interactive Controls)
- Issue #20 requires filtering PRs by status AND common review/workflow signals. Supported criteria:
  text query (matches title/body), state (`open`/`closed`/`merged`/`all`), author, assignee,
  reviewer, draft (any/drafts-only/ready-only), labels (multi, AND-composed), AND the two
  review/workflow SIGNAL filters required by issue #20:
  - **review-decision** (`any`/`approved`/`changes-requested`/`review-required`/`none`) — the
    aggregate review state of the PR; modeled by `ReviewDecisionFilter` and compiled to the
    server-side search qualifier `review:approved` / `review:changes_requested` / `review:required` /
    `review:none` (omitted when `any`).
  - **checks-status** (`any`/`success`/`failing`/`pending`) — the CI/checks rollup status of the PR;
    modeled by `ChecksFilter` and compiled to the server-side search qualifier `status:success` /
    `status:failure` / `status:pending` (omitted when `any`).
  Both signal qualifiers are P00A §2c-verified to filter SERVER-SIDE in the
  `search(type: ISSUE, query: ...)` string, so cursor pagination (`endCursor`/`hasNextPage`) is
  preserved (no client-side post-filter).
- This yields EIGHT interactive filter-control fields, in field-cycling order: (1) state, (2) draft,
  (3) review-decision, (4) checks-status, (5) author, (6) assignee, (7) reviewer, (8) labels. (The
  text query is entered through the separate `/` search input, not the filter-controls field set.)
- Structured filters are AND-composed; text query is AND-composed with structured filters.
- Default committed filter on PR-mode entry is `state = Open` (`committed_filter.state =
  Some(PrFilterState::Open)`) — the default scoped list shows OPEN pull requests — with all other
  structured criteria (author, assignee, reviewer, draft, labels, review-decision, checks-status,
  text query) unset/empty/`Any`. This is the canonical default asserted in
  `analysis/pseudocode/component-001.md` L74 (`enter_prs_mode` sets `committed_filter.state =
  Some(Open)`) and `analysis/domain-model.md` (`PrFilterState` `#[default] Open`,
  `ReviewDecisionFilter`/`ChecksFilter` `#[default] Any`).
- `f` opens filter controls from PR-list focus only (no-op elsewhere).
- Filter controls are FULLY interactive (regression guard for #38/#40-class bugs):
  - `Tab`/`Shift+Tab` move between the EIGHT fields (wrap-around).
  - `Space` cycles enumerated fields (state, draft, review-decision, checks-status).
  - Character entry edits text fields (author, assignee, reviewer, labels) and updates the DRAFT
    filter immediately.
  - `Enter`/Apply commits the draft and refreshes the scoped list.
  - `Ctrl-c`/Clear resets committed criteria back to the default (`state = Some(Open)`,
    `review_decision = Any`, `checks_status = Any`, all other criteria empty —
    `clear_committed_filter`, component-001 L270-274) and refreshes.
  - `Esc`/Cancel closes controls without committing draft edits.
- `/` focuses the search input; `Enter` applies the trimmed query; clearing query text restores
  results constrained only by committed structured filters.

Behavior contract:
- GIVEN the user opens filter controls and sets `state=open` and `draft=ready-only`
- WHEN each field is edited
- THEN the DRAFT filter reflects each change immediately

- GIVEN draft filter `state=open, label=bug`
- WHEN Apply is pressed
- THEN the PR list refreshes showing only open PRs with the "bug" label

- GIVEN the user cycles `review-decision` to `approved` and `checks-status` to `failing`
- WHEN Apply is pressed
- THEN the scoped list refreshes showing only PRs whose review decision is approved and whose
  checks rollup is failing, via the server-side `review:approved status:failure` qualifiers (cursor
  pagination preserved)

### REQ-PR-009: PR Detail — Metadata, Body, Reviews, Checks, Comments
- The detail view is a single unified scrollable region containing, in order: metadata header
  (number, title, state+draft, author, created/updated, branches `head_ref → base_ref`, labels,
  assignees, milestone), description body, review-state summary, CI/check summary, comments
  timeline, and the new-comment field.
- Review summary lists each `PrReview` (author, state, submitted_at, optional body) plus an
  aggregate `review_decision` line.
- Check summary lists each `PrCheck` (name, status, conclusion) plus an aggregate
  `checks_status` rollup line.
- Branch info shows `head_ref` and `base_ref`.
- `external_url` is shown as a display-only URL (the open-in-browser target; `o` opens it via
  `gh pr view --web` — see REQ-PR-012).
- Markdown body/comments are displayed as terminal-friendly rendered text.
- The detail viewport height is provided as a prop derived from the typed layout module (NOT
  read independently via `crossterm::size()` inside scroll math), and the maximum scroll offset
  is derived from the ACTUAL rendered content length (not a heuristic line estimate) — regression
  guards for #37/#39-class bugs.

Behavior contract:
- GIVEN PR #142 is selected with 3 reviews, 4 checks, and 5 comments
- WHEN PR detail loads
- THEN metadata, body, the review summary (3 items + decision), the check summary (4 items +
  rollup), branch info, the comments timeline, and a display-only `external_url` are all shown

### REQ-PR-010: Inline Comment and Reply (Consistent Composer Focus + Auto-Scroll)
- No modal flow; comment creation is inline only.
- The new-comment field is always present at the bottom of the unified detail view.
- A reply field appears under a focused comment when `r` is used, pre-filled with `@author`.
- The comment action (`c`) is consistent with the selected detail subfocus: pressing `c` opens
  the new-comment composer AND moves `pr_detail_subfocus` to `NewComment` (regression guard for
  #56-class bugs). Opening any composer auto-scrolls the detail viewport so the active composer
  is visible (never rendered off-screen below the viewport).
- After a comment is successfully created, the detail viewport follows the new comment so it is
  visible.
- Save: `Cmd+Enter` (macOS) / `Ctrl+Enter` (non-mac). Cancel: `Esc`.
- Mutable-control exclusivity: at most one inline control active at a time.
- Review/check items are read-only: `c`/`r`/`e` targeting them is CONSUMED and surfaces a
  non-blocking visible/logged NOTICE (read-only hint) — never a silent `None`/no-op.
- Comment/body EDITING is OUT OF SCOPE for v1 (deferred — see Scope Classification table). The `e`
  key ANYWHERE in PR detail (body, review, check, own comment, or new-comment composer) is CONSUMED
  and surfaces a read-only notice ("PR body, reviews, and checks are not editable in v1"); it is NOT
  a silent drop. There is NO inline-edit event, reducer, gh endpoint, dispatch path, UI, or test for
  PR comment/body editing in v1. (This intentionally diverges from Issues Mode, where `e` DOES open
  an inline editor — PR mode keeps the v1 surface to browse / comment / reply / send-to-agent /
  open-in-browser.)

Behavior contract:
- GIVEN the user is viewing PR detail with the body subfocus
- WHEN `c` is pressed
- THEN the new-comment composer opens, `pr_detail_subfocus` becomes `NewComment`, and the
  viewport scrolls so the composer is visible

- GIVEN the user submits a new comment successfully
- WHEN the comment is appended
- THEN the viewport follows so the newly created comment is visible

#### Scope adjudication: comment edit parity (FINAL)

This adjudication is FINAL and binds every document in this plan; no file may contradict it.

- **NEW comment creation and reply ARE in scope (v1).** They are the primary, non-gated PR
  interaction. This is parity-where-applicable with Issues Mode (`c` to comment, `r` to reply).
- **EDIT of an existing PR comment OR the PR body is OUT of scope (v1, deferred).** Pressing `e`
  anywhere in PR detail is CONSUMED and surfaces a read-only notice ("PR body, reviews, and checks
  are not editable in v1") — never a silent `None`/no-op. NO edit event, reducer, gh endpoint,
  dispatch path, UI, or test is implemented in v1.

Grounding (why this is the correct scope, not an arbitrary omission):
- **Issue #20** names comments as the PRIMARY interaction ("comments as the primary interaction")
  and EXPLICITLY defers complex/advanced/mutation operations. Comment/body editing is a mutation
  operation, so it falls under that explicit deferral; creating a new comment / reply is the primary
  interaction it keeps in scope.
- **Issue #46** asks for the "same keybind patterns as issues mode WHERE APPLICABLE." The "where
  applicable" qualifier is the operative limit: comment/reply keybinds apply (and are implemented);
  the Issues-Mode `e`-driven inline EDIT does NOT apply to v1 PR mode because issue #20 deferred
  mutation/advanced operations. Parity is honored where applicable and deliberately NOT extended to
  the deferred edit operation.

Follow-up: PR comment/body inline edit is a documented follow-up (a future version), not a silent
gap. The read-only `e` notice is the user-visible, consumed signal that the capability is
intentionally deferred.

### REQ-PR-011: Send-to-Agent
- `S` from PR detail, when no inline control is active, opens the agent chooser.
- Chooser: `Up/Down` move selection, `Enter` confirms and sends, `Esc` cancels.
- `build_pr_send_payload` returns a NEW `PrSendPayload` struct in `src/github/mod.rs` that mirrors
  the existing `SendPayload` (src/github/mod.rs:78-89) field-for-field in spirit — structured, owned
  fields only. Fields: `repository`, `pr_number`, `pr_title`, `pr_body`, `pr_state`, `head_ref`,
  `base_ref`, `external_url`, `review_summary: Vec<String>`, `check_summary: Vec<String>`,
  `focused_comment: Option<String>` (the focused comment body, if a comment is focused at trigger
  time), `focused_comment_author: Option<String>`, and `pr_base_prompt` (from the repository's
  `issue_base_prompt`). The payload carries NO `prompt_markdown`/`work_dir`/`signature`: the markdown
  is rendered later by `prs_dispatch::format_pr_prompt(&PrSendPayload)` (mirroring
  `issues_dispatch::format_issue_prompt(&SendPayload)`), and `work_dir`/`signature` come from the
  chosen agent via `PrSendInfo` (mirroring `IssueSendInfo`), NOT from the payload.
- No-agent state: send is disabled and a non-blocking message is shown.
- The launched agent receives a prompt referencing a written `.jefe/pr-prompt.md` file (mirroring
  the issues `.jefe/issue-prompt.md` flow); the dispatch layer writes it via `write_pr_prompt`.

Behavior contract:
- GIVEN the user is viewing PR #142 with a comment by @pat focused and agents exist
- WHEN `S` is pressed and "backend-owner" is confirmed
- THEN the payload includes PR data + branch/review/check summary + @pat's comment +
  `issue_base_prompt`, and the agent is launched against `.jefe/pr-prompt.md`

### REQ-PR-012: Open-in-Browser for Deferred Operations
- This requirement REALIZES the OUT-of-scope rows of the "Essential PR Operations — Scope
  Classification" table above. The deferred operations enumerated there — merge, approve,
  request-changes, submit-review, close/reopen, ready-for-review — are NOT performed in-app; they
  are the deliberate handoff to the browser because each is a mutating GitHub operation overlapping
  the merge-queue / branch-protection / review-gate machinery issue #20 explicitly excludes (see
  that table's justification column). NOTE: open-in-browser is a PLAN DESIGN DECISION chosen to
  satisfy issue #20's call to "support essential interaction operations" and to provide the single
  v1 handoff substitute for those deferred mutations; it is NOT an explicit verbatim requirement of
  issue #20.
- The PR detail surfaces the `external_url` as a DISPLAY-ONLY string (it is never edited in-app).
- `o` (from PR-list focus or PR-detail focus) opens the SELECTED pull request in the default
  browser. This is a real, fully-routed feature: the `o` key emits `PrOpenInBrowser`, which the
  dispatch layer fulfils off the UI thread by invoking `gh pr view <number> --repo <owner>/<name>
  --web` via `spawn_gh_task_with_panic`. `gh` opens the platform default browser; no bespoke OS
  URL-opener is introduced (none exists in the codebase today). Success surfaces a non-blocking
  notice; failure surfaces a scoped, categorized error (never a silent drop).
- `o` performs NO in-app merge/approve/request-changes/submit-review mutation — it only navigates
  the user to the PR page in the browser, where those deferred operations are completed.
- When no PR is selected/loaded, `o` is consumed and surfaces a `NoSelectionToOpen` notice (it is
  never a silent no-op).
- Help (`?`/`h`/`F1`) lists `o = open PR in browser` among the PR-Mode bindings.

Behavior contract:
- GIVEN the user is viewing PR #142 and wants to merge or approve it
- WHEN `o` is pressed
- THEN PR #142 is opened in the browser via `gh pr view 142 --repo <owner>/<name> --web` (spawned
  off-thread) and a non-blocking "opening…/opened" notice is shown; no in-app merge/approve
  keybinding exists and no in-app mutation occurs

- GIVEN the PR list is empty (no PR selected)
- WHEN `o` is pressed
- THEN the key is consumed and a `NoSelectionToOpen` notice ("No pull request selected to open") is
  shown (no silent no-op, no `gh` invocation)

### REQ-PR-013: Authentication and Error Handling
- v1 uses the active `gh` CLI auth context.
- Missing/invalid auth blocks PR operations and shows explicit remediation guidance
  ("Run: gh auth login").
- Non-auth errors (network/API/rate-limit/repo-access): show a scoped error in the list/detail,
  keep mode and focus stable, preserve unsaved drafts where feasible, and provide a retry
  affordance.
- All error paths surface a user-visible/logged message — no silent `None` match arms that drop
  unavailable-context cases (regression guard for #37/#39-class bugs).
- Repository scope change discards unsent inline drafts with a non-blocking notice.
- `github_repo` slug is validated for `owner/name` format before issuing requests; invalid or
  missing config yields a scoped "configure repository" message rather than a failed request.

Behavior contract:
- GIVEN `gh` CLI is not authenticated
- WHEN the user enters PR Mode
- THEN PR operations are blocked and remediation guidance is shown

- GIVEN a repository with no configured `github_repo` slug
- WHEN the user enters PR Mode for it
- THEN a scoped "configure repository (owner/name)" message is shown (not a request error)

### REQ-PR-014: Empty States
- No accessible repositories: explicit message.
- No PRs matching the current scoped criteria: scoped empty state (not an error).
- No reviews / no checks / no comments on the selected PR: scoped "none yet" displays.
- No agents available for send-to-agent: send disabled + message.

Behavior contract:
- GIVEN the selected repository has no open PRs
- WHEN PR Mode loads
- THEN the PR list shows "No pull requests match current filters" and the detail shows a scoped
  empty state (never stale prior-repository data)

---

## Non-Functional Requirements

### REQ-PR-NFR-001: Responsiveness (Non-Blocking I/O)
- All `gh` CLI calls execute off the UI thread via `spawn_gh_task_with_panic`; keyboard input is
  never blocked by network I/O.
- Loading states are shown during list/detail/review/check/comment operations.
- A panicking background task clears the relevant loading flag and surfaces an error rather than
  leaving a stuck spinner.

### REQ-PR-NFR-002: Reliability
- API failures must not crash the application.
- Mode and focus remain stable through errors.
- Stale responses (wrong scope or request id) are discarded deterministically.
- `prs_state` is transient and EXCLUDED from persistence (backward-compat invariant). Persistence
  does not serialize `AppState`; the on-disk state is a separate DTO (`persistence::State`) populated
  by `to_persisted_state`, which explicitly enumerates the persisted fields and omits `prs_state`
  exactly as it omits `issues_state`. The acceptance invariant is three-part and testable:
  (a) `prs_state` is NOT written to `state.json` (`to_persisted_state` emits no PR data);
  (b) an OLD `state.json` written before PR mode existed (no PR fields) still loads successfully
  with all prior fields intact; and (c) the resulting `AppState` has
  `prs_state == PullRequestsState::default()` (inactive). These are RED-tested in P04 and asserted
  in P04A (finding #2).

Behavior contract (backward-compat):
- GIVEN a `state.json` file written by a pre-PR-mode build (containing no PR-related fields)
- WHEN the application loads it
- THEN it loads successfully, all prior persisted fields are intact, and `AppState.prs_state` is at
  its inactive default (no PR data is ever written back on the next save)

### REQ-PR-NFR-003: Maintainability and Complexity Discipline
- The GitHub client boundary is isolated and unit-testable.
- PR state management follows the existing event/reducer/message-bus pattern.
- Every changed/added function stays within `clippy.toml` thresholds: cognitive-complexity ≤ 15,
  function lines ≤ 60, function args ≤ 6, type-complexity ≤ 250, struct-bools ≤ 3.
- NO `#[allow(clippy::...)]` attributes and NO threshold overrides anywhere in first-party code;
  handlers are decomposed into small functions so no override is ever needed.
- No duplicated layout constants; the typed `src/layout.rs` module is the single source.

---

## Testability Requirements

- All PR key-routing paths are testable via `AppState::apply()` / `apply_message()` event tests.
- The GitHub client boundary is structured so parse helpers are unit-testable against fixture JSON.
- PR list selection/pagination/scroll-following logic is testable without API calls.
- Inline composer exclusivity and composer-focus/auto-scroll behavior are testable via state
  assertions.
- Filter composition and per-field draft updates are testable as pure logic.
- Repo-nav-in-PR-mode (RepoList focus drives reload) is testable independent of `pane_focus`.
- Tests assert with `assert!(matches!(...))` patterns; no `unwrap`/`expect`/panic in test or
  production paths; no `assert!(true)`; no `#[ignore]`.

---

## Error/Edge Case Expectations

- `gh` CLI not installed: block operations, show install guidance (`GhError::NotInstalled`).
- Not authenticated: block operations, show `gh auth login` guidance.
- API rate limit / network / access denied: scoped error with retry; mode/focus stable.
- Repository has no PRs: empty state, not an error.
- PR closed/merged between list and detail load: handled gracefully with a scoped message.
- Review/check data missing or empty: scoped "none yet" summary, not an error.
- Malformed/partial review or check entries are NEVER silently dropped: the parser preserves each
  as a displayable degraded record (e.g. "(unknown reviewer)"/"(unparseable check)") that is still
  rendered and counted, so review/check counts match the source data.
- Very long PR body/comments/review bodies: scroll behavior with overflow derived from real
  rendered length; long titles truncated with an ellipsis by pane width.
- Concurrent scope change during an API call: the stale response is discarded by scope/request-id
  guard.
- Invalid `github_repo` format: scoped configuration message, no malformed request issued.

# Mockups — Pull Requests Mode

Plan ID: `PLAN-20260624-PR-MODE`

Layout placement, pane composition, and control anchoring in this file are normative where
referenced by `project-plans/issue20/plan/00-overview.md`. Visual styling examples remain
illustrative. Normative behavior is specified in `functional-overview.md`,
`technical-overview.md`, and the contracts under `project-plans/issue20/plan/*.md`.

## Layout Architecture

PR Mode uses a **two-column layout**, mirroring Issues Mode (not three columns):

1. **Left column**: Repositories sidebar — fixed width (22u = `LEFT_COL_WIDTH`/`PRS_SIDEBAR_WIDTH`),
   full screen height. Identical placement and sizing to the baseline dashboard and Issues Mode.
2. **Right column**: PR workspace — flex-grow, full height. Contains filters, the PR list, and the
   unified detail view (metadata + body + reviews + checks + comments + new-comment field).

PR detail, reviews, checks, and comments are a **single unified scrollable view**, not separate
panes or regions. The detail view contains, in continuous scroll order:
metadata → body → review summary → check summary → comments timeline → new comment field.

### Normative measurements (verifier-checkable)

| Aspect | Value | Source |
|--------|-------|--------|
| Column count | Two (repos + workspace) | mirrors Issues Mode |
| Repos sidebar width | 22u (`PRS_SIDEBAR_WIDTH == LEFT_COL_WIDTH`) | `src/layout.rs` |
| List/detail vertical split | ~30% list / ~70% detail of the workspace rows (after status/keybind bars and optional error/filter bands) | `prs_pane_rows()` |
| Status bar | top, full width, height 1 | `StatusBar` (reused) |
| Keybind bar | bottom, full width, height 1 | `KeybindBar` (reused) |
| Detail viewport height | passed as a prop derived from `prs_detail_viewport_rows()` | `src/layout.rs` |
| List title | truncated with an ellipsis to list pane content width (`pr_list_content_width()`) | `src/layout.rs` |

## 1) Baseline Dashboard Shell (Preserved)

```text
┌────────────────────────────────────────────────────────────────────────────────────────────────┐
│ Status bar                                                                                     │
├───────────────────────┬──────────────────────────────────────────────┬─────────────────────────┤
│ Repositories          │ Agents (top portion of center column)        │ Preview                │
│ (full height)         │                                              │ (full height)          │
│                       ├──────────────────────────────────────────────┤                         │
│                       │ Terminal (bottom portion, much taller)       │                         │
├───────────────────────┴──────────────────────────────────────────────┴─────────────────────────┤
│ Keybind bar                                                                                    │
└────────────────────────────────────────────────────────────────────────────────────────────────┘
```

## 2) PR Mode — List-Focused

Two-column layout: repos sidebar (full height) + PR workspace.

```text
┌ Status: PR MODE | Repo scope: vybestack/llxprt-jefe | Auth: gh signed-in (v1) ───────────────┐
├───────────────────────┬────────────────────────────────────────────────────────────────────────┤
│ Repositories          │ Pull Requests workspace                                               │
│ (full height)         │                                                                        │
│ > llxprt-jefe (gh OK) │ Filters: [state: open] [draft: any] [review: any] [checks: any]      │
│   docs-site  (gh OK)  │          [author: any] [assignee: any] [reviewer: any] [labels: any] │
│                       │          [search: ____________________________________________ ]      │
│   toy-app    (gh -)   │                                                                        │
│                       │ ┌────────────────────────────────────────────────────────────────────┐ │
│                       │ │ > #84 Add PR mode to dashboard    open  ✓checks  ~review  pat  2c │ │
│                       │ │   #82 Fix scroll clipping in list open  ✗checks  ✔review  ada  5c │ │
│                       │ │   #80 [draft] Spike: review panes draft •checks  -review  lee  0c │ │
│                       │ │   ...                                                              │ │
│                       │ │   [list scroll position indicator — selected row kept visible]    │ │
│                       │ └────────────────────────────────────────────────────────────────────┘ │
│                       │                                                                        │
│                       │ [f opens filter editor only while this list has focus]               │
├───────────────────────┴────────────────────────────────────────────────────────────────────────┤
│ Keybind: p focus PR list | r focus repos | a/Esc exit | / search | Tab cycle panes            │
└────────────────────────────────────────────────────────────────────────────────────────────────┘
```

Row legend (review/check status glyphs are illustrative):
`✓checks` success · `✗checks` failure · `•checks` pending · `✔review` approved ·
`~review` review-required/changes · `-review` none.

## 3) PR Mode — Detail-Focused (Unified Scrollable View)

The PR workspace splits vertically: PR list on top (selection drives detail), unified detail on
the bottom (one scrollable view: metadata → body → reviews → checks → comments → new comment).

```text
┌ Status: PR MODE | Focus: PR Detail ────────────────────────────────────────────────────────────┐
├───────────────────────┬────────────────────────────────────────────────────────────────────────┤
│ Repositories          │ Top: PR list (selection drives detail)                                │
│ (full height)         │                                                                        │
│ > llxprt-jefe         │ Bottom: PR detail (one scrollable view)                               │
│                       │ ┌────────────────────────────────────────────────────────────────────┐ │
│                       │ │ #84 Add PR mode to dashboard                                       │ │
│                       │ │ OPEN | author: pat | issue20 -> main | labels: feat | assignees:- │ │
│                       │ │ created: 2026-06-20 | updated: 2026-06-24                          │ │
│                       │ │ url: https://github.com/vybestack/llxprt-jefe/pull/84 (browser)   │ │
│                       │ │                                                                    │ │
│                       │ │ Description                                                         │ │
│                       │ │ [read-mode body text...]                                           │ │
│                       │ │                                                                    │ │
│                       │ │ Reviews  (decision: REVIEW_REQUIRED)                               │ │
│                       │ │ - ada    CHANGES_REQUESTED  2026-06-23  "please split handler"    │ │
│                       │ │ - lee    APPROVED           2026-06-24                             │ │
│                       │ │                                                                    │ │
│                       │ │ Checks  (rollup: SUCCESS)                                           │ │
│                       │ │ - ci/fmt      success   passed                                     │ │
│                       │ │ - ci/clippy   success   passed                                     │ │
│                       │ │ - ci/test     pending   running                                    │ │
│                       │ │                                                                    │ │
│                       │ │ Comments                                                            │ │
│                       │ │ - pat  2026-06-22  "ready for review"                              │ │
│                       │ │ - ada  2026-06-23 (focused)  "see review notes"                   │ │
│                       │ │   [Reply inline field: _______________________________________]   │ │
│                       │ │   [Save (Cmd/Ctrl+Enter)] [Cancel (Esc)]                          │ │
│                       │ │                                                                    │ │
│                       │ │ New comment                                                        │ │
│                       │ │ [Inline field: ________________________________________________]  │ │
│                       │ │ [Save (Cmd/Ctrl+Enter)] [Cancel (Esc)]                            │ │
│                       │ │                                                                    │ │
│                       │ │ [detail scroll position indicator — follows active composer]      │ │
│                       │ └────────────────────────────────────────────────────────────────────┘ │
├───────────────────────┴────────────────────────────────────────────────────────────────────────┤
│ c new comment | r reply | S send to agent | j/k subfocus | Tab cycle panes                     │
└────────────────────────────────────────────────────────────────────────────────────────────────┘
```

Key layout points:
- Metadata, body, reviews, checks, and comments are **one unified scrollable view** — not separate
  panes or regions.
- The scroll-position indicator is for the single combined view; scroll overflow is derived from
  the actual rendered content length.
- The reviews and checks sections are **read-only summaries** (navigable, never editable).
- Inline reply/new-comment fields appear in-place within the unified scroll; opening a composer
  auto-scrolls to reveal it.
- The new-comment field is at the bottom of the same scrollable view.

## 4) Send-to-Agent (Anchored in PR Workspace)

```text
┌ Send PR to agent (compact chooser anchored in PR workspace) ───────────────────────────────────┐
│ PR: #84 Add PR mode to dashboard (issue20 -> main)                                             │
│ Repository: vybestack/llxprt-jefe                                                              │
│ Target agent: [backend-owner ▼]   (existing agents only)                                       │
│                                                                                                │
│ Base prompt source: repository default (issue_base_prompt)                                     │
│ Prompt preview:                                                                                │
│ - repository base prompt                                                                       │
│ - PR title/body/branches/metadata context                                                      │
│ - review + check summary                                                                       │
│ - focused comment (if any)                                                                     │
│                                                                                                │
│ [Send (Enter)] [Cancel (Esc)]                                                                  │
└────────────────────────────────────────────────────────────────────────────────────────────────┘
```

## 5) Inline Error / Config Surfaces

```text
[PR list inline error banner]
Unable to load pull requests for vybestack/llxprt-jefe.
[Retry]

[Repository not configured]
No GitHub repo configured for this repository.
Set the GitHub slug in Edit Repository (owner/name).
[Open repository settings]

[Auth error]
GitHub CLI is not authenticated. Run: gh auth login
[Open auth help]

[Connectivity error]
Unable to reach the GitHub API endpoint.
[Retry]
```

## 6) Key Routing Snapshot

```text
Global in PR Mode:
  p    focus PR list
  a    exit PR Mode to Agents Mode
  Esc  cancel inner active control; otherwise exit PR Mode
  Tab / Shift+Tab  cycle panes (repo list -> PR list -> PR detail -> ...)
                   from EVERY pane INCLUDING PR detail (issue #46)
  ?/h/F1  help (with PR-Mode bindings)

PR List focus:
  Up/Down      move selection (selection-following keeps row visible)
  PageUp/Down  page scroll
  Home/End     boundary jump
  Left/Right   cycle panes (back / forward)
  Tab/Shift+Tab  cycle panes (fall through to pane-cycle; not consumed here)
  f            open filter controls (PR-list focus only)
  /            focus search input
  Enter        focus selected PR detail
  o            open selected PR in browser (gh pr view <n> --web); no-PR -> notice

PR Detail focus (unified scrollable view):
  Up/Down      scroll detail view
  j            cycle SUBFOCUS next: body -> reviews -> checks -> comments -> new comment -> body
  k            reverse pr-detail subfocus cycle
  Tab/Shift+Tab  cycle panes (NOT subfocus; issue #46 -> Tab cycles panes from detail too)
  Left         optional reverse pane-cycle back to PR list (parity; not the sole escape)
  c            open new-comment composer (sets subfocus, auto-scrolls)
  r            inline reply for focused comment (notice if comment not focused)
  e            read-only notice (PR body/reviews/checks not editable in v1)
  o            open selected PR in browser (gh pr view <n> --web); no-PR -> notice
  S            send-to-agent chooser (only when no inline composer active)

Repo List focus (PR Mode):
  Up/Down      move repository selection and reload PR scope
               (driven by PrFocus::RepoList, NOT dashboard pane_focus)
  Right        cycle to next pane
  Tab/Shift+Tab  cycle panes (fall through to pane-cycle; not consumed here)
  Enter        no-op (selection already active)

Suppressed in PR Mode:
  dashboard split binding on s/S
  dashboard a focus-agents binding
  dashboard destructive lifecycle shortcuts (Ctrl-d, Ctrl-k, l)

No-op in PR Mode:
  lowercase s
  Enter in repo list (selection already active)
  c/r/e on review/check items (read-only)
```

## 7) Search Lifecycle with Esc

```text
State A: search focused, text non-empty
Esc -> clear text, keep search focused

State B: search focused, text empty
Esc -> blur search input, keep PR Mode active

State C: no inner control consuming Esc
Esc -> exit PR Mode
```

## 8) Inline Composer (Comment / Reply)

```text
Allowed states:
- composer active (new comment OR reply)
- composer inactive

Disallowed:
- two composers active simultaneously
```

All inline controls appear in-place within the unified detail scrollable view and auto-scroll
into view when opened.

### New comment inline (at bottom of unified detail view)
```text
[New Comment]
+----------------------------------------------------------------+
| Investigated in staging; attaching trace IDs...               |
+----------------------------------------------------------------+
[Save (Cmd/Ctrl+Enter)] [Cancel (Esc)]
```

### Inline reply (within comments section of unified detail view)
```text
@ada
  please split the handler to satisfy cognitive-complexity
  [Reply]
  +--------------------------------------------------------------+
  | @ada Done — extracted handle_pr_detail_key sub-handlers.    |
  +--------------------------------------------------------------+
  [Save (Cmd/Ctrl+Enter)] [Cancel (Esc)]
```

## 9) Filter/Search Controls (Fully Interactive)

```text
PR List Header Controls
  Query:    [pr mode...........................................]   (separate `/` search input)
  EIGHT interactive filter fields (Tab cycling order):
  State:    [open v]      (Space cycles: open/closed/merged/all)
  Draft:    [any v]       (Space cycles: any/drafts-only/ready-only)
  Review:   [any v]       (Space cycles: any/approved/changes-requested/review-required/none)
  Checks:   [any v]       (Space cycles: any/success/failing/pending)
  Author:   [........]
  Assignee: [........]
  Reviewer: [........]
  Labels:   [feat,bug]

Actions:
  [Apply (Enter)] [Clear (Ctrl-c)] [Cancel (Esc)]

Field navigation:
  Tab / Shift+Tab move between the EIGHT fields (wrap-around); Space cycles the four enumerated
  fields (State, Draft, Review, Checks); character entry updates the four text fields (Author,
  Assignee, Reviewer, Labels) on the DRAFT filter live. The Query is the separate `/` search input.
  Review and Checks are issue #20 review/workflow signal filters, emitted as server-side
  `review:` / `status:` search qualifiers (cursor-pagination-safe).

Composition:
  all structured criteria (state, draft, review-decision, checks-status, author, assignee,
  reviewer, labels) AND text query

Default:
  committed filter state = open (Some(PrFilterState::Open)); all other structured
  criteria unset/empty (draft = any, review = any, checks = any,
  author/assignee/reviewer/labels empty, query empty)
```

## 10) Send-to-Agent Chooser

```text
Send to Agent (existing agents only)

( ) triage-bot
(x) backend-owner
( ) qa-assist

[Send (Enter)] [Cancel (Esc)]
```

No-agent state:

```text
No agents available. Create/select an agent in Agents Mode.
```

## 11) Empty and Error States

```text
No accessible repositories in current gh context.
No pull requests match current filters.
No reviews yet.
No checks reported.
No comments yet.

GitHub CLI is not authenticated. Run: gh auth login
Could not load pull requests for vybestack/llxprt-jefe. [Retry]
```

## Layout Summary

| Aspect | Correct (mirrors Issues Mode) | Wrong (drifted) |
|--------|-------------------------------|-----------------|
| Column count | **Two** (repos + workspace) | Three (repos + list + detail) |
| Repos sidebar | Full screen height, fixed 22u | Same — but three-column dilutes workspace |
| Detail + reviews + checks + comments | **One unified scrollable view** | Separate panes/regions per section |
| Reviews/checks | Read-only summaries within the detail scroll | Editable or separate interactive panes |
| Scroll indicators | One, for the unified detail view | Multiple for split regions |
| Detail viewport height | Prop from `prs_detail_viewport_rows()` | Independent `crossterm::size()` read |
| List title | Truncated with ellipsis to pane width | Clipped / overflowing |
| Inline controls | In-place within unified scroll, auto-scrolled into view | Ambiguous anchoring; rendered off-screen |

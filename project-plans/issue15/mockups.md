# Mockups — Issues Mode

Layout placement, pane composition, and control anchoring in this file are normative where referenced by `project-plans/issue15/plan/00-overview.md`. Visual styling examples remain illustrative.
Normative behavior is specified in `functional-overview.md` and the contracts under `project-plans/issue15/plan/*.md`.

## Layout Architecture

Issues Mode uses a **two-column layout**, not three columns:

1. **Left column**: Repositories sidebar — fixed width (22u), full screen height. Identical placement and sizing to the baseline dashboard.
2. **Right column**: Issues workspace — flex-grow, full height. Contains filters, issue list, and unified detail+comments view.

Issue detail and comments are a **single unified scrollable view**, not separate panes or regions. The detail view contains: issue metadata → body → comments timeline → new comment field, all in one continuous scroll.

## 1) Baseline Dashboard Shell (Preserved)

```text
┌────────────────────────────────────────────────────────────────────────────────────────────────┐
│ Status bar                                                                                     │
├───────────────────────┬──────────────────────────────────────────────┬─────────────────────────┤
│ Repositories          │ Agents (top portion of center column)        │ Preview                │
│ (full height)         │                                              │ (full height)          │
│                       ├──────────────────────────────────────────────┤                         │
│                       │ Terminal (bottom portion, much taller)       │                         │
│                       │                                              │                         │
│                       │                                              │                         │
├───────────────────────┴──────────────────────────────────────────────┴─────────────────────────┤
│ Keybind bar                                                                                    │
└────────────────────────────────────────────────────────────────────────────────────────────────┘
```

## 2) Issues Mode — List-Focused

Two-column layout: repos sidebar (full height) + issues workspace.

```text
┌ Status: ISSUE MODE | Repo scope: vybestack/llxprt-jefe | Auth: gh signed-in (v1) ────────────┐
├───────────────────────┬────────────────────────────────────────────────────────────────────────┤
│ Repositories          │ Issues workspace                                                      │
│ (full height)         │                                                                        │
│ > llxprt-jefe (gh [OK])  │ Filters: [state: open] [assignee: any] [labels: any] [milestone:any] │
│   docs-site  (gh [OK])   │          [type:any] [project:any] [search: ______________________ ]   │
│   toy-app    (gh -)   │                                                                        │
│                       │ ┌────────────────────────────────────────────────────────────────────┐ │
│                       │ │ > #17 Create a feature list and state diagram                     │ │
│                       │ │   #16 Create an initial UI design                                 │ │
│                       │ │   #15 Github Integration Main Issue                               │ │
│                       │ │   ...                                                              │ │
│                       │ │   [list scroll position indicator]                                │ │
│                       │ └────────────────────────────────────────────────────────────────────┘ │
│                       │                                                                        │
│                       │ [f opens filter editor only while this list has focus]               │
├───────────────────────┴────────────────────────────────────────────────────────────────────────┤
│ Keybind: i focus issue list | r focus repos | a or Esc exit issues mode | / focus search      │
└────────────────────────────────────────────────────────────────────────────────────────────────┘
```

## 3) Issues Mode — Detail-Focused (Inline Comment/Reply/Edit)

The issues workspace splits vertically: issue list on top (selection drives detail), unified detail+thread on bottom (one scrollable view).

```text
┌ Status: ISSUE MODE | Focus: Issue Detail ──────────────────────────────────────────────────────┐
├───────────────────────┬────────────────────────────────────────────────────────────────────────┤
│ Repositories          │ Top: issue list (selection drives detail)                             │
│ (full height)         │                                                                        │
│ > llxprt-jefe         │ Bottom: issue detail + thread (one scrollable view)                   │
│                       │ ┌────────────────────────────────────────────────────────────────────┐ │
│                       │ │ #17 Create a feature list and state diagram                        │ │
│                       │ │ OPEN | assignees: - | labels: - | milestone: - | type: -         │ │
│                       │ │ project: -                                                         │ │
│                       │ │                                                                    │ │
│                       │ │ Issue description                                                   │ │
│                       │ │ [read mode text...]                                                │ │
│                       │ │                                                                    │ │
│                       │ │ Comments                                                            │ │
│                       │ │ - Comment A                                                        │ │
│                       │ │   [Reply inline field: ________________________________________]   │ │
│                       │ │   [Submit] [Cancel]                                                │ │
│                       │ │                                                                    │ │
│                       │ │ - Comment B (focused)                                              │ │
│                       │ │                                                                    │ │
│                       │ │ New comment                                                        │ │
│                       │ │ [Inline field: ________________________________________________]   │ │
│                       │ │ [Submit] [Cancel]                                                  │ │
│                       │ │                                                                    │ │
│                       │ │ [detail/thread scroll position indicator]                          │ │
│                       │ └────────────────────────────────────────────────────────────────────┘ │
├───────────────────────┴────────────────────────────────────────────────────────────────────────┤
│ e on focused body => inline issue-body edit; e on focused comment => inline comment edit       │
└────────────────────────────────────────────────────────────────────────────────────────────────┘
```

Key layout points:
- Detail and comments are **one unified scrollable view** — not separate panes or regions.
- Scroll position indicator is for the single combined detail+comments view.
- Inline reply/edit fields appear in-place within the unified scroll.
- New comment field is at the bottom of the same scrollable view.

## 4) Send-to-Agent (Anchored in Issues Workspace)

```text
┌ Send issue to agent (inline panel or compact modal anchored in issues workspace) ─────────────┐
│ Issue: #17 Create a feature list and state diagram                                             │
│ Repository: vybestack/llxprt-jefe                                                              │
│ Target agent: [agent-a ▼]   (existing agents only)                                             │
│                                                                                                │
│ Base prompt source: repository default                                                         │
│ Prompt preview:                                                                                │
│ - repository base prompt                                                                       │
│ - issue title/body/metadata context                                                            │
│ - comments included option                                                                     │
│                                                                                                │
│ [Launch] [Cancel]                                                                              │
└────────────────────────────────────────────────────────────────────────────────────────────────┘
```

## 5) Inline Error Surfaces

```text
[Issue list inline error banner]
GraphQL request failed: insufficient repository permissions for project/type fields.
[Retry] [Open auth help]

[Repository not configured in detail pane]
No GitHub repo configured for this repository.
Set repository GitHub slug in Edit Repository (owner/name).
[Open repository settings]

[Connectivity error]
Unable to reach GitHub API endpoint.
[Retry]
```

## 6) Key Routing Snapshot

```text
Global in Issues Mode:
  i    focus issue list
  a    exit Issues Mode to Agents Mode
  Esc  cancel inner active control; otherwise exit Issues Mode

Issue List focus:
  Up/Down      move selection
  PageUp/Down  page scroll
  Home/End     boundary jump
  f            open filter controls (issue-list focus only)
  /            focus search input
  Enter        focus selected issue detail

Issue Detail focus (unified scrollable view):
  Up/Down      scroll detail+comments view
  Tab          cycle subfocus: body -> comments -> new comment -> body
  Shift+Tab    reverse issue-detail subfocus cycle
  e            edit focused issue body/comment inline
  r            open inline reply for focused comment (no-op if comment not focused)
  S            send-to-agent chooser (only when no inline editor/composer active)

Suppressed in Issues Mode:
  dashboard split binding on s/S
  dashboard a focus-agents binding
  dashboard destructive lifecycle shortcuts (Ctrl-d, Ctrl-k, l)

No-op in Issues Mode:
  lowercase s
  Enter in repo list (selection already active)
```

## 7) Search Lifecycle with Esc

```text
State A: search focused, text non-empty
Esc -> clear text, keep search focused

State B: search focused, text empty
Esc -> blur search input, keep Issues Mode active

State C: no inner control consuming Esc
Esc -> exit Issues Mode
```

## 8) Inline Composer and Editor (Mutually Exclusive)

```text
Allowed states:
- editor active, composer inactive
- composer active, editor inactive
- neither active

Disallowed state:
- editor active and composer active at the same time
```

All inline controls appear in-place within the unified detail+comments scrollable view:

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
@pat
  Can we reproduce with token expiry?
  [Reply]
  +--------------------------------------------------------------+
  | @pat Yes, repro steps confirmed on latest main.             |
  +--------------------------------------------------------------+
  [Save (Cmd/Ctrl+Enter)] [Cancel (Esc)]
```

### Inline edit (within unified detail view, on focused body or comment)
```text
[Editing Focused Item]
+----------------------------------------------------------------+
| Updated root-cause notes and mitigation details...            |
+----------------------------------------------------------------+
[Save (Cmd/Ctrl+Enter)] [Cancel (Esc)]
```

## 9) Filter/Search Controls

```text
Issue List Header Controls
  Query: [refresh token.................................]
  State: [open v]
  Author: [........]
  Assignee: [me v]
  Labels: [bug,auth]
  Mentioned: [me v]
  Updated: [after ....] [before ....]

Actions:
  [Apply] [Clear] [Cancel]

Composition:
  all structured criteria AND text query

Default:
  no structured filters committed
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
No issues match current filters.
No comments yet.

GitHub CLI is not authenticated. Run: gh auth login
Could not load issues for acme/api. [Retry]
```

## 12) Repository Config: `issue_base_prompt`

```text
Repository Settings: acme/api

Issue Base Prompt
(Reusable instruction text included in send-to-agent payload)

+----------------------------------------------------------------+
| Prioritize reproducible diagnosis and include rollback risk.   |
+----------------------------------------------------------------+

[Save] [Reset to last saved]
```

## Layout Summary

| Aspect | Correct (per #16) | Wrong (drifted) |
|--------|-------------------|-----------------|
| Column count | **Two** (repos + workspace) | Three (repos + list + detail) |
| Repos sidebar | Full screen height, fixed 22u | Same — but three-column dilutes workspace |
| Detail + comments | **One unified scrollable view** | Separate detail pane + separate comments region |
| Comments region | Part of detail scroll | Full-width separate section at bottom |
| Scroll indicators | One, for unified detail+comments | Potentially multiple for split regions |
| Inline controls | In-place within unified scroll | Ambiguous anchoring across split regions |

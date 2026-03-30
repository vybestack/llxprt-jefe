# Mockups — Issues Mode (Illustrative, Non-Normative)

These mockups are illustrative only.
Normative behavior is specified in `functional-overview.md` and `technical-overview.md`.

## 1) Issues Mode Layout

```text
+----------------------------------------------------------------------------------------------------+
| Header: [Mode: Issues] [Repo Scope: acme/api] [Sort: Updated desc] [Filters]                      |
+----------------------------+------------------------------------+----------------------------------+
| Repositories               | Issue List                         | Issue Detail                     |
|----------------------------|------------------------------------|----------------------------------|
| > acme/api                 | > #142 Fix auth refresh            | #142 Fix auth refresh            |
|   acme/web                 |   open  @sam  upd 2h               | state: open                      |
|   acme/infra               |   labels: bug,auth                 | author: @sam                     |
|                            |   comments: 5                      | created: ...  updated: ...       |
|                            |------------------------------------| labels: ...                      |
|                            |   #141 Add export CSV              | assignees: ...                   |
|                            |   closed @lee  upd 1d              | milestone: v1.4                  |
|                            |                                    | body: ...                        |
|                            |                                    | [Open in GitHub (o)] [Send to Agent (S)]
+----------------------------+------------------------------------+----------------------------------+
| Detail sub-region: Comments                                                                         |
|------------------------------------------------------------------------------------------------------|
| @pat 2026-03-28                                                                                     |
|   Can we reproduce with token expiry?                                                               |
|   [Press r for inline reply]                                                                        |
|                                                                                                      |
| @sam 2026-03-29 (edited)                                                                            |
|   Added logs and narrowed to refresh path.                                                          |
+------------------------------------------------------------------------------------------------------+
```

## 2) Key Routing Snapshot

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

Issue Detail focus:
  Up/Down      scroll detail
  Tab          cycle subfocus: body -> comments -> new comment -> body
  Shift+Tab    reverse issue-detail subfocus cycle
  e            edit focused issue body/comment inline
  r            open inline reply for focused comment (no-op if comment not focused)
  o            open selected issue in GitHub
  S            send-to-agent chooser (only when no inline editor/composer active)

Suppressed in Issues Mode:
  dashboard split binding on s/S
  dashboard a focus-agents binding
  dashboard destructive lifecycle shortcuts (Ctrl-d, Ctrl-k, l)

No-op in Issues Mode:
  lowercase s
  Enter in repo list (selection already active)
```

## 3) Search Lifecycle with Esc

```text
State A: search focused, text non-empty
Esc -> clear text, keep search focused

State B: search focused, text empty
Esc -> blur search input, keep Issues Mode active

State C: no inner control consuming Esc
Esc -> exit Issues Mode
```

## 4) Inline Composer and Editor (Mutually Exclusive)

```text
Allowed states:
- editor active, composer inactive
- composer active, editor inactive
- neither active

Disallowed state:
- editor active and composer active at the same time
```

### New comment inline
```text
[New Comment]
+----------------------------------------------------------------+
| Investigated in staging; attaching trace IDs...               |
+----------------------------------------------------------------+
[Save (Cmd/Ctrl+Enter)] [Cancel (Esc)]
```

### Inline reply
```text
@pat
  Can we reproduce with token expiry?
  [Reply]
  +--------------------------------------------------------------+
  | @pat Yes, repro steps confirmed on latest main.             |
  +--------------------------------------------------------------+
  [Save (Cmd/Ctrl+Enter)] [Cancel (Esc)]
```

### Inline edit (issue body/comment)
```text
[Editing Focused Item]
+----------------------------------------------------------------+
| Updated root-cause notes and mitigation details...            |
+----------------------------------------------------------------+
[Save (Cmd/Ctrl+Enter)] [Cancel (Esc)]
```

## 5) Filter/Search Controls

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

## 6) Send-to-Agent Chooser

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

## 7) Empty and Error States

```text
No accessible repositories in current gh context.
No issues match current filters.
No comments yet.

GitHub CLI is not authenticated. Run: gh auth login
Could not load issues for acme/api. [Retry]
```

## 8) Repository Config: `issue_base_prompt`

```text
Repository Settings: acme/api

Issue Base Prompt
(Reusable instruction text included in send-to-agent payload)

+----------------------------------------------------------------+
| Prioritize reproducible diagnosis and include rollback risk.   |
+----------------------------------------------------------------+

[Save] [Reset to last saved]
```

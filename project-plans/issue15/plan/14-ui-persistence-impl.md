# Phase 14: UI Components + Persistence Implementation

## Phase ID
`PLAN-20260329-ISSUES-MODE.P14`

## Prerequisites
- Required: Phase P13A completed
- Verify previous phase markers/artifacts exist: `.completed/P13.md`, `.completed/P13A.md`
- Expected files from previous phase: failing (RED) test suite for UI + persistence (14 tests)

## Requirements Implemented (Expanded)

### REQ-ISS-006: Issue List Display — Implementation
**Requirement text**: Each row: number, title, state, author, updated timestamp, assignee summary, label summary, comment count. Default sort: `updated_at desc`, tie-breaker `number asc`.

Behavior contract:
- GIVEN `IssuesState` with 5 issues, second selected
- WHEN issue list component renders
- THEN 5 rows displayed with all 8 fields visible; second row is highlighted

- GIVEN `IssuesState` with `list_loading = true`
- WHEN issue list component renders
- THEN loading indicator is shown

Why it matters:
- Issue list is the primary browsing surface; all 8 fields must be present so the user can triage without opening each issue.

### REQ-ISS-008: Filter Controls — Implementation
**Requirement text**: Filter form with state, author, assignee, labels, date bounds. Apply/Clear/Cancel.

Behavior contract:
- GIVEN filter controls open with draft filter
- WHEN filter controls component renders
- THEN all filter fields visible with current draft values; Apply/Clear/Cancel actions available

Why it matters:
- Filter controls reduce the visible issue set; all fields must render correctly so users can narrow large backlogs.

### REQ-ISS-009: Issue Detail and Comments — Implementation
**Requirement text**: Detail displays all fields, comments timeline, markdown as terminal-friendly text.

Behavior contract:
- GIVEN `IssueDetail` with body text and 3 comments
- WHEN detail component renders
- THEN header fields, body text, and 3 comment blocks are displayed

Why it matters:
- Detail pane is the primary reading surface; markdown must be translated to terminal-friendly output so users can read formatted content.

### REQ-ISS-010: Inline Create/Edit — UI Implementation
**Requirement text**: New-comment field, reply field with @author, edit field. Save: Ctrl+Enter. Cancel: Esc. At most one inline mutable control active at a time.

Behavior contract:
- GIVEN inline composer active with target `NewComment`
- WHEN detail component renders
- THEN new-comment input area is visible with draft text and save/cancel hints

Why it matters:
- Inline exclusivity guards must be enforced at the rendering level as well as the state level to prevent two active input areas simultaneously.

### REQ-ISS-011: Agent Chooser — Implementation
**Requirement text**: Overlay with agent list, selection, confirm/cancel.

Behavior contract:
- GIVEN agent chooser open with 3 agents, second selected
- WHEN agent chooser component renders
- THEN 3 agent names listed; second highlighted; Enter/Esc hints shown

Why it matters:
- Agent chooser is the send-to-agent entry point; it must render selection state accurately for the user to choose the correct agent.

### REQ-ISS-012: Repository Config `issue_base_prompt` — Implementation
**Requirement text**: Multiline field in repository config screen. Save and Reset.

Behavior contract:
- GIVEN repository form with `issue_base_prompt` field
- WHEN field is rendered and edited
- THEN multiline input is functional; Save persists value; Reset restores last-saved

Why it matters:
- `issue_base_prompt` is included in every send-to-agent payload; a broken form means the value is never set or persisted correctly.

### REQ-ISS-014: Empty States — Implementation
**Requirement text**: Explicit empty state messages for no issues, no comments, no agents.

Behavior contract:
- GIVEN empty issue list after filter applied
- WHEN issue list renders
- THEN "No issues match current filters" message displayed

Why it matters:
- Empty states must be explicit messages, not blank areas; users need to know whether the list is loading, empty, or filtered-out.

## Implementation Tasks

### Files to modify
- `src/ui/components/issue_list.rs`
  - Implement full issue list rendering: row layout, field display, selection highlight, loading state, empty state
  - marker: `@plan PLAN-20260329-ISSUES-MODE.P14`
  - marker: `@requirement REQ-ISS-006,014`
  - Traceability: component function MUST include `@plan`, `@requirement` markers

- `src/ui/components/issue_detail.rs`
  - Implement detail rendering: header fields, body text (terminal-friendly markdown), comments timeline, inline controls
  - marker: `@plan PLAN-20260329-ISSUES-MODE.P14`
  - marker: `@requirement REQ-ISS-009,010`

- `src/ui/components/filter_controls.rs`
  - Implement filter form: all filter fields, Apply/Clear/Cancel actions
  - marker: `@plan PLAN-20260329-ISSUES-MODE.P14`
  - marker: `@requirement REQ-ISS-008`

- `src/ui/components/agent_chooser.rs`
  - Implement agent chooser overlay: agent list, selection highlight, confirm/cancel
  - marker: `@plan PLAN-20260329-ISSUES-MODE.P14`
  - marker: `@requirement REQ-ISS-011`

- `src/ui/screens/issues.rs`
  - Implement full three-pane layout with conditional visibility for filter controls and agent chooser overlays
  - marker: `@plan PLAN-20260329-ISSUES-MODE.P14`
  - marker: `@requirement REQ-ISS-001`

- `src/ui/screens/new_repository.rs`
  - Implement `issue_base_prompt` multiline field with cursor handling, Save, Reset
  - marker: `@plan PLAN-20260329-ISSUES-MODE.P14`
  - marker: `@requirement REQ-ISS-012`

- `src/ui/components/keybind_bar.rs`
  - Implement issues mode keybinding display with correct bindings per focus domain
  - marker: `@plan PLAN-20260329-ISSUES-MODE.P14`

- `src/ui/screens/dashboard.rs`
  - Finalize conditional rendering between agents and issues mode layouts
  - marker: `@plan PLAN-20260329-ISSUES-MODE.P14`
  - marker: `@requirement REQ-ISS-001`

- `src/persistence/mod.rs`
  - Verify `issue_base_prompt` serialization works end-to-end (may already work via domain type)
  - marker: `@plan PLAN-20260329-ISSUES-MODE.P14`
  - marker: `@requirement REQ-ISS-012`

### Pseudocode traceability
- Uses pseudocode lines:
  - component-001 lines 89–95: `list_loading`, `selected_issue_index`, `has_more_issues` flags → issue list rendering states
  - component-001 lines 107, 133–157: `detail_subfocus` variant → detail subfocus highlight rendering
  - component-001 lines 115–127: `inline_state`, `agent_chooser`, `filter_controls_open` → overlay visibility
  - component-002 lines 62–74: `build_send_payload` reads `issue_base_prompt` → form must save correctly
  - component-003 lines 63–72: `r` and `S` hint visibility conditions → keybind bar conditional display

## Verification Commands

```bash
cargo fmt --all --check
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo test --workspace --all-features
```

## Structural Verification Checklist
- [ ] All P13 RED tests now pass (GREEN)
- [ ] No new compilation warnings
- [ ] `@plan`, `@requirement` markers present in all changed files
- [ ] All existing tests pass (zero regressions)
- [ ] No stub rendering content remains (no "placeholder" or "loading..." as final state when data is loaded)

## Semantic Verification Checklist (Mandatory)
- [ ] Issue list renders all 8 fields per row (number, title, state, author, updated, assignees, labels, comment count)
- [ ] Issue list shows loading, empty, and normal states correctly
- [ ] Issue detail renders all detail fields + body + comments timeline
- [ ] Inline composer/editor visible when active; hidden when not
- [ ] Filter controls render all filter fields with Apply/Clear/Cancel
- [ ] Agent chooser renders agent list with selection
- [ ] Empty states show correct messages for all 3 categories (issues, comments, agents)
- [ ] Keybind bar shows issues mode bindings (different from agents mode)
- [ ] `issue_base_prompt` form field works in repository config (multiline, Save, Reset)
- [ ] Dashboard switches between agents and issues layouts based on `screen_mode` (`Dashboard` vs `DashboardIssues`)
- [ ] Feature behavior is reachable from real app flow: UI rendering path handles all `IssuesState` conditions that key routing + state reducer can produce
- [ ] No placeholder/deferred rendering patterns remain in production code paths

## Deferred Implementation Detection (Mandatory)

```bash
grep -RIn "TODO\|FIXME\|HACK\|placeholder\|for now\|will be implemented" src/ui/ src/persistence/
```

## Success Criteria
- [ ] All UI + persistence tests GREEN (14 tests)
- [ ] Verification commands pass
- [ ] No placeholder rendering code remains
- [ ] Dashboard renders agents mode correctly when `ScreenMode::Dashboard` (backward compat preserved)
- [ ] Persistence round-trip works for existing and new state

## Failure Recovery
- rollback steps: `git restore src/ui/ src/persistence/`
- blocking issues: rendering crashes, iocraft component errors, broken agents mode rendering

## Phase Completion Marker
Create: `project-plans/issue15/.completed/P14.md`

Contents:
- phase ID: `PLAN-20260329-ISSUES-MODE.P14`
- timestamp
- files changed
- tests that went from RED to GREEN (list)
- verification command outputs
- semantic verification summary

# Phase 10: Key Routing and Input Dispatch TDD

## Phase ID
`PLAN-20260329-ISSUES-MODE.P10`

## Prerequisites
- Required: Phase P09A completed
- Verify previous phase markers/artifacts exist: `.completed/P09.md`, `.completed/P09A.md`
- Expected files from previous phase: key routing stubs in `src/app_input.rs`, input mode extensions in `src/input.rs`

## Requirements Implemented (Expanded)

### REQ-ISS-001: Mode Entry and Exit — TDD
**Requirement text**: `i` from non-issues context enters `dashboard_issues` with `focus=issue_list`. `a` from `dashboard_issues` exits to `dashboard_agents`.

Behavior contract:
- GIVEN state in `Dashboard` (agents mode)
- WHEN `i` key is dispatched through key handler
- THEN `EnterIssuesMode` event fires; state transitions to `DashboardIssues`

- GIVEN state in `DashboardIssues`
- WHEN `a` key is dispatched
- THEN `ExitIssuesMode` event fires; state transitions to `Dashboard`

Why it matters:
- Mode entry and exit are the gatekeeping transitions; if they fail, Issues Mode is permanently unreachable or unescapable

### REQ-ISS-002: Key Suppression and Routing Priority — TDD
**Requirement text**: While in Issues Mode: suppress `a` focus-agents, `s/S` split-mode, destructive lifecycle keys (`Ctrl-d`, `Ctrl-k`, `l`). Priority chain: inline > chooser > search > filter > focus-domain > global > suppression.

Behavior contract:
- GIVEN state in `DashboardIssues` mode
- WHEN `s`, `Ctrl-d`, `Ctrl-k`, or `l` are dispatched
- THEN keys are consumed as no-op; state is unchanged

- GIVEN issues mode with inline editor active
- WHEN `Esc` is pressed
- THEN inline editor is cancelled; mode remains `DashboardIssues`; chooser and mode exit do not fire

- GIVEN issues mode with agent chooser open and no inline active
- WHEN `Esc` is pressed
- THEN agent chooser closes; mode remains `DashboardIssues`

- GIVEN issues mode with search input focused containing text
- WHEN `Esc` is pressed
- THEN search text is cleared; search input remains focused

Why it matters:
- Suppression prevents destructive agent operations from firing during issue browsing; priority ordering determines which control owns each key

### REQ-ISS-003: Pane Focus and Navigation — TDD
**Requirement text**: Issues Mode pane cycle: `repo_list -> issue_list -> issue_detail -> repo_list`. Issue list: Up/Down, PageUp/PageDown, Home/End, Enter focuses detail.

Behavior contract:
- GIVEN issue list focused with issues loaded
- WHEN `Down` is pressed
- THEN `IssuesNavigateDown` event fires; selection moves down

- GIVEN issue list focused with issue selected
- WHEN `Enter` is pressed
- THEN focus transitions to `IssueDetail`

- GIVEN issues mode active
- WHEN `Tab` is pressed
- THEN focus cycles: repo_list → issue_list → issue_detail → repo_list

- GIVEN issues mode active
- WHEN `Shift+Tab` is pressed
- THEN focus cycles in reverse

Why it matters:
- Navigation is how the user moves through the three-pane layout; incorrect cycling traps focus

### REQ-ISS-004: Esc Precedence Chain — TDD
**Requirement text**: Esc precedence: cancel inline > cancel chooser > clear search text > blur search > close filter controls > exit mode.

Behavior contract:
- GIVEN inline editor active
- WHEN `Esc` is pressed
- THEN inline is cancelled; higher-priority targets do not fire

- GIVEN agent chooser open, no inline active
- WHEN `Esc` is pressed
- THEN chooser is closed; mode does not exit

Why it matters:
- The precedence chain ensures each Esc press is unambiguous; violating the order cancels the wrong control

### REQ-ISS-008: Search and Filter Keys — TDD
**Requirement text**: `/` focuses search; `f` opens filter controls from issue-list focus only.

Behavior contract:
- GIVEN issue list focused
- WHEN `/` is pressed
- THEN search input is focused

- GIVEN issue list focused
- WHEN `f` is pressed
- THEN filter controls open

- GIVEN issue detail focused (not issue list)
- WHEN `f` is pressed
- THEN `f` is no-op with hint

Why it matters:
- Filter and search are only meaningful when the issue list is in focus; opening them from other panes is confusing

### REQ-ISS-010: Inline Mutation Keys — TDD
**Requirement text**: `e` edits focused issue body or comment. `r` on focused comment opens inline reply with `@author` prefill. Exclusivity: at most one inline control active at a time.

Behavior contract:
- GIVEN issue detail with body focused, no inline control active
- WHEN `e` is pressed
- THEN inline editor opens for issue body

- GIVEN issue detail with comment focused, no inline control active
- WHEN `e` is pressed
- THEN inline editor opens for that comment

- GIVEN issue detail with comment focused, no inline control active
- WHEN `r` is pressed
- THEN reply composer opens with `@author ` pre-filled

- GIVEN issue detail with no comment focused
- WHEN `r` is pressed
- THEN `r` is no-op with hint; no composer opens

- GIVEN inline composer active
- WHEN `e` is pressed
- THEN `e` is consumed by the inline handler; no second editor opens (exclusivity)

Why it matters:
- Exclusivity prevents two simultaneous composers from producing conflicting edits

### REQ-ISS-011: Send-to-Agent Keys — TDD
**Requirement text**: `S` from issue detail opens agent chooser. Chooser: Up/Down, Enter confirm, Esc cancel. No-agent case: disable send, show message.

Behavior contract:
- GIVEN issue detail focused, no inline control active, agents exist
- WHEN `S` is pressed
- THEN agent chooser opens with agents listed

- GIVEN issue detail focused, no inline control active, no agents exist
- WHEN `S` is pressed
- THEN "No agents available" message shown; chooser does not open

- GIVEN agent chooser open, inline active
- WHEN `S` is pressed
- THEN `S` is consumed by inline handler; chooser does not open a second chooser

Why it matters:
- `S` must be context-sensitive; silently sending to an agent when none exist, or when inline is active, produces undefined behavior

## Implementation Tasks

### Files to create or modify
- Tests in inline `#[cfg(test)]` modules in `src/app_input.rs` or `src/state/mod.rs`:
  - `test_i_key_enters_issues_mode`
    - marker: `@plan PLAN-20260329-ISSUES-MODE.P10`
    - marker: `@requirement REQ-ISS-001`
    - marker: `@pseudocode component-003 lines 01-38`
  - `test_a_key_exits_issues_mode`
    - marker: `@plan PLAN-20260329-ISSUES-MODE.P10`
    - marker: `@requirement REQ-ISS-001`
    - marker: `@pseudocode component-003 lines 01-38`
  - `test_s_key_suppressed_in_issues_mode`
    - marker: `@plan PLAN-20260329-ISSUES-MODE.P10`
    - marker: `@requirement REQ-ISS-002`
    - marker: `@pseudocode component-003 lines 28-38`
  - `test_ctrl_d_suppressed_in_issues_mode`
    - marker: `@plan PLAN-20260329-ISSUES-MODE.P10`
    - marker: `@requirement REQ-ISS-002`
    - marker: `@pseudocode component-003 lines 28-38`
  - `test_ctrl_k_suppressed_in_issues_mode`
    - marker: `@plan PLAN-20260329-ISSUES-MODE.P10`
    - marker: `@requirement REQ-ISS-002`
    - marker: `@pseudocode component-003 lines 28-38`
  - `test_l_key_suppressed_in_issues_mode`
    - marker: `@plan PLAN-20260329-ISSUES-MODE.P10`
    - marker: `@requirement REQ-ISS-002`
    - marker: `@pseudocode component-003 lines 28-38`
  - `test_slash_focuses_search_in_issues_mode`
    - marker: `@plan PLAN-20260329-ISSUES-MODE.P10`
    - marker: `@requirement REQ-ISS-002`
    - marker: `@requirement REQ-ISS-008`
    - marker: `@pseudocode component-003 lines 112-119`
  - `test_f_opens_filter_from_issue_list_focus`
    - marker: `@plan PLAN-20260329-ISSUES-MODE.P10`
    - marker: `@requirement REQ-ISS-008`
    - marker: `@pseudocode component-003 lines 120-127`
  - `test_f_noop_from_non_issue_list_focus`
    - marker: `@plan PLAN-20260329-ISSUES-MODE.P10`
    - marker: `@requirement REQ-ISS-008`
    - marker: `@pseudocode component-003 lines 120-127`
  - `test_esc_inline_priority_over_mode_exit`
    - marker: `@plan PLAN-20260329-ISSUES-MODE.P10`
    - marker: `@requirement REQ-ISS-002`
    - marker: `@requirement REQ-ISS-004`
    - marker: `@pseudocode component-003 lines 01-17`
    - marker: `@pseudocode component-001 lines 115-127`
  - `test_esc_chooser_priority_over_mode_exit`
    - marker: `@plan PLAN-20260329-ISSUES-MODE.P10`
    - marker: `@requirement REQ-ISS-002`
    - marker: `@requirement REQ-ISS-004`
    - marker: `@pseudocode component-003 lines 01-17`
    - marker: `@pseudocode component-001 lines 115-127`
  - `test_down_in_issue_list_dispatches_navigate`
    - marker: `@plan PLAN-20260329-ISSUES-MODE.P10`
    - marker: `@requirement REQ-ISS-003`
    - marker: `@pseudocode component-003 lines 39-50`
  - `test_enter_in_issue_list_focuses_detail`
    - marker: `@plan PLAN-20260329-ISSUES-MODE.P10`
    - marker: `@requirement REQ-ISS-003`
    - marker: `@pseudocode component-003 lines 39-50`
  - `test_tab_cycles_issues_pane_focus`
    - marker: `@plan PLAN-20260329-ISSUES-MODE.P10`
    - marker: `@requirement REQ-ISS-003`
    - marker: `@pseudocode component-001 lines 71-82`
  - `test_shift_tab_reverse_cycles`
    - marker: `@plan PLAN-20260329-ISSUES-MODE.P10`
    - marker: `@requirement REQ-ISS-003`
    - marker: `@pseudocode component-001 lines 71-82`
  - `test_e_opens_editor_on_body`
    - marker: `@plan PLAN-20260329-ISSUES-MODE.P10`
    - marker: `@requirement REQ-ISS-010`
    - marker: `@pseudocode component-003 lines 51-72`
  - `test_e_opens_editor_on_comment`
    - marker: `@plan PLAN-20260329-ISSUES-MODE.P10`
    - marker: `@requirement REQ-ISS-010`
    - marker: `@pseudocode component-003 lines 51-72`
  - `test_r_opens_reply_on_comment`
    - marker: `@plan PLAN-20260329-ISSUES-MODE.P10`
    - marker: `@requirement REQ-ISS-010`
    - marker: `@pseudocode component-003 lines 51-72`
    - marker: `@pseudocode component-003 lines 136-137`
  - `test_r_noop_when_not_on_comment`
    - marker: `@plan PLAN-20260329-ISSUES-MODE.P10`
    - marker: `@requirement REQ-ISS-010`
    - marker: `@pseudocode component-003 lines 51-72`
  - `test_S_opens_agent_chooser`
    - marker: `@plan PLAN-20260329-ISSUES-MODE.P10`
    - marker: `@requirement REQ-ISS-011`
    - marker: `@pseudocode component-003 lines 102-111`
  - `test_S_noop_when_inline_active`
    - marker: `@plan PLAN-20260329-ISSUES-MODE.P10`
    - marker: `@requirement REQ-ISS-011`
    - marker: `@pseudocode component-003 lines 138-141`
  - `test_S_shows_message_when_no_agents`
    - marker: `@plan PLAN-20260329-ISSUES-MODE.P10`
    - marker: `@requirement REQ-ISS-011`
    - marker: `@pseudocode component-003 lines 102-111`
  - `test_input_mode_issues_normal`
    - marker: `@plan PLAN-20260329-ISSUES-MODE.P10`
    - marker: `@requirement REQ-ISS-002`
    - marker: `@pseudocode component-003 lines 01-17`
  - `test_input_mode_issues_inline`
    - marker: `@plan PLAN-20260329-ISSUES-MODE.P10`
    - marker: `@requirement REQ-ISS-002`
    - marker: `@pseudocode component-003 lines 01-17`
  - `test_input_mode_issues_chooser`
    - marker: `@plan PLAN-20260329-ISSUES-MODE.P10`
    - marker: `@requirement REQ-ISS-002`
    - marker: `@pseudocode component-003 lines 01-17`

### Pseudocode traceability (if impl phase)
- Uses pseudocode component-003 lines 01-141 (full key routing + inline + chooser)
  - lines 01-38: priority chain and suppression
  - lines 39-50: issue list key handler
  - lines 51-72: issue detail key handler
  - lines 73-101: inline key handler and submit
  - lines 102-111: agent chooser key handler
  - lines 112-119: search input key handler
  - lines 120-127: filter controls key handler
  - lines 136-137: compose_reply_prefill
  - lines 138-141: exclusivity_guard
- Uses pseudocode component-001 lines 71-82 (pane focus cycle)
- Uses pseudocode component-001 lines 115-127 (Esc precedence — state-side)

## Verification Commands

```bash
cargo fmt --all --check
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo test --workspace --all-features
```

## Structural Verification Checklist
- [ ] All 25 planned test names exist and compile
- [ ] Tests target key routing behavior outcomes (state changes), not internal dispatch wiring
- [ ] At least one required test fails (RED step)
- [ ] No skipped phase dependencies
- [ ] Plan/requirement/pseudocode traceability markers present in ALL test code
- [ ] All existing non-issues-mode tests still pass

## Semantic Verification Checklist (Mandatory)
- [ ] Suppression tests cover all 4 specified keys (`s`, `Ctrl-d`, `Ctrl-k`, `l`) — state is UNCHANGED after key press
- [ ] Priority tests verify inline > chooser > focus-domain > global ordering — Esc cancels the highest-priority active control, not mode exit
- [ ] Navigation tests cover all specified keys per focus domain (Up, Down, PageUp, PageDown, Home, End, Enter)
- [ ] Tab/Shift+Tab tests verify full pane cycle: repo_list → issue_list → issue_detail → repo_list
- [ ] Inline mutation tests cover `e` (body + comment), `r` (comment + noop hint), save (`Ctrl+Enter`), cancel (`Esc`)
- [ ] Exclusivity test verifies `e` while inline active does not open a second editor
- [ ] Agent chooser tests cover: open, no-agent case, inline-active suppression
- [ ] `InputMode` detection tests cover all 5 issues mode states (normal, inline, chooser, search, filter)
- [ ] Feature behavior is reachable from real app flow: tests exercise the dispatch path from key event to state change
- [ ] No placeholder test patterns (`assert!(true)`, `#[ignore]`, empty bodies)

## Deferred Implementation Detection (Mandatory)

```bash
grep -RIn "TODO\|FIXME\|HACK\|placeholder\|for now\|will be implemented" src/
```

## Success Criteria
- [ ] RED test suite established for key routing (25 tests)
- [ ] Verification commands pass except expected RED failures
- [ ] Semantic checks pass

## Failure Recovery
- rollback steps: Simplify tests that are too tightly coupled to dispatch internals; rewrite as state-transition tests
- blocking issues: tests that pass without implementation, tests depending on internal dispatch mechanism

## Phase Completion Marker
Create: `project-plans/issue15/.completed/P10.md`

Contents:
- phase ID: `PLAN-20260329-ISSUES-MODE.P10`
- timestamp
- files changed
- tests added: 25 test names
- RED test verification: list of failing tests
- verification command outputs
- semantic verification summary

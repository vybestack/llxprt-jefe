# Phase 11: Key Routing and Input Dispatch Implementation

## Phase ID
`PLAN-20260329-ISSUES-MODE.P11`

## Prerequisites
- Required: Phase P10A completed
- Verify previous phase markers/artifacts exist: `.completed/P10.md`, `.completed/P10A.md`
- Expected files from previous phase: failing (RED) test suite for key routing (25 tests) in `src/app_input.rs` and/or `src/state/mod.rs`

## Requirements Implemented (Expanded)

### REQ-ISS-001: Mode Entry and Exit — Implementation
**Requirement text**: `i` from non-issues context enters `dashboard_issues` with `focus=issue_list`. `a` from `dashboard_issues` exits to `dashboard_agents`. Exit restores prior focus.

Behavior contract:
- GIVEN normal dashboard mode with repository selected
- WHEN `i` is pressed
- THEN issues mode activates, issue list load begins for selected repository via `GhClient::list_issues()`

- GIVEN `DashboardIssues` mode
- WHEN `a` is pressed
- THEN `ExitIssuesMode` fires; state transitions to `Dashboard`; prior agent focus restored per REQ-ISS-005 rules

Why it matters:
- Mode entry triggers the initial data load; mode exit must restore focus cleanly to avoid user disorientation

### REQ-ISS-002: Full Key Routing — Implementation
**Requirement text**: While in Issues Mode: suppress keys, route by 7-level priority chain, handle per-focus-domain dispatch.

Behavior contract:
- GIVEN state in `DashboardIssues` with issue list focused
- WHEN `Down` key is dispatched
- THEN `IssuesNavigateDown` event fires and state updates accordingly

- GIVEN state in `DashboardIssues` with any focus domain
- WHEN `s`, `Ctrl-d`, `Ctrl-k`, or `l` are dispatched
- THEN key is consumed as no-op; `AppState` before and after are equal; no `GhClient` method is called

Why it matters:
- Key routing connects user input to state transitions; suppression prevents destructive agent operations from firing in the wrong mode

### REQ-ISS-003: Pane Navigation — Implementation
**Requirement text**: Pane cycle Tab/Shift+Tab; per-domain navigation keys.

Behavior contract:
- GIVEN `DashboardIssues` with `repo_list` focused
- WHEN `Down` is pressed and next repository exists
- THEN repository selection moves down; issue list reloads for new scope via `GhClient::list_issues()`

- GIVEN `DashboardIssues` with `issue_list` focused
- WHEN `Enter` is pressed with issue selected
- THEN focus transitions to `IssueDetail`; `GhClient::get_issue_detail()` and `GhClient::list_comments()` are called

Why it matters:
- Navigation is the primary driver of data loading; each focus transition triggers the correct API call

### REQ-ISS-004: Esc Precedence Chain — Implementation
**Requirement text**: Esc precedence: cancel inline > cancel chooser > clear search text > blur search > close filter controls > exit mode.

Behavior contract:
- GIVEN inline control active
- WHEN `Esc` is pressed
- THEN inline is cancelled; mode remains `DashboardIssues`; no other cancel fires

- GIVEN search focused with text "bug"
- WHEN `Esc` is pressed
- THEN text is cleared; search input remains focused; mode does not exit

Why it matters:
- Each Esc press must be unambiguous; wrong precedence dismisses the wrong control or exits the mode unexpectedly

### REQ-ISS-008: Search and Filter Keys — Implementation
**Requirement text**: `/` focuses search; `f` opens filter controls from issue-list focus only.

Behavior contract:
- GIVEN issue list focused
- WHEN `f` is pressed
- THEN filter controls open; `f` from any other focus domain is no-op with hint

Why it matters:
- Scoping filter/search to issue-list focus prevents confusing activations from detail or repo-list panes

### REQ-ISS-010: Inline Mutation Key Handlers — Implementation
**Requirement text**: `e` edits body/comment; `r` replies with @author prefill; save `Ctrl+Enter`, cancel `Esc`; exclusivity guard.

Behavior contract:
- GIVEN inline composer active in issues mode
- WHEN `Ctrl+Enter` is pressed
- THEN `InlineSubmit` fires; `GhClient::create_comment()` (or update variant) is called; result dispatched as state event

- GIVEN inline composer active
- WHEN `e` is pressed
- THEN `e` is consumed by inline handler (Level 1 intercept); no second editor opens

Why it matters:
- Inline submit is the critical write path; exclusivity prevents duplicate API calls

### REQ-ISS-011: Send-to-Agent — Implementation
**Requirement text**: `S` opens agent chooser; chooser navigates with Up/Down, confirms with Enter, cancels with Esc.

Behavior contract:
- GIVEN agent chooser open, agents listed, user confirms with `Enter`
- WHEN `Enter` is pressed
- THEN `build_send_payload()` is called; payload is delivered to agent runtime; chooser closes

Why it matters:
- Agent delivery is the primary integration value of Issues Mode; payload composition must be complete and correct

### REQ-ISS-013: GitHub Client Wiring — Implementation
**Requirement text**: Wire `GhClient` calls into key dispatch for all loading and mutation operations.

Behavior contract:
- GIVEN issues mode entered with repository selected
- WHEN mode activates
- THEN `GhClient::list_issues()` is called; result dispatched as `IssueListLoaded` or `IssueListLoadFailed`

- GIVEN user has unsent draft comment and switches repository
- WHEN repository scope changes
- THEN draft is discarded; non-blocking notice is shown; issue list reloads for new repo

Why it matters:
- All data loading flows through `GhClient`; stale or missing wiring leaves the UI empty or out of sync

## Implementation Tasks

### Files to modify
- `src/app_input.rs` — implement `handle_issues_mode_key()` with full 7-level priority chain:
  - Level 1: inline editor/composer keys (Esc, Ctrl+Enter, Char, Backspace)
    - marker: `@plan PLAN-20260329-ISSUES-MODE.P11`
    - marker: `@requirement REQ-ISS-010`
    - marker: `@pseudocode component-003 lines 73-101`
  - Level 2: agent chooser keys (Up, Down, Enter, Esc)
    - marker: `@plan PLAN-20260329-ISSUES-MODE.P11`
    - marker: `@requirement REQ-ISS-011`
    - marker: `@pseudocode component-003 lines 102-111`
  - Level 3: search input keys (Enter, Esc, Char, Backspace)
    - marker: `@plan PLAN-20260329-ISSUES-MODE.P11`
    - marker: `@requirement REQ-ISS-008`
    - marker: `@pseudocode component-003 lines 112-119`
  - Level 4: filter controls keys (Tab, Shift+Tab, Enter, Esc, Char)
    - marker: `@plan PLAN-20260329-ISSUES-MODE.P11`
    - marker: `@requirement REQ-ISS-008`
    - marker: `@pseudocode component-003 lines 120-127`
  - Level 5: focus-domain handlers (repo_list, issue_list, issue_detail)
    - marker: `@plan PLAN-20260329-ISSUES-MODE.P11`
    - marker: `@requirement REQ-ISS-003`
    - marker: `@pseudocode component-003 lines 39-72`
  - Level 6: issues-global keys (i, a, Esc, Tab, Shift+Tab, ?/h/F1)
    - marker: `@plan PLAN-20260329-ISSUES-MODE.P11`
    - marker: `@requirement REQ-ISS-001`
    - marker: `@requirement REQ-ISS-002`
    - marker: `@pseudocode component-003 lines 25-31`
  - Level 7: suppressed keys (s, Ctrl-d, Ctrl-k, l)
    - marker: `@plan PLAN-20260329-ISSUES-MODE.P11`
    - marker: `@requirement REQ-ISS-002`
    - marker: `@pseudocode component-003 lines 33-38`
  - Implement per-focus-domain handlers:
    - `handle_repo_list_key_issues_mode()` — Up/Down with scope change side effect, draft discard notice
      - marker: `@pseudocode component-003 lines 128-135`
    - `handle_issue_list_key()` — Up/Down/PageUp/PageDown/Home/End/Enter/f//
      - marker: `@pseudocode component-003 lines 39-50`
    - `handle_issue_detail_key()` — Up/Down/Tab/Shift+Tab/e/r/S
      - marker: `@pseudocode component-003 lines 51-72`
  - Implement inline key handler: character input, save (Ctrl+Enter), cancel (Esc)
    - marker: `@pseudocode component-003 lines 73-101`
  - Implement inline submit: branch on composer vs editor target, call appropriate `GhClient` method
    - marker: `@pseudocode component-003 lines 80-101`
  - Implement agent chooser key handler: Up/Down/Enter/Esc; compose payload on confirm
    - marker: `@pseudocode component-003 lines 102-111`
  - Implement search input key handler: Enter applies, Esc clears/blurs per precedence
    - marker: `@pseudocode component-003 lines 112-119`
  - Implement filter controls key handler: Tab, Enter apply, Esc cancel
    - marker: `@pseudocode component-003 lines 120-127`
  - Implement repo scope change handler: clear inline draft with notice, reset paging, reload
    - marker: `@pseudocode component-003 lines 128-135`
  - Implement `compose_reply_prefill()`: `@author ` string generation
    - marker: `@pseudocode component-003 lines 136-137`
  - Implement `exclusivity_guard()`: reject second inline open
    - marker: `@pseudocode component-003 lines 138-141`
  - Wire `GhClient` calls:
    - Mode entry → `list_issues()` — `@pseudocode component-002 lines 09-25`
    - Issue selection → `get_issue_detail()` + `list_comments()` — `@pseudocode component-002 lines 26-43`
    - Pagination trigger → `list_issues()` with cursor — `@pseudocode component-002 lines 09-25`
    - Comment pagination → `list_comments()` with cursor — `@pseudocode component-002 lines 33-43`
    - Inline submit → `create_comment()` / `update_comment()` / `update_issue_body()` — `@pseudocode component-002 lines 44-61`
    - Send-to-agent → `build_send_payload()` + deliver to agent runtime — `@pseudocode component-002 lines 62-74`
  - Remove all stub returns
  - Module-level doc comment must include `@plan PLAN-20260329-ISSUES-MODE.P11`
  - All requirement markers: `@requirement REQ-ISS-001,002,003,004,008,010,011,013`

- `src/input.rs` — finalize `input_mode_for_state()` for all 5 issues mode states (may already be complete from P09):
  - marker: `@plan PLAN-20260329-ISSUES-MODE.P11`
  - marker: `@requirement REQ-ISS-002`

- `src/main.rs` — ensure `GhClient` is initialized and accessible; wire issues mode dispatch into terminal event handler:
  - marker: `@plan PLAN-20260329-ISSUES-MODE.P11`

### Pseudocode traceability (if impl phase)
- Uses pseudocode component-003 lines 01-141 (full key routing + inline + chooser)
  - lines 01-38: `route_issues_mode_key` priority chain
  - lines 39-50: `handle_issue_list_key`
  - lines 51-72: `handle_issue_detail_key`
  - lines 73-101: `handle_inline_key` + `handle_inline_submit`
  - lines 102-111: `handle_agent_chooser_key`
  - lines 112-119: `handle_search_input_key`
  - lines 120-127: `handle_filter_controls_key`
  - lines 128-135: `handle_repo_scope_change_in_issues_mode`
  - lines 136-137: `compose_reply_prefill`
  - lines 138-141: `exclusivity_guard`
- Uses pseudocode component-001 lines 115-127 (Esc precedence — state-side)
- Uses pseudocode component-002 lines 09-25 (list_issues wiring)
- Uses pseudocode component-002 lines 26-43 (detail + comments wiring)
- Uses pseudocode component-002 lines 44-61 (mutation wiring)
- Uses pseudocode component-002 lines 62-74 (send payload wiring)

## Verification Commands

```bash
cargo fmt --all --check
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo test --workspace --all-features
```

## Structural Verification Checklist
- [ ] All P10 RED tests now pass (GREEN)
- [ ] No stub returns remain in key routing code
- [ ] Phase/requirement/pseudocode markers present in all changed files
- [ ] All existing tests pass (zero regressions)
- [ ] `GhClient` calls are wired for all data loading and mutation paths
- [ ] Existing normal-mode key dispatch unchanged in behavior
- [ ] Issues mode dispatch integrated into existing chain, not a parallel handler
- [ ] `i` key in normal dashboard mode does not conflict with existing bindings

## Semantic Verification Checklist (Mandatory)
- [ ] `i` enters issues mode and triggers `list_issues()` call
- [ ] `a` exits issues mode with focus restoration per REQ-ISS-005 rules
- [ ] Suppressed keys (`s`, `Ctrl-d`, `Ctrl-k`, `l`) are consumed as no-op with zero state change in any focus domain — verified by unit tests asserting state equality before/after
- [ ] Priority chain: inline > chooser > search > filter > focus-domain > global > suppression — verified by Esc behavior tests
- [ ] Esc precedence chain works at all 6 levels (inline → chooser → search-clear → search-blur → filter-close → exit) — verified by dedicated tests
- [ ] Navigation keys dispatch correct events per focus domain; Enter on issue triggers `get_issue_detail()` + `list_comments()`
- [ ] `e` opens editor for body/comment; `r` opens reply composer with `@author ` prefill — verified by tests
- [ ] `S` opens agent chooser when valid; chooser keys (Up/Down/Enter/Esc) work — verified by tests
- [ ] Scope change (repo selection) triggers issue list reload and state invalidation
- [ ] Draft discard on scope change: active inline is cancelled with non-blocking notice when user navigates to different repo
- [ ] All `GhClient` calls wired: `list_issues`, `get_issue_detail`, `list_comments`, `create_comment`, `update_comment`, `update_issue_body`, `build_send_payload`
- [ ] Feature behavior is reachable from real app flow: full chain from key press → dispatch → `GhClient` call → state event → state update
- [ ] No placeholder/deferred implementation patterns remain

## Deferred Implementation Detection (Mandatory)

```bash
grep -RIn "TODO\|FIXME\|HACK\|placeholder\|for now\|will be implemented" src/app_input.rs src/input.rs
```

## Success Criteria
- [ ] All key routing tests GREEN (25 tests)
- [ ] Verification commands pass
- [ ] No placeholder code remains
- [ ] Semantic checks pass

## Failure Recovery
- rollback steps: `git restore src/app_input.rs src/input.rs src/main.rs`
- blocking issues: broken existing key handling, `GhClient` call failures, priority chain errors

## Phase Completion Marker
Create: `project-plans/issue15/.completed/P11.md`

Contents:
- phase ID: `PLAN-20260329-ISSUES-MODE.P11`
- timestamp
- files changed
- tests that went from RED to GREEN (list)
- `GhClient` wiring verification
- verification command outputs
- semantic verification summary

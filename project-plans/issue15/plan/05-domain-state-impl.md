# Phase 05: Domain + State Contracts Implementation

## Phase ID
`PLAN-20260329-ISSUES-MODE.P05`

## Prerequisites
- Required: Phase P04A completed
- Verify previous phase markers/artifacts exist: `.completed/P04.md`, `.completed/P04A.md`
- Expected files from previous phase: failing (RED) test suite for domain + state contracts

## Requirements Implemented (Expanded)

### REQ-ISS-001: Mode Entry and Exit — Implementation
**Requirement text**: `i` from non-issues context enters `dashboard_issues` with `focus=issue_list`. `i` while already in `dashboard_issues` refocuses `issue_list`. `a` from `dashboard_issues` exits to `dashboard_agents`. `Esc` follows issues-mode precedence chain; exits mode only when no higher-priority cancel target exists.

Behavior contract:
- GIVEN: `AppState::apply(EnterIssuesMode)` is called
- WHEN: state is in `Dashboard` mode
- THEN: `screen_mode` becomes `DashboardIssues`, `issues_state.active` is true, `issues_state.issue_focus` is `IssueList`, prior focus is saved, issues data is cleared

Why it matters:
- Foundation for all issues mode behavior. Every subsequent phase depends on mode lifecycle.

### REQ-ISS-005: Exit-Focus Restoration — Implementation
**Requirement text**: On exit from Issues Mode, restore prior agent focus only if: token exists, target still exists, target is focusable. Otherwise fall back to default agent-list focus.

Behavior contract:
- GIVEN: saved prior focus references agent at index 2
- WHEN: exiting issues mode and agent still exists
- THEN: `pane_focus` restored to `Agents`, `selected_agent_index` restored to 2

Why it matters:
- Preserving prior context reduces disorientation; incorrect restoration would silently invalidate user selections.

### REQ-ISS-003 + REQ-ISS-004: Navigation, Focus Cycling, Esc Precedence — Implementation
**Requirement text**: Issues Mode pane cycle: `repo_list -> issue_list -> issue_detail -> repo_list` (Tab/Shift+Tab). 6-level Esc precedence: (1) cancel inline, (2) cancel chooser, (3) clear non-empty search, (4) blur empty search, (5) close filter controls, (6) exit mode.

Behavior contract:
- GIVEN: Esc pressed with inline editor active
- WHEN: `handle_esc_in_issues_mode()` processes
- THEN: inline editor is cancelled; mode remains active; all other state preserved

Why it matters:
- Correct Esc precedence prevents accidental exits and data loss during active edit sessions.

### REQ-ISS-006 + REQ-ISS-007: Issue List Loading and Pagination — Implementation
**Requirement text**: First non-empty load selects first issue and auto-loads detail. Empty list shows scoped empty state. When selection reaches last loaded row and more exist, next page loads automatically. Comment pagination appends in stable order without reordering.

Behavior contract:
- GIVEN: `IssueListLoaded` with 20 issues
- WHEN: applied
- THEN: first issue selected, `has_more_issues` and cursor set, `list_loading` false

Why it matters:
- Correct selection on load and stable pagination are user-visible invariants.

### REQ-ISS-010: Inline Exclusivity and Mutation Events — Implementation
**Requirement text**: At most one inline mutable control (editor OR composer) active at a time.

Behavior contract:
- GIVEN: inline composer active
- WHEN: `OpenInlineEditor` event fires
- THEN: state unchanged (guard rejects)

Why it matters:
- Exclusivity prevents concurrent mutation that could corrupt comment thread state.

### REQ-ISS-008: Filter/Search State Transitions — Implementation
**Requirement text**: Supported: text query, state, author, assignee, labels (multi AND), mentioned, updated date bounds. Structured filters AND-composed; text query AND-composed with structured filters. `f` opens filter controls. Apply/Clear/Cancel behavior for filter controls. `/` focuses search.

Behavior contract:
- GIVEN: filter applied with state=open
- WHEN: new issue list loads with issues
- THEN: if previously selected issue number exists in new list, selection stays; otherwise reseats to first

Why it matters:
- Selection stability during filter transitions prevents the detail pane from unexpectedly showing different content.

## Implementation Tasks

### Files to modify
- `src/state/mod.rs`
  - Implement `apply()` match arms for ALL issue events:
    - `EnterIssuesMode` — save prior focus, set mode, clear data, emit side effect
    - `ExitIssuesMode` — restore focus, clear issues state, discard drafts with notice
    - `RefocusIssueList` — set focus to IssueList
    - `IssuesNavigateUp/Down/PageUp/PageDown/Home/End` — navigation handlers per focus domain
    - `IssuesEnter` — focus detail from issue list
    - `IssuesCycleFocus/Reverse` — pane cycling
    - `IssueListLoaded/PageLoaded` — list state updates with scope guard
    - `IssueDetailLoaded` — detail state update with scope guard
    - `IssueCommentsPageLoaded` — append comments
    - `IssueListLoadFailed/DetailLoadFailed/CommentsPageFailed` — error state
    - `OpenFilterControls/CloseFilterControls/ApplyFilter/ClearFilter` — filter state
    - `FocusSearchInput/BlurSearchInput/ApplySearch/ClearSearch` — search state
    - `OpenNewCommentComposer/OpenReplyComposer/OpenInlineEditor` — inline with exclusivity guard
    - `InlineChar/Backspace/Submit/CancelOrEsc` — inline editing
    - `CommentCreated/CommentCreateFailed/IssueBodyUpdated/CommentUpdated/MutationFailed` — mutation results
    - `OpenAgentChooser/Navigate/Confirm/Cancel` — agent chooser
    - `SendToAgentCompleted/Failed` — send result
  - Implement helper functions:
    - `enter_issues_mode()` per pseudocode component-001 lines 33-40
    - `exit_issues_mode()` per pseudocode component-001 lines 41-51
    - `handle_esc_in_issues_mode()` per pseudocode component-001 lines 115-127
    - `cycle_issues_focus()` / `cycle_issues_focus_reverse()` per pseudocode component-001 lines 71-82
    - `handle_detail_subfocus_tab()` / `handle_detail_subfocus_shift_tab()` per pseudocode component-001 lines 133-157
    - `selection_after_filter_change()` per pseudocode component-001 lines 158-165
  - marker: `@plan PLAN-20260329-ISSUES-MODE.P05`
  - marker: `@requirement REQ-ISS-001,REQ-ISS-003,REQ-ISS-004,REQ-ISS-005,REQ-ISS-006,REQ-ISS-007,REQ-ISS-008,REQ-ISS-010`
  - marker: `@pseudocode component-001 lines 01-165`

- `src/domain/mod.rs`
  - Finalize any helper methods on domain types (e.g., `Issue::matches_number()`)
  - marker: `@plan PLAN-20260329-ISSUES-MODE.P05`
  - marker: `@requirement REQ-ISS-006`
  - marker: `@pseudocode component-001 lines 83-96`

### Pseudocode traceability (line refs)
- component-001: lines 01-32 (dispatch table)
- component-001: lines 33-40 (enter_issues_mode)
- component-001: lines 41-51 (exit_issues_mode)
- component-001: lines 52-70 (navigate up/down)
- component-001: lines 71-82 (cycle focus / reverse)
- component-001: lines 83-96 (issue list loaded)
- component-001: lines 97-102 (issue list page loaded)
- component-001: lines 103-108 (issue detail loaded)
- component-001: lines 109-114 (comments page loaded)
- component-001: lines 115-127 (handle_esc_in_issues_mode)
- component-001: lines 129-132 (handle_issues_enter)
- component-001: lines 133-157 (detail subfocus tab/shift-tab)
- component-001: lines 158-165 (selection after filter change)
- component-002: lines 79-86 (GhError enum — referenced for load-failed event mapping)

## Verification Commands

```bash
cargo fmt --all --check
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo test --workspace --all-features
```

## Structural Verification Checklist
- [ ] All P04 RED tests now pass (GREEN)
- [ ] No new compilation warnings
- [ ] Phase markers present in changed files
- [ ] No skipped phase dependencies
- [ ] `apply()` has match arms for all issue events (no unmatched variants)

## Semantic Verification Checklist (Mandatory)
- [ ] Mode enter/exit transitions work correctly (tests green)
- [ ] Prior focus save/restore works with validity checks (target exists, target removed)
- [ ] Esc precedence chain works at all 6 levels in correct priority order
- [ ] Issue list selection rules work (first on load, none on empty, reseating on filter change)
- [ ] Pagination append works without reordering existing items
- [ ] Inline exclusivity enforced (second control blocked)
- [ ] Stale scope responses are discarded (scope guard on list/detail loaded events)
- [ ] Draft discard on repository scope change works: active inline is cancelled, notice emitted
- [ ] Feature behavior is reachable from real app flow: state transitions produce valid `AppState` that can be rendered by UI and used by key routing
- [ ] No placeholder/deferred implementation patterns remain in `src/state/mod.rs` apply() for issue events

## Deferred Implementation Detection (Mandatory)

```bash
# Reject if these appear in implementation code:
grep -RIn "TODO\|FIXME\|HACK\|placeholder\|for now\|will be implemented" src/state/mod.rs src/domain/mod.rs
```

## Success Criteria
- [ ] All domain + state tests GREEN
- [ ] Verification commands pass
- [ ] Semantic checks pass

## Failure Recovery
- rollback steps: `git restore src/state/mod.rs src/domain/mod.rs`
- blocking issues to resolve before next phase: failing tests, broken existing behavior, incomplete match arms

## Phase Completion Marker
Create: `project-plans/issue15/.completed/P05.md`

Contents:
- phase ID: `PLAN-20260329-ISSUES-MODE.P05`
- timestamp
- files changed
- tests that went from RED to GREEN (list)
- verification command outputs
- semantic verification summary

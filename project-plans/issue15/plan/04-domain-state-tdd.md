# Phase 04: Domain + State Contracts TDD

## Phase ID
`PLAN-20260329-ISSUES-MODE.P04`

## Prerequisites
- Required: Phase P03A completed
- Verify previous phase markers/artifacts exist: `.completed/P03.md`, `.completed/P03A.md`
- Expected files from previous phase: domain types in `src/domain/mod.rs`, state types in `src/state/mod.rs`, input mode extensions in `src/input.rs`, GitHub client stubs in `src/github/mod.rs`

## Requirements Implemented (Expanded)

### REQ-ISS-001: Mode Entry and Exit — TDD
**Requirement text**: `i` from non-issues context enters `dashboard_issues` with `focus=issue_list`. `i` while already in `dashboard_issues` refocuses `issue_list`. `a` from `dashboard_issues` exits to `dashboard_agents`. `Esc` follows issues-mode precedence chain; exits mode only when no higher-priority cancel target exists.

Behavior contract:
- GIVEN: `AppState` in `Dashboard` mode with `pane_focus=Agents`
- WHEN: `EnterIssuesMode` event is applied
- THEN: `screen_mode` becomes `DashboardIssues`, `issues_state.issue_focus` is `IssueList`, prior agent focus is saved

- GIVEN: `AppState` in `DashboardIssues` mode
- WHEN: `ExitIssuesMode` event is applied
- THEN: `screen_mode` becomes `Dashboard`, `issues_state.active` is false, prior agent focus is restored if valid

Why it matters:
- Mode lifecycle is the foundation — all other issues-mode behavior depends on it.

### REQ-ISS-005: Exit-Focus Restoration — TDD
**Requirement text**: On exit from Issues Mode, restore prior agent focus only if: token exists, target still exists, target is focusable. Otherwise fall back to default agent-list focus.

Behavior contract:
- GIVEN: user had agent at index 2 selected before entering Issues Mode
- WHEN: user exits Issues Mode and agent still exists at index 2
- THEN: agent focus is restored to index 2

- GIVEN: user had agent at index 5 selected, but agents list now has only 3 items
- WHEN: user exits Issues Mode
- THEN: fall back to default agent-list focus (index 0 or Agents pane)

Why it matters:
- Preserving prior context reduces disorientation when switching modes.

### REQ-ISS-003: Pane Focus and Navigation — TDD
**Requirement text**: Issues Mode pane cycle: `repo_list -> issue_list -> issue_detail -> repo_list` (Tab/Shift+Tab). Repository list: Up/Down moves selection; scope updates immediately. Issue list: Up/Down, PageUp/PageDown, Home/End, Enter focuses detail. Issue detail: Up/Down scroll; Tab subfocus cycle through body/comments/new-comment. `r` on focused comment opens inline reply; `r` elsewhere is no-op with hint.

Behavior contract:
- GIVEN: `issue_focus = RepoList`
- WHEN: `IssuesCycleFocus` event is applied
- THEN: `issue_focus` becomes `IssueList`

- GIVEN: `issue_focus = IssueDetail`
- WHEN: `IssuesCycleFocusReverse` event is applied
- THEN: `issue_focus` becomes `IssueList`

Why it matters:
- Focus routing is the mechanism through which all key events reach the correct handler.

### REQ-ISS-004: Esc Precedence Chain — TDD
**Requirement text**: (1) Cancel active inline edit/composer. (2) Cancel active send-to-agent chooser. (3) If search input focused and non-empty: clear search text, keep search focused. (4) If search input focused and empty: blur search input, keep Issues Mode. (5) Close active transient controls (filter controls). (6) Exit Issues Mode.

Behavior contract:
- GIVEN: inline editor is active
- WHEN: Esc is handled in issues mode
- THEN: inline editor is cancelled; mode remains `DashboardIssues`

- GIVEN: agent chooser is open
- WHEN: Esc is handled and no inline control active
- THEN: agent chooser is closed; mode remains

- GIVEN: search input focused with query "bug"
- WHEN: Esc is handled and no inline/chooser active
- THEN: search text is cleared; search input remains focused

- GIVEN: search input focused with empty query
- WHEN: Esc is handled
- THEN: search input is blurred; mode remains

- GIVEN: filter controls open, no search/inline/chooser active
- WHEN: Esc is handled
- THEN: filter controls close (cancel, no commit)

- GIVEN: no inline, no chooser, no search, no filter controls active
- WHEN: Esc is handled
- THEN: mode exits to `Dashboard`

Why it matters:
- Correct Esc precedence prevents accidental mode exits and data loss.

### REQ-ISS-006: Issue List Selection Rules — TDD
**Requirement text**: Each row: number, title, state, author, updated timestamp, assignee summary, label summary, comment count. Default sort: `updated_at desc`, tie-breaker `number asc`. First non-empty load selects first issue and auto-loads detail. On filter/search change: keep selection if present; else select first. Empty list shows scoped empty state.

Behavior contract:
- GIVEN: `IssueListLoaded` event with 20 issues
- WHEN: applied to state
- THEN: `selected_issue_index = Some(0)`, `list_loading = false`

- GIVEN: `IssueListLoaded` event with 0 issues
- WHEN: applied to state
- THEN: `selected_issue_index = None`, `issue_detail = None`

Why it matters:
- Selection rules determine what the user sees in detail; incorrect selection is a visible regression.

### REQ-ISS-007: Pagination and Lazy Loading — TDD
**Requirement text**: Lists are paginated/lazy-loaded. When selection reaches last loaded row and more exist, next page loads automatically. Repository switch invalidates prior paging context. Comment pagination appends in stable order without reordering. Comment pagination failure retains loaded comments and exposes retry.

Behavior contract:
- GIVEN: `IssueListPageLoaded` with 20 additional issues
- WHEN: applied to state
- THEN: issues are appended (now 40 total); `has_more_issues` updated; selection unchanged

Why it matters:
- Append-without-reorder is a stability invariant; violations cause visible list flicker and index drift.

### REQ-ISS-010: Inline Control Exclusivity — TDD
**Requirement text**: No modal flow; all inline. At most one inline mutable control (editor OR composer) active at a time.

Behavior contract:
- GIVEN: inline composer is active (`inline_state = Composer{...}`)
- WHEN: `OpenInlineEditor` event fires
- THEN: event is rejected; composer remains active; state unchanged

Why it matters:
- Exclusivity prevents concurrent mutation that could corrupt comment thread state.

### REQ-ISS-012: Repository `issue_base_prompt` Persistence — TDD
**Requirement text**: Multiline field in existing repository config screen. Save and Reset controls. Reset restores last-saved value. Empty value valid. Persisted via existing repository config persistence path.

Behavior contract:
- GIVEN: `Repository` with `issue_base_prompt = "Prioritize diagnosis"`
- WHEN: serialized to JSON and deserialized
- THEN: `issue_base_prompt` value is preserved

- GIVEN: legacy JSON without `issue_base_prompt` field
- WHEN: deserialized to `Repository`
- THEN: `issue_base_prompt` defaults to empty string

Why it matters:
- Backward compatibility with existing persisted data is a hard gate — existing users must not lose configuration.

## Implementation Tasks

### Files to create or modify
- Tests in `src/state/mod.rs` (inline `#[cfg(test)]` module):
  - `test_enter_issues_mode_saves_prior_focus`
    - marker: `@plan PLAN-20260329-ISSUES-MODE.P04`
    - marker: `@requirement REQ-ISS-001`
    - marker: `@pseudocode component-001 lines 33-40`
  - `test_enter_issues_mode_sets_screen_mode`
    - marker: `@plan PLAN-20260329-ISSUES-MODE.P04`
    - marker: `@requirement REQ-ISS-001`
    - marker: `@pseudocode component-001 lines 33-40`
  - `test_exit_issues_mode_restores_focus`
    - marker: `@plan PLAN-20260329-ISSUES-MODE.P04`
    - marker: `@requirement REQ-ISS-005`
    - marker: `@pseudocode component-001 lines 41-51`
  - `test_exit_issues_mode_fallback_when_target_gone`
    - marker: `@plan PLAN-20260329-ISSUES-MODE.P04`
    - marker: `@requirement REQ-ISS-005`
    - marker: `@pseudocode component-001 lines 41-51`
  - `test_exit_issues_mode_discards_draft_with_notice`
    - marker: `@plan PLAN-20260329-ISSUES-MODE.P04`
    - marker: `@requirement REQ-ISS-001,REQ-ISS-013`
    - marker: `@pseudocode component-001 lines 41-51`
  - `test_issues_navigate_up_in_issue_list`
    - marker: `@plan PLAN-20260329-ISSUES-MODE.P04`
    - marker: `@requirement REQ-ISS-003`
    - marker: `@pseudocode component-001 lines 52-70`
  - `test_issues_navigate_down_triggers_pagination`
    - marker: `@plan PLAN-20260329-ISSUES-MODE.P04`
    - marker: `@requirement REQ-ISS-003,REQ-ISS-007`
    - marker: `@pseudocode component-001 lines 52-70, 97-102`
  - `test_issues_cycle_focus_tab`
    - marker: `@plan PLAN-20260329-ISSUES-MODE.P04`
    - marker: `@requirement REQ-ISS-003`
    - marker: `@pseudocode component-001 lines 71-82`
  - `test_issues_cycle_focus_shift_tab`
    - marker: `@plan PLAN-20260329-ISSUES-MODE.P04`
    - marker: `@requirement REQ-ISS-003`
    - marker: `@pseudocode component-001 lines 71-82`
  - `test_detail_subfocus_tab_with_comments`
    - marker: `@plan PLAN-20260329-ISSUES-MODE.P04`
    - marker: `@requirement REQ-ISS-003`
    - marker: `@pseudocode component-001 lines 133-157`
  - `test_detail_subfocus_tab_no_comments`
    - marker: `@plan PLAN-20260329-ISSUES-MODE.P04`
    - marker: `@requirement REQ-ISS-003`
    - marker: `@pseudocode component-001 lines 133-157`
  - `test_esc_cancels_inline_editor`
    - marker: `@plan PLAN-20260329-ISSUES-MODE.P04`
    - marker: `@requirement REQ-ISS-004`
    - marker: `@pseudocode component-001 lines 115-127`
  - `test_esc_cancels_agent_chooser`
    - marker: `@plan PLAN-20260329-ISSUES-MODE.P04`
    - marker: `@requirement REQ-ISS-004`
    - marker: `@pseudocode component-001 lines 115-127`
  - `test_esc_clears_nonempty_search`
    - marker: `@plan PLAN-20260329-ISSUES-MODE.P04`
    - marker: `@requirement REQ-ISS-004`
    - marker: `@pseudocode component-001 lines 115-127`
  - `test_esc_blurs_empty_search`
    - marker: `@plan PLAN-20260329-ISSUES-MODE.P04`
    - marker: `@requirement REQ-ISS-004`
    - marker: `@pseudocode component-001 lines 115-127`
  - `test_esc_closes_filter_controls`
    - marker: `@plan PLAN-20260329-ISSUES-MODE.P04`
    - marker: `@requirement REQ-ISS-004`
    - marker: `@pseudocode component-001 lines 115-127`
  - `test_esc_exits_issues_mode`
    - marker: `@plan PLAN-20260329-ISSUES-MODE.P04`
    - marker: `@requirement REQ-ISS-004`
    - marker: `@pseudocode component-001 lines 115-127`
  - `test_issue_list_loaded_selects_first`
    - marker: `@plan PLAN-20260329-ISSUES-MODE.P04`
    - marker: `@requirement REQ-ISS-006`
    - marker: `@pseudocode component-001 lines 83-96`
  - `test_issue_list_loaded_empty`
    - marker: `@plan PLAN-20260329-ISSUES-MODE.P04`
    - marker: `@requirement REQ-ISS-006,REQ-ISS-014`
    - marker: `@pseudocode component-001 lines 83-96`
  - `test_issue_list_page_loaded_appends`
    - marker: `@plan PLAN-20260329-ISSUES-MODE.P04`
    - marker: `@requirement REQ-ISS-007`
    - marker: `@pseudocode component-001 lines 97-102`
  - `test_selection_after_filter_change_keeps_existing`
    - marker: `@plan PLAN-20260329-ISSUES-MODE.P04`
    - marker: `@requirement REQ-ISS-008`
    - marker: `@pseudocode component-001 lines 158-165`
  - `test_selection_after_filter_change_reseats`
    - marker: `@plan PLAN-20260329-ISSUES-MODE.P04`
    - marker: `@requirement REQ-ISS-008`
    - marker: `@pseudocode component-001 lines 158-165`
  - `test_inline_exclusivity_blocks_second_control`
    - marker: `@plan PLAN-20260329-ISSUES-MODE.P04`
    - marker: `@requirement REQ-ISS-010`
    - marker: `@pseudocode component-003 lines 138-141`
  - `test_stale_scope_list_loaded_discarded`
    - marker: `@plan PLAN-20260329-ISSUES-MODE.P04`
    - marker: `@requirement REQ-ISS-001`
    - marker: `@pseudocode component-001 lines 83-96`

- Tests in `src/domain/mod.rs` (inline `#[cfg(test)]` module):
  - `test_issue_base_prompt_serde_roundtrip`
    - marker: `@plan PLAN-20260329-ISSUES-MODE.P04`
    - marker: `@requirement REQ-ISS-012`
  - `test_issue_base_prompt_backward_compat`
    - marker: `@plan PLAN-20260329-ISSUES-MODE.P04`
    - marker: `@requirement REQ-ISS-012`

### Pseudocode traceability (line refs)
- component-001: lines 33-51 (enter/exit mode)
- component-001: lines 52-82 (navigation/focus cycling)
- component-001: lines 83-127 (list loading, Esc chain)
- component-001: lines 129-157 (detail subfocus)
- component-001: lines 158-165 (selection after filter change)
- component-003: lines 138-141 (exclusivity guard)

## Verification Commands

```bash
cargo fmt --all --check
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo test --workspace --all-features
```

## Structural Verification Checklist
- [ ] All planned test names exist and compile
- [ ] Tests target behavior contracts (state transitions, not internal wiring)
- [ ] At least one required test fails before implementation updates (RED step confirmed)
- [ ] No skipped phase dependencies
- [ ] Plan/requirement/pseudocode traceability markers present in ALL test code

## Semantic Verification Checklist (Mandatory)
- [ ] Tests cover mode lifecycle: enter (save focus, set mode), exit (restore focus, discard draft)
- [ ] Tests cover Esc precedence chain at all 6 levels with correct priority ordering
- [ ] Tests cover issue list selection rules: first on load, none on empty, reseating on filter change
- [ ] Tests cover pagination append (no reorder)
- [ ] Tests cover inline control exclusivity (second control rejected)
- [ ] Tests cover `issue_base_prompt` persistence backward compat
- [ ] Tests cover detail subfocus cycling with and without comments
- [ ] Tests verify stale-scope suppression: `IssueListLoaded` event with wrong repo scope is discarded
- [ ] Tests verify draft discard on repo switch: active inline composer is cancelled with notice when repository changes
- [ ] Feature behavior is reachable from real app flow: tests exercise `AppState::apply()` with issue events, which is the same path used by the real app
- [ ] No placeholder/deferred test patterns (no `assert!(true)`, no `#[ignore]`)

## Deferred Implementation Detection (Mandatory)

```bash
# Reject if these appear in implementation code:
grep -RIn "TODO\|FIXME\|HACK\|placeholder\|for now\|will be implemented" src/
```

## Success Criteria
- [ ] RED test suite established for domain + state contracts
- [ ] Verification commands pass except expected RED failures for unimplemented reducer logic
- [ ] Semantic checks pass

## Failure Recovery
- rollback steps: Trim brittle tests and rewrite around externally observable behavior; remove tests that depend on internal implementation details
- blocking issues to resolve before next phase: insufficient failing tests, weak behavior assertions, tests that pass without implementation

## Phase Completion Marker
Create: `project-plans/issue15/.completed/P04.md`

Contents:
- phase ID: `PLAN-20260329-ISSUES-MODE.P04`
- timestamp
- files changed: list of test files
- tests added: count and names
- RED test verification: list of failing tests (expected)
- verification command outputs
- semantic verification summary

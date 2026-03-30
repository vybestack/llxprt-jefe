# Phase 04A: Domain + State Contracts TDD Verification

## Phase ID
`PLAN-20260329-ISSUES-MODE.P04A`

## Prerequisites
- Required: Phase P04 completed.
- Verify previous artifacts: `.completed/P04.md` exists.
- Expected files from previous phase: failing test suite in `src/state/mod.rs` and `src/domain/mod.rs` test modules.

## Requirements Implemented (Expanded)

### Verification of TDD Test Coverage for REQ-ISS-001,003,004,005,006,007,008,010,012
**Requirement text**: Confirm failing tests cover all planned behavior contracts for domain + state layer.

Behavior contract:
- GIVEN RED test suite from P04
- WHEN verification checks are executed
- THEN test names map to requirements, tests compile, failures are for unimplemented behavior (not compilation errors), pseudocode line ranges are referenced, and traceability markers are present.

Why it matters:
- Ensures TDD discipline: implementation in P05 is driven by these tests. Missing tests = missing implementation.

## Implementation Tasks

### Files to create
- `project-plans/issue15/.completed/P04A.md`
  - marker: `@plan PLAN-20260329-ISSUES-MODE.P04A`

### Files to modify
- `project-plans/issue15/plan/00-overview.md` -- tracker update

### Pseudocode traceability (if impl phase)
- N/A (verification phase)

## Verification Commands

```bash
cargo fmt --all --check
cargo clippy --workspace --all-targets --all-features -- -D warnings
# Tests expected to have RED failures -- verify they compile but fail:
cargo test --workspace --all-features 2>&1 | tail -40
```

### Test Name Verification
```bash
# Verify all planned test names exist
for test_name in test_enter_issues_mode_saves_prior_focus test_enter_issues_mode_sets_screen_mode test_exit_issues_mode_restores_focus test_exit_issues_mode_fallback_when_target_gone test_exit_issues_mode_discards_draft_with_notice test_issues_navigate_up_in_issue_list test_issues_navigate_down_triggers_pagination test_issues_cycle_focus_tab test_issues_cycle_focus_shift_tab test_detail_subfocus_tab_with_comments test_detail_subfocus_tab_no_comments test_esc_cancels_inline_editor test_esc_cancels_agent_chooser test_esc_clears_nonempty_search test_esc_blurs_empty_search test_esc_closes_filter_controls test_esc_exits_issues_mode test_issue_list_loaded_selects_first test_issue_list_loaded_empty test_issue_list_page_loaded_appends test_selection_after_filter_change_keeps_existing test_selection_after_filter_change_reseats test_inline_exclusivity_blocks_second_control test_stale_scope_list_loaded_discarded test_issue_base_prompt_serde_roundtrip test_issue_base_prompt_backward_compat; do
  grep -rn "$test_name" src/ && echo "OK: $test_name found" || echo "MISSING: $test_name"
done
```

### Traceability Marker Verification
```bash
# Verify all test functions have @plan, @requirement, @pseudocode markers
echo "--- @plan markers in test code ---"
grep -c "@plan PLAN-20260329-ISSUES-MODE.P04" src/state/types.rs src/state/mod.rs src/domain/mod.rs || echo "WARN: missing plan markers"

echo "--- @requirement markers in test code ---"
grep -c "@requirement" src/state/types.rs src/state/mod.rs src/domain/mod.rs || echo "WARN: missing requirement markers"

echo "--- @pseudocode markers in test code ---"
grep -c "@pseudocode" src/state/types.rs src/state/mod.rs src/domain/mod.rs || echo "WARN: missing pseudocode markers"
```

## Structural Verification Checklist
- [ ] All 25 planned test names exist in source code (+ 1 stale-scope test = 26 total).
- [ ] Tests compile without errors.
- [ ] Expected failures are for unimplemented behavior (not compilation errors).
- [ ] Every test function has `@plan`, `@requirement`, and `@pseudocode` markers in doc comment or nearby comment.
- [ ] Tracker updated.

## Semantic Verification Checklist (Mandatory)
- [ ] Every REQ-ISS covered in P04 has at least one test (cross-reference table):
  - REQ-ISS-001: enter/exit mode tests + stale-scope test
  - REQ-ISS-003: navigation/focus cycling tests + detail subfocus tests (with and without comments)
  - REQ-ISS-004: Esc precedence tests (all 6 levels in order)
  - REQ-ISS-005: exit focus restoration tests (valid + stale fallback)
  - REQ-ISS-006: list loaded/empty tests
  - REQ-ISS-007: page loaded append test + navigate-triggers-pagination test
  - REQ-ISS-008: selection after filter change tests (keep existing + reseat)
  - REQ-ISS-010: exclusivity test (second control rejected)
  - REQ-ISS-012: serde round-trip and backward compat tests
  - REQ-ISS-013: draft discard with notice on exit
- [ ] Tests verify key routing behavior: `s`, `Ctrl-d`, `Ctrl-k`, `l` suppression is covered by P10, but stale-scope and draft-discard are covered here.
- [ ] Tests verify stale-scope suppression: `IssueListLoaded` with wrong repo ID is discarded (state unchanged).
- [ ] Tests verify draft discard on repo switch: active inline is cancelled and notice is emitted.
- [ ] Tests assert observable behavior (state field values), not implementation internals.
- [ ] Feature behavior is reachable from real app flow: tests use `AppState::apply()` which is the production code path.
- [ ] No placeholder test patterns (`assert!(true)`, `#[ignore]`, empty test bodies).

## Deferred Implementation Detection (Mandatory)

```bash
grep -RIn "TODO\|FIXME\|HACK\|placeholder\|for now\|will be implemented" src/
```

## Success Criteria
- [ ] TDD verification pass (RED tests confirmed).
- [ ] All planned test names present.
- [ ] All traceability markers present.

## Failure Recovery
- rollback steps: Add missing tests. Fix compilation errors in test code. Add missing traceability markers.
- blocking issues: Tests that pass without implementation (false green), missing test names, missing markers.

## Phase Completion Marker
Create: `project-plans/issue15/.completed/P04A.md`

Contents:
- phase ID: `PLAN-20260329-ISSUES-MODE.P04A`
- timestamp
- test name verification output (all 26 names)
- traceability marker verification output
- RED test failure list
- verification outputs
- semantic verification summary

# Phase 13A: UI Components + Persistence TDD Verification

## Phase ID
`PLAN-20260329-ISSUES-MODE.P13A`

## Prerequisites
- Required: Phase P13 completed.
- Verify previous artifacts: `.completed/P13.md` exists.
- Expected files from previous phase: failing test suite for UI + persistence (14 tests).

## Requirements Implemented (Expanded)

### Verification of TDD Test Coverage for REQ-ISS-002,006,008,009,010,012,014
**Requirement text**: Confirm failing tests cover all planned rendering and persistence behavior contracts.

Behavior contract:
- GIVEN RED test suite from P13
- WHEN verification checks are executed
- THEN all 14 test names exist, tests compile, failures are for unimplemented rendering/persistence (not compilation), traceability markers present.

## Implementation Tasks

### Files to create
- `project-plans/issue15/.completed/P13A.md`
  - marker: `@plan PLAN-20260329-ISSUES-MODE.P13A`

### Files to modify
- `project-plans/issue15/plan/00-overview.md` -- tracker update

### Pseudocode traceability (if impl phase)
- N/A (verification phase)

## Verification Commands

```bash
cargo fmt --all --check
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo test --workspace --all-features 2>&1 | tail -40
```

### Test Name Verification
```bash
for test_name in test_issue_base_prompt_state_round_trip test_issue_base_prompt_state_backward_compat test_issue_list_row_count test_issue_list_selection_highlight test_issue_list_loading_state test_issue_list_empty_state test_issue_detail_all_fields test_issue_detail_comments_timeline test_issue_detail_inline_composer_visible test_filter_controls_value_binding test_empty_state_no_issues test_empty_state_no_comments test_empty_state_no_agents_for_send test_keybind_bar_issues_mode; do
  grep -rn "$test_name" src/ && echo "OK: $test_name found" || echo "MISSING: $test_name"
done
```

### Traceability Marker Verification
```bash
echo "--- @plan markers in UI/persistence tests ---"
grep -rc "@plan PLAN-20260329-ISSUES-MODE.P13" src/persistence/mod.rs src/state/mod.rs src/ui/ || echo "WARN: missing"

echo "--- @requirement markers in UI/persistence tests ---"
grep -rc "@requirement" src/persistence/mod.rs src/state/mod.rs src/ui/ || echo "WARN: missing"
```

## Structural Verification Checklist
- [ ] All 14 planned test names exist.
- [ ] Tests compile.
- [ ] Expected failures verified.
- [ ] Traceability markers present in all tests.
- [ ] Tracker updated.

## Semantic Verification Checklist (Mandatory)
- [ ] Persistence tests cover round-trip and backward compat through full state path.
- [ ] UI tests cover all state variants (loading, empty, normal, inline active).
- [ ] Empty state tests present for all 3 categories (issues, comments, agents).
- [ ] Feature behavior is reachable from real app flow: tests verify conditions the real rendering encounters.
- [ ] No placeholder test patterns.

## Deferred Implementation Detection (Mandatory)

```bash
grep -RIn "TODO\|FIXME\|HACK\|placeholder\|for now\|will be implemented" src/
```

## Success Criteria
- [ ] TDD verification pass (RED tests confirmed, all 14 present).
- [ ] Traceability markers present.

## Failure Recovery
- rollback steps: Add missing tests. Add missing traceability markers.
- blocking issues: tests passing with stub rendering.

## Phase Completion Marker
Create: `project-plans/issue15/.completed/P13A.md`

Contents:
- phase ID: `PLAN-20260329-ISSUES-MODE.P13A`
- timestamp
- test name verification output (all 14)
- traceability marker verification output
- RED test failure list
- verification outputs
- semantic verification summary

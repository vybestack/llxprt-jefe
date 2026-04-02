# Phase 10A: Key Routing + Input Dispatch TDD Verification

## Phase ID
`PLAN-20260329-ISSUES-MODE.P10A`

## Prerequisites
- Required: Phase P10 completed.
- Verify previous artifacts: `.completed/P10.md` exists.
- Expected files from previous phase: failing test suite for key routing (35 tests).

## Requirements Implemented (Expanded)

### Verification of TDD Test Coverage for REQ-ISS-001,002,003,004,008,009,010,011
**Requirement text**: Confirm failing tests cover all planned key routing behavior contracts.

Behavior contract:
- GIVEN RED test suite from P10
- WHEN verification checks are executed
- THEN all 35 test names exist, tests compile, failures are for unimplemented dispatch (not compilation), tests use state-based assertions, traceability markers present.

### Behavioral Runtime-Path Evidence Requirement (Mandatory)
Verifier output must include all of the following before issuing PASS:
1. At least one file:line proof that tested key events flow through the same dispatch entry points used in production.
2. At least one file:line proof that suppression assertions verify unchanged state, not only missing events.
3. A contradiction scan across P10/P10A/P11 expectations for test counts and routing priority ordering.
4. Output must end with exactly one atomic verdict line: `Phase 10A: PASS` or `Phase 10A: FAIL`.

## Implementation Tasks

### Files to create
- `project-plans/issue15/.completed/P10A.md`
  - marker: `@plan PLAN-20260329-ISSUES-MODE.P10A`

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
for test_name in test_i_key_enters_issues_mode test_a_key_exits_issues_mode test_s_key_suppressed_in_issues_mode test_ctrl_d_suppressed_in_issues_mode test_ctrl_k_suppressed_in_issues_mode test_l_key_suppressed_in_issues_mode test_slash_focuses_search_in_issues_mode test_f_opens_filter_from_issue_list_focus test_f_noop_from_non_issue_list_focus test_esc_inline_priority_over_mode_exit test_esc_chooser_priority_over_mode_exit test_down_in_issue_list_dispatches_navigate test_up_in_issue_list_dispatches_navigate test_page_up_in_issue_list_dispatches_navigate test_page_down_in_issue_list_dispatches_navigate test_home_in_issue_list_dispatches_navigate test_end_in_issue_list_dispatches_navigate test_enter_in_issue_list_focuses_detail test_tab_cycles_issues_pane_focus test_shift_tab_reverse_cycles test_e_opens_editor_on_body test_e_opens_editor_on_comment test_r_opens_reply_on_comment test_r_noop_when_not_on_comment test_ctrl_enter_submits_inline test_esc_cancels_inline_editor test_S_opens_agent_chooser test_S_noop_when_inline_active test_S_shows_message_when_no_agents test_o_key_noop_in_issue_detail test_input_mode_issues_normal test_input_mode_issues_inline test_input_mode_issues_chooser test_input_mode_issues_search test_input_mode_issues_filter; do
  grep -rn "$test_name" src/ && echo "OK: $test_name found" || echo "MISSING: $test_name"
done
```

### Traceability Marker Verification
```bash
echo "--- @plan markers in test code ---"
grep -rc "@plan PLAN-20260329-ISSUES-MODE.P10" src/app_input/ src/state/ || echo "WARN: missing"

echo "--- @pseudocode markers in test code ---"
grep -rc "@pseudocode component-003" src/app_input/ src/state/ || echo "WARN: missing"
```

## Structural Verification Checklist
- [ ] All 35 planned test names exist.
- [ ] Tests compile.
- [ ] Expected failures verified.
- [ ] Traceability markers present in all tests.
- [ ] Tracker updated.

## Semantic Verification Checklist (Mandatory)
- [ ] All REQ-ISS covered in P10 have tests (cross-reference):
  - REQ-ISS-001: `i`/`a` mode entry/exit tests
  - REQ-ISS-002: suppression tests (4 keys, state-unchanged assertions) + priority tests + input mode tests (5: normal, inline, chooser, search, filter)
  - REQ-ISS-003: navigation tests (up, down, pageup, pagedown, home, end, enter) + pane focus tests (tab, shift+tab)
  - REQ-ISS-004: Esc priority tests (inline > chooser)
  - REQ-ISS-008: filter/search key tests (/, f, f-noop)
  - REQ-ISS-009: `o` key no-op test (display-only URL per spec)
  - REQ-ISS-010: inline edit/reply tests (e-body, e-comment, r-comment, r-noop, ctrl+enter submit, esc cancel)
  - REQ-ISS-011: agent chooser tests (S-open, S-noop-inline, S-no-agents)
- [ ] Tests verify key dispatch outcomes (state changes), not just function calls.
- [ ] Suppression tests verify state is UNCHANGED after key press (not just that no event fires).
- [ ] Feature behavior is reachable from real app flow: tests exercise the same dispatch chain as production.
- [ ] No placeholder test patterns.

## Deferred Implementation Detection (Mandatory)

```bash
grep -RIn "TODO\|FIXME\|HACK\|placeholder\|for now\|will be implemented" src/
```

## Success Criteria
- [ ] TDD verification pass (RED tests confirmed, all 35 present).
- [ ] Traceability markers present.

## Failure Recovery
- rollback steps: Add missing tests. Add missing traceability markers.
- blocking issues: tests passing without implementation, missing test names.

## Phase Completion Marker
Create: `project-plans/issue15/.completed/P10A.md`

Contents:
- phase ID: `PLAN-20260329-ISSUES-MODE.P10A`
- timestamp
- test name verification output (all 35)
- traceability marker verification output
- RED test failure list
- verification outputs
- semantic verification summary

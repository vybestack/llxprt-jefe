# Phase 07A: GitHub Client Boundary TDD Verification

## Phase ID
`PLAN-20260329-ISSUES-MODE.P07A`

## Prerequisites
- Required: Phase P07 completed.
- Verify previous artifacts: `.completed/P07.md` exists.
- Expected files from previous phase: failing test suite in `src/github/mod.rs` test module.

## Requirements Implemented (Expanded)

### Verification of TDD Test Coverage for REQ-ISS-006,007,008,009,010,011,013
**Requirement text**: Confirm failing tests cover all planned behavior contracts for GitHub client boundary.

Behavior contract:
- GIVEN RED test suite from P07
- WHEN verification checks are executed
- THEN all 18 test names exist, tests compile, failures are for unimplemented logic (not compilation), tests use fixture data, and traceability markers are present.

### Behavioral Runtime-Path Evidence Requirement (Mandatory)
Verifier output must include all of the following before issuing PASS:
1. At least one file:line runtime-path proof showing how a `GhClient` result maps to a typed issues event consumed by the state layer.
2. At least one file:line proof that test coverage exercises production parsing/payload-building paths rather than detached helper-only logic.
3. A contradiction scan across P07/P07A/component-002 notes any mismatched method names, response field names, or pagination semantics.
4. Output must end with exactly one atomic verdict line: `Phase 07A: PASS` or `Phase 07A: FAIL`.

## Implementation Tasks

### Files to create
- `project-plans/issue15/.completed/P07A.md`
  - marker: `@plan PLAN-20260329-ISSUES-MODE.P07A`

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
for test_name in test_check_auth_success test_check_auth_not_authenticated test_list_issues_parses_json test_list_issues_sorts_by_updated_desc test_list_issues_filter_args_construction test_list_issues_empty_result test_get_issue_detail_parses_json test_get_issue_detail_optional_milestone test_list_comments_parses_json test_list_comments_pagination test_create_comment_success test_update_comment_success test_update_issue_body_success test_build_send_payload_with_comment test_build_send_payload_without_comment test_error_categorization_rate_limit test_error_categorization_not_authenticated test_error_categorization_access_denied; do
  grep -rn "$test_name" src/github/ && echo "OK: $test_name found" || echo "MISSING: $test_name"
done
```

### Traceability Marker Verification
```bash
# Verify test functions have @plan, @requirement, @pseudocode markers
echo "--- @plan markers in test code ---"
grep -c "@plan PLAN-20260329-ISSUES-MODE.P07" src/github/mod.rs || echo "WARN: missing"

echo "--- @pseudocode markers in test code ---"
grep -c "@pseudocode component-002" src/github/mod.rs || echo "WARN: missing"
```

## Structural Verification Checklist
- [ ] All 18 planned test names exist in source code.
- [ ] Tests compile without errors.
- [ ] Expected failures are for unimplemented parsing/logic (not compilation errors).
- [ ] Every test has `@plan`, `@requirement`, `@pseudocode` markers.
- [ ] Tracker updated.

## Semantic Verification Checklist (Mandatory)
- [ ] Tests cover all `GhClient` methods (auth, list, detail, comments, create, update, payload).
- [ ] Tests use fixture data, not real API calls — verified by absence of `Command::new("gh")` in test code.
- [ ] Error categorization tests cover: rate_limit, not_authenticated, access_denied.
- [ ] Send payload tests verify all required fields present (not just non-null).
- [ ] Filter args construction test verifies correct mapping for state, labels, author, assignee, search.
- [ ] Feature behavior is reachable from real app flow: tests exercise the same code paths that production will use.
- [ ] No placeholder test patterns (`assert!(true)`, `#[ignore]`, empty bodies).

## Deferred Implementation Detection (Mandatory)

```bash
grep -RIn "TODO\|FIXME\|HACK\|placeholder\|for now\|will be implemented" src/github/
```

## Success Criteria
- [ ] TDD verification pass (RED tests confirmed, all 18 present).
- [ ] Traceability markers present.

## Failure Recovery
- rollback steps: Add missing tests. Fix test compilation errors. Add missing traceability markers.
- blocking issues: tests passing without implementation, missing test names, missing markers.

## Phase Completion Marker
Create: `project-plans/issue15/.completed/P07A.md`

Contents:
- phase ID: `PLAN-20260329-ISSUES-MODE.P07A`
- timestamp
- test name verification output (all 18)
- traceability marker verification output
- RED test failure list
- verification outputs
- semantic verification summary

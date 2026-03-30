# Phase 15A: Integration Hardening Verification

## Phase ID
`PLAN-20260329-ISSUES-MODE.P15A`

## Prerequisites
- Required: Phase P15 completed.
- Verify previous artifacts: `.completed/P15.md` exists.
- Expected files from previous phase: integration tests passing, all prior tests passing, zero deferred patterns.

## Requirements Implemented (Expanded)

### Verification of Integration Correctness for All REQ-ISS-* and REQ-ISS-NFR-*
**Requirement text**: Confirm full integration is correct, robust, and complete.

Behavior contract:
- GIVEN all integration tests and prior tests
- WHEN full verification suite runs
- THEN zero failures, zero deferred patterns, zero regressions, all requirements traceable.

Why it matters:
- Integration is the phase where component-level correctness is tested as a system. Gaps here — stale-scope handling, draft lifecycle, error recovery — are invisible in unit tests but critical to real-world reliability. This verification confirms the system is ready for the quality gate.

## Implementation Tasks

### Files to create
- `project-plans/issue15/.completed/P15A.md`
  - marker: `@plan PLAN-20260329-ISSUES-MODE.P15A`

### Files to modify
- `project-plans/issue15/plan/00-overview.md` -- tracker update

### Pseudocode traceability (if impl phase)
- N/A (verification phase)

## Verification Commands

```bash
cargo fmt --all --check
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo test --workspace --all-features
```

### Full Deferred Implementation Gate
```bash
grep -RIn "TODO\|FIXME\|HACK\|placeholder\|for now\|will be implemented" src/
grep -rn "todo!()\|unimplemented!()" src/ && echo "FAIL" || echo "OK: zero stubs"
```

### Comprehensive Test Summary
```bash
cargo test --workspace --all-features 2>&1 | grep "test result:"
```

### Requirement Traceability Verification
```bash
# Verify requirement markers exist in source code
for req in REQ-ISS-001 REQ-ISS-002 REQ-ISS-003 REQ-ISS-004 REQ-ISS-005 REQ-ISS-006 REQ-ISS-007 REQ-ISS-008 REQ-ISS-009 REQ-ISS-010 REQ-ISS-011 REQ-ISS-012 REQ-ISS-013 REQ-ISS-014; do
  if grep -rq "$req" src/; then
    echo "OK: $req traced in source"
  else
    echo "MISSING: $req not traced in source"
  fi
done
```

## Structural Verification Checklist
- [ ] All integration tests pass (19 tests).
- [ ] All prior tests pass (full regression -- total count).
- [ ] Zero deferred patterns in `src/`.
- [ ] No skipped phases.
- [ ] Plan/requirement traceability present across all implementation files.
- [ ] Tests compile and run.
- [ ] Tracker updated.

## Semantic Verification Checklist (Mandatory)
- [ ] Full mode lifecycle works end-to-end: `i` -> browse -> interact -> `a`/`Esc`.
- [ ] All key bindings verified in all focus domains (repo_list, issue_list, issue_detail).
- [ ] Suppressed keys verified: `s`, `Ctrl-d`, `Ctrl-k`, `l` produce no state change in issues mode.
- [ ] Esc chain verified at all 6 levels (inline, chooser, search-clear, search-blur, filter, exit).
- [ ] Error handling verified for all `GhError` variants — no crash, stable mode.
- [ ] Draft preservation verified on API error.
- [ ] Draft discard verified on scope change.
- [ ] Stale-scope response suppression verified.
- [ ] Pagination verified for issues (auto-load) and comments (append, stable order).
- [ ] Focus restoration verified for valid and stale targets.
- [ ] Scope change invalidation verified.
- [ ] Send-to-agent payload verified with all fields.
- [ ] Inline exclusivity verified for all combinations.
- [ ] All 14 functional requirements traceable to source code markers.
- [ ] Feature behavior is reachable from real app flow: integration tests prove end-to-end flow.
- [ ] No placeholder/deferred patterns remain (verified by grep gate).

## Deferred Implementation Detection (Mandatory)

```bash
grep -RIn "TODO\|FIXME\|HACK\|placeholder\|for now\|will be implemented" src/
```

## Success Criteria
- [ ] Integration verification pass.
- [ ] Zero deferred patterns.
- [ ] All requirements traced.
- [ ] Verification commands pass.
- [ ] Semantic checks pass.

## Failure Recovery
- rollback steps: Fix remaining integration issues. If a specific test fails, identify the component boundary and fix the integration glue.
- blocking issues: failing tests, deferred patterns, missing requirement traceability.

## Phase Completion Marker
Create: `project-plans/issue15/.completed/P15A.md`

Contents:
- phase ID: `PLAN-20260329-ISSUES-MODE.P15A`
- timestamp
- deferred-impl gate output
- requirement traceability output (all 14)
- full test suite summary
- verification outputs
- semantic verification summary

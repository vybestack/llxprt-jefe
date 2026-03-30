# Phase 05A: Domain + State Contracts Implementation Verification

## Phase ID
`PLAN-20260329-ISSUES-MODE.P05A`

## Prerequisites
- Required: Phase P05 completed.
- Verify previous artifacts: `.completed/P05.md` exists.
- Expected files from previous phase: implemented state reducer in `src/state/mod.rs` with all P04 tests GREEN, all issue event arms fully implemented (no `todo!()`).

## Requirements Implemented (Expanded)

### Verification of State Contract Implementation for REQ-ISS-001,003,004,005,006,007,008,010
**Requirement text**: Confirm all domain + state behavior is implemented, all tests pass, and no regressions exist.

Behavior contract:
- GIVEN implemented state reducer with all issue-mode event arms
- WHEN all tests execute
- THEN all GREEN; no regressions in existing tests; no placeholder code in state reducer.

Why it matters:
- State layer is the foundation for all subsequent phases (key routing, UI, integration). A partially-implemented or broken state reducer produces cascading failures that are hard to diagnose through higher layers. Must be solid before proceeding.

## Implementation Tasks

### Files to create
- `project-plans/issue15/.completed/P05A.md`
  - marker: `@plan PLAN-20260329-ISSUES-MODE.P05A`

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

### No-Placeholder Verification
```bash
# Verify no todo!()/unimplemented!() remain in state reducer issue event arms
grep -n "todo!()\|unimplemented!()" src/state/mod.rs && echo "WARN: stubs may remain" || echo "OK: no stubs in state"
```

## Structural Verification Checklist
- [ ] All planned tests pass (zero failures).
- [ ] No existing tests broken (zero regressions).
- [ ] No skipped phases.
- [ ] Plan/requirement traceability present in all changed files.
- [ ] Tests compile and run.
- [ ] Tracker updated.

## Semantic Verification Checklist (Mandatory)
- [ ] Mode lifecycle works end-to-end in state layer (enter -> active -> exit -> restored).
- [ ] Focus save/restore handles edge cases: target removed after save, empty agents list.
- [ ] Esc chain handles all 6 priority levels correctly and in order.
- [ ] Inline exclusivity prevents dual-control states.
- [ ] Scope guard discards stale responses (verified: IssueListLoaded with mismatched repo ID is no-op).
- [ ] Draft discard on repo switch (verified: active inline is cancelled and notice emitted on ExitIssuesMode/scope change).
- [ ] Feature behavior is reachable from real app flow: state transitions produce valid AppState for subsequent key routing and UI rendering.
- [ ] No placeholder/deferred implementation patterns remain in issue event handling code.

## Deferred Implementation Detection (Mandatory)

```bash
grep -RIn "TODO\|FIXME\|HACK\|placeholder\|for now\|will be implemented" src/state/mod.rs src/domain/mod.rs
```

## Integration Contract Acceptance Gates
- [ ] **Backward compat**: Existing tests pass.
- [ ] **Old behavior preserved**: Non-issues `apply()` arms unchanged.

## Success Criteria
- [ ] Implementation verification pass.
- [ ] Verification commands pass.
- [ ] Semantic checks pass.

## Failure Recovery
- rollback steps: Fix failing tests or reducer logic. If regression found, bisect between P04A and P05 changes.
- blocking issues: test regressions, incomplete event handling.

## Phase Completion Marker
Create: `project-plans/issue15/.completed/P05A.md`

Contents:
- phase ID: `PLAN-20260329-ISSUES-MODE.P05A`
- timestamp
- test results summary (pass count, fail count)
- no-placeholder verification output
- verification outputs
- semantic verification summary

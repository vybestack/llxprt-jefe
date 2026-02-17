# Phase 07A: Runtime TDD Verification

## Phase ID
`PLAN-20260216-FIRSTVERSION-V1.P07A`

## Prerequisites
- Required: P07 completed.
- Verify previous marker: `.completed/P07.md`.
- Expected files: runtime RED test suites.

## Requirements Implemented (Expanded)

### REQ-TECH-006: runtime test gate
**Requirement text**: Verify runtime tests are requirement-mapped, behavior-first, and define lifecycle correctness.

Behavior contract:
- GIVEN runtime RED tests
- WHEN verification executes
- THEN tests are sufficient to guide P08 implementation.

Why it matters:
- Runtime correctness depends on strong behavioral tests.

## Implementation Tasks

### Files to create
- `project-plans/firstversion/.completed/P07A.md`
  - marker: `@plan PLAN-20260216-FIRSTVERSION-V1.P07A`
  - marker: `@requirement REQ-TECH-006`

### Files to modify
- `project-plans/firstversion/plan/00-overview.md` tracker
  - marker: `@plan PLAN-20260216-FIRSTVERSION-V1.P07A`
  - marker: `@requirement REQ-TECH-007`

### Pseudocode traceability (if impl phase)
- N/A

## Verification Commands

```bash
cargo fmt --all --check
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo test --workspace --all-features
```

## Structural Verification Checklist
- [ ] P07 marker exists.
- [ ] Runtime tests present and requirement-tagged.
- [ ] RED results captured.

## Semantic Verification Checklist (Mandatory)
- [ ] Runtime tests verify behavior over internals.
- [ ] Attach/kill/relaunch/input focus edge cases covered.
- [ ] Failure states preserve recoverability assumptions.

## Deferred Implementation Detection (Mandatory)

```bash
grep -RIn "TODO\|FIXME\|HACK\|placeholder\|for now\|will be implemented" tests/ src/
```

## Success Criteria
- [ ] Runtime TDD baseline approved for P08 implementation.

## Failure Recovery
- rollback steps: patch missing behavior tests and rerun P07A.
- blocking issues: insufficient runtime edge coverage.

## Phase Completion Marker
Create: `project-plans/firstversion/.completed/P07A.md`

Contents:
- phase ID
- timestamp
- files changed
- tests added/updated
- verification outputs
- semantic verification summary

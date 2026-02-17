# Phase 05A: Core Contracts Implementation Verification

## Phase ID
`PLAN-20260216-FIRSTVERSION-V1.P05A`

## Prerequisites
- Required: P05 completed.
- Verify previous phase marker: `.completed/P05.md`.
- Expected files from previous phase: implemented domain/state/persistence/theme core.

## Requirements Implemented (Expanded)

### REQ-TECH-001 + REQ-TECH-005: core boundary correctness verification
**Requirement text**: Verify implemented core contracts respect boundary ownership and persistence/theme constraints.

Behavior contract:
- GIVEN P05 implementation
- WHEN verification runs
- THEN no prohibited couplings exist and fallback contracts are satisfied.

Why it matters:
- Core layer errors propagate into all later runtime/UI phases.

## Implementation Tasks

### Files to create
- `project-plans/firstversion/.completed/P05A.md`
  - marker: `@plan PLAN-20260216-FIRSTVERSION-V1.P05A`
  - marker: `@requirement REQ-TECH-001`

### Files to modify
- `project-plans/firstversion/plan/00-overview.md`
  - update tracker for P05A
  - marker: `@plan PLAN-20260216-FIRSTVERSION-V1.P05A`
  - marker: `@requirement REQ-TECH-007`

### Pseudocode traceability (if impl phase)
- N/A

## Verification Commands

```bash
cargo fmt --all --check
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo test --workspace --all-features
```

(Optional coverage gate if applicable)

```bash
cargo llvm-cov --workspace --all-features --fail-under-lines 90
```

## Structural Verification Checklist
- [ ] P05 marker exists.
- [ ] All core test suites pass.
- [ ] Boundary modules compile and are wired.
- [ ] Tracker updated.

## Semantic Verification Checklist (Mandatory)
- [ ] Persistence fallback semantics match REQ-FUNC-001.
- [ ] Theme default/fallback semantics match REQ-FUNC-009.
- [ ] State transitions remain deterministic and typed.

## Deferred Implementation Detection (Mandatory)

```bash
grep -RIn "TODO\|FIXME\|HACK\|placeholder\|for now\|will be implemented" src/
```

## Success Criteria
- [ ] Core implementation accepted as baseline for runtime phases.

## Failure Recovery
- rollback steps: reopen P05 and patch failing boundary semantics.
- blocking issues to resolve before next phase: failed contract tests, unresolved coupling.

## Phase Completion Marker
Create: `project-plans/firstversion/.completed/P05A.md`

Contents:
- phase ID
- timestamp
- files changed
- tests added/updated
- verification outputs
- semantic verification summary

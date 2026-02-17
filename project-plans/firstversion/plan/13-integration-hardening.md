# Phase 13: Integration Hardening

## Phase ID
`PLAN-20260216-FIRSTVERSION-V1.P13`

## Prerequisites
- Required: P12A completed.
- Verify previous markers: `.completed/P12.md`, `.completed/P12A.md`.
- Expected files: integrated runtime/UI/core/persistence/theme.

## Requirements Implemented (Expanded)

### REQ-TECH-009 + REQ-FUNC-002..010
**Requirement text**: Harden cross-layer integration and ensure all user flows are reachable and coherent.

Behavior contract:
- GIVEN completed feature slices
- WHEN integration hardening runs
- THEN event flow, runtime orchestration, persistence, and UI behavior work end-to-end with no boundary leakage.

Why it matters:
- Produces stable, operable v1 behavior rather than isolated passing modules.

## Implementation Tasks

### Files to create
- `tests/integration/firstversion_end_to_end.rs`
- `tests/integration/recovery_paths.rs`
  - marker: `@plan PLAN-20260216-FIRSTVERSION-V1.P13`
  - marker: `@requirement REQ-TECH-009`

### Files to modify
- `src/app.rs`
- `src/main.rs`
- `tests/common/integration_harness.rs`
  - marker: `@plan PLAN-20260216-FIRSTVERSION-V1.P13`
  - marker: `@requirement REQ-FUNC-010`

### Pseudocode traceability (if impl phase)
- Uses pseudocode lines:
  - component-001: 01-45
  - component-002: 01-35
  - component-003: 01-33

## Verification Commands

```bash
cargo fmt --all --check
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo test --workspace --all-features
```

(Optional coverage gate if applicable)


Integration hardening must validate:
- `analysis/integration-contract-completeness-matrix.md`
- `analysis/requirement-phase-traceability-matrix.md`
- `analysis/runtime-lifecycle-acceptance-matrix.md`
- `analysis/hybrid-strategy-compliance-matrix.md`

```bash
cargo llvm-cov --workspace --all-features --fail-under-lines 90
```

## Structural Verification Checklist
- [ ] Integration tests added and wired.
- [ ] App bootstrap/event flows updated.
- [ ] No skipped phase dependencies.

## Semantic Verification Checklist (Mandatory)
- [ ] Full keyboard/operator workflows function end-to-end.
- [ ] Runtime failure/recovery paths are non-destructive and visible.
- [ ] Persistence/theme behavior survives restart scenarios.
- [ ] Architectural boundaries remain intact under integration load.

## Deferred Implementation Detection (Mandatory)

```bash
grep -RIn "TODO\|FIXME\|HACK\|placeholder\|for now\|will be implemented" src/ tests/
```

## Success Criteria
- [ ] End-to-end integration behavior stable and spec-complete.

## Failure Recovery
- rollback steps: isolate failing integration seam and patch boundary-conformant behavior.
- blocking issues: unresolved E2E flow failures.

## Phase Completion Marker
Create: `project-plans/firstversion/.completed/P13.md`

Contents:
- phase ID
- timestamp
- files changed
- tests added/updated
- verification outputs
- semantic verification summary

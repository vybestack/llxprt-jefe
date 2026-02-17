# Phase 03: Core Contracts Stub

## Phase ID
`PLAN-20260216-FIRSTVERSION-V1.P03`

## Prerequisites
- Required: P02A completed.
- Verify previous phase markers/artifacts exist: `.completed/P02.md`, `.completed/P02A.md`.
- Expected files from previous phase: specification + analysis + pseudocode components.

## Requirements Implemented (Expanded)

### REQ-TECH-001 + REQ-TECH-002: Core layer contract skeleton
**Requirement text**: Create compile-safe skeletons for rebuilt non-UI core boundaries and typed interfaces.

Behavior contract:
- GIVEN completed specification and pseudocode
- WHEN core stubs are added
- THEN modules, types, and interfaces exist with clear ownership and no cross-layer shortcut.

Why it matters:
- Establishes architecture-safe foundation before behavior implementation.

## Implementation Tasks

### Files to create
- `src/domain/mod.rs` - canonical entity/types skeletons
  - marker: `@plan PLAN-20260216-FIRSTVERSION-V1.P03`
  - marker: `@requirement REQ-TECH-001`
- `src/state/mod.rs` - AppState/event enum skeletons
  - marker: `@plan PLAN-20260216-FIRSTVERSION-V1.P03`
  - marker: `@requirement REQ-TECH-003`
- `src/runtime/mod.rs` - runtime boundary trait/skeleton
  - marker: `@plan PLAN-20260216-FIRSTVERSION-V1.P03`
  - marker: `@requirement REQ-TECH-004`
- `src/persistence/mod.rs` - persistence boundary skeleton
  - marker: `@plan PLAN-20260216-FIRSTVERSION-V1.P03`
  - marker: `@requirement REQ-TECH-005`
- `src/theme/mod.rs` - theme boundary skeleton
  - marker: `@plan PLAN-20260216-FIRSTVERSION-V1.P03`
  - marker: `@requirement REQ-FUNC-009`

### Files to modify
- `src/main.rs` and/or `src/app.rs` to wire new module boundaries (stubs only)
  - marker: `@plan PLAN-20260216-FIRSTVERSION-V1.P03`
  - marker: `@requirement REQ-TECH-001`

### Pseudocode traceability (if impl phase)
- Uses pseudocode lines:
  - component-001: 01-12
  - component-002: 01-06
  - component-003: 01-08

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
- [ ] Core module skeletons created.
- [ ] Boundaries compile with stub wiring.
- [ ] Phase markers included in changed files.
- [ ] No skipped phase dependencies.

## Semantic Verification Checklist (Mandatory)
- [ ] Ownership split reflects technical-overview boundaries.
- [ ] No UI direct runtime/filesystem side effects remain in new stubs.
- [ ] Hybrid strategy is encoded (UI reused; core rebuilt).

## Deferred Implementation Detection (Mandatory)

```bash
grep -RIn "TODO\|FIXME\|HACK\|placeholder\|for now\|will be implemented" src/
```

## Success Criteria
- [ ] Compile-safe core boundary stubs exist.
- [ ] Verification commands pass.
- [ ] Semantic checks pass.

## Failure Recovery
- rollback steps: revert boundary stubs that violate ownership; reapply with clean contracts.
- blocking issues to resolve before next phase: missing type contracts, boundary leakage.

## Phase Completion Marker
Create: `project-plans/firstversion/.completed/P03.md`

Contents:
- phase ID
- timestamp
- files changed
- tests added/updated
- verification outputs
- semantic verification summary

# Phase 06: Runtime Boundary Stub

## Phase ID
`PLAN-20260216-FIRSTVERSION-V1.P06`

## Prerequisites
- Required: P05A completed.
- Verify previous markers: `.completed/P05.md`, `.completed/P05A.md`.
- Expected files: validated core contracts.

## Requirements Implemented (Expanded)

### REQ-TECH-004: runtime orchestration boundary skeleton
**Requirement text**: Create compile-safe runtime manager interface and session identity model without full behavior implementation.

Behavior contract:
- GIVEN validated core contracts
- WHEN runtime stubs are added
- THEN kill/relaunch/attach/input/liveness APIs are typed and callable.

Why it matters:
- Establishes stable seam for runtime TDD phase.

## Implementation Tasks

### Files to create
- `src/runtime/manager.rs`
- `src/runtime/session.rs`
- `src/runtime/errors.rs`
  - marker: `@plan PLAN-20260216-FIRSTVERSION-V1.P06`
  - marker: `@requirement REQ-TECH-004`

### Files to modify
- `src/runtime/mod.rs`
- `src/app.rs` runtime command wiring (stub route only)
  - marker: `@plan PLAN-20260216-FIRSTVERSION-V1.P06`
  - marker: `@requirement REQ-FUNC-007`

### Pseudocode traceability (if impl phase)
- Uses pseudocode lines:
  - component-002: 01-14

## Verification Commands

```bash
cargo fmt --all --check
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo test --workspace --all-features
```

## Structural Verification Checklist
- [ ] Runtime stub modules compile.
- [ ] App wiring references runtime boundary only.
- [ ] No skipped phase dependencies.

## Semantic Verification Checklist (Mandatory)
- [ ] Runtime API covers required lifecycle operations.
- [ ] No direct UI ownership of runtime orchestration.

## Deferred Implementation Detection (Mandatory)

```bash
grep -RIn "TODO\|FIXME\|HACK\|placeholder\|for now\|will be implemented" src/
```

## Success Criteria
- [ ] Runtime seam established for P07 TDD.

## Failure Recovery
- rollback steps: correct runtime API shape and boundary ownership.
- blocking issues: missing method contracts, invalid wiring.

## Phase Completion Marker
Create: `project-plans/firstversion/.completed/P06.md`

Contents:
- phase ID
- timestamp
- files changed
- tests added/updated
- verification outputs
- semantic verification summary

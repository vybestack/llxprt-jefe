# Phase 05: Core Contracts Implementation

## Phase ID
`PLAN-20260216-FIRSTVERSION-V1.P05`

## Prerequisites
- Required: P04A completed.
- Verify previous phase markers/artifacts exist: `.completed/P04.md`, `.completed/P04A.md`.
- Expected files from previous phase: core contract test suites.

## Requirements Implemented (Expanded)

### REQ-TECH-001/002/003/005: Implement rebuilt non-UI core contracts
**Requirement text**: Implement typed domain/state/persistence/theme core behavior to satisfy P04 tests and architecture boundaries.

Behavior contract:
- GIVEN failing contract tests
- WHEN implementation is completed
- THEN tests pass with deterministic state transitions and validated persistence/theme rules.

Why it matters:
- Delivers the rebuilt core that differentiates firstversion from toy1 internals.

### REQ-FUNC-001/009/010: startup + fallback + error behavior
**Requirement text**: Ensure startup fallback, Green Screen fallback, and recoverable error surfacing are implemented.

Behavior contract:
- GIVEN missing/malformed inputs or invalid theme slug
- WHEN boundary APIs execute
- THEN canonical fallback and user-visible error state behavior occurs.

Why it matters:
- Meets baseline reliability and UX requirements.

## Implementation Tasks

### Files to create
- `src/domain/entities.rs`
- `src/state/events.rs`
- `src/state/reducer.rs`
- `src/persistence/io.rs`
- `src/theme/manager.rs`
  - marker: `@plan PLAN-20260216-FIRSTVERSION-V1.P05`
  - marker: `@requirement REQ-TECH-001`

### Files to modify
- `src/domain/mod.rs`
- `src/state/mod.rs`
- `src/persistence/mod.rs`
- `src/theme/mod.rs`
- `src/app.rs` (wire new contracts into app state flow)
  - marker: `@plan PLAN-20260216-FIRSTVERSION-V1.P05`
  - marker: `@requirement REQ-TECH-003`

### Pseudocode traceability (if impl phase)
- Uses pseudocode lines:
  - component-001: 01-45
  - component-003: 01-33

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
- [ ] Core implementation files created/updated.
- [ ] P04 RED tests now pass.
- [ ] Phase markers and requirement tags present.
- [ ] No skipped phase dependencies.

## Semantic Verification Checklist (Mandatory)
- [ ] Reducer transitions are deterministic and boundary-safe.
- [ ] Persistence validates + falls back safely.
- [ ] Theme manager enforces Green Screen default/fallback.
- [ ] Recoverable errors are surfaced without crashing app flow.

## Deferred Implementation Detection (Mandatory)

```bash
grep -RIn "TODO\|FIXME\|HACK\|placeholder\|for now\|will be implemented" src/
```

## Success Criteria
- [ ] Core non-UI contracts implemented and passing tests.
- [ ] Verification commands pass.
- [ ] Semantic checks pass.

## Failure Recovery
- rollback steps: isolate failing boundary and patch with typed contract fix.
- blocking issues to resolve before next phase: failing core tests, boundary leakage.

## Phase Completion Marker
Create: `project-plans/firstversion/.completed/P05.md`

Contents:
- phase ID
- timestamp
- files changed
- tests added/updated
- verification outputs
- semantic verification summary

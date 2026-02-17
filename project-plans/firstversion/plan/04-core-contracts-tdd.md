# Phase 04: Core Contracts TDD

## Phase ID
`PLAN-20260216-FIRSTVERSION-V1.P04`

## Prerequisites
- Required: P03A completed.
- Verify previous phase markers/artifacts exist: `.completed/P03.md`, `.completed/P03A.md`.
- Expected files from previous phase: domain/state/runtime/persistence/theme stubs.

## Requirements Implemented (Expanded)

### REQ-TECH-002 + REQ-TECH-003: Typed contract behavior tests
**Requirement text**: Write failing tests that define expected behavior for typed domain/event/state/persistence/theme contracts.

Behavior contract:
- GIVEN compile-safe stubs
- WHEN test suite for core behavior is added
- THEN tests fail for unimplemented behavior and describe expected semantics.

Why it matters:
- Enforces RED step and prevents speculative implementation.

### REQ-FUNC-001 + REQ-FUNC-009: persistence/theme baseline tests
**Requirement text**: Define startup fallback, malformed payload handling, and Green Screen fallback behavior in tests.

Behavior contract:
- GIVEN invalid/missing persistence/theme inputs
- WHEN startup/theme resolve paths execute
- THEN safe defaults and fallback behavior are asserted.

Why it matters:
- Protects highest-risk startup and UX baseline behavior.

## Implementation Tasks

### Files to create
- `tests/core/domain_state_contracts.rs`
  - marker: `@plan PLAN-20260216-FIRSTVERSION-V1.P04`
  - marker: `@requirement REQ-TECH-002`
- `tests/core/persistence_theme_contracts.rs`
  - marker: `@plan PLAN-20260216-FIRSTVERSION-V1.P04`
  - marker: `@requirement REQ-FUNC-001`
  - marker: `@requirement REQ-FUNC-009`

### Files to modify
- `Cargo.toml` (test-only dependencies/features if needed)
  - marker: `@plan PLAN-20260216-FIRSTVERSION-V1.P04`
  - marker: `@requirement REQ-TECH-007`

### Pseudocode traceability (if impl phase)
- Uses pseudocode lines:
  - component-001: 01-33
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
- [ ] New core tests exist and compile.
- [ ] Tests target behavior contracts, not internals only.
- [ ] At least one required test fails before implementation updates.
- [ ] No skipped phase dependencies.

## Semantic Verification Checklist (Mandatory)
- [ ] Tests cover typed event transitions and invariants.
- [ ] Tests cover malformed/missing persistence fallback.
- [ ] Tests cover Green Screen default/fallback behavior.

## Deferred Implementation Detection (Mandatory)

```bash
grep -RIn "TODO\|FIXME\|HACK\|placeholder\|for now\|will be implemented" tests/ src/
```

## Success Criteria
- [ ] RED test suite established for core contracts.
- [ ] Verification commands pass except expected RED failures.
- [ ] Semantic checks pass.

## Failure Recovery
- rollback steps: trim brittle tests and rewrite around externally observable behavior.
- blocking issues to resolve before next phase: insufficient failing tests, weak behavior assertions.

## Phase Completion Marker
Create: `project-plans/firstversion/.completed/P04.md`

Contents:
- phase ID
- timestamp
- files changed
- tests added/updated
- verification outputs
- semantic verification summary

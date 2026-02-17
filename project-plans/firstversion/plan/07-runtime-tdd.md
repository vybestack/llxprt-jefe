# Phase 07: Runtime Lifecycle TDD

## Phase ID
`PLAN-20260216-FIRSTVERSION-V1.P07`

## Prerequisites
- Required: P06A completed.
- Verify previous markers: `.completed/P06.md`, `.completed/P06A.md`.
- Expected files: runtime stubs.

## Requirements Implemented (Expanded)

### REQ-FUNC-005 + REQ-FUNC-007: runtime behavior tests
**Requirement text**: Add failing tests for terminal focus routing, attach/reattach safety, kill, relaunch, and status transitions.

Behavior contract:
- GIVEN runtime stubs
- WHEN runtime tests are added
- THEN tests define required lifecycle semantics and fail pre-implementation.

Why it matters:
- Locks behavior before implementation and avoids regression-prone runtime rewrites.

## Implementation Tasks

### Files to create
- `tests/runtime/runtime_lifecycle.rs`
  - marker: `@plan PLAN-20260216-FIRSTVERSION-V1.P07`
  - marker: `@requirement REQ-FUNC-007`
- `tests/runtime/terminal_focus_routing.rs`
  - marker: `@plan PLAN-20260216-FIRSTVERSION-V1.P07`
  - marker: `@requirement REQ-FUNC-005`

### Files to modify
- `tests/common/mod.rs` fixtures/helpers for runtime mocks or harness
  - marker: `@plan PLAN-20260216-FIRSTVERSION-V1.P07`
  - marker: `@requirement REQ-TECH-009`

### Pseudocode traceability (if impl phase)
- Uses pseudocode lines:
  - component-002: 07-35

## Verification Commands

```bash
cargo fmt --all --check
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo test --workspace --all-features
```

## Structural Verification Checklist
- [ ] Runtime test files present.
- [ ] RED evidence recorded for lifecycle tests.
- [ ] No skipped phase dependencies.

## Semantic Verification Checklist (Mandatory)
- [ ] Tests capture attach/reattach safety expectations.
- [ ] Tests capture focused/unfocused input routing.
- [ ] Tests capture kill/relaunch/dead-state transitions.

## Deferred Implementation Detection (Mandatory)

```bash
grep -RIn "TODO\|FIXME\|HACK\|placeholder\|for now\|will be implemented" tests/ src/
```

## Success Criteria
- [ ] Runtime RED tests are explicit and actionable.

## Failure Recovery
- rollback steps: replace fragile test internals with behavior assertions.
- blocking issues: missing runtime edge-case coverage.

## Phase Completion Marker
Create: `project-plans/firstversion/.completed/P07.md`

Contents:
- phase ID
- timestamp
- files changed
- tests added/updated
- verification outputs
- semantic verification summary

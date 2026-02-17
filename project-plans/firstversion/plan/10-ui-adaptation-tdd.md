# Phase 10: UI Adaptation TDD

## Phase ID
`PLAN-20260216-FIRSTVERSION-V1.P10`

## Prerequisites
- Required: P09A completed.
- Verify previous markers: `.completed/P09.md`, `.completed/P09A.md`.
- Expected files: UI adaptation stubs.

## Requirements Implemented (Expanded)

### REQ-FUNC-002..008 + REQ-FUNC-010
**Requirement text**: Add failing behavior tests for dashboard navigation, split mode, forms/modals, help/search, and error visibility.

Behavior contract:
- GIVEN UI stubs
- WHEN UI behavior tests are written
- THEN RED tests define user-visible keyboard and modal behavior contracts.

Why it matters:
- Prevents UI regressions while adapting toy1 patterns.

## Implementation Tasks

### Files to create
- `tests/ui/dashboard_navigation.rs`
- `tests/ui/split_mode_behavior.rs`
- `tests/ui/forms_and_modals.rs`
- `tests/ui/help_search_behavior.rs`
  - marker: `@plan PLAN-20260216-FIRSTVERSION-V1.P10`
  - marker: `@requirement REQ-FUNC-002`

### Files to modify
- `tests/common/ui_harness.rs`
  - marker: `@plan PLAN-20260216-FIRSTVERSION-V1.P10`
  - marker: `@requirement REQ-TECH-009`

### Pseudocode traceability (if impl phase)
- Uses pseudocode lines:
  - component-001: 13-37

## Verification Commands

```bash
cargo fmt --all --check
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo test --workspace --all-features
```


### Additional acceptance artifacts to cover in tests
- `analysis/f12-cross-view-consistency-matrix.md`
- `analysis/search-help-acceptance-contract.md`
- `analysis/crud-validation-error-matrix.md`

## Structural Verification Checklist
- [ ] UI RED tests exist and compile.
- [ ] Tests map to requirements and key user paths.
- [ ] No skipped phase dependencies.

## Semantic Verification Checklist (Mandatory)
- [ ] Tests verify deterministic keyboard behavior.
- [ ] Tests cover split-mode reorder/grab rules.
- [ ] Tests cover forms, confirms, help/search reversibility.
- [ ] Tests cover error visibility behavior.

## Deferred Implementation Detection (Mandatory)

```bash
grep -RIn "TODO\|FIXME\|HACK\|placeholder\|for now\|will be implemented" tests/ src/ui src/
```

## Success Criteria
- [ ] RED UI test suite established for P11 implementation.

## Failure Recovery
- rollback steps: rewrite brittle tests to focus on user-visible behavior.
- blocking issues: missing or weak UI behavior assertions.

## Phase Completion Marker
Create: `project-plans/firstversion/.completed/P10.md`

Contents:
- phase ID
- timestamp
- files changed
- tests added/updated
- verification outputs
- semantic verification summary

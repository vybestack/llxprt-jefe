# Phase 11: UI Adaptation Implementation

## Phase ID
`PLAN-20260216-FIRSTVERSION-V1.P11`

## Prerequisites
- Required: P10A completed.
- Verify previous markers: `.completed/P10.md`, `.completed/P10A.md`.
- Expected files: UI RED tests + adaptation stubs.

## Requirements Implemented (Expanded)

### REQ-FUNC-002..008 + REQ-FUNC-010 + REQ-TECH-010
**Requirement text**: Implement UI behavior by adapting toy1 composition patterns onto rebuilt core contracts.

Behavior contract:
- GIVEN UI RED tests
- WHEN implementation is completed
- THEN dashboard/split/forms/help/search and focus/keybinding behavior pass and remain boundary-safe.

Why it matters:
- Delivers user-facing workflows while preserving architecture goals.

## Implementation Tasks

### Files to create
- `src/ui/screens/search.rs`
- `src/ui/modals/confirm.rs`
  - marker: `@plan PLAN-20260216-FIRSTVERSION-V1.P11`
  - marker: `@requirement REQ-FUNC-008`

### Files to modify
- `src/ui/screens/dashboard.rs`
- `src/ui/screens/split.rs`
- `src/ui/modals/forms.rs`
- `src/ui/modals/help.rs`
- `src/ui/keybindings.rs`
- `src/app.rs` (event wiring)
  - marker: `@plan PLAN-20260216-FIRSTVERSION-V1.P11`
  - marker: `@requirement REQ-FUNC-005`

### Pseudocode traceability (if impl phase)
- Uses pseudocode lines:
  - component-001: 13-37

## Verification Commands

```bash
cargo fmt --all --check
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo test --workspace --all-features
```

(Optional coverage gate if applicable)


Implementation must satisfy:
- `analysis/f12-cross-view-consistency-matrix.md`
- `analysis/search-help-acceptance-contract.md`
- `analysis/crud-validation-error-matrix.md`
- `analysis/hybrid-strategy-compliance-matrix.md`

```bash
cargo llvm-cov --workspace --all-features --fail-under-lines 90
```

## Structural Verification Checklist
- [ ] Planned UI files updated.
- [ ] UI RED tests pass.
- [ ] Phase markers and requirement tags present.

## Semantic Verification Checklist (Mandatory)
- [ ] Focus/unfocus behavior is explicit and consistent.
- [ ] Split mode behavior matches functional spec and mockups.
- [ ] Forms/modals/help/search are reversible and non-destructive.
- [ ] UI consumes core contracts without boundary violations.

## Deferred Implementation Detection (Mandatory)

```bash
grep -RIn "TODO\|FIXME\|HACK\|placeholder\|for now\|will be implemented" src/ui src/
```

## Success Criteria
- [ ] User-facing UI adaptation implemented and verified.

## Failure Recovery
- rollback steps: revert violating UI paths and reapply with event-driven wiring.
- blocking issues: failing UI tests, keyboard/focus regressions.

## Phase Completion Marker
Create: `project-plans/firstversion/.completed/P11.md`

Contents:
- phase ID
- timestamp
- files changed
- tests added/updated
- verification outputs
- semantic verification summary

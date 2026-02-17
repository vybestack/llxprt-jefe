# Phase 09A: UI Adaptation Stub Verification

## Phase ID
`PLAN-20260216-FIRSTVERSION-V1.P09A`

## Prerequisites
- Required: P09 completed.
- Verify previous marker: `.completed/P09.md`.
- Expected files: UI adaptation stubs.

## Requirements Implemented (Expanded)

### REQ-TECH-010 + REQ-FUNC-002
**Requirement text**: Verify toy1-pattern UI reuse is present and wired through rebuilt core contracts.

Behavior contract:
- GIVEN UI adaptation stubs
- WHEN verification runs
- THEN UI structure aligns with mockups and boundary contracts.

Why it matters:
- Confirms hybrid strategy is being executed correctly.

## Implementation Tasks

### Files to create
- `project-plans/firstversion/.completed/P09A.md`
  - marker: `@plan PLAN-20260216-FIRSTVERSION-V1.P09A`
  - marker: `@requirement REQ-TECH-010`

### Files to modify
- `project-plans/firstversion/plan/00-overview.md` tracker
  - marker: `@plan PLAN-20260216-FIRSTVERSION-V1.P09A`
  - marker: `@requirement REQ-TECH-006`

### Pseudocode traceability (if impl phase)
- N/A

## Verification Commands

```bash
cargo fmt --all --check
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo test --workspace --all-features
```

## Structural Verification Checklist
- [ ] P09 marker exists.
- [ ] UI stubs for dashboard/split/forms/help/search are present.
- [ ] Tracker updated.

## Semantic Verification Checklist (Mandatory)
- [ ] Key keyboard flows are represented in stubs.
- [ ] UI contract does not bypass app-state/events.
- [ ] Mockup-aligned behavior seams are visible.

## Deferred Implementation Detection (Mandatory)

```bash
grep -RIn "TODO\|FIXME\|HACK\|placeholder\|for now\|will be implemented" src/ui src/
```

## Success Criteria
- [ ] UI stubs approved for UI behavior TDD phase.

## Failure Recovery
- rollback steps: patch missing screen/modal/focus structures and rerun P09A.
- blocking issues: incomplete UI shell coverage.

## Phase Completion Marker
Create: `project-plans/firstversion/.completed/P09A.md`

Contents:
- phase ID
- timestamp
- files changed
- tests added/updated
- verification outputs
- semantic verification summary

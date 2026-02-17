# Phase 04A: Core Contracts TDD Verification

## Phase ID
`PLAN-20260216-FIRSTVERSION-V1.P04A`

## Prerequisites
- Required: P04 completed.
- Verify previous phase marker: `.completed/P04.md`.
- Expected files from previous phase: core behavior test suites.

## Requirements Implemented (Expanded)

### REQ-TECH-006: Test-quality verification
**Requirement text**: Verify core tests are behavior-driven, requirement-mapped, and sufficient for implementation guidance.

Behavior contract:
- GIVEN P04 test suites
- WHEN verification executes
- THEN failing tests reflect real required behavior and provide implementation target clarity.

Why it matters:
- Prevents false-green implementation with weak test coverage.

## Implementation Tasks

### Files to create
- `project-plans/firstversion/.completed/P04A.md`
  - marker: `@plan PLAN-20260216-FIRSTVERSION-V1.P04A`
  - marker: `@requirement REQ-TECH-006`

### Files to modify
- `project-plans/firstversion/plan/00-overview.md`
  - update tracker for P04A
  - marker: `@plan PLAN-20260216-FIRSTVERSION-V1.P04A`
  - marker: `@requirement REQ-TECH-007`

### Pseudocode traceability (if impl phase)
- N/A

## Verification Commands

```bash
cargo fmt --all --check
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo test --workspace --all-features
```

## Structural Verification Checklist
- [ ] P04 marker exists.
- [ ] Core test files are present and requirement-tagged.
- [ ] RED behavior evidence recorded.

## Semantic Verification Checklist (Mandatory)
- [ ] Tests verify required behavior and edge/error paths.
- [ ] Tests are not mostly implementation-detail assertions.
- [ ] Theme and persistence fallback requirements are represented.

## Deferred Implementation Detection (Mandatory)

```bash
grep -RIn "TODO\|FIXME\|HACK\|placeholder\|for now\|will be implemented" tests/ src/
```

## Success Criteria
- [ ] Core TDD baseline accepted for implementation phase P05.

## Failure Recovery
- rollback steps: rewrite weak tests; rerun P04A.
- blocking issues to resolve before next phase: insufficient RED coverage.

## Phase Completion Marker
Create: `project-plans/firstversion/.completed/P04A.md`

Contents:
- phase ID
- timestamp
- files changed
- tests added/updated
- verification outputs
- semantic verification summary

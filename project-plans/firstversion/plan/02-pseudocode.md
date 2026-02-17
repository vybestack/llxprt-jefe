# Phase 02: Pseudocode Authoring

## Phase ID
`PLAN-20260216-FIRSTVERSION-V1.P02`

## Prerequisites
- Required: Phase P01A completed.
- Verify previous artifacts: `.completed/P01.md`, `.completed/P01A.md`.
- Expected files from previous phase: `analysis/domain-model.md`.

## Requirements Implemented (Expanded)

### REQ-TECH-006: Algorithmic pseudocode baseline
**Requirement text**: Provide numbered pseudocode for key components and integration paths.

Behavior contract:
- GIVEN validated domain model
- WHEN pseudocode components are authored
- THEN implementation phases can reference explicit line ranges.

Why it matters:
- Enables deterministic implementation and review.

### REQ-TECH-004 + REQ-TECH-005: Runtime/persistence/theme sequencing
**Requirement text**: Encode runtime lifecycle sequencing and persistence/theme fallback logic algorithmically.

Behavior contract:
- GIVEN technical architecture contracts
- WHEN pseudocode is reviewed
- THEN kill/relaunch/attach + file validation/atomic writes/theme fallback are explicit.

Why it matters:
- These are high-risk correctness paths.

## Implementation Tasks

### Files to create
- `project-plans/firstversion/analysis/pseudocode/component-001.md`
  - marker: `@plan PLAN-20260216-FIRSTVERSION-V1.P02`
  - marker: `@requirement REQ-TECH-006`
- `project-plans/firstversion/analysis/pseudocode/component-002.md`
  - marker: `@plan PLAN-20260216-FIRSTVERSION-V1.P02`
  - marker: `@requirement REQ-TECH-004`
- `project-plans/firstversion/analysis/pseudocode/component-003.md`
  - marker: `@plan PLAN-20260216-FIRSTVERSION-V1.P02`
  - marker: `@requirement REQ-TECH-005`

### Files to modify
- `project-plans/firstversion/plan/00-overview.md`
  - update tracker for P02
  - marker: `@plan PLAN-20260216-FIRSTVERSION-V1.P02`
  - marker: `@requirement REQ-TECH-006`


### Additional pseudocode-linked contracts to verify in this phase
- Validate lifecycle transitions against `analysis/runtime-lifecycle-acceptance-matrix.md`
- Validate F12 routing expectations against `analysis/f12-cross-view-consistency-matrix.md`
- Validate persistence/theme algorithm alignment against:
  - `analysis/persistence-matrix.md`
  - `analysis/theme-precedence-fallback-policy.md`
- Validate exact persistence path resolution order is encoded in pseudocode:
  - `JEFE_SETTINGS_PATH` -> `JEFE_CONFIG_DIR/settings.toml` -> platform default
  - `JEFE_STATE_PATH` -> `JEFE_STATE_DIR/state.json` -> platform default

### Pseudocode traceability (if impl phase)
- future implementation phases must cite line ranges from:
  - component-001
  - component-002
  - component-003

## Verification Commands

```bash
cargo fmt --all --check
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo test --workspace --all-features
```

Optional coverage gate:

```bash
cargo llvm-cov --workspace --all-features --fail-under-lines 90
```

## Structural Verification Checklist
- [ ] Three pseudocode component files exist.
- [ ] Algorithms are line-numbered and concrete.
- [ ] Validation/error/ordering constraints are present.
- [ ] Tracker updated.

## Semantic Verification Checklist (Mandatory)
- [ ] Pseudocode covers all required major flows.
- [ ] Runtime lifecycle and persistence/theme logic are complete.
- [ ] Hybrid strategy boundaries are reflected.

## Deferred Implementation Detection (Mandatory)

```bash
grep -RIn "TODO\|FIXME\|HACK\|placeholder\|for now\|will be implemented" project-plans/firstversion/analysis/pseudocode/
```

## Success Criteria
- [ ] Pseudocode accepted as implementation contract baseline.

## Failure Recovery
- rollback steps: patch pseudocode gaps and rerun P02 verification.
- blocking issues to resolve before next phase: missing algorithm sections/line references.

## Phase Completion Marker
Create: `project-plans/firstversion/.completed/P02.md`

Contents:
- phase ID
- timestamp
- files changed
- verification outputs
- semantic verification summary

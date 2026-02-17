# Phase 00A: Preflight Verification

## Phase ID
`PLAN-20260216-FIRSTVERSION-V1.P00A`

## Prerequisites
- Required: specification finalized.
- Verify existing docs: overview.md, technical-overview.md, ui-mockups.md.
- Expected files from previous phase: none.

## Requirements Implemented (Expanded)

### REQ-TECH-007: Toolchain readiness
**Requirement text**: Required Rust quality commands are available and workspace-compatible.

Behavior contract:
- GIVEN repository workspace
- WHEN preflight commands run
- THEN toolchain and quality gates are confirmed executable.

Why it matters:
- Prevents planning against unavailable gates.

### REQ-TECH-001: Boundary feasibility
**Requirement text**: Verify target module boundaries and toy1 UI reference paths are present.

Behavior contract:
- GIVEN hybrid strategy
- WHEN source paths are inspected
- THEN concrete file targets are validated.

Why it matters:
- Ensures phases use real paths and interfaces.

## Implementation Tasks

### Files to create
- `project-plans/firstversion/.completed/P00A.md` - preflight evidence log
  - marker: `@plan PLAN-20260216-FIRSTVERSION-V1.P00A`
  - marker: `@requirement REQ-TECH-007`

### Files to modify
- `project-plans/firstversion/plan/00-overview.md` - tracker status updates
  - marker: `@plan PLAN-20260216-FIRSTVERSION-V1.P00A`
  - marker: `@requirement REQ-TECH-001`

### Pseudocode traceability (if impl phase)
- N/A (preflight phase)

## Verification Commands

```bash
cargo --version
rustc --version
cargo clippy --version
cargo fmt --all --check
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo test --workspace --all-features
```

Optional coverage gate:

```bash
cargo llvm-cov --version
```

## Structural Verification Checklist
- [ ] Required source docs and reference paths exist.
- [ ] Toolchain commands execute.
- [ ] Exact persistence path policy is documented and cross-referenced (overview/technical/spec/analysis).
- [ ] Preflight evidence captured.
- [ ] Tracker updated.

## Semantic Verification Checklist (Mandatory)
- [ ] Plan assumptions map to real files/modules.
- [ ] Hybrid strategy is feasible in this codebase.
- [ ] No blocker remains unresolved.

## Deferred Implementation Detection (Mandatory)

```bash
grep -RIn "TODO\|FIXME\|HACK\|placeholder\|for now\|will be implemented" project-plans/firstversion/plan/
```

## Success Criteria
- [ ] Preflight pass recorded.
- [ ] Any blocker has explicit remediation path.

## Failure Recovery
- rollback steps: stop phase progression; patch plan targets and assumptions.
- blocking issues to resolve before next phase: unavailable toolchain/path/contracts.

## Phase Completion Marker
Create: `project-plans/firstversion/.completed/P00A.md`

Contents:
- phase ID
- timestamp
- files checked
- command outputs
- pass/fail decision

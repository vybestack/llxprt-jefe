# Phase 08: Runtime Implementation

## Phase ID
`PLAN-20260216-FIRSTVERSION-V1.P08`

## Prerequisites
- Required: P07A completed.
- Verify previous markers: `.completed/P07.md`, `.completed/P07A.md`.
- Expected files: runtime lifecycle RED tests.

## Requirements Implemented (Expanded)

### REQ-FUNC-005 + REQ-FUNC-007 + REQ-TECH-004
**Requirement text**: Implement runtime lifecycle behavior to satisfy runtime TDD suite and architectural boundaries.

Behavior contract:
- GIVEN runtime RED tests
- WHEN runtime manager implementation is complete
- THEN attach/reattach, focus routing, kill/relaunch, and liveness behavior pass with recoverable error handling.

Why it matters:
- Runtime behavior is a core product differentiator and risk area.

## Implementation Tasks

### Files to create
- `src/runtime/attach.rs`
- `src/runtime/liveness.rs`
- `src/runtime/commands.rs`
  - marker: `@plan PLAN-20260216-FIRSTVERSION-V1.P08`
  - marker: `@requirement REQ-TECH-004`

### Files to modify
- `src/runtime/manager.rs`
- `src/runtime/session.rs`
- `src/app.rs` runtime event integration
  - marker: `@plan PLAN-20260216-FIRSTVERSION-V1.P08`
  - marker: `@requirement REQ-FUNC-007`

### Pseudocode traceability (if impl phase)
- Uses pseudocode lines:
  - component-002: 01-35

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
- [ ] Runtime implementation files created/updated.
- [ ] P07 runtime tests pass.
- [ ] No skipped dependencies.

## Semantic Verification Checklist (Mandatory)
- [ ] Input forwarding happens only in focused terminal mode.
- [ ] Rearchitected runtime remains recoverable after failure.
- [ ] Kill/relaunch preserve launch signature/profile/mode.

## Deferred Implementation Detection (Mandatory)

```bash
grep -RIn "TODO\|FIXME\|HACK\|placeholder\|for now\|will be implemented" src/
```

## Success Criteria
- [ ] Runtime behavior implemented and verified.

## Failure Recovery
- rollback steps: isolate failing lifecycle path; patch contract-conformant behavior.
- blocking issues: test failures in attach/kill/relaunch/liveness.

## Phase Completion Marker
Create: `project-plans/firstversion/.completed/P08.md`

Contents:
- phase ID
- timestamp
- files changed
- tests added/updated
- verification outputs
- semantic verification summary

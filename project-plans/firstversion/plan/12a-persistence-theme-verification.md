# Phase 12A: Persistence + Theme Verification

## Phase ID
`PLAN-20260216-FIRSTVERSION-V1.P12A`

## Prerequisites
- Required: P12 completed.
- Verify previous marker: `.completed/P12.md`.
- Expected files: persistence/theme hardening implementation.

## Requirements Implemented (Expanded)

### REQ-TECH-005 + REQ-FUNC-009 verification
**Requirement text**: Verify persistence contract and Green Screen fallback policy are fully enforced.

Behavior contract:
- GIVEN implemented persistence/theme paths
- WHEN verification executes
- THEN settings/state file behavior and theme fallback satisfy contract under normal and error conditions.

Why it matters:
- Ensures core durability and non-bright default UX guarantee.

## Implementation Tasks

### Files to create
- `project-plans/firstversion/.completed/P12A.md`
  - marker: `@plan PLAN-20260216-FIRSTVERSION-V1.P12A`
  - marker: `@requirement REQ-TECH-005`

### Files to modify
- `project-plans/firstversion/plan/00-overview.md` tracker
  - marker: `@plan PLAN-20260216-FIRSTVERSION-V1.P12A`
  - marker: `@requirement REQ-FUNC-009`

### Pseudocode traceability (if impl phase)
- N/A

## Verification Commands

```bash
cargo fmt --all --check
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo test --workspace --all-features
```

Coverage gate (if enabled):

```bash
cargo llvm-cov --workspace --all-features --fail-under-lines 90
```

No-SQLite proof gate:

```bash
cargo tree | grep -i sqlite && echo "unexpected sqlite dependency found" && exit 1 || true
grep -RIn "sqlite\|rusqlite\|sqlx" src/ tests/ && echo "unexpected sqlite usage found" && exit 1 || true
```

Path-contract proof gate:

```bash
grep -RIn "JEFE_SETTINGS_PATH\|JEFE_CONFIG_DIR\|JEFE_STATE_PATH\|JEFE_STATE_DIR\|XDG_CONFIG_HOME\|XDG_STATE_HOME\|APPDATA\|LOCALAPPDATA" src/ tests/
```

## Structural Verification Checklist
- [ ] P12 marker exists.
- [ ] persistence/theme tests pass.
- [ ] tracker updated.

## Semantic Verification Checklist (Mandatory)
- [ ] Invalid persistence payloads fall back safely.
- [ ] Green Screen default/fallback is guaranteed.
- [ ] No unsupported persistence backend path exists.
- [ ] Effective paths resolve by documented precedence and are user-overridable.

## Deferred Implementation Detection (Mandatory)

```bash
grep -RIn "TODO\|FIXME\|HACK\|placeholder\|for now\|will be implemented" src/ tests/
```

## Success Criteria
- [ ] Persistence/theme behavior approved for full integration hardening.

## Failure Recovery
- rollback steps: patch failing fallback/persistence path; rerun P12A.
- blocking issues: fallback regression, persistence integrity failures.

## Phase Completion Marker
Create: `project-plans/firstversion/.completed/P12A.md`

Contents:
- phase ID
- timestamp
- files changed
- tests added/updated
- verification outputs
- semantic verification summary

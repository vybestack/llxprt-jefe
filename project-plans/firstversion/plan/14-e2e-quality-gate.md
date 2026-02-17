# Phase 14: End-to-End Quality Gate

## Phase ID
`PLAN-20260216-FIRSTVERSION-V1.P14`

## Prerequisites
- Required: P13A completed.
- Verify previous markers: `.completed/P13.md`, `.completed/P13A.md`.
- Expected files: complete integrated implementation.

## Requirements Implemented (Expanded)

### REQ-TECH-007 + REQ-TECH-008
**Requirement text**: Run full quality gate suite and enforce anti-placeholder and no-regression checks.

Behavior contract:
- GIVEN integrated implementation
- WHEN quality gate executes
- THEN formatting/lint/tests/(coverage if enabled) pass and prohibited placeholder patterns are absent.

Why it matters:
- Defines objective release readiness criteria.

## Implementation Tasks

### Files to create
- `project-plans/firstversion/.completed/P14.md`
  - marker: `@plan PLAN-20260216-FIRSTVERSION-V1.P14`
  - marker: `@requirement REQ-TECH-007`

### Files to modify
- `project-plans/firstversion/plan/00-overview.md` tracker
  - marker: `@plan PLAN-20260216-FIRSTVERSION-V1.P14`
  - marker: `@requirement REQ-TECH-008`

### Pseudocode traceability (if impl phase)
- N/A

## Verification Commands

```bash
cargo fmt --all --check
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo test --workspace --all-features
```

Coverage gate if enabled:

```bash
cargo llvm-cov --workspace --all-features --fail-under-lines 90
```

Anti-placeholder gate:

Final gate must include requirement-level signoff against:
- `analysis/requirement-phase-traceability-matrix.md`
- `analysis/search-help-acceptance-contract.md`
- `analysis/f12-cross-view-consistency-matrix.md`
- `analysis/runtime-lifecycle-acceptance-matrix.md`
- `analysis/persistence-matrix.md`
- `analysis/theme-precedence-fallback-policy.md`
- `analysis/hybrid-strategy-compliance-matrix.md`

Path-contract final signoff must confirm:
- settings path precedence: `JEFE_SETTINGS_PATH` -> `JEFE_CONFIG_DIR/settings.toml` -> platform default
- state path precedence: `JEFE_STATE_PATH` -> `JEFE_STATE_DIR/state.json` -> platform default

No-SQLite proof gate:

```bash
cargo tree | grep -i sqlite && echo "unexpected sqlite dependency found" && exit 1 || true
grep -RIn "sqlite\|rusqlite\|sqlx" src/ tests/ && echo "unexpected sqlite usage found" && exit 1 || true
```

Path-contract proof gate:

```bash
grep -RIn "JEFE_SETTINGS_PATH\|JEFE_CONFIG_DIR\|JEFE_STATE_PATH\|JEFE_STATE_DIR\|XDG_CONFIG_HOME\|XDG_STATE_HOME\|APPDATA\|LOCALAPPDATA" src/ tests/
```

```bash
grep -RIn "TODO\|FIXME\|HACK\|placeholder\|for now\|will be implemented" src/ tests/
```

## Structural Verification Checklist
- [ ] All required gate commands executed.
- [ ] Outputs captured in completion artifact.
- [ ] Tracker updated.

## Semantic Verification Checklist (Mandatory)
- [ ] No unresolved requirement gap from prior verification phases.
- [ ] No hidden regression in terminal focus/split/lifecycle flows.
- [ ] Green-screen-first theme behavior still intact.

## Success Criteria
- [ ] Full quality gate pass recorded.

## Failure Recovery
- rollback steps: isolate failed gate category, remediate, rerun P14.
- blocking issues: lint/test/coverage failure or prohibited markers found.

## Phase Completion Marker
Create: `project-plans/firstversion/.completed/P14.md`

Contents:
- phase ID
- timestamp
- files changed
- tests added/updated
- verification outputs
- semantic verification summary

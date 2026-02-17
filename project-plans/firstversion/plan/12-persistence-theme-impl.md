# Phase 12: Persistence + Theme Implementation Hardening

## Phase ID
`PLAN-20260216-FIRSTVERSION-V1.P12`

## Prerequisites
- Required: P11A completed.
- Verify previous markers: `.completed/P11.md`, `.completed/P11A.md`.
- Expected files: integrated UI + core + runtime.

## Requirements Implemented (Expanded)

### REQ-FUNC-001 + REQ-FUNC-009 + REQ-TECH-005
**Requirement text**: Complete persistence and theme behavior with strict fallback guarantees and file contracts.

Behavior contract:
- GIVEN integrated app
- WHEN startup/save/theme-change paths execute
- THEN settings/state persistence, schema validation, atomic writes, and Green Screen fallback all function correctly.

Why it matters:
- Finalizes baseline resilience and visual contract.

## Implementation Tasks

### Files to create
- `src/persistence/schema.rs`
- `src/theme/fallback.rs`
  - marker: `@plan PLAN-20260216-FIRSTVERSION-V1.P12`
  - marker: `@requirement REQ-TECH-005`

### Files to modify
- `src/persistence/io.rs`
- `src/persistence/mod.rs`
- `src/theme/manager.rs`
- `src/theme/mod.rs`
- `tests/core/persistence_theme_contracts.rs`
  - marker: `@plan PLAN-20260216-FIRSTVERSION-V1.P12`
  - marker: `@requirement REQ-FUNC-009`

### Pseudocode traceability (if impl phase)
- Uses pseudocode lines:
  - component-003: 01-33

## Verification Commands

```bash
cargo fmt --all --check
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo test --workspace --all-features
```

Implementation must satisfy:
- `analysis/persistence-matrix.md`
- `analysis/theme-precedence-fallback-policy.md`

Exact path policy to implement:
- `settings.toml`: `JEFE_SETTINGS_PATH` -> `JEFE_CONFIG_DIR/settings.toml` -> platform default
- `state.json`: `JEFE_STATE_PATH` -> `JEFE_STATE_DIR/state.json` -> platform default

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
- [ ] Persistence and theme hardening files created/updated.
- [ ] File contracts reference only settings.toml/state.json.
- [ ] Path resolution order is implemented exactly as specified.
- [ ] Tests pass.

## Semantic Verification Checklist (Mandatory)
- [ ] Missing/malformed persistence falls back safely.
- [ ] Theme default/fallback always resolves to Green Screen.
- [ ] No SQLite dependency path introduced.

## Deferred Implementation Detection (Mandatory)

```bash
grep -RIn "TODO\|FIXME\|HACK\|placeholder\|for now\|will be implemented" src/ tests/
```

## Success Criteria
- [ ] Persistence/theme behavior complete and compliant.

## Failure Recovery
- rollback steps: patch schema/atomic/fallback logic and rerun.
- blocking issues: failed fallback semantics, persistence corruption risks.

## Phase Completion Marker
Create: `project-plans/firstversion/.completed/P12.md`

Contents:
- phase ID
- timestamp
- files changed
- tests added/updated
- verification outputs
- semantic verification summary

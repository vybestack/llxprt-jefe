# Plan: Jefe Firstversion v1

Plan ID: `PLAN-20260216-FIRSTVERSION-V1`
Generated: 2026-02-16
Total Phases: 14 core phases (+ paired verification phases)
Requirements: REQ-FUNC-001..010, REQ-TECH-001..010

## Critical Reminders

Before implementing any phase:
1. Preflight verification is complete (P00A)
2. Integration points are explicitly listed
3. TDD cycle is defined per slice
4. Lint/test/coverage gates are declared

## Plan Scope

## Analysis Artifacts Required by This Plan

- `analysis/requirement-phase-traceability-matrix.md`
- `analysis/hybrid-strategy-compliance-matrix.md`
- `analysis/runtime-lifecycle-acceptance-matrix.md`
- `analysis/f12-cross-view-consistency-matrix.md`
- `analysis/search-help-acceptance-contract.md`
- `analysis/crud-validation-error-matrix.md`
- `analysis/theme-precedence-fallback-policy.md`
- `analysis/persistence-matrix.md`
- `analysis/integration-contract-completeness-matrix.md`

These are mandatory reference artifacts for phase verification and final release readiness.

Build firstversion from current repository specs using hybrid strategy:
- **Reuse/adapt toy1 UI composition patterns**.
- **Rebuild non-UI core layers** (domain/events/runtime/persistence/theme) to meet architecture contract.

## Integration Contract

### Existing Callers
- `src/main.rs` app bootstrap and event loop entry
- UI input dispatch and key handling pathways
- Runtime action paths (kill/relaunch/attach/focus)

### Existing Code Replaced/Removed
- Any direct UI-to-runtime or UI-to-filesystem side effects bypassing boundaries
- Weakly typed status/event flow where present

### User Access Path
- Dashboard keyboard flows (`↑↓←→`, `r`, `a`, `t`, `F12`)
- Split mode flow (`s`, repo filter, grab/reorder, `m`, `esc`)
- CRUD forms/modals (`n`, `N`, `Enter`, `d`)
- lifecycle controls (`k`, `l`)
- help/search (`?`/`h`/`F1`, `/`)
- theme switching (`1`,`2`,`3`) with persistence

### Data/State Migration
- Parse existing persisted files if valid
- sanitize malformed references
- preserve safe defaults for invalid/missing payloads
- enforce exact persistence path resolution contract:
  - `settings.toml`: `JEFE_SETTINGS_PATH` -> `JEFE_CONFIG_DIR/settings.toml` -> platform default
  - `state.json`: `JEFE_STATE_PATH` -> `JEFE_STATE_DIR/state.json` -> platform default

### End-to-End Verification
- Keyboard E2E flow tests
- runtime kill/relaunch tests
- startup persistence fallback tests
- theme fallback tests

## Execution Tracker

| Phase | Status | Verified | Semantic Verified | Notes |
|------:|--------|----------|-------------------|-------|
| P00A  | [OK]     | [OK]       | [OK]                | Preflight PASS 2026-02-16 |
| P01   | [OK]   | [OK]     | [OK]              | Analysis PASS 2026-02-16 |
| P01A  | [OK]   | [OK]     | [OK]              | Analysis verify PASS 2026-02-16 |
| P02   | [OK]   | [OK]     | [OK]              | Pseudocode PASS 2026-02-16 |
| P02A  | [OK]   | [OK]     | [OK]              | Pseudocode verify PASS 2026-02-16 |
| P03   | [OK]   | [OK]     | [OK]              | Core contracts stub PASS 2026-02-16 |
| P03A  | [OK]   | [OK]     | [OK]              | Verify PASS 2026-02-16 |
| P04   | [OK]   | [OK]     | [OK]              | Core contracts TDD PASS 2026-02-16 (14 RED) |
| P04A  | [OK]   | [OK]     | [OK]              | Verify PASS 2026-02-16 |
| P05   | [OK]   | [OK]     | [OK]              | Core contracts impl PASS 2026-02-16 (36 GREEN) |
| P05A  | [OK]   | [OK]     | [OK]              | Verify PASS 2026-02-16 |
| P06   | [OK]   | [OK]     | [OK]              | Runtime stub PASS 2026-02-16 |
| P06A  | [OK]   | [OK]     | [OK]              | Verify PASS 2026-02-16 |
| P07   | [OK]   | [OK]     | [OK]              | Runtime TDD PASS 2026-02-16 (28 tests) |
| P07A  | [OK]   | [OK]     | [OK]              | Verify PASS 2026-02-16 |
| P08   | [OK]   | [OK]     | [OK]              | Runtime impl PASS 2026-02-17 (83 tests) |
| P08A  | [OK]   | [OK]     | [OK]              | Verify PASS 2026-02-17 |
| P09   | [OK]   | [OK]     | [OK]              | UI adaptation stub PASS 2026-02-17 (83 tests) |
| P09A  | [OK]   | [OK]     | [OK]              | Verify PASS 2026-02-17 |
| P10   | [OK]   | [OK]     | [OK]              | UI adaptation TDD PASS 2026-02-17 (50 tests, 1 RED for P11) |
| P10A  | [OK]   | [OK]     | [OK]              | Verify PASS 2026-02-17 |
| P11   | [OK]   | [OK]     | [OK]              | UI adaptation impl PASS 2026-02-17 (110 tests GREEN) |
| P11A  | [OK]   | [OK]     | [OK]              | Verify PASS 2026-02-17 |
| P12   | [OK]   | [OK]     | [OK]              | Persistence+theme impl PASS 2026-02-17 (133 tests) |
| P12A  | [OK]   | [OK]     | [OK]              | Verify PASS 2026-02-17 |
| P13   | [OK]   | [OK]     | [OK]              | Integration hardening PASS 2026-02-17 (155 tests) |
| P13A  | [OK]   | [OK]     | [OK]              | Verify PASS 2026-02-17 |
| P14   | [OK]   | [OK]     | [OK]              | E2E quality gate PASS 2026-02-17 (155 tests) |
| P14A  | [OK]   | [OK]     | [OK]              | Final verification PASS 2026-02-17 - PLAN COMPLETE |

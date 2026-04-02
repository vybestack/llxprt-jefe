# Phase 12A: UI Components + Persistence Stub Verification

## Phase ID
`PLAN-20260329-ISSUES-MODE.P12A`

## Prerequisites
- Required: Phase P12 completed.
- Verify previous artifacts: `.completed/P12.md` exists.
- Expected files from previous phase: new UI component files, modified dashboard/form/keybind files.

## Requirements Implemented (Expanded)

### Verification of UI + Persistence Stub Correctness for REQ-ISS-001,006,008,009,011,012
**Requirement text**: Confirm stubs compile, render without crashes, persistence integration works, and traceability markers present.

Behavior contract:
- GIVEN UI stubs added in P12
- WHEN verification is executed
- THEN all new files exist and compile, dashboard renders in both modes, repository form includes new field, persistence backward compat maintained.

## Implementation Tasks

### Files to create
- `project-plans/issue15/.completed/P12A.md`
  - marker: `@plan PLAN-20260329-ISSUES-MODE.P12A`

### Files to modify
- `project-plans/issue15/plan/00-overview.md` -- tracker update

### Pseudocode traceability (if impl phase)
- N/A (verification phase)

## Verification Commands

```bash
cargo fmt --all --check
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo test --workspace --all-features
```

### UI Component File Existence Verification
```bash
test -f src/ui/screens/issues.rs && echo "OK: issues.rs" || echo "MISSING: issues.rs"
test -f src/ui/components/issue_list.rs && echo "OK: issue_list.rs" || echo "MISSING: issue_list.rs"
test -f src/ui/components/issue_detail.rs && echo "OK: issue_detail.rs" || echo "MISSING: issue_detail.rs"
test -f src/ui/components/filter_controls.rs && echo "OK: filter_controls.rs" || echo "MISSING: filter_controls.rs"
test -f src/ui/components/agent_chooser.rs && echo "OK: agent_chooser.rs" || echo "MISSING: agent_chooser.rs"
```

### Module Declaration Verification
```bash
grep -n "pub mod issue_list\|pub mod issue_detail\|pub mod filter_controls\|pub mod agent_chooser" src/ui/components/mod.rs
grep -n "pub mod issues" src/ui/screens/mod.rs
```

### Persistence Verification
```bash
grep -n "issue_base_prompt" src/persistence/mod.rs src/domain/mod.rs
```

### Dashboard Conditional Rendering Verification
```bash
# Verify dashboard switches on ScreenMode::DashboardIssues
grep -n "DashboardIssues" src/ui/screens/dashboard.rs
```

### Traceability Marker Verification
```bash
for file in src/ui/screens/issues.rs src/ui/components/issue_list.rs src/ui/components/issue_detail.rs src/ui/components/filter_controls.rs src/ui/components/agent_chooser.rs; do
  echo "--- $file ---"
  grep -c "@plan\|@requirement" "$file" || echo "WARN: no markers"
done
```

## Structural Verification Checklist
- [ ] All 5 new UI files exist.
- [ ] Module declarations added.
- [ ] Dashboard conditional rendering compiles with `ScreenMode::DashboardIssues` branch.
- [ ] `issue_base_prompt` in persistence path.
- [ ] Traceability markers present in all new files.
- [ ] Tracker updated.

## Semantic Verification Checklist (Mandatory)
- [ ] Dashboard renders correctly in agents mode (`ScreenMode::Dashboard`) — existing behavior preserved.
- [ ] Issues mode layout is structurally present (two-column composition in `issues.rs`: repos sidebar + issues workspace) — not just an empty file.
- [ ] Repository form includes `issue_base_prompt` field — verified by `grep`.
- [ ] Keybind bar has issues mode bindings — different from agents mode.
- [ ] Feature behavior is reachable from real app flow: state with `ScreenMode::DashboardIssues` triggers issues layout rendering path.

## Deferred Implementation Detection (Mandatory)

```bash
grep -RIn "TODO\|FIXME\|HACK\|placeholder\|for now\|will be implemented" src/ui/ src/persistence/
```

## Success Criteria
- [ ] Stub verification pass.
- [ ] All existing tests pass.

## Failure Recovery
- rollback steps: Fix compilation or rendering issues.
- blocking issues: broken dashboard, missing module declarations.

## Phase Completion Marker
Create: `project-plans/issue15/.completed/P12A.md`

Contents:
- phase ID: `PLAN-20260329-ISSUES-MODE.P12A`
- timestamp
- file existence verification output
- module declaration verification output
- dashboard conditional rendering verification output
- traceability marker verification output
- verification outputs
- semantic verification summary

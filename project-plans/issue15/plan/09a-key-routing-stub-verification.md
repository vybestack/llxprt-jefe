# Phase 09A: Key Routing + Input Dispatch Stub Verification

## Phase ID
`PLAN-20260329-ISSUES-MODE.P09A`

## Prerequisites
- Required: Phase P09 completed.
- Verify previous artifacts: `.completed/P09.md` exists.
- Expected files from previous phase: key routing stubs in `src/app_input.rs` and input mode extensions in `src/input.rs`.

## Requirements Implemented (Expanded)

### Verification of Key Routing Stub Correctness for REQ-ISS-002
**Requirement text**: Confirm stubs compile, integrate into existing dispatch chain, and do not break existing key handling.

Behavior contract:
- GIVEN key routing stubs added in P09
- WHEN verification is executed
- THEN dispatch function exists, input mode detection works for all 5 issues states, existing tests pass, suppression stubs present, traceability markers present.

## Implementation Tasks

### Files to create
- `project-plans/issue15/.completed/P09A.md`
  - marker: `@plan PLAN-20260329-ISSUES-MODE.P09A`

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

### Key Routing Presence Verification
```bash
# Verify issues mode dispatch function exists
grep -n "fn handle_issues_mode_key\|fn handle_issues_key" src/app_input.rs

# Verify InputMode issues variants
grep -n "IssuesNormal\|IssuesInline\|IssuesSearch\|IssuesFilter\|IssuesChooser" src/input.rs

# Verify input_mode_for_state handles DashboardIssues
grep -n "DashboardIssues" src/input.rs

# Verify GhClient in context
grep -n "GhClient\|gh_client\|github" src/main.rs
```

### Existing Behavior Preservation Verification
```bash
# Verify existing key bindings are NOT modified
echo "--- Agents mode 'a' key (should still set PaneFocus::Agents) ---"
grep -A3 "KeyCode::Char.*'a'" src/app_input.rs | head -6

echo "--- Split mode 's' key (should still enter split in Dashboard mode) ---"
grep -A3 "KeyCode::Char.*'s'" src/app_input.rs | head -6

echo "--- Ctrl-d (should still work in non-issues mode) ---"
grep -A3 "Ctrl.*'d'" src/app_input.rs | head -6
```

### Traceability Marker Verification
```bash
grep -c "@plan PLAN-20260329-ISSUES-MODE.P09\|@requirement REQ-ISS-002\|@pseudocode component-003" src/app_input.rs src/input.rs || echo "WARN: missing markers"
```

## Structural Verification Checklist
- [ ] Key routing dispatch function exists and compiles in `src/app_input.rs`.
- [ ] InputMode extensions exist (all 5 issues variants).
- [ ] `input_mode_for_state()` handles `DashboardIssues`.
- [ ] GhClient accessible from app context.
- [ ] Suppression stubs present for `s`, `Ctrl-d`, `Ctrl-k`, `l`.
- [ ] All existing tests pass.
- [ ] Traceability markers present.
- [ ] Tracker updated.

## Semantic Verification Checklist (Mandatory)
- [ ] Existing normal-mode behavior unchanged — verify by running existing key dispatch tests.
- [ ] Key routing for `a` key still sets `PaneFocus::Agents` when in `ScreenMode::Dashboard` (not intercepted by issues stub).
- [ ] Key routing for `s`/`S` still enters split mode when in `ScreenMode::Dashboard` (not intercepted).
- [ ] Key routing for `Ctrl-d`, `Ctrl-k`, `l` still works in non-issues mode (not intercepted).
- [ ] Issues mode dispatch is reachable when `screen_mode == DashboardIssues`.
- [ ] Input mode priority: inline > chooser > search > filter > normal (check code order in `input_mode_for_state()`).
- [ ] Feature behavior is reachable from real app flow: key press with issues mode state reaches `handle_issues_mode_key()`.

## Deferred Implementation Detection (Mandatory)

```bash
grep -RIn "TODO\|FIXME\|HACK\|placeholder\|for now\|will be implemented" src/app_input.rs src/input.rs
```

Note: Stub bodies with early returns in `handle_issues_mode_key()` are allowed.

## Success Criteria
- [ ] Stub verification pass.
- [ ] All existing key handling unaffected.

## Failure Recovery
- rollback steps: Fix compilation issues or dispatch wiring.
- blocking issues: broken existing key handling, input mode detection errors.

## Phase Completion Marker
Create: `project-plans/issue15/.completed/P09A.md`

Contents:
- phase ID: `PLAN-20260329-ISSUES-MODE.P09A`
- timestamp
- dispatch function location
- input mode detection verification output
- existing behavior preservation verification output
- traceability marker verification output
- verification outputs
- semantic verification summary

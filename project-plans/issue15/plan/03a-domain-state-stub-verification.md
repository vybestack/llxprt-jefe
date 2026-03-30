# Phase 03A: Domain + State Contracts Stub Verification

## Phase ID
`PLAN-20260329-ISSUES-MODE.P03A`

## Prerequisites
- Required: Phase P03 completed.
- Verify previous artifacts: `.completed/P03.md` exists.
- Expected files from previous phase: updated `src/domain/mod.rs`, `src/state/types.rs`, `src/state/mod.rs`, `src/input.rs`, `src/lib.rs`, new `src/github/mod.rs`.

## Requirements Implemented (Expanded)

### Verification of Stub Correctness for REQ-ISS-001, 006, 009, 012, 013
**Requirement text**: Confirm stubs compile, integrate with existing code, do not break existing behavior, and maintain backward compatibility.

Behavior contract:
- GIVEN compile-safe stubs added in P03
- WHEN verification is executed
- THEN all existing tests pass, new types are reachable from code, serde backward compat is confirmed, GitHub client module is isolated, and all traceability markers are present.

Why it matters:
- Stub correctness is the foundation for TDD. Broken stubs would cascade into all subsequent phases.

## Implementation Tasks

### Files to create
- `project-plans/issue15/.completed/P03A.md`
  - marker: `@plan PLAN-20260329-ISSUES-MODE.P03A`

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

### Backward Compatibility Verification
```bash
# Verify Repository has issue_base_prompt with serde default
grep -n "issue_base_prompt" src/domain/mod.rs
grep -A1 "issue_base_prompt" src/domain/mod.rs | grep "serde(default)" || echo "WARN: missing serde(default)"

# Verify IssuesState in AppState
grep -n "issues_state" src/state/types.rs

# Verify DashboardIssues variant
grep -n "DashboardIssues" src/state/types.rs

# Verify github module
grep -n "pub mod github" src/lib.rs

# Verify InputMode extensions
grep -n "Issues" src/input.rs
```

### Module Isolation Verification
```bash
# GitHub module must not import UI or state
grep -n "use crate::ui\|use crate::state\|use crate::app_input" src/github/mod.rs && echo "FAIL: github module has forbidden imports" || echo "OK: github module isolation verified"
```

### ScreenMode Match Arm Verification
```bash
# All match arms on ScreenMode should handle DashboardIssues
grep -n "ScreenMode::" src/state/types.rs src/state/mod.rs src/app_input/mod.rs src/input.rs src/ui/ -r | grep -v "DashboardIssues" | grep "Dashboard\b" || echo "OK: all match arms likely updated"
```

### Enum Preservation Verification
```bash
# Verify PaneFocus is NOT modified (still exactly 3 variants)
echo "--- PaneFocus (should be unchanged: Repositories, Agents, Terminal) ---"
grep -A6 "pub enum PaneFocus" src/state/types.rs

# Verify IssueFocus is a SEPARATE new enum
echo "--- IssueFocus (should be new: RepoList, IssueList, IssueDetail) ---"
grep -A5 "pub enum IssueFocus" src/state/types.rs

# Verify existing ScreenMode variants preserved
echo "--- ScreenMode (should have Dashboard, Split, DashboardIssues) ---"
grep -A6 "pub enum ScreenMode" src/state/types.rs

# Verify existing InputMode variants preserved
echo "--- InputMode (should have 6 original + 5 new) ---"
grep -A15 "pub enum InputMode" src/input.rs
```

### Traceability Marker Verification
```bash
# Verify @plan, @requirement, @pseudocode markers in changed files
for file in src/domain/mod.rs src/state/types.rs src/state/mod.rs src/input.rs src/github/mod.rs src/lib.rs; do
  echo "--- $file ---"
  grep -c "@plan\|@requirement\|@pseudocode" "$file" || echo "WARN: no markers found"
done
```

## Structural Verification Checklist
- [ ] All files from P03 are present and compile.
- [ ] `cargo test` passes with zero failures.
- [ ] `PaneFocus` is UNCHANGED (exactly: `Repositories`, `Agents`, `Terminal`).
- [ ] `IssueFocus` is a new separate enum (not added to `PaneFocus`).
- [ ] `ScreenMode` has `Dashboard`, `Split`, `DashboardIssues` (existing variants preserved).
- [ ] `InputMode` has all 6 original variants plus 5 new issues variants.
- [ ] No skipped phase dependencies.
- [ ] `@plan`, `@requirement`, `@pseudocode` markers present in ALL changed files.
- [ ] Tracker updated.

## Semantic Verification Checklist (Mandatory)
- [ ] Backward compatibility: existing Repository JSON without `issue_base_prompt` deserializes cleanly to empty string.
- [ ] No existing key routing or event handling changed in behavior — verify by running existing tests.
- [ ] Key routing for `a` key still sets `PaneFocus::Agents` when in `ScreenMode::Dashboard` (existing behavior preserved — verified by test or grep of src/app_input/normal.rs L174).
- [ ] Key routing for `s`/`S` still enters split mode when in `ScreenMode::Dashboard` (existing behavior preserved — verified by test or grep of src/app_input/normal.rs L148-150).
- [ ] Key routing for `Ctrl-d`, `Ctrl-k`, `l` still works in non-issues mode (existing behavior preserved — verified by test or grep of src/app_input/normal.rs L129-145).
- [ ] GitHub client module is isolated (no `crate::ui` or `crate::state` imports).
- [ ] Feature behavior is reachable from real app flow: `ScreenMode::DashboardIssues` is a valid state, `input_mode_for_state()` returns issues-mode variant.
- [ ] All existing `ScreenMode` match arms handle `DashboardIssues` (no unreachable patterns).
- [ ] `IssueFocus` is used for issues mode focus tracking (not `PaneFocus`).
- [ ] `IssuesState` struct contains all fields needed for issues mode: `issue_focus`, `issues`, `selected_issue_index`, `issue_detail`, `comments`, `inline_state`, `agent_chooser`, `filter_state`, `search_query`, `search_input_focused`, `filter_controls_open`, `list_loading`, `detail_loading`, `has_more_issues`, `prior_agent_focus`.
- [ ] Stale-scope suppression infrastructure is present: `IssuesState` has a `scope_repo_id` field (or equivalent) for matching loaded data against current repository.
- [ ] Draft discard infrastructure is present: `ExitIssuesMode` event stub exists and is wired into `apply()` match.
- [ ] `@plan`, `@requirement`, `@pseudocode` markers are present in doc comments for EACH new type/variant/field — not just at file level.

## Deferred Implementation Detection (Mandatory)

```bash
grep -RIn "TODO\|FIXME\|HACK\|placeholder\|for now\|will be implemented" src/
```

Note: `todo!()` in `src/github/mod.rs` stub bodies is expected and allowed. `todo!()` in state reducer `apply()` match arms for new events is allowed until P05.

## Integration Contract Acceptance Gates
- [ ] **Backward compat**: Deserialization test passes.
- [ ] **Old behavior preserved**: Run existing test suite — zero failures.
- [ ] **Module isolation**: GitHub module has no forbidden imports.
- [ ] **Enum preservation**: `PaneFocus` unmodified; `ScreenMode::Dashboard` and `ScreenMode::Split` preserved.

## Success Criteria
- [ ] Stub verification pass.
- [ ] All integration contract acceptance gates pass.
- [ ] Traceability markers present.

## Failure Recovery
- rollback steps: Fix compilation or serde issues in stub code. If `ScreenMode` match arms are broken, check all `match state.screen_mode` in codebase.
- blocking issues: broken backward compat, compile errors, broken existing tests, modified PaneFocus.

## Phase Completion Marker
Create: `project-plans/issue15/.completed/P03A.md`

Contents:
- phase ID: `PLAN-20260329-ISSUES-MODE.P03A`
- timestamp
- files checked
- enum preservation verification output
- backward compatibility verification output
- module isolation verification output
- traceability marker verification output
- verification command outputs
- semantic verification summary

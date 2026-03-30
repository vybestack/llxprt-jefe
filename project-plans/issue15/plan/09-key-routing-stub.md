# Phase 09: Key Routing and Input Dispatch Stub

## Phase ID
`PLAN-20260329-ISSUES-MODE.P09`

## Prerequisites
- Required: Phase P08A completed
- Verify previous phase markers/artifacts exist: `.completed/P08.md`, `.completed/P08A.md`
- Expected files from previous phase: implemented domain + state + GitHub client (all tests GREEN, zero stubs in `src/github/mod.rs`)

## Requirements Implemented (Expanded)

### REQ-ISS-002: Key Routing and Suppression — Skeleton
**Requirement text**: While in Issues Mode: suppress dashboard `a` focus-agents, `s/S` split-mode, split-mode `Esc`, destructive lifecycle keys (`Ctrl-d`, `Ctrl-k`, `l`). Route `/` to issue-list search; `?`/`h`/`F1` to help with Issues Mode bindings. Lowercase `s` is explicit no-op in Issues Mode.

Behavior contract:
- GIVEN existing key dispatch in `src/app_input.rs` via `handle_normal_key_event()`
- WHEN issues mode dispatch branch is added
- THEN keys route through issues-mode priority chain when `screen_mode == DashboardIssues`; all existing normal-mode routing is unaffected

- GIVEN `AppState` with `screen_mode == DashboardIssues` and `inline_state == Composer`
- WHEN `input_mode_for_state()` is called
- THEN returns `InputMode::IssuesInline`

- GIVEN `AppState` with `screen_mode == DashboardIssues` and no inline/chooser/search/filter active
- WHEN `input_mode_for_state()` is called
- THEN returns `InputMode::IssuesNormal`

Why it matters:
- Key routing is the highest-complexity integration point; stub ensures compilability before TDD and prevents breakage of existing dispatch

## Implementation Tasks

### Files to modify
- `src/input.rs` — extend `input_mode_for_state()` with full issues mode detection:
  - If `screen_mode == DashboardIssues` AND `inline_state != None` → `IssuesInline`
  - If `screen_mode == DashboardIssues` AND `agent_chooser.is_some()` → `IssuesChooser`
  - If `screen_mode == DashboardIssues` AND `search_input_focused` → `IssuesSearch`
  - If `screen_mode == DashboardIssues` AND `filter_controls_open` → `IssuesFilter`
  - If `screen_mode == DashboardIssues` → `IssuesNormal`
  - marker: `@plan PLAN-20260329-ISSUES-MODE.P09`
  - marker: `@requirement REQ-ISS-002`
  - marker: `@pseudocode component-003 lines 01-17`

- `src/app_input.rs` — add key routing skeleton:
  - Add `handle_issues_mode_key()` function stub (takes key event, state, context; returns early)
  - Add issues mode branch in main key dispatch: when `InputMode::IssuesNormal | IssuesInline | IssuesSearch | IssuesFilter | IssuesChooser` → call `handle_issues_mode_key()`
  - Add suppression rule stubs: consume `s`, `Ctrl-d`, `Ctrl-k`, `l` as no-op when in issues mode
  - Import `GhClient` type for use in context
  - marker: `@plan PLAN-20260329-ISSUES-MODE.P09`
  - marker: `@requirement REQ-ISS-002`
  - marker: `@pseudocode component-003 lines 01-38`

- `src/main.rs` — ensure `GhClient` instance is created and accessible from `SharedContext`:
  - marker: `@plan PLAN-20260329-ISSUES-MODE.P09`

### Pseudocode traceability (if impl phase)
- Uses pseudocode component-003 lines 01-38 (routing skeleton, priority detection, suppression rules)

## Verification Commands

```bash
cargo fmt --all --check
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo test --workspace --all-features
```

## Structural Verification Checklist
- [ ] Issues mode key dispatch function exists and compiles in `src/app_input.rs`
- [ ] `input_mode_for_state()` handles all 5 issues mode states (normal, inline, search, filter, chooser)
- [ ] Suppression stubs exist for `s`, `Ctrl-d`, `Ctrl-k`, `l`
- [ ] `GhClient` accessible from context in `src/main.rs`
- [ ] All existing tests pass
- [ ] Phase/requirement/pseudocode markers present in ALL new code

## Semantic Verification Checklist (Mandatory)
- [ ] Existing normal-mode key routing is unaffected (verified by existing key dispatch tests)
- [ ] `a` key still sets `PaneFocus::Agents` when in `ScreenMode::Dashboard` (existing behavior preserved)
- [ ] `s`/`S` still enters split mode when in `ScreenMode::Dashboard` (existing behavior preserved)
- [ ] `Ctrl-d`, `Ctrl-k`, `l` still work in non-issues mode (existing behavior preserved)
- [ ] Issues mode dispatch is reachable from main key handler when `screen_mode == DashboardIssues`
- [ ] `input_mode_for_state()` correctly prioritizes inline > chooser > search > filter > normal for issues mode
- [ ] Feature behavior is reachable from real app flow: `i` key press can reach issues mode dispatch path (even if behavior is stubbed)
- [ ] Suppressed keys (`s`, `Ctrl-d`, `Ctrl-k`, `l`) are identified and stubbed as no-op in issues mode
- [ ] No architecture fork: issues mode dispatch is integrated into existing dispatch chain, not a parallel handler

## Deferred Implementation Detection (Mandatory)

```bash
grep -RIn "TODO\|FIXME\|HACK\|placeholder\|for now\|will be implemented" src/app_input.rs src/input.rs
```

Note: stub bodies with early returns in `handle_issues_mode_key()` are allowed in this phase.

## Success Criteria
- [ ] Compile-safe key routing stubs exist
- [ ] Verification commands pass
- [ ] Semantic checks pass

## Failure Recovery
- rollback steps: `git restore src/app_input.rs src/input.rs src/main.rs`
- blocking issues: compilation errors from existing key handling changes, `GhClient` wiring issues

## Phase Completion Marker
Create: `project-plans/issue15/.completed/P09.md`

Contents:
- phase ID: `PLAN-20260329-ISSUES-MODE.P09`
- timestamp
- files changed
- dispatch function location
- `input_mode_for_state()` issues mode detection confirmed
- verification command outputs
- semantic verification summary

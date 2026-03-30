# Phase 12: UI Components + Persistence Stub

## Phase ID
`PLAN-20260329-ISSUES-MODE.P12`

## Prerequisites
- Required: Phase P11A completed
- Verify previous phase markers/artifacts exist: `.completed/P11.md`, `.completed/P11A.md`
- Expected files from previous phase: implemented domain + state + GitHub client + key routing (all tests GREEN)

## Requirements Implemented (Expanded)

### REQ-ISS-001: Mode-Conditional Dashboard Layout
**Requirement text**: `i` from non-issues context enters `dashboard_issues` with `focus=issue_list`. Issues Mode renders three-pane layout: repo list, issue list, issue detail.

Behavior contract:
- GIVEN `screen_mode == ScreenMode::DashboardIssues`
- WHEN dashboard renders
- THEN issues mode layout (`src/ui/screens/issues.rs`) is rendered instead of agents mode layout

- GIVEN `screen_mode == ScreenMode::Dashboard`
- WHEN dashboard renders
- THEN agents mode layout renders unchanged (no regression)

Why it matters:
- The dashboard conditional is the rendering entry point for all issues mode UI; without it no issues UI is ever displayed.

### REQ-ISS-006: Issue List Display — UI Skeleton
**Requirement text**: Each row: number, title, state, author, updated timestamp, assignee summary, label summary, comment count.

Behavior contract:
- GIVEN implemented state and key routing
- WHEN UI component stubs are added for issue list
- THEN `src/ui/components/issue_list.rs` exists, compiles, and is wired into dashboard conditional rendering

Why it matters:
- UI stubs establish the rendering skeleton so TDD tests can target rendering outcomes.

### REQ-ISS-008: Filtering and Search — UI Skeleton
**Requirement text**: `f` opens filter controls (issue-list focus only). Structured filters AND-composed with text query.

Behavior contract:
- GIVEN filter controls state
- WHEN filter controls component stub exists
- THEN `src/ui/components/filter_controls.rs` compiles and is conditionally rendered when `filter_controls_open == true`

Why it matters:
- Filter controls stub must exist before TDD tests can assert render conditions against it.

### REQ-ISS-009: Issue Detail and Comments — UI Skeleton
**Requirement text**: Detail displays all fields, comments timeline, inline controls.

Behavior contract:
- GIVEN implemented state with IssuesState
- WHEN UI component stub for issue detail is added
- THEN `src/ui/components/issue_detail.rs` exists, compiles, and renders stub content when detail is loaded

Why it matters:
- Detail pane stub is required for the three-pane layout to compile and for subsequent TDD phases to target.

### REQ-ISS-011: Send-to-Agent — UI Skeleton
**Requirement text**: Agent chooser overlay with list, selection, confirm/cancel.

Behavior contract:
- GIVEN agent chooser state (`agent_chooser.is_some()`)
- WHEN agent chooser component stub exists
- THEN `src/ui/components/agent_chooser.rs` compiles and is conditionally rendered

Why it matters:
- Agent chooser stub must be wired before integration tests can exercise send-to-agent flows.

### REQ-ISS-012: Repository Config `issue_base_prompt` — UI Skeleton
**Requirement text**: Multiline field in existing repository config screen with Save and Reset.

Behavior contract:
- GIVEN existing repository edit form in `src/ui/screens/new_repository.rs`
- WHEN `issue_base_prompt` field is added
- THEN field appears in form, is editable, and value is included in form submission

Why it matters:
- Persistence backward compatibility must be verified at stub time; a missing serde default causes deserialization failures on existing state files.

## Implementation Tasks

### Files to create
- `src/ui/screens/issues.rs` — Issues mode screen layout (three-pane: repo list, issue list, issue detail)
  - marker: `@plan PLAN-20260329-ISSUES-MODE.P12`
  - marker: `@requirement REQ-ISS-001,006`
  - Traceability: module-level doc comment MUST include `@plan`, `@requirement` markers

- `src/ui/components/issue_list.rs` — Issue list pane component (stub rendering)
  - marker: `@plan PLAN-20260329-ISSUES-MODE.P12`
  - marker: `@requirement REQ-ISS-006`

- `src/ui/components/issue_detail.rs` — Issue detail pane component (stub rendering)
  - marker: `@plan PLAN-20260329-ISSUES-MODE.P12`
  - marker: `@requirement REQ-ISS-009,010`

- `src/ui/components/filter_controls.rs` — Filter controls component (stub rendering)
  - marker: `@plan PLAN-20260329-ISSUES-MODE.P12`
  - marker: `@requirement REQ-ISS-008`

- `src/ui/components/agent_chooser.rs` — Agent chooser overlay (stub rendering)
  - marker: `@plan PLAN-20260329-ISSUES-MODE.P12`
  - marker: `@requirement REQ-ISS-011`

### Files to modify
- `src/ui/mod.rs`
  - Export new components (if needed for screen module access)
  - marker: `@plan PLAN-20260329-ISSUES-MODE.P12`

- `src/ui/components/mod.rs`
  - Add `pub mod issue_list;`
  - Add `pub mod issue_detail;`
  - Add `pub mod filter_controls;`
  - Add `pub mod agent_chooser;`
  - marker: `@plan PLAN-20260329-ISSUES-MODE.P12`

- `src/ui/screens/mod.rs`
  - Add `pub mod issues;`
  - marker: `@plan PLAN-20260329-ISSUES-MODE.P12`

- `src/ui/screens/new_repository.rs`
  - Add `issue_base_prompt` multiline field to repository create/edit form
  - Add `issue_base_prompt` to `RepositoryFormFields` and cursor handling
  - marker: `@plan PLAN-20260329-ISSUES-MODE.P12`
  - marker: `@requirement REQ-ISS-012`

- `src/ui/screens/dashboard.rs`
  - Add conditional rendering: when `screen_mode == ScreenMode::DashboardIssues`, render issues layout instead of agents layout
  - marker: `@plan PLAN-20260329-ISSUES-MODE.P12`
  - marker: `@requirement REQ-ISS-001`

- `src/ui/components/keybind_bar.rs`
  - Add issues mode keybinding display (when `screen_mode == ScreenMode::DashboardIssues`)
  - marker: `@plan PLAN-20260329-ISSUES-MODE.P12`

- `src/persistence/mod.rs`
  - Verify `issue_base_prompt` serialization in repository save path (via domain type change from P03)
  - marker: `@plan PLAN-20260329-ISSUES-MODE.P12`
  - marker: `@requirement REQ-ISS-012`

### Pseudocode traceability
- N/A (stub phase — UI layout, no algorithmic content)

## Verification Commands

```bash
cargo fmt --all --check
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo test --workspace --all-features
```

## Structural Verification Checklist
- [ ] All 5 new UI component files exist and compile
- [ ] Issues screen layout (`issues.rs`) exists
- [ ] Module declarations added in `components/mod.rs` and `screens/mod.rs`
- [ ] Dashboard conditionally renders issues layout when `ScreenMode::DashboardIssues`
- [ ] Repository form includes `issue_base_prompt` field
- [ ] Keybind bar has issues mode variant
- [ ] `issue_base_prompt` serialized in persistence path
- [ ] All existing tests pass
- [ ] `@plan`, `@requirement` markers present in all new files

## Semantic Verification Checklist (Mandatory)
- [ ] Dashboard rendering is not broken for normal/agents mode (`ScreenMode::Dashboard`) — verify by inspection or existing tests
- [ ] Issues mode layout has three panes (repo list, issue list, issue detail) — visible in component composition
- [ ] Keybind bar shows issues mode bindings when in issues mode (different from agents mode bindings)
- [ ] Persistence backward compatibility maintained (serde default on `issue_base_prompt`)
- [ ] Feature behavior is reachable from real app flow: entering issues mode via `i` key produces `ScreenMode::DashboardIssues` state that dashboard rendering can now display
- [ ] No placeholder/deferred patterns except stub rendering content (allowed in stub phase)

## Deferred Implementation Detection (Mandatory)

```bash
grep -RIn "TODO\|FIXME\|HACK\|placeholder\|for now\|will be implemented" src/ui/ src/persistence/
```

Note: Stub rendering (empty/placeholder UI content like "Issue list loading...") is allowed in this phase.

## Success Criteria
- [ ] Compile-safe UI + persistence stubs exist
- [ ] Verification commands pass
- [ ] Dashboard renders correctly in agents mode (backward compat preserved)
- [ ] Persistence works with existing state.json files

## Failure Recovery
- rollback steps: `git restore src/ui/ src/persistence/`
- blocking issues: compilation errors, broken dashboard rendering, iocraft component errors

## Phase Completion Marker
Create: `project-plans/issue15/.completed/P12.md`

Contents:
- phase ID: `PLAN-20260329-ISSUES-MODE.P12`
- timestamp
- files changed: list of new + modified files
- verification command outputs
- semantic verification summary

# Phase 13: UI Components + Persistence TDD

## Phase ID
`PLAN-20260329-ISSUES-MODE.P13`

## Prerequisites
- Required: Phase P12A completed
- Verify previous phase markers/artifacts exist: `.completed/P12.md`, `.completed/P12A.md`
- Expected files from previous phase: UI component stubs, persistence integration, modified dashboard/form

## Requirements Implemented (Expanded)

### REQ-ISS-012: Persistence Round-Trip — TDD
**Requirement text**: `issue_base_prompt` persisted via existing repository config persistence path. Empty value valid.

Behavior contract:
- GIVEN `Repository` with `issue_base_prompt = "Prioritize diagnosis"`
- WHEN state is serialized to JSON and deserialized
- THEN `issue_base_prompt` value is preserved

- GIVEN legacy `state.json` without `issue_base_prompt` field
- WHEN deserialized
- THEN `issue_base_prompt` defaults to empty string

Why it matters:
- Backward compatibility with existing state files is a hard requirement; a deserialization failure on missing field would break every existing user's application on upgrade.

### REQ-ISS-006: Issue List Rendering — TDD
**Requirement text**: Each row displays all fields. Selection highlight. Loading/empty states.

Behavior contract:
- GIVEN `IssuesState` with 5 issues, second selected
- WHEN issue list rendering logic is tested
- THEN 5 rows present; row at index 1 is selected

- GIVEN `IssuesState` with `list_loading = true`
- WHEN issue list rendering logic is tested
- THEN loading state is indicated

- GIVEN `IssuesState` with empty issues list
- WHEN issue list rendering logic is tested
- THEN "No issues match current filters" message state is present

Why it matters:
- Issue list rendering tests drive the RED step that forces correct state-driven rendering implementation in P14.

### REQ-ISS-009: Issue Detail Rendering — TDD
**Requirement text**: All detail fields displayed, comments timeline, inline controls visibility.

Behavior contract:
- GIVEN `IssueDetail` with 3 comments and inline composer active
- WHEN detail rendering logic is tested
- THEN all detail fields present, 3 comments in timeline, composer state active

Why it matters:
- Detail rendering tests must verify state conditions rather than iocraft internals to remain stable through rendering refactors.

### REQ-ISS-008: Filter Controls Rendering — TDD
**Requirement text**: Filter form with all fields, Apply/Clear/Cancel.

Behavior contract:
- GIVEN draft filter with state=open, labels=["bug"]
- WHEN filter controls rendering logic is tested
- THEN filter state and label values are present in draft

Why it matters:
- Filter controls tests establish the expected data-binding contract before implementation fills in the form fields.

### REQ-ISS-014: Empty State Rendering — TDD
**Requirement text**: No issues, no comments, no agents for send all produce explicit messages.

Behavior contract:
- GIVEN empty issue list
- WHEN issue list renders
- THEN empty state message is present

- GIVEN no agents available
- WHEN send-to-agent is attempted
- THEN "No agents available" state is indicated

Why it matters:
- Empty states are a distinct code path that must be explicitly tested; they are easy to accidentally skip when data is normally present during development.

### REQ-ISS-002: Keybind Bar Issues Mode — TDD
**Requirement text**: `?`/`h`/`F1` routes to help with Issues Mode bindings; keybind bar reflects current mode.

Behavior contract:
- GIVEN `screen_mode == ScreenMode::DashboardIssues`
- WHEN keybind bar renders
- THEN issues mode bindings are shown (different from agents mode bindings)

Why it matters:
- Keybind bar must show context-appropriate bindings; a test drives correct conditional display logic in the bar component.

## Implementation Tasks

### Files to create or modify
- Tests in `src/persistence/mod.rs` `#[cfg(test)]` module:
  - `test_issue_base_prompt_state_round_trip` — REQ-ISS-012
    - marker: `@plan PLAN-20260329-ISSUES-MODE.P13`, `@requirement REQ-ISS-012`
  - `test_issue_base_prompt_state_backward_compat` — REQ-ISS-012
    - marker: `@plan PLAN-20260329-ISSUES-MODE.P13`, `@requirement REQ-ISS-012`

- Tests in UI component files or `src/state/mod.rs` `#[cfg(test)]`:
  - `test_issue_list_row_count` — REQ-ISS-006
    - marker: `@plan PLAN-20260329-ISSUES-MODE.P13`, `@requirement REQ-ISS-006`
  - `test_issue_list_selection_highlight` — REQ-ISS-006
    - marker: `@plan PLAN-20260329-ISSUES-MODE.P13`, `@requirement REQ-ISS-006`
  - `test_issue_list_loading_state` — REQ-ISS-006
    - marker: `@plan PLAN-20260329-ISSUES-MODE.P13`, `@requirement REQ-ISS-006`
  - `test_issue_list_empty_state` — REQ-ISS-006,014
    - marker: `@plan PLAN-20260329-ISSUES-MODE.P13`, `@requirement REQ-ISS-006,014`
  - `test_issue_detail_all_fields` — REQ-ISS-009
    - marker: `@plan PLAN-20260329-ISSUES-MODE.P13`, `@requirement REQ-ISS-009`
  - `test_issue_detail_comments_timeline` — REQ-ISS-009
    - marker: `@plan PLAN-20260329-ISSUES-MODE.P13`, `@requirement REQ-ISS-009`
  - `test_issue_detail_inline_composer_visible` — REQ-ISS-010
    - marker: `@plan PLAN-20260329-ISSUES-MODE.P13`, `@requirement REQ-ISS-010`
  - `test_filter_controls_value_binding` — REQ-ISS-008
    - marker: `@plan PLAN-20260329-ISSUES-MODE.P13`, `@requirement REQ-ISS-008`
  - `test_empty_state_no_issues` — REQ-ISS-014
    - marker: `@plan PLAN-20260329-ISSUES-MODE.P13`, `@requirement REQ-ISS-014`
  - `test_empty_state_no_comments` — REQ-ISS-014
    - marker: `@plan PLAN-20260329-ISSUES-MODE.P13`, `@requirement REQ-ISS-014`
  - `test_empty_state_no_agents_for_send` — REQ-ISS-014
    - marker: `@plan PLAN-20260329-ISSUES-MODE.P13`, `@requirement REQ-ISS-014`
  - `test_keybind_bar_issues_mode` — REQ-ISS-002
    - marker: `@plan PLAN-20260329-ISSUES-MODE.P13`, `@requirement REQ-ISS-002`

### Pseudocode traceability
- N/A (rendering/persistence tests, not algorithmic)

## Verification Commands

```bash
cargo fmt --all --check
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo test --workspace --all-features
```

## Structural Verification Checklist
- [ ] All 14 planned test names exist and compile
- [ ] Tests target rendering behavior and persistence contracts
- [ ] At least one test fails (RED step)
- [ ] No skipped phase dependencies
- [ ] `@plan` and `@requirement` traceability markers present in ALL test code

## Semantic Verification Checklist (Mandatory)
- [ ] Persistence round-trip tests cover new `issue_base_prompt` field through full state serialization path
- [ ] Backward compat test uses legacy JSON without `issue_base_prompt` field — verifies default empty string
- [ ] UI tests cover all specified states: loading, empty, normal (with selection), inline active
- [ ] Empty state tests cover: no issues, no comments, no agents for send — all 3 categories
- [ ] Keybind bar tests verify issues mode bindings are present (different from agents mode bindings)
- [ ] Feature behavior is reachable from real app flow: tests verify state conditions that the real rendering path will encounter
- [ ] No placeholder test patterns (`assert!(true)`, `#[ignore]`, empty test bodies)

## Deferred Implementation Detection (Mandatory)

```bash
grep -RIn "TODO\|FIXME\|HACK\|placeholder\|for now\|will be implemented" src/
```

## Success Criteria
- [ ] RED test suite established for UI + persistence (14 tests)
- [ ] Verification commands pass except expected RED failures
- [ ] All existing tests still pass (zero regressions from test additions)

## Failure Recovery
- rollback steps: Simplify rendering tests if too tightly coupled to iocraft internals; focus on state-level assertions
- blocking issues: tests passing with stub rendering, tests depending on iocraft rendering internals

## Phase Completion Marker
Create: `project-plans/issue15/.completed/P13.md`

Contents:
- phase ID: `PLAN-20260329-ISSUES-MODE.P13`
- timestamp
- files changed
- tests added: 14 test names
- RED test verification
- verification command outputs
- semantic verification summary

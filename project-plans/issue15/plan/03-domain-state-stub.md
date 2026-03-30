# Phase 03: Domain + State Contracts Stub

## Phase ID
`PLAN-20260329-ISSUES-MODE.P03`

## Prerequisites
- Required: Phase P02A completed
- Verify previous phase markers/artifacts exist: `.completed/P02.md`, `.completed/P02A.md`
- Expected files from previous phase: specification + analysis + pseudocode components (all 3)

## Requirements Implemented (Expanded)

### REQ-ISS-006: Issue List Display and Sorting
**Requirement text**: Each row: number, title, state, author, updated timestamp, assignee summary, label summary, comment count. Default sort: `updated_at desc`, tie-breaker `number asc`. First non-empty load selects first issue and auto-loads detail. On filter/search change: keep selection if present; else select first. Empty list shows scoped empty state.

Behavior contract:
- GIVEN: completed specification and pseudocode
- WHEN: domain types for Issue, IssueDetail, IssueComment, IssueState, IssueFilter are added to `src/domain/mod.rs`
- THEN: new types compile cleanly with all specified fields and integrate with existing code without breaking existing tests

Why it matters:
- Establishes type-safe foundation before behavior implementation. All subsequent phases depend on these domain types.

### REQ-ISS-009: Issue Detail and Comments
**Requirement text**: Detail displays: repo owner/name, number, title, state, author, timestamps, labels, assignees, milestone, body, external URL, comments timeline. Each comment: author, created, edited indicator, body. Markdown displayed as terminal-friendly rendered text. `external_url` is display-only (shown in the detail pane as a reference field).

Behavior contract:
- GIVEN: specification for IssueDetail fields
- WHEN: `IssueDetail` and `IssueComment` types are defined
- THEN: all specified fields are present with correct types

Why it matters:
- Detail types must be correct before the client boundary or UI layers are built on top of them.

### REQ-ISS-012: Repository Config `issue_base_prompt`
**Requirement text**: Multiline field in existing repository config screen. Save and Reset controls. Reset restores last-saved value. Empty value valid. Persisted via existing repository config persistence path.

Behavior contract:
- GIVEN: existing `Repository` struct in `src/domain/mod.rs`
- WHEN: `issue_base_prompt: String` field is added with `#[serde(default)]`
- THEN: existing serialized repositories deserialize without error; new field defaults to empty string

Why it matters:
- Must not break existing persistence. Backward compatibility is a hard gate for this field.

### REQ-ISS-001: Mode Entry and Exit
**Requirement text**: `i` from non-issues context enters `dashboard_issues` with `focus=issue_list`. `i` while already in `dashboard_issues` refocuses `issue_list`. `a` from `dashboard_issues` exits to `dashboard_agents`. `Esc` follows issues-mode precedence chain; exits mode only when no higher-priority cancel target exists.

Behavior contract:
- GIVEN: existing `ScreenMode` enum with `Dashboard`, `Split` variants in `src/state/mod.rs`
- WHEN: `DashboardIssues` variant is added
- THEN: existing code compiles and existing match arms for `ScreenMode` are updated to handle the new variant

Why it matters:
- Mode entry is the top-level gating mechanism for all issues-mode behavior.

### REQ-ISS-013: Authentication and Error Handling
**Requirement text**: v1 uses active `gh` CLI auth context. Missing/invalid auth blocks operations with remediation guidance. Non-auth errors: scoped error in list/detail, stable mode/focus, draft preservation, retry affordance.

Behavior contract:
- GIVEN: `src/github/mod.rs` is created
- WHEN: `GhClient` struct and method stubs are defined
- THEN: module compiles, is declared in `src/lib.rs`, and has no imports from `crate::ui` or `crate::state`

Why it matters:
- Establishing the client boundary as an isolated module prevents architectural drift in later phases.

## Implementation Tasks

### Files to create
- `src/github/mod.rs` — GitHub client boundary module skeleton
  - `GhClient` struct with method stubs (all return `todo!()`)
  - `GhError` enum skeleton
  - marker: `@plan PLAN-20260329-ISSUES-MODE.P03`
  - marker: `@requirement REQ-ISS-013`
  - marker: `@pseudocode component-002 lines 01-03`

### Files to modify
- `src/domain/mod.rs`
  - Add `Issue` struct with fields: number, title, state, author_login, updated_at, assignee_summary, labels_summary, comment_count
  - Add `IssueDetail` struct with all detail fields from specification
  - Add `IssueComment` struct with fields: comment_id, author_login, created_at, edited_at, body
  - Add `IssueState` enum: Open, Closed
  - Add `IssueFilterState` enum: Open, Closed, All
  - Add `IssueFilter` struct with all filter fields
  - Add `issue_base_prompt: String` field to `Repository` with `#[serde(default)]`
  - marker: `@plan PLAN-20260329-ISSUES-MODE.P03`
  - marker: `@requirement REQ-ISS-006,REQ-ISS-009,REQ-ISS-012`
  - marker: `@pseudocode component-001 lines 83-96`

- `src/state/mod.rs`
  - Add `DashboardIssues` variant to `ScreenMode` enum — do NOT rename or remove `Dashboard` or `Split`
  - Add `IssuesState` struct with all fields from domain model analysis
  - Add `IssueFocus` enum: `RepoList`, `IssueList`, `IssueDetail` — NEW type, separate from `PaneFocus`; do NOT modify `PaneFocus`
  - Add `DetailSubfocus` enum: Body, Comment(usize), NewComment
  - Add `InlineState` enum: None, Composer{...}, Editor{...}
  - Add `ComposerTarget` enum: NewComment, Reply{comment_index, author}
  - Add `EditorTarget` enum: IssueBody, Comment{comment_index}
  - Add `AgentChooserState` struct
  - Add `PriorAgentFocus` struct
  - Add `issues_state: IssuesState` field to `AppState` struct
  - Add issue event variants to `AppEvent` enum (stub match arms with `=> self` or `todo!()`)
  - Update ALL existing `ScreenMode` match arms to handle `DashboardIssues`
  - marker: `@plan PLAN-20260329-ISSUES-MODE.P03`
  - marker: `@requirement REQ-ISS-001`
  - marker: `@pseudocode component-001 lines 01-05, lines 33-40`

- `src/input.rs`
  - Extend `InputMode` enum with issues-mode variants: `IssuesNormal`, `IssuesInline`, `IssuesSearch`, `IssuesFilter`, `IssuesChooser` — do NOT rename or remove existing variants
  - Extend `input_mode_for_state()` to detect `ScreenMode::DashboardIssues` and return appropriate `InputMode`; detection order: inline > chooser > search > filter > normal; this branch must be BEFORE the existing `Normal` fallback
  - marker: `@plan PLAN-20260329-ISSUES-MODE.P03`
  - marker: `@requirement REQ-ISS-002`
  - marker: `@pseudocode component-003 lines 01-02`

- `src/lib.rs`
  - Add `pub mod github;` module declaration
  - marker: `@plan PLAN-20260329-ISSUES-MODE.P03`

- `src/persistence/mod.rs`
  - Verify `Repository` re-export includes `issue_base_prompt` field (via domain type change)
  - No separate change needed if `State` struct uses `Repository` from domain directly
  - marker: `@plan PLAN-20260329-ISSUES-MODE.P03`
  - marker: `@requirement REQ-ISS-012`

### Pseudocode traceability (line refs)
- component-001: lines 01-05 (dispatch skeleton), lines 33-40 (enter_issues_mode structure)
- component-002: lines 01-03 (GhClient struct)
- component-003: lines 01-02 (route skeleton)

## Verification Commands

```bash
cargo fmt --all --check
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo test --workspace --all-features
```

## Structural Verification Checklist
- [ ] New domain types compile (`Issue`, `IssueDetail`, `IssueComment`, `IssueState`, `IssueFilter`, `IssueFilterState`)
- [ ] `IssuesState` and sub-types added to `src/state/mod.rs`
- [ ] `DashboardIssues` variant added to `ScreenMode`
- [ ] `IssueFocus` is a NEW separate enum (not added to `PaneFocus`)
- [ ] `PaneFocus` is UNCHANGED (still exactly: `Repositories`, `Agents`, `Terminal`)
- [ ] All existing `ScreenMode` match arms updated to handle `DashboardIssues`
- [ ] Issue event variants added to `AppEvent` (stub match arms)
- [ ] `issue_base_prompt: String` field on `Repository` with `#[serde(default)]`
- [ ] `src/github/mod.rs` exists with `GhClient` skeleton
- [ ] `src/lib.rs` declares `pub mod github`
- [ ] `InputMode` extended with 5 issues-mode variants
- [ ] Existing `InputMode` variants UNCHANGED
- [ ] `input_mode_for_state()` handles `ScreenMode::DashboardIssues`
- [ ] All existing tests pass
- [ ] Plan/requirement/pseudocode traceability markers included in ALL changed files

## Semantic Verification Checklist (Mandatory)
- [ ] Existing `state.json` deserialization works with new `issue_base_prompt` field (backward compat verified)
- [ ] No existing key routing or event handling is broken — verify by running existing tests
- [ ] Key routing for `a` key still sets `PaneFocus::Agents` when in `ScreenMode::Dashboard` (existing behavior preserved)
- [ ] Key routing for `s`/`S` still enters split mode when in `ScreenMode::Dashboard` (existing behavior preserved)
- [ ] Key routing for `Ctrl-d`, `Ctrl-k`, `l` still works in non-issues mode (existing behavior preserved)
- [ ] GitHub client boundary is a separate module with no imports from `crate::ui` or `crate::state`
- [ ] Feature behavior is reachable from real app flow: `ScreenMode::DashboardIssues` is a valid state, `input_mode_for_state()` returns issues-mode variant
- [ ] No placeholder/deferred implementation patterns remain (except `todo!()` in stub method bodies within `src/github/mod.rs`, which is allowed in this stub phase only)

## Deferred Implementation Detection (Mandatory)

```bash
# Reject if these appear in implementation code:
grep -RIn "TODO\|FIXME\|HACK\|placeholder\|for now\|will be implemented" src/
```

Note: `todo!()` / `unimplemented!()` in stub method bodies within `src/github/mod.rs` is allowed in this stub phase only. They MUST be removed by P08 (GitHub client implementation).

## Success Criteria
- [ ] Compile-safe domain + state + GitHub client stubs exist
- [ ] Verification commands pass
- [ ] Semantic checks pass

## Failure Recovery
- rollback steps: `git restore src/domain/mod.rs src/state/mod.rs src/input.rs src/lib.rs src/persistence/mod.rs` and `rm -rf src/github/`
- blocking issues to resolve before next phase: missing type contracts, serde backward compat failure, broken existing match arms

## Phase Completion Marker
Create: `project-plans/issue15/.completed/P03.md`

Contents:
- phase ID: `PLAN-20260329-ISSUES-MODE.P03`
- timestamp
- files changed: list of files created/modified
- tests added/updated: (none in stub phase; existing tests must pass)
- verification command outputs
- backward compatibility test result
- semantic verification summary

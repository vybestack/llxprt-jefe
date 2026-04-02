# Phase 02: Pseudocode Authoring

## Phase ID
`PLAN-20260329-ISSUES-MODE.P02`

## Prerequisites
- Required: Phase P01A completed.
- Verify previous artifacts: `.completed/P01.md` and `.completed/P01A.md` exist.
- Expected files from previous phase: `analysis/domain-model.md` (complete and verified).

## Requirements Implemented (Expanded)

### REQ-ISS-NFR-003: Maintainability -- Algorithmic Pseudocode Baseline
**Requirement text**: GitHub client boundary is isolated and testable. Event/reducer pattern is followed.

Behavior contract:
- GIVEN validated domain model
- WHEN pseudocode components are authored
- THEN implementation phases can reference explicit numbered line ranges for deterministic implementation and review.

Why it matters:
- Enables deterministic implementation: every `apply()` match arm, every `GhClient` method, every key routing priority level has a pseudocode reference.

### REQ-ISS-001: Mode Entry and Exit
**Requirement text**: `i` from non-issues context enters Issues Mode (`ScreenMode::DashboardIssues`) with `IssueFocus::IssueList`. `a` exits to Agents Mode (`ScreenMode::Dashboard`). `Esc` follows precedence chain.

Behavior contract:
- GIVEN Issues Mode specification
- WHEN pseudocode for state reducer is authored
- THEN `enter_issues_mode()` and `exit_issues_mode()` functions are encoded with prior focus save/restore (`PriorAgentFocus`), state clearing, and side effect emission.

### REQ-ISS-002 + REQ-ISS-004: Key Routing and Esc Precedence Sequencing
**Requirement text**: Encode key routing priority chain and Esc precedence algorithmically.

Behavior contract:
- GIVEN Issues Mode key routing specification
- WHEN pseudocode is reviewed
- THEN priority ordering (inline > chooser > search > filter > issues-global > focus-domain > pane-cycle > suppression), suppression rules, and 6-level Esc chain are explicit and verifiable.

Why it matters:
- Key routing is the highest-complexity correctness path in Issues Mode. Pseudocode prevents ambiguity.

### REQ-ISS-003: Pane Focus and Navigation
**Requirement text**: Issues Mode pane cycle: `RepoList -> IssueList -> IssueDetail -> RepoList` via `IssueFocus` (Tab/Shift+Tab). NOT via `PaneFocus`.

Behavior contract:
- GIVEN pane focus pseudocode
- WHEN reviewed
- THEN cycle order uses `IssueFocus` enum (separate from `PaneFocus`) and detail subfocus cycling are algorithmically explicit.

### REQ-ISS-006 + REQ-ISS-007: Issue List and Pagination
**Requirement text**: Issue list display, sorting, selection rules, pagination auto-load.

Behavior contract:
- GIVEN pseudocode for list loading
- WHEN reviewed
- THEN scope guard (repo ID match), sorting invariant (`updated_at` desc, `number` asc), selection reseating, and pagination trigger are explicit.

### REQ-ISS-010: Inline Create/Edit
**Requirement text**: Inline composer/editor with exclusivity guard.

Behavior contract:
- GIVEN pseudocode for inline state transitions
- WHEN reviewed
- THEN exclusivity guard (`inline_state != None` blocks new inline), save/cancel flows, and `@author` reply prefill are explicit.

### REQ-ISS-011: Send-to-Agent
**Requirement text**: `S` opens agent chooser; payload composition includes issue data + comment + `issue_base_prompt`.

Behavior contract:
- GIVEN pseudocode for send payload
- WHEN reviewed
- THEN `build_send_payload()` algorithm is explicit with all required fields including `issue_base_prompt`.

### REQ-ISS-013: Authentication, Error Handling, Stale-Scope, and Draft Lifecycle
**Requirement text**: `gh` CLI auth check, error categorization, scoped error display, stale-scope suppression, draft discard on scope change.

Behavior contract:
- GIVEN pseudocode for GitHub client and state reducer
- WHEN reviewed
- THEN auth check, error categorization enum (7 variants), scoped error handling, stale-scope guard (`if response.repo_id != current_repo_id { DISCARD }`), and draft discard with notice on scope change are explicit.

## Implementation Tasks

### Files to create
- `project-plans/issue15/analysis/pseudocode/component-001.md` -- State + event reducer pseudocode
  - marker: `@plan PLAN-20260329-ISSUES-MODE.P02`
  - marker: `@requirement REQ-ISS-001,003,004,005,006,007,008,010,014`
  - Must be numbered line-by-line (01:, 02:, etc.)
  - Must include: enter/exit mode, `IssueFocus` cycling, `PriorAgentFocus` save/restore, `IssueListLoaded` with scope guard and sorting, `PageLoaded` append, `IssueDetailLoaded`, filter/search events, inline state transitions (exclusivity guard), Esc precedence chain (6 levels), selection reseating, empty state handling, scope change invalidation, stale-scope response discard, draft discard with notice

- `project-plans/issue15/analysis/pseudocode/component-002.md` -- GitHub client boundary pseudocode
  - marker: `@plan PLAN-20260329-ISSUES-MODE.P02`
  - marker: `@requirement REQ-ISS-006,007,008,009,010,011,013,014`
  - Must be numbered line-by-line
  - Must include: `check_auth()`, `list_issues()` with filter args and sorting, `get_issue_detail()`, `list_comments()` with pagination, `create_comment()`, `update_comment()`, `update_issue_body()`, `build_send_payload()` with all fields, `GhError` categorization (7 variants), error parsing from stderr

- `project-plans/issue15/analysis/pseudocode/component-003.md` -- Key routing + inline mutation + agent chooser pseudocode
  - marker: `@plan PLAN-20260329-ISSUES-MODE.P02`
  - marker: `@requirement REQ-ISS-001,002,003,004,010,011,012`
  - Must be numbered line-by-line
  - Must include: 8-level priority chain, suppression rules (list of suppressed keys: `s`, `Ctrl-d`, `Ctrl-k`, `l`), inline key handling (save `Ctrl+Enter`, cancel `Esc`, text entry), agent chooser key handling (`Up/Down`, `Enter`, `Esc`), search input handling, filter controls handling, issues-global unwind/mode controls, focus-domain handling, pane-cycle handling, scope change handler (with draft discard), reply `@author` prefill, exclusivity guard, `InputMode` resolution for all 5 issues variants

### Files to modify
- `project-plans/issue15/plan/00-overview.md`
  - update tracker for P02
  - marker: `@plan PLAN-20260329-ISSUES-MODE.P02`

### Pseudocode traceability (if impl phase)
- Future implementation phases must cite line ranges from:
  - component-001 (state + event reducer): lines 01-~165
  - component-002 (GitHub client boundary): lines 01-~82
  - component-003 (key routing + inline mutation + agent chooser): lines 01-~139

## Verification Commands

```bash
cargo fmt --all --check
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo test --workspace --all-features
```

## Structural Verification Checklist
- [ ] Three pseudocode component files exist under `analysis/pseudocode/`.
- [ ] All algorithms are line-numbered (no unnumbered lines).
- [ ] Validation/error/ordering constraints are present in pseudocode.
- [ ] `@plan` and `@requirement` markers present in each component file.
- [ ] Tracker updated.

## Semantic Verification Checklist (Mandatory)
- [ ] State reducer pseudocode (component-001) covers:
  - Enter/exit mode with `PriorAgentFocus` save/restore
  - `IssueFocus` cycling (NOT `PaneFocus`)
  - List/detail loading with scope guard
  - Stale-scope response discard
  - Filter/search events
  - Esc precedence (all 6 levels explicitly numbered)
  - Selection rules (first-select, keep-after-filter, reseat-after-scope-change)
  - Detail subfocus cycling (with comments, without comments)
  - Inline state transitions (exclusivity guard)
  - Scope change invalidation (clear all issues state, discard draft with notice)
  - Empty state handling
- [ ] GitHub client pseudocode (component-002) covers:
  - Auth check with `GhError::NotAuthenticated`/`NotInstalled` distinction
  - `list_issues()` with full filter arg construction and response sorting
  - `get_issue_detail()` with all fields
  - Comments pagination (append, `cursor`/`has_more` derived from GraphQL `pageInfo`)
  - Create/update comment
  - Update issue body
  - `build_send_payload()` with ALL required fields (including `issue_base_prompt`)
  - Error categorization (all 7 `GhError` variants)
- [ ] Key routing pseudocode (component-003) covers:
  - 8-level priority chain (explicitly enumerated)
  - Suppression rules (all 4 suppressed keys listed)
  - Inline key handling (text entry, `Ctrl+Enter` save, `Esc` cancel)
  - Agent chooser keys (`Up/Down`, `Enter`, `Esc`)
  - Search input keys (`Enter` apply, `Esc` clear/blur)
  - Filter controls keys
  - Scope change handler (with draft discard and notice)
  - Reply `@author` prefill logic
  - Exclusivity guard (reject second inline when first active)
  - `InputMode` resolution for all 5 issues variants (IssuesNormal, IssuesInline, IssuesSearch, IssuesFilter, IssuesChooser)
- [ ] Feature behavior is reachable from real app flow: pseudocode encodes `i` key -> `route_issues_mode_key()` -> dispatch event -> `apply()` -> state mutation.
- [ ] No placeholder pseudocode (no "..." or "handle other cases" or "etc.").
- [ ] Terminology matches glossary: `IssueFocus` (not `PaneFocus`), `DashboardIssues` (not `dashboard_issues`), etc.

## Deferred Implementation Detection (Mandatory)

```bash
grep -RIn "TODO\|FIXME\|HACK\|placeholder\|for now\|will be implemented\|\.\.\." project-plans/issue15/analysis/pseudocode/
```

## Success Criteria
- [ ] Pseudocode accepted as implementation contract baseline.
- [ ] Every REQ-ISS-* covered by at least one pseudocode line range.
- [ ] Terminology consistent with glossary.

## Failure Recovery
- rollback steps: Patch pseudocode gaps and rerun P02 verification. Add missing algorithm sections.
- blocking issues to resolve before next phase: missing algorithm sections, unnumbered lines, placeholder pseudocode, terminology mismatches.

## Phase Completion Marker
Create: `project-plans/issue15/.completed/P02.md`

Contents:
- phase ID: `PLAN-20260329-ISSUES-MODE.P02`
- timestamp
- files changed: list of pseudocode files created
- REQ-to-line-range traceability table (all 17 REQs mapped to component:line-range)
- verification outputs
- semantic verification summary
nge)
- verification outputs
- semantic verification summary

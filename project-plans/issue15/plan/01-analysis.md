# Phase 01: Analysis Consolidation

## Phase ID
`PLAN-20260329-ISSUES-MODE.P01`

## Prerequisites
- Required: Phase P00A completed.
- Verify previous artifacts: `.completed/P00A.md` exists with PASS decision.
- Expected files from previous phase: preflight evidence log, updated overview tracker.

## Requirements Implemented (Expanded)

### REQ-ISS-NFR-003: Maintainability — Architecture Ownership Map
**Requirement text**: GitHub client boundary is isolated and testable. Event/reducer pattern is followed.

Behavior contract:
- GIVEN Issues Mode specification and codebase structure verified in preflight
- WHEN analysis is completed
- THEN each layer (domain, state, GitHub client, UI, persistence) has explicit responsibilities and forbidden couplings documented; integration touchpoints map to verified file paths.

Why it matters:
- Prevents accidental side-effect leakage between GitHub client and state, or UI and `gh` CLI. Establishes clear ownership before any code is written.

### REQ-ISS-001: Mode Entry and Exit
**Requirement text**: `i` from non-issues context enters `dashboard_issues` with `focus=issue_list`. `i` while already in `dashboard_issues` refocuses `issue_list`. `a` from `dashboard_issues` exits to `dashboard_agents`. `Esc` follows issues-mode precedence chain.

Behavior contract:
- GIVEN user is in Agents Mode (`ScreenMode::Dashboard`)
- WHEN `i` is pressed
- THEN state transitions to `ScreenMode::DashboardIssues` with `IssueFocus::IssueList`

Why it matters:
- Mode lifecycle is the foundational flow; all other behavior depends on entering/exiting correctly.

### REQ-ISS-002: Key Routing and Suppression
**Requirement text**: While in Issues Mode: suppress `a` focus-agents, `s/S` split-mode, split-mode `Esc`, destructive lifecycle keys (`Ctrl-d`, `Ctrl-k`, `l`). Route `/` to issue-list search; `?`/`h`/`F1` to help with Issues Mode bindings.

Behavior contract:
- GIVEN user is in Issues Mode (`ScreenMode::DashboardIssues`)
- WHEN `Ctrl-d` is pressed
- THEN the key is consumed as no-op (agent destructive action suppressed)

Why it matters:
- Key routing errors are the most common source of cross-mode bugs. Explicit suppression mapping prevents accidental agent operations from issues mode.

### REQ-ISS-003 through REQ-ISS-014: Full Requirement Set Mapping
**Requirement text**: Map all 14 functional requirements plus 3 NFRs to user-triggered flows, state transitions, and integration paths in the domain model analysis.

Behavior contract:
- GIVEN full requirement set from specification
- WHEN analysis is authored
- THEN every REQ-ISS-* identifier is reachable via at least one explicit user path and has at least one testable assertion documented.

Why it matters:
- Ensures plan completeness and avoids dead/unreachable work. Analysis gaps propagate into missing implementation phases.

## Implementation Tasks

### Files to create
- `project-plans/issue15/analysis/domain-model.md` (if not already present; update if exists)
  - marker: `@plan PLAN-20260329-ISSUES-MODE.P01`
  - marker: `@requirement REQ-ISS-NFR-003`
  - Must include:
    1. Entity definitions with field types (mapping to actual Rust types)
    2. State aggregate (`IssuesState`) with all fields
    3. Event taxonomy with explicit enum variant names for `AppEvent`
    4. Edge/error model (all `GhError` variants and recovery paths)
    5. Integration touchpoints (file paths verified in preflight, function signatures)
    6. Existing code modification map (enum changes, struct field additions, match arm updates)
    7. New code creation map (new files and their responsibilities)
    8. **Baseline-to-target mapping** (see 00-overview.md for enum evolution diagrams):
       - `ScreenMode`: add `DashboardIssues` (DO NOT rename `Dashboard` or `Split`)
       - `PaneFocus`: UNCHANGED (Repositories, Agents, Terminal)
       - `IssueFocus`: NEW separate enum (RepoList, IssueList, IssueDetail)
       - `InputMode`: add 5 new variants (do not rename existing 6)
       - `AppEvent`: add issue event variants (do not modify existing variants)

### Files to modify
- `project-plans/issue15/plan/00-overview.md`
  - update tracker for P01
  - marker: `@plan PLAN-20260329-ISSUES-MODE.P01`

### Pseudocode traceability (if impl phase)
- N/A (analysis phase)

## Verification Commands

```bash
cargo fmt --all --check
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo test --workspace --all-features
```

## Structural Verification Checklist
- [ ] Domain model analysis file exists and is complete.
- [ ] All new entity types defined with fields and Rust types.
- [ ] Events taxonomy is complete (lifecycle, navigation, data loading, filter/search, inline mutation, send-to-agent).
- [ ] Integration touchpoints list references verified file paths from preflight.
- [ ] Existing code modification map is documented with specific files, enum names, and change descriptions.
- [ ] New code creation map lists all new files to create.
- [ ] Baseline-to-target enum evolution is explicit (what changes, what does NOT change).
- [ ] Tracker updated.

## Semantic Verification Checklist (Mandatory)
- [ ] Analysis covers all major flows:
  - Mode lifecycle (REQ-ISS-001): enter via `i` -> `ScreenMode::DashboardIssues`, exit via `a` -> `ScreenMode::Dashboard`
  - Key routing (REQ-ISS-002): 7-level priority, suppression of `s`/`Ctrl-d`/`Ctrl-k`/`l`
  - Navigation (REQ-ISS-003): `IssueFocus` cycling (not `PaneFocus`), detail subfocus
  - Esc precedence (REQ-ISS-004): 6-level chain
  - Focus restoration (REQ-ISS-005): save/restore/validate/fallback
  - Issue list display (REQ-ISS-006): 8 fields, sorting, selection
  - Pagination (REQ-ISS-007): auto-load, comment append
  - Filtering/search (REQ-ISS-008): structured AND composition
  - Issue detail (REQ-ISS-009): all fields, comments, browser open
  - Inline create/edit (REQ-ISS-010): exclusivity, save/cancel, @author prefill
  - Send-to-agent (REQ-ISS-011): chooser, payload with `issue_base_prompt`
  - Repository config (REQ-ISS-012): `issue_base_prompt` field, persist, Save/Reset
  - Auth/error (REQ-ISS-013): auth check, error categories, draft preservation, draft discard on scope change, stale-scope suppression
  - Empty states (REQ-ISS-014): 3 categories (issues, comments, agents)
- [ ] GitHub client boundary isolation is explicit (no UI/state imports allowed).
- [ ] Feature behavior is reachable from real app flow: analysis describes `i` key -> `handle_normal_key_event()` -> `AppEvent::EnterIssuesMode` -> `apply()` -> state mutation -> `ScreenMode::DashboardIssues` -> UI render.
- [ ] No missing requirement mapping (every REQ-ISS-* appears in analysis text).
- [ ] Terminology is consistent with glossary in 00-overview.md (`ScreenMode::DashboardIssues` not "dashboard_issues", `IssueFocus` not `PaneFocus`, etc.).

## Deferred Implementation Detection (Mandatory)

```bash
grep -RIn "TODO\|FIXME\|HACK\|placeholder\|for now\|will be implemented\|TBD\|to be determined" project-plans/issue15/analysis/
```

## Success Criteria
- [ ] Analysis artifact approved for pseudocode derivation.
- [ ] Quality gates pass.
- [ ] All 14+3 requirements traceable to analysis content.
- [ ] Enum evolution mapping is explicit and matches 00-overview.md.

## Failure Recovery
- rollback steps: Revise domain mapping and ownership contracts. Re-examine specification for missed requirements.
- blocking issues to resolve before next phase: missing invariants, missing ownership assignments, missing flow mapping for any REQ-ISS-* identifier, incorrect enum evolution plan.

## Phase Completion Marker
Create: `project-plans/issue15/.completed/P01.md`

Contents:
- phase ID: `PLAN-20260329-ISSUES-MODE.P01`
- timestamp
- files changed: list of analysis artifacts created/updated
- verification outputs: quality gate results
- semantic verification summary: REQ coverage confirmation
- enum evolution mapping confirmation

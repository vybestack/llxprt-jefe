# Phase 08A: GitHub Client Boundary Implementation Verification

## Phase ID
`PLAN-20260329-ISSUES-MODE.P08A`

## Prerequisites
- Required: Phase P08 completed.
- Verify previous artifacts: `.completed/P08.md` exists.
- Expected files from previous phase: implemented `src/github/mod.rs` with all 18 tests GREEN, zero stubs, and all public functions carrying traceability markers.

## Requirements Implemented (Expanded)

### Verification of GitHub Client Implementation for REQ-ISS-006,007,008,009,010,011,013
**Requirement text**: Confirm all client methods are implemented, tests pass, no stubs remain, and module isolation is maintained.

Behavior contract:
- GIVEN implemented `GhClient` with all methods functional
- WHEN all tests execute and verification checks run
- THEN all 18 tests GREEN, zero `todo!()` stubs, no forbidden imports, all error categories covered, traceability markers present.

Why it matters:
- The GitHub client boundary is the sole I/O layer for all issues-mode data. Any stub or isolation violation here propagates failures into every upstream phase (key routing, state, UI).

## Implementation Tasks

### Files to create
- `project-plans/issue15/.completed/P08A.md`
  - marker: `@plan PLAN-20260329-ISSUES-MODE.P08A`

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

### No-Stub Gate
```bash
grep -rn "todo!()\|unimplemented!()" src/github/ && echo "FAIL: stubs remain" || echo "OK: no stubs"
```

### Module Isolation Gate
```bash
grep -n "use crate::ui\|use crate::state\|use crate::app_input" src/github/mod.rs && echo "FAIL: forbidden imports" || echo "OK: isolation verified"
```

### Traceability Gate
```bash
# Every public function in github/mod.rs must have @plan + @requirement + @pseudocode
echo "--- Plan markers per function ---"
grep -B5 "pub fn" src/github/mod.rs | grep "@plan\|@requirement\|@pseudocode\|pub fn"
```

## Structural Verification Checklist
- [ ] All 18 tests pass.
- [ ] No stubs remain in `src/github/`.
- [ ] Module isolation maintained (no forbidden imports).
- [ ] Traceability markers present for every public function.
- [ ] No skipped phases.
- [ ] Plan/requirement traceability present.
- [ ] Tests compile and run.
- [ ] Tracker updated.

## Semantic Verification Checklist (Mandatory)
- [ ] All `GhClient` methods are functional (not stubbed).
- [ ] Error categorization covers all specified cases (rate limit, auth, access denied, parse, network) — verified against component-002 lines 75-82.
- [ ] Send payload composition is complete with all required fields (repository slug, issue number/title/body/state/labels/assignees, focused comment, issue_base_prompt) — verified against component-002 lines 62-74.
- [ ] Filter args construction is correct for all filter fields (state, author, assignee, labels, mentioned, search) — verified against component-002 lines 09-25.
- [ ] Sorting is verified: `updated_at` desc, `number` asc tie-break — per REQ-ISS-006 and component-002 lines 20-22.
- [ ] Comment pagination appends in stable order without reordering loaded comments — per REQ-ISS-007 and component-002 lines 33-43.
- [ ] Comment pagination failure retains loaded comments and exposes retry — per REQ-ISS-007.
- [ ] Auth check correctly distinguishes NotAuthenticated (gh installed, not logged in) from NotInstalled (gh binary missing) — per component-002 lines 04-08.
- [ ] Rate limit detection parses stderr correctly — per component-002 lines 75-82.
- [ ] Feature behavior is reachable from real app flow: `GhClient` methods produce results compatible with state events (`IssueListLoaded`, `IssueDetailLoaded`, `IssueCommentsPageLoaded`, `CommentCreated`, etc.).
- [ ] No placeholder/deferred patterns remain.

## Deferred Implementation Detection (Mandatory)

```bash
grep -RIn "TODO\|FIXME\|HACK\|placeholder\|for now\|will be implemented" src/github/
```

## Success Criteria
- [ ] Implementation verification pass.
- [ ] All gates pass (no-stub, module isolation, traceability).
- [ ] Verification commands pass.
- [ ] Semantic checks pass.

## Failure Recovery
- rollback steps: Fix failing tests or logic. If stubs remain, complete the method implementations. If module isolation is violated, remove forbidden imports.
- blocking issues: test regressions, remaining stubs, forbidden imports in github module.

## Phase Completion Marker
Create: `project-plans/issue15/.completed/P08A.md`

Contents:
- phase ID: `PLAN-20260329-ISSUES-MODE.P08A`
- timestamp
- no-stub gate output
- module isolation gate output
- traceability gate output
- verification outputs
- semantic verification summary

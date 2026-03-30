# Phase 11A: Key Routing + Input Dispatch Implementation Verification

## Phase ID
`PLAN-20260329-ISSUES-MODE.P11A`

## Prerequisites
- Required: Phase P11 completed.
- Verify previous artifacts: `.completed/P11.md` exists.
- Expected files from previous phase: implemented key routing in `src/app_input.rs` and `src/input.rs` with all 25 tests GREEN, `GhClient` wired for all operations, no stubs remaining.

## Requirements Implemented (Expanded)

### Verification of Key Routing Implementation for REQ-ISS-001,002,003,004,008,010,011,013
**Requirement text**: Confirm all key routing behavior is implemented, tests pass, no stubs remain, existing behavior is unaffected, and traceability markers are present.

Behavior contract:
- GIVEN implemented key routing in `handle_issues_mode_key()` and all sub-handlers
- WHEN all tests execute and verification checks run
- THEN all 25 key routing tests GREEN, all existing tests GREEN, no stub returns in dispatch code, GhClient wired for all operations.

Why it matters:
- Key routing is the only path from user input to state change. Stubs or missing wiring here means features are unreachable from the real app even if state and client layers are correct. Regression in existing routing breaks the entire agents-mode workflow.

## Implementation Tasks

### Files to create
- `project-plans/issue15/.completed/P11A.md`
  - marker: `@plan PLAN-20260329-ISSUES-MODE.P11A`

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

### No-Stub Verification
```bash
grep -rn "todo!()\|unimplemented!()" src/app_input.rs src/input.rs && echo "FAIL: stubs remain" || echo "OK: no stubs"
```

### GhClient Wiring Verification
```bash
# Verify GhClient methods are called from app_input
grep -n "list_issues\|get_issue_detail\|list_comments\|create_comment\|update_comment\|update_issue_body\|build_send_payload\|check_auth" src/app_input.rs
```

### Existing Behavior Regression Verification
```bash
# Verify existing key bindings still function
echo "--- Existing key dispatch tests ---"
cargo test --workspace --all-features -- handle_normal_key 2>&1 | tail -10
```

### Traceability Gate
```bash
# Every new function in app_input.rs must have @plan + @requirement + @pseudocode
echo "--- Plan markers in key routing functions ---"
grep -B5 "fn handle_issues\|fn handle_issue_list\|fn handle_issue_detail\|fn handle_inline\|fn handle_agent_chooser\|fn handle_search_input\|fn handle_filter" src/app_input.rs | grep "@plan\|@requirement\|@pseudocode\|fn "
```

## Structural Verification Checklist
- [ ] All 25 key routing tests pass.
- [ ] All existing tests pass (zero regressions).
- [ ] No stubs remain in routing code.
- [ ] No skipped phases.
- [ ] Plan/requirement traceability present in all new functions.
- [ ] Tests compile and run.
- [ ] Tracker updated.

## Semantic Verification Checklist (Mandatory)
- [ ] Full key routing chain functional (all 7 priority levels).
- [ ] Suppression rules verified: `s`, `Ctrl-d`, `Ctrl-k`, `l` consumed as no-op in issues mode — state unchanged.
- [ ] Esc precedence chain verified at all 6 levels (inline -> chooser -> search-clear -> search-blur -> filter-close -> exit) — each level independently testable.
- [ ] Existing dashboard/agents mode behavior unaffected — `a`, `s`, `Ctrl-d`, `Ctrl-k`, `l` still work outside issues mode.
- [ ] GhClient calls wired for all data loading and mutation operations.
- [ ] Stale-scope suppression: loading response with wrong repo ID is discarded.
- [ ] Draft discard on scope change: switching repos while inline active cancels draft with notice.
- [ ] Feature behavior is reachable from real app flow: user can press `i` -> browse issues -> interact -> press `a` to exit.
- [ ] No placeholder/deferred patterns remain.

## Deferred Implementation Detection (Mandatory)

```bash
grep -RIn "TODO\|FIXME\|HACK\|placeholder\|for now\|will be implemented" src/app_input.rs src/input.rs
```

## Success Criteria
- [ ] Implementation verification pass.
- [ ] All gates pass (no-stub, GhClient wiring, traceability, regression).
- [ ] Verification commands pass.
- [ ] Semantic checks pass.

## Failure Recovery
- rollback steps: Fix failing tests or routing logic. If existing key regression, identify which match arm was broken and restore it.
- blocking issues: test regressions, broken existing key handling.

## Phase Completion Marker
Create: `project-plans/issue15/.completed/P11A.md`

Contents:
- phase ID: `PLAN-20260329-ISSUES-MODE.P11A`
- timestamp
- no-stub verification output
- GhClient wiring verification output
- existing behavior regression verification output
- traceability gate output
- verification outputs
- semantic verification summary

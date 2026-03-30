# Phase 16A: Final Verification

## Phase ID
`PLAN-20260329-ISSUES-MODE.P16A`

## Prerequisites
- Required: Phase P16 completed.
- Verify previous artifacts: `.completed/P16.md` exists with all gates passing.
- Expected files from previous phase: all quality gate audits passed, all tests GREEN, zero deferred patterns confirmed.

## Requirements Implemented (Expanded)

### Final Verification of All REQ-ISS-001..014 + REQ-ISS-NFR-001..003
**Requirement text**: Final confirmation that Issues Mode is complete, correct, and ready for merge.

Behavior contract:
- GIVEN all prior phases completed and verified
- WHEN final verification runs
- THEN all tests pass, all quality gates pass, all requirements are traced, no deferred patterns exist, existing enums are preserved, and the feature is ready for code review.

Why it matters:
- This is the merge gate. Any gap left here — a missing requirement trace, a preserved deferred pattern, a broken existing test — becomes a defect in production. The cost of catching it here is near zero; the cost after merge is orders of magnitude higher.

## Implementation Tasks

### Files to create
- `project-plans/issue15/.completed/P16A.md` -- Final completion marker
  - marker: `@plan PLAN-20260329-ISSUES-MODE.P16A`

### Files to modify
- `project-plans/issue15/plan/00-overview.md` -- final tracker update (all phases complete)

### Pseudocode traceability (if impl phase)
- N/A (final verification)

## Verification Commands

```bash
# Full quality gate suite
cargo fmt --all --check
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo test --workspace --all-features
```

### Zero Deferred Patterns (Final Gate)
```bash
grep -RIn "TODO\|FIXME\|HACK\|placeholder\|for now\|will be implemented" src/ && echo "FAIL" || echo "OK"
grep -rn "todo!()\|unimplemented!()" src/ && echo "FAIL" || echo "OK"
```

### Architecture Integrity (Final Gate)
```bash
# No forbidden imports
grep -rn "use crate::ui\|use crate::state\|use crate::app_input" src/github/ && echo "FAIL" || echo "OK"

# No fork files
find src/ -name "*v2*" -o -name "*_new*" -o -name "*_old*" | head -5

# Enum preservation
echo "--- PaneFocus (MUST be unchanged: Repositories, Agents, Terminal) ---"
grep -A6 "pub enum PaneFocus" src/state/mod.rs

echo "--- IssueFocus (MUST be separate: RepoList, IssueList, IssueDetail) ---"
grep -A5 "pub enum IssueFocus" src/state/mod.rs

echo "--- ScreenMode (MUST have: Dashboard, Split, DashboardIssues) ---"
grep -A6 "pub enum ScreenMode" src/state/mod.rs

# Count new files
find src/github/ src/ui/components/issue_list.rs src/ui/components/issue_detail.rs src/ui/components/filter_controls.rs src/ui/components/agent_chooser.rs src/ui/screens/issues.rs -type f 2>/dev/null | wc -l
```

### Comprehensive Test Count
```bash
cargo test --workspace --all-features 2>&1 | grep "test result:"
```

### Phase Completion Audit
```bash
# Verify all phase completion markers exist
for phase in P00A P01 P01A P02 P02A P03 P03A P04 P04A P05 P05A P06 P06A P07 P07A P08 P08A P09 P09A P10 P10A P11 P11A P12 P12A P13 P13A P14 P14A P15 P15A P16; do
  if test -f "project-plans/issue15/.completed/$phase.md"; then
    echo "OK: $phase completed"
  else
    echo "MISSING: $phase not completed"
  fi
done
```

## Structural Verification Checklist
- [ ] All quality gate commands pass.
- [ ] All 32 phase completion markers exist.
- [ ] Zero deferred patterns.
- [ ] Zero architecture forks.
- [ ] No skipped phases.
- [ ] Plan/requirement traceability present throughout `src/`.
- [ ] Tests compile and run.
- [ ] `PaneFocus` unchanged (3 original variants only).
- [ ] `IssueFocus` is separate enum.
- [ ] `ScreenMode` has `Dashboard`, `Split`, `DashboardIssues`.
- [ ] All new files accounted for (6 new files).

## Semantic Verification Checklist (Mandatory)
- [ ] **Complete user journey verified**: User can enter issues mode (`i`), browse issue list, select issue, view detail, create comment, reply to comment, edit body, send to agent, filter issues, search issues, paginate, and exit (`a`/`Esc`).
- [ ] **Error journey verified**: Auth failure, rate limit, network error, 404 — all handled gracefully.
- [ ] **Empty state journey verified**: No repos, no issues, no comments, no agents — all show correct messages.
- [ ] **Backward compatibility verified**: Existing agents mode works. Existing state.json loads. `PaneFocus` unchanged. `ScreenMode::Dashboard` and `ScreenMode::Split` preserved.
- [ ] **Key routing integrity verified**: All suppressed keys confirmed no-op in issues mode. All existing keys confirmed functional outside issues mode.
- [ ] **Draft lifecycle verified**: Draft preserved on error; draft discarded on scope change with notice.
- [ ] **Stale-scope verified**: Response for wrong repo ID discarded.
- [ ] **All 17 requirements traced**: Each REQ-ISS-* has at least one test and one source code marker.
- [ ] Feature behavior is reachable from real app flow: the feature is usable by a real user.
- [ ] No placeholder/deferred patterns remain anywhere in `src/`.

## Deferred Implementation Detection (Mandatory)

```bash
grep -RIn "TODO\|FIXME\|HACK\|placeholder\|for now\|will be implemented" src/
```

## Final Deliverables Checklist
- [ ] All source files compile, lint, and test clean.
- [ ] All plan phase markers (.completed/) present.
- [ ] Specification matches implementation.
- [ ] Analysis and pseudocode artifacts preserved for reference.
- [ ] No test regressions.
- [ ] Glossary terminology used consistently throughout code (see 00-overview.md glossary).

## Success Criteria
- [ ] Feature is complete and verified.
- [ ] All quality gates pass.
- [ ] All semantic checks pass (all 17 REQs).
- [ ] Ready for code review and merge.

## Failure Recovery
- rollback steps: Address any remaining issues found in final verification. Re-run P16A until all gates pass.
- blocking issues: any failing gate or missing completion marker.

## Phase Completion Marker
Create: `project-plans/issue15/.completed/P16A.md`

Contents:
- phase ID: `PLAN-20260329-ISSUES-MODE.P16A`
- timestamp
- FINAL VERIFICATION: PASS/FAIL
- full test suite result
- deferred-impl gate result
- architecture integrity gate result (including enum preservation)
- phase completion audit result
- all 17 requirements traced (table)
- summary: feature ready for review

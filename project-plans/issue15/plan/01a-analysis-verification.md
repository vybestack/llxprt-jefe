# Phase 01A: Analysis Verification

## Phase ID
`PLAN-20260329-ISSUES-MODE.P01A`

## Prerequisites
- Required: Phase P01 completed.
- Verify previous artifacts: `.completed/P01.md` exists.
- Expected files from previous phase: `analysis/domain-model.md` (created or updated).

## Requirements Implemented (Expanded)

### Verification of REQ-ISS-001..014 + REQ-ISS-NFR-001..003 Coverage
**Requirement text**: Confirm analysis covers all 14 functional requirements and 3 non-functional requirements with explicit traceability.

Behavior contract:
- GIVEN completed domain model analysis
- WHEN verification checks are executed
- THEN every REQ-ISS-* identifier is traceable to at least one domain entity, event, or flow in the analysis document; enum evolution mapping matches 00-overview.md glossary.

Why it matters:
- Analysis gaps propagate into missing pseudocode and missing implementation phases. Catching gaps here is far cheaper than catching them during implementation.

## Implementation Tasks

### Files to create
- `project-plans/issue15/.completed/P01A.md`
  - marker: `@plan PLAN-20260329-ISSUES-MODE.P01A`

### Files to modify
- `project-plans/issue15/plan/00-overview.md` -- tracker update
  - marker: `@plan PLAN-20260329-ISSUES-MODE.P01A`

### Pseudocode traceability (if impl phase)
- N/A (verification phase)

## Verification Commands

```bash
cargo fmt --all --check
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo test --workspace --all-features
```

### Requirement Coverage Verification
```bash
# Verify every REQ-ISS-NNN appears in analysis
for req in REQ-ISS-001 REQ-ISS-002 REQ-ISS-003 REQ-ISS-004 REQ-ISS-005 REQ-ISS-006 REQ-ISS-007 REQ-ISS-008 REQ-ISS-009 REQ-ISS-010 REQ-ISS-011 REQ-ISS-012 REQ-ISS-013 REQ-ISS-014 REQ-ISS-NFR-001 REQ-ISS-NFR-002 REQ-ISS-NFR-003; do
  if grep -q "$req" project-plans/issue15/analysis/domain-model.md; then
    echo "OK: $req found"
  else
    echo "MISSING: $req not found in analysis"
  fi
done
```

### Enum Evolution Verification
```bash
# Verify analysis uses correct terminology
echo "--- ScreenMode terminology (should say DashboardIssues, not dashboard_issues) ---"
grep -n "DashboardIssues\|dashboard_issues" project-plans/issue15/analysis/domain-model.md

echo "--- PaneFocus unchanged (should say PaneFocus is NOT modified) ---"
grep -n "PaneFocus" project-plans/issue15/analysis/domain-model.md

echo "--- IssueFocus separate (should define IssueFocus as new separate enum) ---"
grep -n "IssueFocus" project-plans/issue15/analysis/domain-model.md

echo "--- InputMode terminology (should list IssuesNormal etc., not IssueMode) ---"
grep -n "IssuesNormal\|InputMode" project-plans/issue15/analysis/domain-model.md
```

### Key Routing Behavioral Verification
```bash
# Verify analysis documents suppression behavior
echo "--- Suppression rules documented ---"
grep -n "suppress\|no-op\|consumed" project-plans/issue15/analysis/domain-model.md | head -10

# Verify Esc precedence documented
echo "--- Esc precedence chain documented ---"
grep -n "Esc\|precedence" project-plans/issue15/analysis/domain-model.md | head -10

# Verify stale-scope documented
echo "--- Stale-scope behavior documented ---"
grep -n "stale\|scope change\|invalidat" project-plans/issue15/analysis/domain-model.md | head -10

# Verify draft discard documented
echo "--- Draft discard behavior documented ---"
grep -n "draft\|discard" project-plans/issue15/analysis/domain-model.md | head -10
```

## Structural Verification Checklist
- [ ] Domain model analysis exists and is non-empty.
- [ ] All REQ-ISS-001..014 identifiers appear in analysis text.
- [ ] All REQ-ISS-NFR-001..003 identifiers appear in analysis text.
- [ ] Event taxonomy covers all user flows.
- [ ] Enum evolution mapping is consistent with 00-overview.md glossary.
- [ ] Tracker updated.

## Semantic Verification Checklist (Mandatory)
- [ ] Each functional requirement maps to at least one domain entity + event + flow.
- [ ] Edge/error model is complete (gh not installed, not authenticated, rate limit, 404, network failure, scope change during flight, etc.).
- [ ] Integration touchpoints are validated against actual codebase file paths (from preflight).
- [ ] Feature behavior is reachable from real app flow: analysis describes `i` key -> event dispatch -> `ScreenMode::DashboardIssues` -> UI render path.
- [ ] No placeholder/deferred analysis patterns remain (no "TBD", "to be determined", "will be analyzed later").
- [ ] Key routing behavioral coverage verified: suppression, Esc precedence, stale-scope, draft discard all documented.
- [ ] Terminology matches glossary: `ScreenMode::DashboardIssues` (not "dashboard_issues"), `IssueFocus` (not added to `PaneFocus`), etc.

## Deferred Implementation Detection (Mandatory)

```bash
grep -RIn "TODO\|FIXME\|HACK\|placeholder\|for now\|will be implemented\|TBD\|to be determined" project-plans/issue15/analysis/
```

## Success Criteria
- [ ] Analysis verification pass -- all requirements represented.
- [ ] No requirement gap identified.
- [ ] Enum evolution consistent with plan overview.
- [ ] Quality gates pass.

## Failure Recovery
- rollback steps: Update `analysis/domain-model.md` to cover missing requirements, then re-run P01A verification.
- blocking issues: unrepresented requirements, missing edge cases, stale file path references, terminology inconsistencies.

## Phase Completion Marker
Create: `project-plans/issue15/.completed/P01A.md`

Contents:
- phase ID: `PLAN-20260329-ISSUES-MODE.P01A`
- timestamp
- requirement coverage check output (all 17 REQ-ISS-* identifiers)
- enum evolution verification output
- key routing behavioral verification output
- verification outputs
- semantic verification summary

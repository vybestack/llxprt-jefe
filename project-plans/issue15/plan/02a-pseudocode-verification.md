# Phase 02A: Pseudocode Verification

## Phase ID
`PLAN-20260329-ISSUES-MODE.P02A`

## Prerequisites
- Required: Phase P02 completed.
- Verify previous artifacts: `.completed/P02.md` exists.
- Expected files from previous phase: `analysis/pseudocode/component-001.md`, `component-002.md`, `component-003.md`.

## Requirements Implemented (Expanded)

### Verification of Pseudocode Completeness Across All Requirements
**Requirement text**: Confirm pseudocode covers all 14 functional requirements and 3 NFRs and is suitable for deterministic implementation.

Behavior contract:
- GIVEN completed pseudocode components (3 files)
- WHEN verification checks are executed
- THEN every REQ-ISS-* has at least one pseudocode line range addressing it; every function is numbered; no placeholder algorithms exist; terminology matches glossary.

Why it matters:
- Pseudocode gaps propagate directly into missing implementation. This is the last gate before code is written.

## Implementation Tasks

### Files to create
- `project-plans/issue15/.completed/P02A.md`
  - marker: `@plan PLAN-20260329-ISSUES-MODE.P02A`

### Files to modify
- `project-plans/issue15/plan/00-overview.md` -- tracker update
  - marker: `@plan PLAN-20260329-ISSUES-MODE.P02A`

### Pseudocode traceability (if impl phase)
- N/A (verification phase)

## Verification Commands

```bash
cargo fmt --all --check
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo test --workspace --all-features
```

### REQ-to-Pseudocode Traceability Verification
Verify every requirement has explicit pseudocode coverage:

| REQ ID | Component | Line Range | Covered |
|--------|-----------|------------|---------|
| REQ-ISS-001 | 001 | ~33-51 (enter/exit mode) | [ ] |
| REQ-ISS-002 | 003 | ~01-38 (priority/suppress) | [ ] |
| REQ-ISS-003 | 001 | ~52-82 (navigation/focus with IssueFocus) | [ ] |
| REQ-ISS-004 | 001 | ~115-127 (Esc chain, 6 levels) | [ ] |
| REQ-ISS-005 | 001 | ~41-51 (exit restore with PriorAgentFocus) | [ ] |
| REQ-ISS-006 | 001 | ~83-96 (list loaded/selection), 002 | ~09-25 (list_issues) | [ ] |
| REQ-ISS-007 | 001 | ~97-102 (page loaded), 002 | ~33-43 (comments pagination) | [ ] |
| REQ-ISS-008 | 001 | ~22-29 (filter events), 003 | ~110-125 (filter keys) | [ ] |
| REQ-ISS-009 | 002 | ~26-43 (detail/comments) | [ ] |
| REQ-ISS-010 | 003 | ~71-99 (inline key/submit, exclusivity) | [ ] |
| REQ-ISS-011 | 003 | ~100-109 (agent chooser), 002 | ~62-74 (payload with issue_base_prompt) | [ ] |
| REQ-ISS-012 | 002 | ~62-74 (payload includes issue_base_prompt) | [ ] |
| REQ-ISS-013 | 002 | ~04-08 (auth), ~75-82 (error 7 variants), 001 | stale-scope guard, draft discard | [ ] |
| REQ-ISS-014 | 001 | ~90-95 (empty list/state) | [ ] |
| REQ-ISS-NFR-001 | (non-blocking loading -- implicit in async) | [ ] |
| REQ-ISS-NFR-002 | 002 | ~75-82 (error categorization, no panic) | [ ] |
| REQ-ISS-NFR-003 | (module isolation -- architectural constraint) | [ ] |

Note: line ranges are approximate; exact ranges depend on P02 authoring. Verifier must confirm actual line ranges match.

### Feature-Specific Behavioral Verification
```bash
# Verify key routing behavior documented in pseudocode
echo "--- Suppression rules (should list s, Ctrl-d, Ctrl-k, l) ---"
grep -n "suppress\|no-op\|consumed\|SUPPRESS" project-plans/issue15/analysis/pseudocode/component-003.md | head -10

echo "--- Stale-scope guard (should check repo_id match) ---"
grep -n "stale\|scope\|repo_id\|DISCARD" project-plans/issue15/analysis/pseudocode/component-001.md | head -10

echo "--- Draft discard on scope change ---"
grep -n "draft\|discard\|notice" project-plans/issue15/analysis/pseudocode/component-001.md | head -10

echo "--- IssueFocus (NOT PaneFocus) ---"
grep -n "IssueFocus\|PaneFocus" project-plans/issue15/analysis/pseudocode/component-001.md | head -10

echo "--- Esc precedence levels (should have 6 explicit levels) ---"
grep -n "Esc\|precedence\|LEVEL\|level" project-plans/issue15/analysis/pseudocode/component-001.md | head -10

echo "--- Exclusivity guard ---"
grep -n "exclusiv\|active\|reject\|guard" project-plans/issue15/analysis/pseudocode/component-003.md | head -10
```

### Terminology Consistency Verification
```bash
# Should NOT find "dashboard_issues" (lowercase), should find "DashboardIssues"
echo "--- Terminology check ---"
grep -rn "dashboard_issues" project-plans/issue15/analysis/pseudocode/ && echo "WARN: lowercase dashboard_issues found" || echo "OK"
grep -rn "DashboardIssues\|ScreenMode" project-plans/issue15/analysis/pseudocode/ | head -5
```

## Structural Verification Checklist
- [ ] All three pseudocode components exist and have numbered lines.
- [ ] Line references in the traceability table above are concrete (not placeholder ranges).
- [ ] No unnumbered or out-of-sequence lines.
- [ ] `@plan` and `@requirement` markers present in each component.
- [ ] Tracker updated.

## Semantic Verification Checklist (Mandatory)
- [ ] State reducer pseudocode covers: enter/exit mode, `IssueFocus` cycling (NOT `PaneFocus`), list/detail loading with scope guard, stale-scope response discard, filter/search, Esc precedence (all 6 levels), selection rules, detail subfocus cycling (with and without comments), inline state transitions (exclusivity guard), scope change invalidation, draft discard with notice, empty states.
- [ ] GitHub client pseudocode covers: auth check, `list_issues()` (with filter args), `get_issue_detail()`, comments pagination, `create_comment()`, `update_comment()`, `update_issue_body()`, error categorization (all 7 `GhError` variants), `build_send_payload()` (with `issue_base_prompt`).
- [ ] Key routing pseudocode covers: 7-level priority chain, suppression rules (all 4 keys), inline key handling, agent chooser keys, search input keys, filter controls keys, scope change handler (with draft discard), reply `@author` prefill, exclusivity guard, `InputMode` resolution for all 5 issues variants.
- [ ] Feature behavior is reachable from real app flow: pseudocode chain from key press to state mutation to UI render is traceable.
- [ ] No placeholder pseudocode remains (no "...", "handle other cases", "etc.").
- [ ] Terminology consistent with glossary throughout.

## Deferred Implementation Detection (Mandatory)

```bash
grep -RIn "TODO\|FIXME\|HACK\|placeholder\|for now\|will be implemented\|\.\.\." project-plans/issue15/analysis/pseudocode/
```

## Success Criteria
- [ ] Pseudocode verification pass.
- [ ] All traceability table entries checked with confirmed line ranges.
- [ ] Feature-specific behavioral checks pass.
- [ ] Terminology consistent.

## Failure Recovery
- rollback steps: Update pseudocode to address gaps, then re-run P02A.
- blocking issues: missing flow coverage, unnumbered lines, placeholder algorithms, terminology inconsistencies.

## Phase Completion Marker
Create: `project-plans/issue15/.completed/P02A.md`

Contents:
- phase ID: `PLAN-20260329-ISSUES-MODE.P02A`
- timestamp
- completed traceability table (all 17 REQs with confirmed line ranges)
- feature-specific behavioral verification output
- terminology consistency verification output
- verification outputs
- semantic verification summary

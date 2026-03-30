# Phase 16: End-to-End Quality Gate

## Phase ID
`PLAN-20260329-ISSUES-MODE.P16`

## Prerequisites
- Required: Phase P15A completed
- Verify previous phase markers/artifacts exist: `.completed/P15.md`, `.completed/P15A.md`
- Expected files from previous phase: all implementation complete, all integration tests passing, zero deferred patterns

## Requirements Implemented (Expanded)

### REQ-ISS-NFR-001: Responsiveness
**Requirement text**: Issue list and detail loading must not block keyboard input. Loading states shown during API operations.

Behavior contract:
- GIVEN any API call in progress
- WHEN user presses keys
- THEN keyboard input is processed immediately; loading state is visible; no blocking

Why it matters:
- A blocked keyboard during API calls makes the app appear frozen; responsiveness is a baseline usability requirement.

### REQ-ISS-NFR-002: Reliability
**Requirement text**: API failures must not crash the application. Mode and focus remain stable through errors.

Behavior contract:
- GIVEN any error condition
- WHEN error occurs
- THEN application does not panic, crash, or corrupt state

Why it matters:
- Any unhandled error path that panics produces a worse outcome than the original API failure.

### REQ-ISS-NFR-003: Maintainability
**Requirement text**: GitHub client boundary is isolated and testable. Issue state management follows existing event/reducer pattern.

Behavior contract:
- GIVEN complete implementation
- WHEN code is reviewed for architecture integrity
- THEN GitHub module has no forbidden imports, state management uses `AppState::apply(AppEvent)` pattern, no `*v2` files exist, `PaneFocus` is unchanged, `IssueFocus` is separate

Why it matters:
- Architecture violations that pass tests today become load-bearing technical debt that blocks future changes.

### REQ-ISS-001 through REQ-ISS-014: Full Coverage Verification
**Requirement text**: All 14 functional requirements are implemented, tested, and reachable from real app flow.

Behavior contract:
- GIVEN complete implementation
- WHEN every requirement's behavior contract is exercised
- THEN all pass

Why it matters:
- The quality gate is the final checkpoint before marking the feature complete; all requirements must be traceable to source and to passing tests.

## Implementation Tasks

### Files to create or modify
- Final quality tests if gaps found during audit:
  - Additional edge case tests
  - Missing error path tests
  - Performance-related assertions
  - marker: `@plan PLAN-20260329-ISSUES-MODE.P16`
  - marker: `@requirement REQ-ISS-NNN` (per gap found)

### Architecture integrity audits to run
- Full test suite: `cargo fmt`, `cargo clippy`, `cargo test`
- Zero deferred implementation gate: grep for `TODO`, `FIXME`, `HACK`, `todo!()`, `unimplemented!()`
- Architecture isolation gate: no forbidden imports in `src/github/mod.rs`; no `*v2`/`*_new`/`*_old` files
- Enum integrity gate: `PaneFocus` exactly 3 variants; `IssueFocus` is separate; `ScreenMode` has `DashboardIssues`
- Requirement traceability gate: all 17 REQ IDs present in `src/`
- Plan marker gate: `@plan PLAN-20260329-ISSUES-MODE` present in `src/`

### Pseudocode traceability
- Final review against all 3 components to ensure coverage:
  - component-001 lines 01–165 (state reducer)
  - component-002 lines 01–82 (GitHub client)
  - component-003 lines 01–141 (key routing)

## Verification Commands

```bash
cargo fmt --all --check
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo test --workspace --all-features
```

```bash
# Zero deferred implementation gate
grep -RIn "TODO\|FIXME\|HACK\|placeholder\|for now\|will be implemented" src/
grep -rn "todo!()\|unimplemented!()" src/ && echo "FAIL: stubs remain" && exit 1 || echo "OK: zero stubs"

# Architecture isolation gate
grep -n "use crate::ui\|use crate::state\|use crate::app_input" src/github/mod.rs && echo "FAIL" || echo "OK: github isolated"
find src/ -name "*v2*" -o -name "*_new*" -o -name "*_old*" | head -5

# Enum integrity gate
grep -A6 "pub enum PaneFocus" src/state/mod.rs
grep -A5 "pub enum IssueFocus" src/state/mod.rs
grep -A6 "pub enum ScreenMode" src/state/mod.rs

# Requirement traceability gate
for req in REQ-ISS-001 REQ-ISS-002 REQ-ISS-003 REQ-ISS-004 REQ-ISS-005 REQ-ISS-006 REQ-ISS-007 REQ-ISS-008 REQ-ISS-009 REQ-ISS-010 REQ-ISS-011 REQ-ISS-012 REQ-ISS-013 REQ-ISS-014 REQ-ISS-NFR-001 REQ-ISS-NFR-002 REQ-ISS-NFR-003; do
  if grep -rq "$req" src/; then
    echo "OK: $req traced"
  else
    echo "MISSING: $req not traced in source"
  fi
done

# Plan marker gate
grep -rn "@plan PLAN-20260329-ISSUES-MODE" src/ | wc -l
```

### Traceability Marker Validation

This section provides explicit grep/search commands to detect missing `@plan` and `@requirement` markers in all implementation code. Every public function, struct, enum, and module that was added or modified as part of this plan must carry both markers.

```bash
# Count total @plan markers across all implementation files
echo "=== @plan marker coverage ==="
grep -rn "@plan PLAN-20260329-ISSUES-MODE" src/ | wc -l

# Count total @requirement markers across all implementation files
echo "=== @requirement marker coverage ==="
grep -rn "@requirement REQ-ISS-" src/ | wc -l

# List all public functions in implementation files that are MISSING @plan markers
# (checks that the 5 lines before each pub fn include a @plan marker)
echo "=== Public functions missing @plan markers ==="
awk '
  /\/\/\/.*@plan PLAN-20260329-ISSUES-MODE/ { found_plan=1; next }
  /pub fn / {
    if (!found_plan) print FILENAME ":" NR ": " $0
    found_plan=0
  }
  { found_plan=0 }
' src/github/mod.rs src/state/mod.rs src/app_input.rs src/input.rs src/domain/mod.rs 2>/dev/null

# List all public functions in implementation files that are MISSING @requirement markers
echo "=== Public functions missing @requirement markers ==="
awk '
  /\/\/\/.*@requirement REQ-ISS-/ { found_req=1; next }
  /pub fn / {
    if (!found_req) print FILENAME ":" NR ": " $0
    found_req=0
  }
  { found_req=0 }
' src/github/mod.rs src/state/mod.rs src/app_input.rs src/input.rs src/domain/mod.rs 2>/dev/null

# Check each new/modified file individually for @plan presence
echo "=== Per-file @plan marker check ==="
for file in \
  src/github/mod.rs \
  src/state/mod.rs \
  src/domain/mod.rs \
  src/input.rs \
  src/app_input.rs \
  src/ui/screens/issues.rs \
  src/ui/components/issue_list.rs \
  src/ui/components/issue_detail.rs \
  src/ui/components/filter_controls.rs \
  src/ui/components/agent_chooser.rs; do
  count=$(grep -c "@plan PLAN-20260329-ISSUES-MODE" "$file" 2>/dev/null || echo 0)
  if [ "$count" -eq 0 ]; then
    echo "MISSING @plan: $file"
  else
    echo "OK ($count markers): $file"
  fi
done

# Check each new/modified file individually for @requirement presence
echo "=== Per-file @requirement marker check ==="
for file in \
  src/github/mod.rs \
  src/state/mod.rs \
  src/domain/mod.rs \
  src/input.rs \
  src/app_input.rs \
  src/ui/screens/issues.rs \
  src/ui/components/issue_list.rs \
  src/ui/components/issue_detail.rs \
  src/ui/components/filter_controls.rs \
  src/ui/components/agent_chooser.rs; do
  count=$(grep -c "@requirement REQ-ISS-" "$file" 2>/dev/null || echo 0)
  if [ "$count" -eq 0 ]; then
    echo "MISSING @requirement: $file"
  else
    echo "OK ($count markers): $file"
  fi
done

# Verify @pseudocode markers are present in all implementation files
echo "=== Per-file @pseudocode marker check ==="
for file in \
  src/github/mod.rs \
  src/state/mod.rs \
  src/app_input.rs; do
  count=$(grep -c "@pseudocode component-" "$file" 2>/dev/null || echo 0)
  if [ "$count" -eq 0 ]; then
    echo "MISSING @pseudocode: $file"
  else
    echo "OK ($count markers): $file"
  fi
done
```

**Pass criteria**: Every file in the per-file checks above must report `OK`. Any `MISSING` line is a blocking failure. The public-function sweep should produce zero output (no missing markers). The total `@plan` count across `src/` must be ≥ total number of new public functions introduced by this plan.

## Structural Verification Checklist
- [ ] `cargo fmt --all --check` passes
- [ ] `cargo clippy --workspace --all-targets --all-features -- -D warnings` passes
- [ ] `cargo test --workspace --all-features` passes with zero failures
- [ ] Zero deferred implementation patterns in `src/`
- [ ] Zero `todo!()`/`unimplemented!()` in `src/`
- [ ] Zero architecture forks (`*v2`, `*_new`, `*_old` files)
- [ ] `PaneFocus` is unchanged (exactly `Repositories`, `Agents`, `Terminal`)
- [ ] `IssueFocus` is separate enum (not in `PaneFocus`)
- [ ] `ScreenMode` has `DashboardIssues` variant
- [ ] All 17 requirement IDs traced to source code
- [ ] Plan markers present in source code (`@plan` count ≥ expected)
- [ ] `@requirement` markers present in all 10 new/modified implementation files
- [ ] `@pseudocode` markers present in `src/github/mod.rs`, `src/state/mod.rs`, `src/app_input.rs`

## Semantic Verification Checklist (Mandatory)
- [ ] **REQ-ISS-001**: Mode entry via `i` (state → `ScreenMode::DashboardIssues`), exit via `a` (state → `ScreenMode::Dashboard`), `IssuesState` clean on exit
- [ ] **REQ-ISS-002**: Key suppression works (`s`, `Ctrl-d`, `Ctrl-k`, `l` no-op in issues mode); priority chain correct (7 levels)
- [ ] **REQ-ISS-003**: Pane focus cycling works (Tab cycles `RepoList → IssueList → IssueDetail → RepoList` via `IssueFocus`); navigation works per domain
- [ ] **REQ-ISS-004**: Esc precedence chain works at all 6 levels (inline, chooser, search-clear, search-blur, filter, exit)
- [ ] **REQ-ISS-005**: Exit focus restoration works (valid target restored; stale target falls back to default)
- [ ] **REQ-ISS-006**: Issue list displays all 8 fields; selection/sorting correct
- [ ] **REQ-ISS-007**: Pagination auto-loads at boundary; comment pagination appends without reorder
- [ ] **REQ-ISS-008**: Filter/search composition works; Apply/Clear/Cancel
- [ ] **REQ-ISS-009**: Detail displays all fields; comments timeline; `external_url` displayed as reference link
- [ ] **REQ-ISS-010**: Inline create/edit works; exclusivity enforced; save/cancel; @author prefill on reply
- [ ] **REQ-ISS-011**: Send-to-agent works; payload complete with `issue_base_prompt`; no-agent case handled
- [ ] **REQ-ISS-012**: `issue_base_prompt` persists; form field works (multiline, Save, Reset)
- [ ] **REQ-ISS-013**: Auth check works; error handling robust; no crash; draft preserved on error; draft discarded on scope change; stale-scope responses suppressed
- [ ] **REQ-ISS-014**: All empty states display correct messages (no issues, no comments, no agents)
- [ ] **REQ-ISS-NFR-001**: Loading states shown; no blocking
- [ ] **REQ-ISS-NFR-002**: No crash on any error
- [ ] **REQ-ISS-NFR-003**: Module isolation; event/reducer pattern; `PaneFocus` unchanged; `IssueFocus` separate
- [ ] Feature behavior is reachable from real app flow: complete user journey works end-to-end

## Deferred Implementation Detection (Mandatory)

```bash
grep -RIn "TODO\|FIXME\|HACK\|placeholder\|for now\|will be implemented" src/
```

## Success Criteria
- [ ] All quality gates pass
- [ ] All semantic verification checks pass (all 17 REQs)
- [ ] Full test suite GREEN
- [ ] Zero deferred patterns
- [ ] All 17 requirements traced and verified in source
- [ ] Traceability Marker Validation passes (zero MISSING lines in per-file checks)

## Failure Recovery
- rollback steps: Fix quality issues found; do not proceed to P16A until all gates pass
- blocking issues: any failing gate; any untraceable requirement; any deferred pattern; any file with missing `@plan` or `@requirement` markers

## Phase Completion Marker
Create: `project-plans/issue15/.completed/P16.md`

Contents:
- phase ID: `PLAN-20260329-ISSUES-MODE.P16`
- timestamp
- quality gate audit results (all gates)
- full test suite result summary
- requirement traceability table (all 17 REQs)
- architecture integrity audit output
- traceability marker validation output (per-file @plan and @requirement counts)
- verification commands outputs
- semantic verification summary (all 17 REQs checked)

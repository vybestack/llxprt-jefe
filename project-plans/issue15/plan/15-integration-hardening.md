# Phase 15: Integration Hardening

## Phase ID
`PLAN-20260329-ISSUES-MODE.P15`

## Prerequisites
- Required: Phase P14A completed
- Verify previous phase markers/artifacts exist: `.completed/P14.md`, `.completed/P14A.md`
- Expected files from previous phase: implemented domain, state, GitHub client, key routing, UI, and persistence (all tests GREEN)

## Requirements Implemented (Expanded)

### REQ-ISS-001: End-to-End Mode Lifecycle
**Requirement text**: Full mode lifecycle: enter via `i`, browse issues, interact, exit via `a` or Esc. State is clean on entry and exit.

Behavior contract:
- GIVEN application in agents mode (`ScreenMode::Dashboard`) with repository selected
- WHEN user presses `i`, navigates issues, creates a comment, exits via `a`
- THEN mode transitions are clean, `IssuesState` is cleared on exit, agents mode resumes with prior focus restored

Why it matters:
- A leaking `IssuesState` on exit causes residual data to appear on re-entry and can corrupt agent-mode focus state.

### REQ-ISS-002: Key Routing Integration Correctness
**Requirement text**: All key bindings work correctly in context. No key conflicts. Suppression rules hold.

Behavior contract:
- GIVEN all key routing is implemented
- WHEN integration tests exercise every key binding in every focus domain (repo_list, issue_list, issue_detail)
- THEN correct events fire, correct state transitions occur, no unexpected side effects

Why it matters:
- Unit tests on individual handlers cannot catch cross-domain conflicts; only integrated key routing tests prove no binding collides or leaks.

### REQ-ISS-004: Esc Precedence Chain Integration
**Requirement text**: All 6 levels of Esc handling work in integrated context.

Behavior contract:
- GIVEN issues mode with inline editor active + search focused + filter open
- WHEN Esc is pressed repeatedly
- THEN cancels inline first, then clears search, then blurs search, then closes filter, then exits mode — each level independently

Why it matters:
- Esc chain correctness depends on state ordering; integration testing with real state is the only way to verify precedence under all combinations.

### REQ-ISS-005: Exit-Focus Restoration Integration
**Requirement text**: Prior focus restored on exit, validated for staleness.

Behavior contract:
- GIVEN user was on agent "bot-3" before entering Issues Mode
- WHEN user exits Issues Mode and "bot-3" still exists
- THEN focus restores to "bot-3" with correct visual state

- GIVEN user was on agent "deleted-bot" before entering Issues Mode
- WHEN user exits Issues Mode and "deleted-bot" no longer exists
- THEN focus falls back to default agent list focus

Why it matters:
- Stale focus restoration without existence validation causes index-out-of-bounds panics or wrong agent highlighted.

### REQ-ISS-007: Pagination Integration
**Requirement text**: Issue list and comment pagination work end-to-end.

Behavior contract:
- GIVEN issue list with 20 loaded items and has_more=true
- WHEN user navigates to item 20
- THEN next page auto-loads, items append, selection unchanged, no UI flicker

Why it matters:
- Pagination boundary detection requires the state, key routing, and GitHub client to coordinate; integration tests are the only meaningful coverage.

### REQ-ISS-010: Inline Exclusivity Integration
**Requirement text**: At most one inline control active at a time.

Behavior contract:
- GIVEN inline editor active for issue body
- WHEN `r` is pressed on a comment
- THEN `r` is consumed by existing inline handler (exclusivity guard), reply does NOT open

Why it matters:
- Exclusivity failures allow two simultaneous input areas which can corrupt state and produce undefined save behavior.

### REQ-ISS-013: Error Handling Integration
**Requirement text**: API failures do not crash. Mode and focus remain stable. Drafts preserved on error. Draft discarded on scope change. Stale-scope responses suppressed.

Behavior contract:
- GIVEN inline composer with draft text
- WHEN `create_comment()` fails with `GhError::RateLimited`
- THEN error message displayed, draft preserved, mode remains, retry possible

- GIVEN inline composer active with draft text "my comment"
- WHEN user navigates to a different repository
- THEN draft is discarded, inline is cancelled, non-blocking notice shown, issue list reloads for new scope

- GIVEN issues mode with repo "acme/api" selected, list loading in progress
- WHEN user switches to repo "acme/web" before loading completes
- THEN the stale response for "acme/api" is discarded; only "acme/web" data is displayed

Why it matters:
- Error resilience and stale-scope suppression are correctness requirements; failures here crash the app or display wrong-repository data.

### REQ-ISS-NFR-001: Responsiveness
**Requirement text**: Issue list and detail loading must not block keyboard input. Loading states shown.

Behavior contract:
- GIVEN issue list loading in progress
- WHEN user presses Esc
- THEN mode exit occurs immediately (not blocked by loading)

Why it matters:
- Blocking keyboard input during API calls degrades UX and can make the application appear frozen.

### REQ-ISS-NFR-002: Reliability
**Requirement text**: API failures must not crash the application. Mode and focus remain stable through errors.

Behavior contract:
- GIVEN any `GhError` variant
- WHEN it occurs during any operation
- THEN application does not crash; error is displayed; mode/focus remain stable

Why it matters:
- A panic in error handling is worse than the original error; all `GhError` variants must be handled without unwrap.

## Implementation Tasks

### Files to create or modify
- Integration tests (inline or in integration test module):
  - `test_mode_lifecycle_enter_browse_exit` — REQ-ISS-001
    - marker: `@plan PLAN-20260329-ISSUES-MODE.P15`, `@requirement REQ-ISS-001`
  - `test_mode_lifecycle_enter_interact_exit` — REQ-ISS-001
    - marker: `@plan PLAN-20260329-ISSUES-MODE.P15`, `@requirement REQ-ISS-001`
  - `test_key_routing_all_focus_domains` — REQ-ISS-002
    - marker: `@plan PLAN-20260329-ISSUES-MODE.P15`, `@requirement REQ-ISS-002`
  - `test_key_routing_suppression_comprehensive` — REQ-ISS-002
    - marker: `@plan PLAN-20260329-ISSUES-MODE.P15`, `@requirement REQ-ISS-002`
  - `test_error_handling_rate_limit_preserves_draft` — REQ-ISS-013,NFR-002
    - marker: `@plan PLAN-20260329-ISSUES-MODE.P15`, `@requirement REQ-ISS-013,NFR-002`
  - `test_error_handling_auth_failure_blocks_ops` — REQ-ISS-013
    - marker: `@plan PLAN-20260329-ISSUES-MODE.P15`, `@requirement REQ-ISS-013`
  - `test_error_handling_network_error_stable_mode` — REQ-ISS-013,NFR-002
    - marker: `@plan PLAN-20260329-ISSUES-MODE.P15`, `@requirement REQ-ISS-013,NFR-002`
  - `test_pagination_issue_list_auto_load` — REQ-ISS-007
    - marker: `@plan PLAN-20260329-ISSUES-MODE.P15`, `@requirement REQ-ISS-007`
  - `test_pagination_comments_append` — REQ-ISS-007
    - marker: `@plan PLAN-20260329-ISSUES-MODE.P15`, `@requirement REQ-ISS-007`
  - `test_exit_focus_restoration_valid` — REQ-ISS-005
    - marker: `@plan PLAN-20260329-ISSUES-MODE.P15`, `@requirement REQ-ISS-005`
  - `test_exit_focus_restoration_stale` — REQ-ISS-005
    - marker: `@plan PLAN-20260329-ISSUES-MODE.P15`, `@requirement REQ-ISS-005`
  - `test_scope_change_invalidation` — REQ-ISS-001,013
    - marker: `@plan PLAN-20260329-ISSUES-MODE.P15`, `@requirement REQ-ISS-001,013`
  - `test_stale_scope_response_suppressed` — REQ-ISS-013
    - marker: `@plan PLAN-20260329-ISSUES-MODE.P15`, `@requirement REQ-ISS-013`
  - `test_inline_exclusivity_all_combinations` — REQ-ISS-010
    - marker: `@plan PLAN-20260329-ISSUES-MODE.P15`, `@requirement REQ-ISS-010`
  - `test_draft_discard_on_scope_change` — REQ-ISS-013
    - marker: `@plan PLAN-20260329-ISSUES-MODE.P15`, `@requirement REQ-ISS-013`
  - `test_send_to_agent_payload_complete` — REQ-ISS-011
    - marker: `@plan PLAN-20260329-ISSUES-MODE.P15`, `@requirement REQ-ISS-011`
  - `test_send_to_agent_no_agents` — REQ-ISS-011,014
    - marker: `@plan PLAN-20260329-ISSUES-MODE.P15`, `@requirement REQ-ISS-011,014`
  - `test_issue_base_prompt_in_payload` — REQ-ISS-011,012
    - marker: `@plan PLAN-20260329-ISSUES-MODE.P15`, `@requirement REQ-ISS-011,012`
  - `test_esc_chain_all_six_levels_integrated` — REQ-ISS-004
    - marker: `@plan PLAN-20260329-ISSUES-MODE.P15`, `@requirement REQ-ISS-004`

- Fix integration issues found during testing:
  - `src/state/mod.rs` — Fix any state transition edge cases found
  - `src/app_input.rs` — Fix any key routing integration issues
  - `src/github/mod.rs` — Fix any error handling gaps
  - `src/ui/` — Fix any rendering issues found during integration
  - All fixes must maintain `@plan`, `@requirement` markers

### Pseudocode traceability
- Uses pseudocode lines:
  - component-001 lines 33–51: `enter_issues_mode` → `exit_issues_mode` round-trip
  - component-001 lines 41–51: valid/stale prior focus restoration
  - component-001 lines 61–70: issue list pagination boundary detection
  - component-001 lines 84–85: stale-scope `IssueListLoaded` discard
  - component-001 lines 109–114: comment pagination append order
  - component-001 lines 115–127: Esc precedence chain (6 levels)
  - component-001 lines 116–117: inline exclusivity guard
  - component-002 lines 04–08: `GhError::NotAuthenticated` → block operations
  - component-002 lines 19–21: `GhError::RateLimited` → draft preserved
  - component-002 lines 62–74: `build_send_payload` with `issue_base_prompt`
  - component-002 lines 79–86: `GhError::NetworkError` → no crash, mode stable
  - component-003 lines 01–72: priority chain + per-domain dispatch for all 3 focus domains
  - component-003 lines 33–38: suppressed keys consumed as no-op
  - component-003 lines 80–101: enter → inline submit → exit flow
  - component-003 lines 97–102: navigate to boundary → next page loads
  - component-003 lines 102–111: send-to-agent payload assembly
  - component-003 lines 128–135: repo switch clears all issues state, triggers reload
  - component-003 lines 138–141: inline exclusivity guard blocks concurrent open attempts

## Verification Commands

```bash
cargo fmt --all --check
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo test --workspace --all-features
```

## Structural Verification Checklist
- [ ] All 19 integration tests exist and pass
- [ ] All prior tests still pass (zero regressions)
- [ ] No new compilation warnings
- [ ] `@plan`, `@requirement` markers present in all new test code

## Semantic Verification Checklist (Mandatory)
- [ ] Full mode lifecycle works: enter (`i`) → browse → interact → exit (`a`/`Esc`)
- [ ] All key bindings work in all focus domains (repo_list, issue_list, issue_detail) without conflicts
- [ ] Suppressed keys (`s`, `Ctrl-d`, `Ctrl-k`, `l`) produce no state change in issues mode
- [ ] Esc chain works at all 6 levels in integrated context (inline, chooser, search-clear, search-blur, filter, exit)
- [ ] Error handling is robust for all `GhError` variants (rate limit, auth, access denied, parse, network)
- [ ] Draft preservation on error: inline draft is preserved when API call fails
- [ ] Draft discard on scope change: active inline is cancelled with notice when repo changes
- [ ] Stale-scope response suppression: response with wrong repo ID is discarded
- [ ] Pagination works for both issue list (auto-load at boundary) and comments (append, stable order)
- [ ] Focus restoration works for valid (exact match) and stale (fallback) targets
- [ ] Scope change properly invalidates all issues state
- [ ] Send-to-agent payload is complete with all fields including `issue_base_prompt`
- [ ] Inline exclusivity guard works for all combinations (editor vs composer, concurrent attempts)
- [ ] Feature behavior is reachable from real app flow: integration tests prove the full flow from key input to UI state works
- [ ] No placeholder/deferred implementation patterns remain anywhere in `src/`

## Deferred Implementation Detection (Mandatory)

```bash
grep -RIn "TODO\|FIXME\|HACK\|placeholder\|for now\|will be implemented" src/
grep -rn "todo!()\|unimplemented!()" src/ && echo "FAIL: stubs remain" && exit 1 || echo "OK: zero stubs"
```

## Success Criteria
- [ ] All integration tests GREEN (19 tests)
- [ ] All prior tests GREEN (full regression)
- [ ] All semantic checks pass
- [ ] Zero deferred implementation patterns in `src/`
- [ ] No crash or panic on any GhError variant
- [ ] Existing state.json loads without error (backward compat preserved)

## Failure Recovery
- rollback steps: Revert individual file fixes with `git restore`; do not revert entire phases
- blocking issues: integration failures, crash on error, key routing conflicts, state corruption

## Phase Completion Marker
Create: `project-plans/issue15/.completed/P15.md`

Contents:
- phase ID: `PLAN-20260329-ISSUES-MODE.P15`
- timestamp
- files changed
- tests added: 19 integration test names
- deferred-impl grep gate output (must be zero)
- full test suite summary (total pass/fail count)
- verification command outputs
- semantic verification summary

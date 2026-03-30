# Phase 08: GitHub Client Boundary Implementation

## Phase ID
`PLAN-20260329-ISSUES-MODE.P08`

## Prerequisites
- Required: Phase P07A completed
- Verify previous phase markers/artifacts exist: `.completed/P07.md`, `.completed/P07A.md`
- Expected files from previous phase: failing (RED) test suite for GitHub client boundary (18 tests) in `src/github/mod.rs`

## Requirements Implemented (Expanded)

### REQ-ISS-013: Authentication and Error Handling — Implementation
**Requirement text**: v1 uses active `gh` CLI auth context. Missing/invalid auth blocks operations with remediation guidance. Non-auth errors: scoped error in list/detail, stable mode/focus, draft preservation, retry affordance.

Behavior contract:
- GIVEN `gh` CLI is available and authenticated
- WHEN `list_issues()` is called with filter params
- THEN subprocess runs with correct arguments, stdout is parsed to `Vec<Issue>`, sorted by `updated_at` desc with `number` asc tie-breaker

- GIVEN `gh` CLI is not installed
- WHEN any client method is called
- THEN returns `Err(GhError::NotInstalled)` with install guidance

- GIVEN `gh auth status` returns non-zero exit
- WHEN `check_auth()` is called
- THEN returns `Err(GhError::NotAuthenticated(...))` with "run `gh auth login`" guidance

Why it matters:
- This is the data transport layer — all issue display and interaction depends on it working correctly; auth failures must surface actionable remediation, not raw exit codes

### REQ-ISS-006: Issue List Loading — Implementation
**Requirement text**: Each row: number, title, state, author, updated timestamp, assignee summary, label summary, comment count. Default sort: `updated_at` desc, tie-breaker `number` asc.

Behavior contract:
- GIVEN authenticated `gh` CLI and a repository with issues
- WHEN `list_issues()` is called
- THEN returns `Vec<Issue>` sorted by `updated_at` desc, `number` asc tie-break; `has_more` is true iff result count equals page_size

Why it matters:
- Deterministic sort order is required for stable pagination and consistent UI

### REQ-ISS-008: Filter Arg Mapping — Implementation
**Requirement text**: Supported filters: text query, state, author, assignee, labels (multi AND), mentioned. Structured filters AND-composed.

Behavior contract:
- GIVEN `IssueFilter` with state, labels, author, assignee, mentioned, search fields populated
- WHEN CLI args are constructed
- THEN each field maps to correct `gh` CLI flag; label list generates one `--label` flag per entry

Why it matters:
- Incorrect flag mapping silently returns the wrong issue set

### REQ-ISS-007: Pagination — Implementation
**Requirement text**: Lists paginated/lazy-loaded. Comment pagination appends in stable order. Failure retains loaded comments and exposes retry.

Behavior contract:
- GIVEN `list_comments()` called with a page cursor
- WHEN response returns N items where N equals page_size
- THEN `has_more=true`; next call with cursor appends without reordering prior results

Why it matters:
- Pagination correctness determines whether long threads are complete and stable

### REQ-ISS-009: Issue Detail — Implementation
**Requirement text**: Detail displays all fields including optional milestone. `external_url` is displayed as a reference link.

Behavior contract:
- GIVEN a valid GitHub issue number
- WHEN `get_issue_detail()` is called
- THEN returns `IssueDetail` with all fields populated including `external_url`

Why it matters:
- The detail view is the primary reading surface; all fields including the external URL link must be present

### REQ-ISS-010: Mutation Operations — Implementation
**Requirement text**: Create comment, update comment, update issue body.

Behavior contract:
- GIVEN valid comment body
- WHEN `create_comment()` succeeds
- THEN returns parsed `IssueComment` with assigned ID from API response

- GIVEN valid body text and comment ID
- WHEN `update_comment()` succeeds
- THEN returns `Ok(())`

- GIVEN valid body text and issue number
- WHEN `update_issue_body()` succeeds
- THEN returns `Ok(())`

Why it matters:
- Mutations are the write path; partial or silent failures leave the UI and API out of sync

### REQ-ISS-011: Send Payload Composition — Implementation
**Requirement text**: Payload includes repo identifier, issue data, focused comment, `issue_base_prompt`.

Behavior contract:
- GIVEN issue detail and optional focused comment
- WHEN `build_send_payload()` is called
- THEN `SendPayload` contains: repository slug from `repo` argument (no silent substitution), issue_number, issue_title, issue_body, issue_state, issue_labels, issue_assignees, optional focused_comment + focused_comment_author, issue_base_prompt

Why it matters:
- Every field in the payload enables the agent to act with full context; omission silently degrades agent quality

## Implementation Tasks

### Files to modify
- `src/github/mod.rs` — implement all client methods; remove all `todo!()` stubs:
  - `check_auth()`: run `gh auth status`, parse exit code, return `Ok` or `GhError::NotAuthenticated`
    - marker: `@plan PLAN-20260329-ISSUES-MODE.P08`
    - marker: `@requirement REQ-ISS-013`
    - marker: `@pseudocode component-002 lines 04-08`
  - `list_issues()`: build args from `IssueFilter`, run `gh issue list --json ...`, parse JSON, sort by `updated_at` desc / `number` asc, determine `has_more`
    - marker: `@plan PLAN-20260329-ISSUES-MODE.P08`
    - marker: `@requirement REQ-ISS-006`
    - marker: `@requirement REQ-ISS-008`
    - marker: `@pseudocode component-002 lines 09-25`
  - `get_issue_detail()`: run `gh issue view --json ...`, parse JSON to `IssueDetail`
    - marker: `@plan PLAN-20260329-ISSUES-MODE.P08`
    - marker: `@requirement REQ-ISS-009`
    - marker: `@pseudocode component-002 lines 26-32`
  - `list_comments()`: run `gh api /repos/{owner}/{repo}/issues/{number}/comments`, parse JSON, handle pagination
    - marker: `@plan PLAN-20260329-ISSUES-MODE.P08`
    - marker: `@requirement REQ-ISS-007`
    - marker: `@pseudocode component-002 lines 33-43`
  - `create_comment()`: run `gh issue comment`, verify success, parse result
    - marker: `@plan PLAN-20260329-ISSUES-MODE.P08`
    - marker: `@requirement REQ-ISS-010`
    - marker: `@pseudocode component-002 lines 44-48`
  - `update_comment()`: run `gh api --method PATCH`, verify success
    - marker: `@plan PLAN-20260329-ISSUES-MODE.P08`
    - marker: `@requirement REQ-ISS-010`
    - marker: `@pseudocode component-002 lines 49-56`
  - `update_issue_body()`: run `gh issue edit --body`, verify success
    - marker: `@plan PLAN-20260329-ISSUES-MODE.P08`
    - marker: `@requirement REQ-ISS-010`
    - marker: `@pseudocode component-002 lines 57-61`
  - `build_send_payload()`: compose `SendPayload` from inputs; embed repo slug from argument
    - marker: `@plan PLAN-20260329-ISSUES-MODE.P08`
    - marker: `@requirement REQ-ISS-011`
    - marker: `@pseudocode component-002 lines 62-74`
  - Error categorization helper: parse stderr for rate limit, auth, access, network patterns
    - marker: `@plan PLAN-20260329-ISSUES-MODE.P08`
    - marker: `@requirement REQ-ISS-013`
    - marker: `@pseudocode component-002 lines 75-82`
  - **Remove all `todo!()` stubs** — this is a hard gate
  - Module-level doc comment must include `@plan PLAN-20260329-ISSUES-MODE.P08`

### Pseudocode traceability (if impl phase)
- Uses pseudocode component-002 lines 04-82 (full client implementation)
  - lines 01-03: `GhClient` struct definition and threading contract
  - lines 04-08: `check_auth` implementation
  - lines 09-25: `list_issues` (args building, subprocess, parsing, sorting, pagination)
  - lines 26-32: `get_issue_detail` (subprocess, parsing)
  - lines 33-43: `list_comments` (API endpoint, parsing, pagination)
  - lines 44-48: `create_comment` (subprocess, success verification)
  - lines 49-56: `update_comment` (PATCH API, success verification)
  - lines 57-61: `update_issue_body` (edit subprocess)
  - lines 62-74: `build_send_payload` (field mapping)
  - lines 75-82: `GhError` enum variants and categorization

## Verification Commands

```bash
cargo fmt --all --check
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo test --workspace --all-features
```

## Structural Verification Checklist
- [ ] All P07 RED tests now pass (GREEN)
- [ ] No `todo!()` or `unimplemented!()` remain in `src/github/mod.rs`
- [ ] Phase/requirement/pseudocode markers present for each method
- [ ] No new compilation warnings
- [ ] All existing tests pass (zero regressions)
- [ ] `src/github/mod.rs` has no imports from `crate::ui`, `crate::state`, or `crate::app_input`
- [ ] All methods use domain types from `crate::domain`

## Semantic Verification Checklist (Mandatory)
- [ ] Auth check correctly identifies authenticated/unauthenticated states; `GhError::NotInstalled` returned when `gh` is absent (no panic)
- [ ] Issue list parsing handles all JSON fields correctly
- [ ] Sorting is `updated_at` desc, `number` asc tie-break
- [ ] Filter args correctly map all `IssueFilter` fields to CLI arguments (state, author, assignee, labels, mentioned, search)
- [ ] `has_more` detection based on result count equals page_size
- [ ] Comments pagination logic works (page append, `has_more` detection from result count)
- [ ] Mutation operations construct correct CLI commands with proper argument escaping
- [ ] Error categorization correctly identifies: rate limit, auth failure, access denied, parse errors, network errors
- [ ] Send payload `repository` field is the repo slug from the argument (no silent global substitution)
- [ ] `list_issues()` does not cache results between calls — each call is a fresh subprocess
- [ ] Each error variant is returned to caller with no side-effect mutation inside `GhClient`
- [ ] Error messages contain actionable remediation text (e.g., "run `gh auth login`"), not raw exit codes
- [ ] Feature behavior is reachable from real app flow: `GhClient` methods produce typed results that map to state events
- [ ] No placeholder/deferred implementation patterns remain

## Deferred Implementation Detection (Mandatory)

```bash
grep -RIn "TODO\|FIXME\|HACK\|placeholder\|for now\|will be implemented" src/github/
grep -rn "todo!()\|unimplemented!()" src/github/ && echo "FAIL: stubs remain" && exit 1 || echo "OK: no stubs"
```

## Success Criteria
- [ ] All GitHub client tests GREEN (all 18 P07 tests pass)
- [ ] Verification commands pass
- [ ] No placeholder code remains (hard gate)
- [ ] Semantic checks pass

## Failure Recovery
- rollback steps: `git restore src/github/`
- blocking issues: `gh` CLI argument incompatibility, JSON schema mismatch, error categorization gaps

## Phase Completion Marker
Create: `project-plans/issue15/.completed/P08.md`

Contents:
- phase ID: `PLAN-20260329-ISSUES-MODE.P08`
- timestamp
- files changed
- tests that went from RED to GREEN (list)
- no-stub verification output
- module isolation verification output
- verification command outputs
- semantic verification summary

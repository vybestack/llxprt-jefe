# Phase 06A: GitHub Client Boundary Stub Verification

## Phase ID
`PLAN-20260329-ISSUES-MODE.P06A`

## Prerequisites
- Required: Phase P06 completed.
- Verify previous artifacts: `.completed/P06.md` exists.
- Expected files from previous phase: `src/github/mod.rs` with full method signatures and stubs.

## Requirements Implemented (Expanded)

### Verification of GitHub Client Stub Correctness for REQ-ISS-011,013
**Requirement text**: Confirm stubs compile, have correct signatures matching pseudocode, and maintain module isolation.

Behavior contract:
- GIVEN compile-safe GitHub client stubs
- WHEN verification is executed
- THEN all methods have correct signatures, `GhError` has all 7 variants, module has no forbidden imports, all existing tests pass, and traceability markers are present.

## Implementation Tasks

### Files to create
- `project-plans/issue15/.completed/P06A.md`
  - marker: `@plan PLAN-20260329-ISSUES-MODE.P06A`

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

### Module Isolation Verification
```bash
grep -n "use crate::ui\|use crate::state\|use crate::app_input" src/github/mod.rs && echo "FAIL: github module has forbidden imports" || echo "OK: github module isolation verified"
```

### Method Signature Verification
```bash
# Verify all expected method signatures exist
grep -n "fn check_auth\|fn list_issues\|fn get_issue_detail\|fn list_comments\|fn create_comment\|fn update_comment\|fn update_issue_body\|fn build_send_payload" src/github/mod.rs
```

### Error Type Verification
```bash
# Verify all GhError variants
grep -n "NotAuthenticated\|NotInstalled\|RateLimited\|AccessDenied\|ApiError\|ParseError\|NetworkError" src/github/mod.rs
```

### SendPayload Field Verification
```bash
# Verify SendPayload has all required fields
echo "--- SendPayload struct ---"
grep -A20 "pub struct SendPayload" src/github/mod.rs
```

### Traceability Marker Verification
```bash
# Verify @plan, @requirement, @pseudocode markers in github module — per function
echo "--- Markers per function ---"
grep -B5 "pub fn\|pub struct\|pub enum" src/github/mod.rs | grep "@plan\|@requirement\|@pseudocode\|pub fn\|pub struct\|pub enum"
```

## Structural Verification Checklist
- [ ] All 7 method stubs compile (+ build_send_payload = 8 total).
- [ ] `GhError` has all 7 variants.
- [ ] `IssueListResponse` and `CommentsResponse` types exist.
- [ ] `SendPayload` struct exists with all required fields.
- [ ] Module isolation verified.
- [ ] `@plan`, `@requirement`, `@pseudocode` markers present for each method.
- [ ] Tracker updated.

## Semantic Verification Checklist (Mandatory)
- [ ] Method signatures match pseudocode contracts (component-002 lines 04-74): parameter types and return types verified against pseudocode line by line.
  - `check_auth() -> Result<(), GhError>` — component-002 lines 04-08
  - `list_issues(owner, repo, filter, cursor, page_size) -> Result<IssueListResponse, GhError>` — component-002 lines 09-25
  - `get_issue_detail(owner, repo, number) -> Result<IssueDetail, GhError>` — component-002 lines 26-32
  - `list_comments(owner, repo, number, cursor, page_size) -> Result<CommentsResponse, GhError>` — component-002 lines 33-43
  - `create_comment(owner, repo, number, body) -> Result<IssueComment, GhError>` — component-002 lines 44-48
  - `update_comment(owner, repo, comment_id, body) -> Result<(), GhError>` — component-002 lines 49-56
  - `update_issue_body(owner, repo, number, body) -> Result<(), GhError>` — component-002 lines 57-61
  - `build_send_payload(...)  -> SendPayload` — component-002 lines 62-74
- [ ] Error types are complete (all 7 categories from component-002 lines 75-82): NotAuthenticated, NotInstalled, RateLimited, AccessDenied, ApiError, ParseError, NetworkError.
- [ ] No forbidden cross-module imports.
- [ ] `SendPayload` has all specification-required fields: repository slug, issue number, issue title, issue body, issue state, labels, assignees, focused comment body (optional), focused comment author (optional), issue_base_prompt.
- [ ] `IssueListResponse` has fields: `issues: Vec<Issue>`, `cursor: Option<String>`, `has_more: bool`.
- [ ] `CommentsResponse` has fields: `comments: Vec<IssueComment>`, `cursor: Option<String>`, `has_more: bool`.
- [ ] Feature behavior is reachable from real app flow: `GhClient` type can be imported by `app_input.rs` for use in key routing.
- [ ] Method return types align with state events: `list_issues` result maps to `IssueListLoaded` event, `get_issue_detail` to `IssueDetailLoaded`, `list_comments` to `IssueCommentsPageLoaded`, `create_comment` to `CommentCreated`.

## Deferred Implementation Detection (Mandatory)

```bash
grep -RIn "TODO\|FIXME\|HACK\|placeholder\|for now\|will be implemented" src/github/
```

Note: `todo!()` in method bodies is allowed in this stub phase. Must be removed by P08.

## Success Criteria
- [ ] Stub verification pass.
- [ ] Integration contract acceptance gates pass.
- [ ] Traceability markers present.

## Failure Recovery
- rollback steps: Fix signature mismatches against pseudocode.
- blocking issues: missing methods, missing error categories, forbidden imports.

## Phase Completion Marker
Create: `project-plans/issue15/.completed/P06A.md`

Contents:
- phase ID: `PLAN-20260329-ISSUES-MODE.P06A`
- timestamp
- method signature inventory (all 8 verified with pseudocode line references)
- module isolation verification output
- error variant verification output
- SendPayload field verification output
- traceability marker verification output
- verification outputs
- semantic verification summary

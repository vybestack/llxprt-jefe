# Phase 06: GitHub Client Boundary Stub

## Phase ID
`PLAN-20260329-ISSUES-MODE.P06`

## Prerequisites
- Required: Phase P05A completed
- Verify previous phase markers/artifacts exist: `.completed/P05.md`, `.completed/P05A.md`
- Expected files from previous phase: implemented domain + state contracts with all tests GREEN

## Requirements Implemented (Expanded)

### REQ-ISS-013: Authentication and Error Handling — Client Skeleton
**Requirement text**: v1 uses active `gh` CLI auth context. Missing/invalid auth blocks operations with remediation guidance. Non-auth errors: scoped error in list/detail, stable mode/focus, draft preservation, retry affordance. `gh` CLI not installed: block operations, show install guidance. API rate limit: scoped retry affordance.

Behavior contract:
- GIVEN: completed domain types for Issue/IssueDetail/IssueComment
- WHEN: `GhClient` method signatures are fully defined in `src/github/mod.rs`
- THEN: all method signatures exist with correct input/output types matching domain types; methods contain `todo!()` stubs; `GhError` enum covers all error categories

Why it matters:
- Establishes the boundary contract before implementing `gh` CLI subprocess logic. Ensures all callers can type-check against the boundary before implementation.

### REQ-ISS-013: Error Type Definition
**Requirement text**: Non-auth errors: scoped error in list/detail, stable mode/focus, draft preservation, retry affordance.

Behavior contract:
- GIVEN: error specification from analysis
- WHEN: `GhError` is defined with `thiserror` derive
- THEN: all error categories are representable: NotAuthenticated, NotInstalled, RateLimited, AccessDenied, ApiError, ParseError, NetworkError; all implement `Display` and `Error`

Why it matters:
- Typed errors allow callers to distinguish remediation-required failures (not installed, not authenticated) from retryable failures (rate limited, network error).

### REQ-ISS-011: Send Payload Type
**Requirement text**: `S` from issue detail when no inline control active opens agent chooser. Payload: repo identifier, issue number/title/body, metadata, focused comment (if any), `issue_base_prompt`. No-agent state: disable send, show message.

Behavior contract:
- GIVEN: specification for send-to-agent payload
- WHEN: `SendPayload` struct and `build_send_payload()` function signature are defined
- THEN: struct has all required fields and function signature accepts correct inputs

Why it matters:
- Payload type is the contract between the issues domain and the agent integration layer; must be defined before either side implements against it.

## Implementation Tasks

### Files to modify
- `src/github/mod.rs` (expand from P03 skeleton)
  - Define `GhError` enum with `thiserror` derive: NotAuthenticated, NotInstalled, RateLimited, AccessDenied, ApiError, ParseError, NetworkError
  - Define `IssueListResponse` struct: `{ issues: Vec<Issue>, cursor: Option<String>, has_more: bool }`
  - Define `CommentsResponse` struct: `{ comments: Vec<IssueComment>, cursor: Option<String>, has_more: bool }`
  - Define `SendPayload` struct with all fields from pseudocode component-002 lines 62-74
  - Expand `GhClient` struct with full method signatures:
    - `check_auth() -> Result<(), GhError>`
    - `list_issues(owner, repo, filter, page_cursor, page_size) -> Result<IssueListResponse, GhError>`
    - `get_issue_detail(owner, repo, number) -> Result<IssueDetail, GhError>`
    - `list_comments(owner, repo, number, page_cursor, page_size) -> Result<CommentsResponse, GhError>`
    - `create_comment(owner, repo, number, body) -> Result<IssueComment, GhError>`
    - `update_comment(owner, repo, comment_id, body) -> Result<(), GhError>`
    - `update_issue_body(owner, repo, number, body) -> Result<(), GhError>`
  - Define `build_send_payload()` function signature
  - All method bodies contain `todo!()`
  - marker: `@plan PLAN-20260329-ISSUES-MODE.P06`
  - marker: `@requirement REQ-ISS-011,REQ-ISS-013`
  - marker: `@pseudocode component-002 lines 01-82`

### Pseudocode traceability (line refs)
- component-002: lines 01-03 (GhClient struct)
- component-002: lines 04-08 (check_auth)
- component-002: lines 09-25 (list_issues)
- component-002: lines 26-32 (get_issue_detail)
- component-002: lines 33-43 (list_comments)
- component-002: lines 44-48 (create_comment)
- component-002: lines 49-56 (update_comment)
- component-002: lines 57-61 (update_issue_body)
- component-002: lines 62-74 (build_send_payload)
- component-002: lines 75-82 (GhError enum)

## Verification Commands

```bash
cargo fmt --all --check
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo test --workspace --all-features
```

## Structural Verification Checklist
- [ ] `GhClient` struct and all method stubs compile
- [ ] `GhError` enum is defined with all 7 categories
- [ ] `IssueListResponse` and `CommentsResponse` types exist
- [ ] `SendPayload` struct exists with all fields
- [ ] `build_send_payload()` function signature exists
- [ ] All existing tests pass
- [ ] Phase/requirement/pseudocode markers included in `src/github/mod.rs`

## Semantic Verification Checklist (Mandatory)
- [ ] Method signatures match pseudocode contracts (parameter types, return types) — verified against component-002 lines 04-74
- [ ] Error categories cover all failure modes from specification: not installed, not authenticated, rate limited, access denied, API error, parse error, network error
- [ ] GitHub client does not import or depend on `crate::ui` or `crate::state` modules
- [ ] `SendPayload` contains all fields from specification: repository slug, issue number/title/body/state/labels/assignees, focused comment (optional), issue_base_prompt
- [ ] Feature behavior is reachable from real app flow: `GhClient` methods will be called from key routing in P11 to produce state events
- [ ] No placeholder/deferred patterns except `todo!()` in method bodies (allowed in stub phase)

## Deferred Implementation Detection (Mandatory)

```bash
# Reject if these appear in implementation code:
grep -RIn "TODO\|FIXME\|HACK\|placeholder\|for now\|will be implemented" src/github/
```

Note: `todo!()` in stub method bodies is allowed in this stub phase only. Must be removed by P08.

## Success Criteria
- [ ] Compile-safe GitHub client boundary stubs exist with full signatures
- [ ] Verification commands pass
- [ ] Semantic checks pass

## Failure Recovery
- rollback steps: `git restore src/github/`
- blocking issues to resolve before next phase: type mismatches with domain types, missing error categories

## Phase Completion Marker
Create: `project-plans/issue15/.completed/P06.md`

Contents:
- phase ID: `PLAN-20260329-ISSUES-MODE.P06`
- timestamp
- files changed
- method signature inventory
- module isolation verification output
- verification command outputs
- semantic verification summary

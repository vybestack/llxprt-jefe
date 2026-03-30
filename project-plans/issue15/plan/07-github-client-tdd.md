# Phase 07: GitHub Client Boundary TDD

## Phase ID
`PLAN-20260329-ISSUES-MODE.P07`

## Prerequisites
- Required: Phase P06A completed
- Verify previous phase markers/artifacts exist: `.completed/P06.md`, `.completed/P06A.md`
- Expected files from previous phase: GitHub client stubs in `src/github/mod.rs` with full method signatures

## Requirements Implemented (Expanded)

### REQ-ISS-013: Authentication and Error Handling — TDD
**Requirement text**: v1 uses active `gh` CLI auth context. Missing/invalid auth blocks operations with remediation guidance. Non-auth errors: scoped error in list/detail, stable mode/focus, draft preservation, retry affordance.

Behavior contract:
- GIVEN `gh auth status` returns exit code 0
- WHEN `check_auth()` is called
- THEN returns `Ok(())`

- GIVEN `gh auth status` returns non-zero exit
- WHEN `check_auth()` is called
- THEN returns `Err(GhError::NotAuthenticated(...))`

- GIVEN stderr containing "rate limit exceeded"
- WHEN error is categorized
- THEN returns `GhError::RateLimited`

- GIVEN stderr containing "authentication" or "auth"
- WHEN error is categorized
- THEN returns `GhError::NotAuthenticated(...)`

- GIVEN stderr containing "403" or "denied"
- WHEN error is categorized
- THEN returns `GhError::AccessDenied(...)`

Why it matters:
- Auth failures must surface actionable remediation to the user; silent blocking or panics are unacceptable

### REQ-ISS-006: Issue List Parsing and Sorting — TDD
**Requirement text**: Each row: number, title, state, author, updated timestamp, assignee summary, label summary, comment count. Default sort: `updated_at` desc, tie-breaker `number` asc.

Behavior contract:
- GIVEN valid JSON output from `gh issue list`
- WHEN `list_issues()` parses the response
- THEN returns correctly typed `Vec<Issue>` sorted by `updated_at` desc

- GIVEN empty JSON array
- WHEN parsed
- THEN returns empty Vec, `has_more=false`

Why it matters:
- Correct parsing and deterministic sort order are the data foundation for all issue display

### REQ-ISS-008: Filter Args Construction — TDD
**Requirement text**: Supported filters: text query, state, author, assignee, labels (multi AND), mentioned. Structured filters AND-composed.

Behavior contract:
- GIVEN filter with `state=open`, `labels=["bug","auth"]`
- WHEN CLI args are constructed
- THEN args include `--state open --label bug --label auth`

Why it matters:
- Incorrect filter arg mapping silently shows the wrong issue set to the user

### REQ-ISS-009: Issue Detail and Comments Parsing — TDD
**Requirement text**: Detail displays all fields including optional milestone. Comments timeline.

Behavior contract:
- GIVEN valid JSON for issue detail with milestone
- WHEN parsed
- THEN all fields correctly mapped including `milestone: Some("v2.0")`

- GIVEN valid JSON for issue detail without milestone
- WHEN parsed
- THEN `milestone: None`

- GIVEN valid JSON array for comments
- WHEN parsed
- THEN returns `Vec<IssueComment>` with all fields mapped

Why it matters:
- Missing or mistyped optional fields cause incorrect UI display and broken send-to-agent payloads

### REQ-ISS-007: Pagination — TDD
**Requirement text**: Comment timeline supports incremental loading/pagination. Pagination appends in stable order without reordering. Pagination failure retains loaded comments and exposes retry.

Behavior contract:
- GIVEN comment list response with `has_more=true`
- WHEN next page is requested
- THEN new comments append without reordering prior results

Why it matters:
- Unstable ordering or dropped comments degrades trust in the comment timeline

### REQ-ISS-010: Mutation Operations — TDD
**Requirement text**: Create comment, update comment, update issue body. Save: `Cmd+Enter`/`Ctrl+Enter`. Cancel: `Esc`.

Behavior contract:
- GIVEN valid comment body text
- WHEN `create_comment()` succeeds
- THEN returns parsed `IssueComment` with assigned ID

- GIVEN valid body text
- WHEN `update_comment()` succeeds
- THEN returns `Ok(())`

- GIVEN valid body text
- WHEN `update_issue_body()` succeeds
- THEN returns `Ok(())`

Why it matters:
- Mutations are the user-visible write path; incorrect return values break the UI feedback loop

### REQ-ISS-011: Send Payload Composition — TDD
**Requirement text**: Payload includes repo identifier, issue number/title/body, metadata, focused comment (if any), `issue_base_prompt`.

Behavior contract:
- GIVEN issue detail with focused comment and `issue_base_prompt = "Focus on security"`
- WHEN `build_send_payload()` is called
- THEN payload contains all fields: repository, issue_number, issue_title, issue_body, issue_state, issue_labels, issue_assignees, focused_comment, focused_comment_author, issue_base_prompt

- GIVEN issue detail without focused comment
- WHEN `build_send_payload()` is called
- THEN payload has focused_comment fields as None/empty

Why it matters:
- Incomplete payload silently omits context the agent needs; missing `issue_base_prompt` violates per-repo customization

## Implementation Tasks

### Files to create or modify
- `src/github/mod.rs` — add inline `#[cfg(test)]` module with 18 tests:
  - `test_check_auth_success`
    - marker: `@plan PLAN-20260329-ISSUES-MODE.P07`
    - marker: `@requirement REQ-ISS-013`
    - marker: `@pseudocode component-002 lines 04-08`
  - `test_check_auth_not_authenticated`
    - marker: `@plan PLAN-20260329-ISSUES-MODE.P07`
    - marker: `@requirement REQ-ISS-013`
    - marker: `@pseudocode component-002 lines 04-08`
  - `test_list_issues_parses_json`
    - marker: `@plan PLAN-20260329-ISSUES-MODE.P07`
    - marker: `@requirement REQ-ISS-006`
    - marker: `@pseudocode component-002 lines 09-25`
  - `test_list_issues_sorts_by_updated_desc`
    - marker: `@plan PLAN-20260329-ISSUES-MODE.P07`
    - marker: `@requirement REQ-ISS-006`
    - marker: `@pseudocode component-002 lines 09-25`
  - `test_list_issues_filter_args_construction`
    - marker: `@plan PLAN-20260329-ISSUES-MODE.P07`
    - marker: `@requirement REQ-ISS-008`
    - marker: `@pseudocode component-002 lines 09-25`
  - `test_list_issues_empty_result`
    - marker: `@plan PLAN-20260329-ISSUES-MODE.P07`
    - marker: `@requirement REQ-ISS-006`
    - marker: `@pseudocode component-002 lines 09-25`
  - `test_get_issue_detail_parses_json`
    - marker: `@plan PLAN-20260329-ISSUES-MODE.P07`
    - marker: `@requirement REQ-ISS-009`
    - marker: `@pseudocode component-002 lines 26-32`
  - `test_get_issue_detail_optional_milestone`
    - marker: `@plan PLAN-20260329-ISSUES-MODE.P07`
    - marker: `@requirement REQ-ISS-009`
    - marker: `@pseudocode component-002 lines 26-32`
  - `test_list_comments_parses_json`
    - marker: `@plan PLAN-20260329-ISSUES-MODE.P07`
    - marker: `@requirement REQ-ISS-009`
    - marker: `@pseudocode component-002 lines 33-43`
  - `test_list_comments_pagination`
    - marker: `@plan PLAN-20260329-ISSUES-MODE.P07`
    - marker: `@requirement REQ-ISS-007`
    - marker: `@pseudocode component-002 lines 33-43`
  - `test_create_comment_success`
    - marker: `@plan PLAN-20260329-ISSUES-MODE.P07`
    - marker: `@requirement REQ-ISS-010`
    - marker: `@pseudocode component-002 lines 44-48`
  - `test_update_comment_success`
    - marker: `@plan PLAN-20260329-ISSUES-MODE.P07`
    - marker: `@requirement REQ-ISS-010`
    - marker: `@pseudocode component-002 lines 49-56`
  - `test_update_issue_body_success`
    - marker: `@plan PLAN-20260329-ISSUES-MODE.P07`
    - marker: `@requirement REQ-ISS-010`
    - marker: `@pseudocode component-002 lines 57-61`
  - `test_build_send_payload_with_comment`
    - marker: `@plan PLAN-20260329-ISSUES-MODE.P07`
    - marker: `@requirement REQ-ISS-011`
    - marker: `@pseudocode component-002 lines 62-74`
  - `test_build_send_payload_without_comment`
    - marker: `@plan PLAN-20260329-ISSUES-MODE.P07`
    - marker: `@requirement REQ-ISS-011`
    - marker: `@pseudocode component-002 lines 62-74`
  - `test_error_categorization_rate_limit`
    - marker: `@plan PLAN-20260329-ISSUES-MODE.P07`
    - marker: `@requirement REQ-ISS-013`
    - marker: `@pseudocode component-002 lines 79-86`
  - `test_error_categorization_not_authenticated`
    - marker: `@plan PLAN-20260329-ISSUES-MODE.P07`
    - marker: `@requirement REQ-ISS-013`
    - marker: `@pseudocode component-002 lines 79-86`
  - `test_error_categorization_access_denied`
    - marker: `@plan PLAN-20260329-ISSUES-MODE.P07`
    - marker: `@requirement REQ-ISS-013`
    - marker: `@pseudocode component-002 lines 79-86`

### Pseudocode traceability (if impl phase)
- Uses pseudocode component-002 lines 04-86 (all client methods and error types)
  - lines 04-08: auth check
  - lines 09-25: list issues (parsing, sorting, filter args)
  - lines 26-32: get issue detail
  - lines 33-43: list comments
  - lines 44-48: create comment
  - lines 49-56: update comment
  - lines 57-61: update issue body
  - lines 62-74: build send payload
  - lines 79-86: error categorization

## Verification Commands

```bash
cargo fmt --all --check
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo test --workspace --all-features
```

## Structural Verification Checklist
- [ ] All 18 planned test names exist and compile
- [ ] Tests use fixture/mock data, not real API calls
- [ ] At least one required test fails (RED step)
- [ ] No skipped phase dependencies
- [ ] Plan/requirement/pseudocode traceability markers present in ALL test code
- [ ] Module isolation: tests don't depend on UI or state modules
- [ ] All existing tests pass (zero regressions)

## Semantic Verification Checklist (Mandatory)
- [ ] Auth check tests cover success and both failure modes (not authenticated, not installed)
- [ ] Issue list tests cover: JSON parsing, sorting, empty result, filter args construction
- [ ] Detail tests cover: all fields including optional milestone (Some and None)
- [ ] Comment tests cover: parsing and pagination (has_more detection)
- [ ] Mutation tests cover: create_comment, update_comment, update_issue_body
- [ ] Send payload tests cover: with focused comment and without focused comment
- [ ] Error categorization tests cover: rate limit, not authenticated, access denied
- [ ] Feature behavior is reachable from real app flow: tests exercise parsing/composition functions called from real `GhClient` methods
- [ ] No placeholder test patterns (`assert!(true)`, `#[ignore]`, empty bodies)

## Deferred Implementation Detection (Mandatory)

```bash
grep -RIn "TODO\|FIXME\|HACK\|placeholder\|for now\|will be implemented" src/github/
```

## Success Criteria
- [ ] RED test suite established for GitHub client boundary (18 tests)
- [ ] Verification commands pass except expected RED failures
- [ ] Semantic checks pass

## Failure Recovery
- rollback steps: Trim tests that depend on real `gh` calls; ensure tests use mock/fixture data
- blocking issues: tests that pass without implementation, tests depending on external state

## Phase Completion Marker
Create: `project-plans/issue15/.completed/P07.md`

Contents:
- phase ID: `PLAN-20260329-ISSUES-MODE.P07`
- timestamp
- files changed
- tests added: 18 test names
- RED test verification: list of failing tests
- verification command outputs
- semantic verification summary

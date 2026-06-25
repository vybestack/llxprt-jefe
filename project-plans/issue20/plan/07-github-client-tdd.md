# Phase 07 — GitHub Client TDD (RED)

- **Plan ID:** `PLAN-20260624-PR-MODE`
- **Phase ID:** P07
- **Prerequisites:** `.completed/P06A.md` exists with PASS.
- **Citation discipline (finding #6):** `file:line` citations in this phase are guidance captured at planning time and may have drifted. Locate every referenced symbol BY NAME first, refresh any stale line numbers during preflight, and treat a symbol that cannot be found by name as a blocker. See Critical Reminder #6 in `00-overview.md`.

## Purpose

Write behavioral tests for the PR parse helpers and arg builders against fixed `gh --json` sample
payloads. Tests must fail (RED) against the P06 stubs. (Network calls are not unit-tested; parsing
and arg construction are.)

**RED cause (findings #1 & #4):** The P06 stubs are TOTAL and clippy-clean (NO
`todo!()`/`unimplemented!()`, which clippy denies) — they return deterministic WRONG/empty values
(empty `Vec`/`String`, `Default`/degraded records, `Ok(Default::default())`). Therefore every RED
failure here is a BEHAVIORAL assertion mismatch (e.g. parsed fields don't match the fixture, the
arg vector is empty), NEVER a panic. This is exactly why fmt/clippy/build/`check-clippy-allows.sh`
stay GREEN while only `cargo test` is RED.

## Requirements Implemented (Expanded)

### REQ-PR-006,007,009,010,012,013
- **Behavior contract:** GIVEN sample `gh pr list/view --json` outputs, WHEN the parse helpers run,
  THEN they map fields correctly (number/title/state/author/branches/draft/reviewDecision/
  statusCheckRollup/labels/assignees/comments/body/reviews), and error categorization maps gh
  failures to `GhError`.

## Implementation Tasks

### Tests (in `src/github/tests.rs` or `parse_pr` inline `#[cfg(test)]`)
Each test carries markers. Representative tests:
- `test_parse_pr_list_maps_all_fields` — REQ-PR-006 / c002 L138-156.
- `test_parse_pr_list_pagination_cursor_and_has_more` (asserts cursor == GraphQL `endCursor`
  and `has_more` == `hasNextPage`; NO updatedAt/number-derived cursor) — REQ-PR-007 / c002 L138-156.
- `test_parse_pr_list_empty_yields_empty_vec` — REQ-PR-014 / c002 L138-156.
- `test_build_pr_search_args_uses_graphql_search_with_after_cursor` (asserts `api graphql`,
  `search(type: ISSUE ...)`, `is:pr`, `first`, and `after` only when a cursor is supplied) —
  REQ-PR-007,008 / c002 L35-58.
- `test_parse_pr_detail_maps_body_branches_and_external_url` — REQ-PR-009,012 / c002 L157-166.
- `test_parse_pr_detail_reviews_summary` — REQ-PR-009 / c002 L174-180.
- `test_parse_pr_detail_checks_summary_from_status_rollup` — REQ-PR-009 / c002 L181-193.
- `test_parse_status_rollup_handles_checkrun_and_statuscontext_shapes` (Finding 8 — FIXTURE-DRIVEN.
  THIS PHASE (P07) AUTHORS the deterministic committed fixtures and wires them into this test: two
  committed JSON fixtures — ONE containing a `CheckRun` rollup entry and ONE containing a
  `StatusContext` rollup entry (a single combined heterogeneous fixture with both entries is
  acceptable) — embedded as inline raw-string literals (`r#"..."#`) per the current
  `src/github/tests.rs` convention, so they are committed, deterministic, and independent of any live
  API. The field names MUST match the component-002 shape table verified by P00A §2a
  (`name`/`status`/`conclusion`/`detailsUrl` for `CheckRun`; `context`/`state`/`targetUrl` for
  `StatusContext`). The test MUST cover BOTH shapes REGARDLESS of live API availability: asserts each
  maps to the expected `PrCheck` (name, status, conclusion text, url) and that NEITHER entry is
  dropped. If P00A captured live JSON, it is used to corroborate/refine these committed fixtures, but
  the committed fixtures are the binding test inputs so the test never depends on live data) —
  REQ-PR-009,013 / c002 L167-193,205-222.
- `test_parse_pr_state_merged_from_state_enum_and_mergedat` (Finding 3 — asserts state=="MERGED"
  maps to `PrState::Merged`, a closed-not-merged PR (state=="CLOSED", null mergedAt) maps to `Closed`,
  a PR with non-null `mergedAt` but ambiguous state maps to `Merged`, and an open PR maps to `Open`) —
  REQ-PR-006 / c002 L197-201.
- `test_parse_malformed_review_yields_degraded_placeholder_not_dropped` (asserts a malformed
  review entry is RETAINED as a displayable degraded `PrReview`, count preserved; no silent drop) —
  REQ-PR-009,013 / c002 L174-180.
- `test_parse_malformed_check_yields_degraded_placeholder_not_dropped` (same, for checks) —
  REQ-PR-009,013 / c002 L181-193.
- `test_parse_review_decision_variants` (Approved/ChangesRequested/ReviewRequired/None) —
  REQ-PR-009 / c002 L202-204.
- `test_parse_status_rollup_variants` (Success/Failure/Pending/Neutral) — REQ-PR-009 / c002 L205-222.
- `test_parse_pr_state_open_closed_merged` — REQ-PR-006 / c002 L197-201.
- `test_sort_pull_requests_by_updated_desc` — REQ-PR-006 / c002 L194-196.
- `test_build_pr_search_query_includes_state_and_search_filters` — REQ-PR-008 / c002 L59-73.
- `test_build_pr_search_query_emits_draft_qualifier` — Finding 3 / REQ-PR-008 / c002 L71. Asserts
  `is_draft == Some(true)` puts the EXACT token `draft:true` in the composed query, `Some(false)`
  puts `draft:false`, and `None` emits NO draft qualifier. The token is the P00A §2b-VERIFIED
  server-side qualifier for the `search(type: ISSUE, query:...)` endpoint (NOT a client-side
  post-filter, so cursor pagination is preserved). Fixture-backed: the test also feeds a captured
  two-PR search-result JSON (one draft, one non-draft) through `parse_pull_requests_json` and asserts
  `is_draft` parses correctly per row, corroborating the qualifier semantics.
- `test_build_pr_search_query_emits_review_and_checks_qualifiers` — Finding 1 / REQ-PR-008 / c002
  L71f-71u. Asserts the EXACT tokens for each enumerated variant: `review_decision == Approved` ->
  `review:approved`, `ChangesRequested` -> `review:changes_requested`, `ReviewRequired` ->
  `review:required`, `None` -> `review:none`, `Any` -> NO `review:` token; and `checks_status ==
  Success` -> `status:success`, `Failing` -> `status:failure`, `Pending` -> `status:pending`, `Any`
  -> NO `status:` token. Also asserts deterministic qualifier ORDER (state, labels, author,
  assignee, reviewer, draft, review, checks, then free-text query) and that combining both signals
  with a state filter yields a single stable query string. These are the P00A §2c-VERIFIED
  server-side qualifiers for the `search(type: ISSUE, query:...)` endpoint (NOT a client-side
  post-filter, so cursor pagination is preserved).
- `test_build_pr_search_args_preserves_cursor_with_signal_filters` — Finding 1/4 / REQ-PR-008 / c002
  L35-58. Asserts that when `review_decision`/`checks_status` are set, `build_pr_search_args` still
  passes `first = PR_LIST_PAGE_SIZE` and the supplied `after` cursor UNCHANGED (the signal filtering
  happens entirely inside the `query` string), proving the new filters are pagination-safe and
  server-side (`endCursor`/`hasNextPage` semantics intact).
- `test_list_pr_comments_query_targets_pull_request_not_issue` — Finding 1 / REQ-PR-010 / c002
  L102-107. Asserts the GraphQL query string `list_pr_comments` builds contains `pullRequest(`
  (specifically the `repository(...) { pullRequest(number:` object path) and does NOT contain
  `issue(number:` — i.e. PR comments are fetched from `repository.pullRequest`, NOT
  `repository.issue` (which is NULL for a PR number per P00A §2d). Also asserts it selects
  `comments(first:` with `pageInfo { hasNextPage endCursor }` and passes the `after` cursor through
  unchanged, so PR comment pagination uses the SAME real cursor mechanism as the issue path. Build
  the query the way the issue path is tested today (assert on the emitted query string / args), and
  reuse `parse_comments_json`/`parse_page_info` for the response decode.
- `test_list_pr_comments_parses_comments_and_pageinfo` — Finding 1 / REQ-PR-007/010 / c002 L102-107.
  Fixture-backed: feed a captured `repository.pullRequest.comments` JSON envelope (nodes +
  `pageInfo{hasNextPage,endCursor}` + `totalCount`) and assert it decodes to `CommentsResponse`
  (comments oldest→newest, real `cursor`==endCursor, `has_more`==hasNextPage) via the REUSED
  `parse_comments_json`/`parse_page_info` — proving node-shape compatibility with `IssueComment`.
- `test_get_pull_request_detail_sources_comments_via_list_pr_comments` — Finding 1 / REQ-PR-009 /
  c002 L74-101. Asserts `get_pull_request_detail` populates `detail.comments`/`comments_cursor`/
  `has_more_comments` from the `list_pr_comments` first page (NOT from any embedded `gh pr view
  --json comments` field, which is intentionally omitted).
- `test_create_pr_comment_parses_created_comment` — REQ-PR-010 / c002 L108-114 (CREATE uses the REST
  `/repos/{o}/{n}/issues/{number}/comments` endpoint, which accepts a PR number; this path is
  unchanged from the issue create-comment transport).
- `test_build_pr_send_payload_with_focused_comment` (mirrors the issue
  `test_build_send_payload_with_comment`, src/github/tests.rs L585-637; asserts the returned
  `PrSendPayload` carries the REAL struct fields: `repository`, `pr_number`, `pr_title`, `pr_body`,
  `pr_state` ("open"/"closed"/"merged"), `head_ref`, `base_ref`, `external_url`, `review_summary`,
  `check_summary`, `focused_comment`==Some(body), `focused_comment_author`==Some(login),
  `pr_base_prompt`; and that NO `prompt_markdown`/`work_dir`/`signature` field exists) —
  REQ-PR-011 / c002 L123-136.
- `test_build_pr_send_payload_without_focused_comment` (mirrors
  `test_build_send_payload_without_comment`, src/github/tests.rs L638-666; asserts
  `focused_comment`/`focused_comment_author` are `None` and merged-state maps to "merged") —
  REQ-PR-011 / c002 L123-136.
- `test_categorize_error_not_authenticated_and_rate_limited` — REQ-PR-013 / c002 L223-227.
- `test_parse_malformed_json_returns_parse_error_not_panic` — REQ-PR-013 / c002 L138-166.

## Pseudocode Traceability
- component-002 lines 22-227.

## Verification Commands

This is a **TDD(RED)** phase. Run the COMPLETE baseline. The RED exception applies to exactly ONE
command — `cargo test` — which is EXPECTED to fail (the new parse/error-categorization tests have
no implementation yet). Every other gate MUST pass (the test code must COMPILE; only assertions
may fail):
```bash
cargo fmt --all --check                                            # MUST pass
cargo clippy --workspace --all-targets --all-features -- -D warnings # MUST pass
bash scripts/check-clippy-allows.sh                                # MUST pass (no allows/overrides)
cargo build --workspace --all-features --locked                    # MUST pass (RED tests compile)
cargo test --workspace --all-features --locked 2>&1 | tee /tmp/p07.log  # EXPECTED to FAIL (RED)
rg -n "test result: FAILED" /tmp/p07.log   # expect >=1 failure (RED confirmed)
```
RED exception: only `cargo test` may fail, and only because the behavioral tests are unimplemented.
`cargo build`, fmt, clippy, and `check-clippy-allows.sh` MUST all be green.

## Structural Verification Checklist
- [ ] Tests compile and are registered.
- [ ] ≥1 fails RED.
- [ ] Markers present.

## Semantic Verification Checklist (Mandatory)
- [ ] Sample payloads use realistic `gh --json` field names.
- [ ] Malformed-JSON test asserts `GhError::ParseError` (no panic).
- [ ] No `assert!(true)`, no `#[ignore]`, no unwrap/expect in assertions (use `matches!`).

## Deferred Implementation Detection
Inverted HARD gate (absence passes, presence fails) covering all four weak-test smells:
```bash
if rg -nP 'assert!\(true\)|#\[ignore\]|\.unwrap\(\)|\.expect\(' src/github/; then
  echo "FAIL: deferred/weak test smell (assert!(true) | #[ignore] | .unwrap() | .expect())"; exit 1
fi
```

## Success Criteria
- Tests compile; RED confirmed; cover every parse/arg/error behavior.

## Failure Recovery
- Fix test fixtures/compilation.

## Phase Completion Marker (`.completed/P07.md`)
Phase ID, timestamp, test list, RED list, semantic summary.

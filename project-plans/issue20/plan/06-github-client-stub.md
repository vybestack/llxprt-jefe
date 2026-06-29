# Phase 06 — GitHub Client Stub

- **Plan ID:** `PLAN-20260624-PR-MODE`
- **Phase ID:** P06
- **Prerequisites:** `.completed/P05A.md` exists with PASS.
- **Citation discipline (finding #6):** `file:line` citations in this phase are guidance captured at planning time and may have drifted. Locate every referenced symbol BY NAME first, refresh any stale line numbers during preflight, and treat a symbol that cannot be found by name as a blocker. See Critical Reminder #6 in `00-overview.md`.

## Purpose

Add the PR-Mode `gh` CLI client surface (`GhClient` methods + `parse_pr.rs` helpers + response
structs) as compiling stubs, isolated to the GitHub boundary. The boundary modules must NOT import
`crate::ui`, `crate::state`, or `crate::app_input`; they MAY use `crate::domain`, `serde_json`,
`std::process`, and sibling `crate::github` types (`GhError`, `IssueListResponse`, `IssueComment`,
`SendPayload`, and the new PR response/parse structs) — mirroring the current
`src/github/parse.rs` import block (`crate::domain`, `serde_json::Value`,
`super::{GhError, IssueListResponse}`).

**TOTAL-STUB rule (NO `todo!()`/`unimplemented!()` ANYWHERE — findings #1 & #4):** `Cargo.toml`
`[lints.clippy]` DENIES both macros (`todo = "deny"` at L63, `unimplemented = "deny"` at L64), and
clippy fires on their mere PRESENCE regardless of reachability. Since this stub phase requires
`cargo clippy --workspace --all-targets --all-features -- -D warnings` to PASS (no RED exception),
`todo!()`/`unimplemented!()` are FORBIDDEN in EVERY P06-authored body. The new `GhClient` PR methods
and `parse_pr.rs` helpers MUST instead be TOTAL, clippy-clean, panic-free stubs that return
deterministic WRONG/empty values:
- methods returning `Result<T, GhError>` return `Ok(T::default())` (e.g.
  `Ok(PrListResponse::default())`, `Ok(PullRequestDetail::default())`) or, where a default is not
  meaningful, a deterministic `Err(GhError::…)` — NEVER a panic;
- `build_*` helpers returning `Vec<String>`/`String` return an empty `Vec`/`String`;
- parse helpers return `Ok(Default::default())` or an empty collection;
- TOTAL parse fns (`parse_pr_review`, `parse_pr_check`, etc.) return a default/degraded record.
These wrong-but-total values let P07's RED tests fail by BEHAVIORAL assertion (wrong returned
value), never by panic. Required response/parse structs therefore derive/`impl Default`.

## Requirements Implemented (Expanded)

### REQ-PR-006 list, REQ-PR-007 pagination, REQ-PR-009 detail, REQ-PR-010 comments, REQ-PR-012 external_url (display-only) + `open_pull_request_in_browser`, REQ-PR-013 auth/errors
- **Behavior contract:** GIVEN current `GhClient`, WHEN P06 lands, THEN the PR method signatures and
  parse helpers exist and compile; the boundary modules do NOT import `crate::ui`, `crate::state`,
  or `crate::app_input`, and MAY use `crate::domain`, `serde_json`, `std::process`, and sibling
  `crate::github` types (`GhError`, response/parse structs) — matching `src/github/parse.rs`.
- **Why it matters:** Establishes the sync transport surface for the TDD phase to target.

## Implementation Tasks

### Files to modify
- `src/github/mod.rs`:
  - add `mod parse_pr;`
  - add structs `PrListResponse { pull_requests, cursor, has_more }` (mirror `IssueListResponse`;
    derive/`impl Default` so stubs can return `Ok(PrListResponse::default())`).
  - add `impl GhClient` methods (TOTAL stub bodies returning `Ok(Default::default())` or a
    deterministic `Err(GhError::…)` — NEVER `todo!()`/`unimplemented!()`):
    - `list_pull_requests(&self, owner, name, filter: &PrFilter, cursor: Option<&str>) ->
      Result<PrListResponse, GhError>` — c002 L22-34.
    - `get_pull_request_detail(&self, owner, name, number: u64) ->
      Result<PullRequestDetail, GhError>` (fetches comments via a SEPARATE `list_pr_comments`
      call, mirroring `get_issue_detail`'s comments-sourcing) — c002 L74-101.
    - `list_pr_comments(&self, owner, name, number: u64, cursor: Option<&str>, page_size: u32) ->
      Result<CommentsResponse, GhError>` (NEW PR-specific GraphQL comments fetcher querying
      `repository.pullRequest(number:).comments`, NOT `repository.issue(number:)`; reuses
      `parse_comments_json`/`parse_page_info`/`IssueComment`) — c002 L102-107.
    - `create_pr_comment(&self, owner, name, number: u64, body: &str) ->
      Result<IssueComment, GhError>` (uses the **issues** comment REST endpoint, which accepts a PR
      number) — c002 L108-114.
    - `open_pull_request_in_browser(&self, owner, name, number: u64) -> Result<(), GhError>`
      (spawns `gh pr view <number> --repo <owner>/<name> --web`; maps `gh` failure to `GhError`
      via `categorize_error`; REQ-PR-012) — c002 L115-122.
    - `build_pr_send_payload(&self, ...) -> PrSendPayload` — c002 L123-136. Returns a NEW
      `PrSendPayload` struct (mirrors `SendPayload`'s structured/owned-field design; carries NO
      `prompt_markdown`/`work_dir`/`signature` — those are NOT payload concerns).
  - add struct `PrSendPayload` (structured, owned fields mirroring `SendPayload`: `repository`,
    `pr_number`, `pr_title`, `pr_body`, `pr_state`, `head_ref`, `base_ref`, `external_url`,
    `review_summary: Vec<String>`, `check_summary: Vec<String>`, `focused_comment: Option<String>`,
    `focused_comment_author: Option<String>`, `pr_base_prompt`) — c002 L123-129.
  - reuse existing `GhError`, `categorize_error`, the comment parsers
    (`parse_comments_json`/`parse_page_info`/`parse_created_comment_json`), `CommentsResponse`, and
    the `IssueComment` type; reuse `SendPayload` ONLY as the design template (the PR path uses the new
    `PrSendPayload`). Do NOT reuse the issue `list_comments` for PR comment FETCH — it queries
    `repository.issue(number:)`, which is NULL for a PR number (P00A §2d); the new `list_pr_comments`
    queries `repository.pullRequest(number:)` instead.

### Files to create
- `src/github/parse_pr.rs` (TOTAL stub fns returning empty/default values — NEVER `todo!()`):
  - `build_pr_search_args(owner, name, filter, cursor, page_size) -> Vec<String>` (REAL GraphQL
    `search(type: ISSUE ...)` cursor args, mirroring `build_issue_search_args`) — c002 L35-58.
  - `build_pr_search_query(owner, name, filter) -> String` (search qualifier string incl. `is:pr`,
    mirroring `issue_search_query`) — c002 L59-73.
  - `parse_pull_requests_json(&str) -> Result<PrListResponse, GhError>` (reuses `parse_page_info`
    for the REAL endCursor/hasNextPage) — c002 L138-156.
  - `parse_pull_request_detail_json(&str) -> Result<PullRequestDetail, GhError>` — c002 L157-166.
  - `parse_pr_review(json) -> PrReview`, `parse_pr_check(json) -> PrCheck` (TOTAL functions —
    malformed entries become displayable degraded records, never dropped) — c002 L174-193.
  - `parse_review_decision(...) -> Option<PrReviewState>`, `parse_checks_rollup(...) -> PrCheckStatus`,
    `parse_pr_state(...) -> PrState`, `sort_pull_requests(&mut Vec<PullRequest>)` —
    c002 L194-222.

Markers `@plan PLAN-20260624-PR-MODE.P06 @requirement REQ-PR-xxx @pseudocode component-002 lines X-Y`
on every item.

## Pseudocode Traceability
- component-002 lines 01-227.

## Verification Commands

Run the COMPLETE baseline (all gates MUST pass — this is a stub/GREEN phase, no RED exception):
```bash
cargo fmt --all --check
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo build --workspace --all-features --locked
cargo test --workspace --all-features --locked
bash scripts/check-clippy-allows.sh
# or: make ci-check
# Boundary-isolation HARD gate (finding #3 — fail ONLY when a forbidden import is FOUND; rg exits
# nonzero on no-match, so an absence check must be inverted, never run bare):
if rg -n "use crate::ui|use crate::state|use crate::app_input" src/github/ ; then
  echo "FAIL: src/github must not import crate::ui/state/app_input (boundary isolation)"; exit 1
fi
# TOTAL-STUB HARD gate (findings #1 & #4 — no todo!()/unimplemented!() anywhere in P06 source):
if rg -n "todo!\(\)|unimplemented!\(\)" src/github/parse_pr.rs src/github/mod.rs ; then
  echo "FAIL: todo!()/unimplemented!() present (clippy denies both; stubs must be total)"; exit 1
fi
```
All gates above MUST pass. Stub bodies compile; no command is permitted to fail in this phase.

## Structural Verification Checklist
- [ ] Build green; all PR method/helper signatures present.
- [ ] `PrListResponse` mirrors `IssueListResponse`.
- [ ] Existing `GhClient` methods unchanged.
- [ ] Markers present.

## Semantic Verification Checklist (Mandatory)
- [ ] Boundary modules import NO `crate::ui`/`crate::state`/`crate::app_input`; they MAY use
  `crate::domain`, `serde_json`, `std::process`, and sibling `crate::github` types
  (`GhError`, response/parse structs) — mirror `src/github/parse.rs`.
- [ ] `create_pr_comment` targets the issues comment REST endpoint (documented in fn doc; valid
  for PR numbers).
- [ ] `list_pr_comments` exists as a NEW PR-specific method; its query targets
  `repository.pullRequest(number:).comments`, NOT `repository.issue(number:)` (P00A §2d). The issue
  `list_comments` is NOT reused for PR comment FETCH.
- [ ] `GhError` reused (no new error enum).
- [ ] NO `todo!()`/`unimplemented!()` anywhere in `src/github/mod.rs` or `src/github/parse_pr.rs`
  (findings #1 & #4): clippy denies both macros, so every stub is a TOTAL, panic-free body returning
  a deterministic WRONG/default value. P07's RED is a behavioral assertion mismatch, not a panic.

## Deferred Implementation Detection
```bash
# Stub phase: record (do NOT fail on) non-macro markers in PR-owned files; these become hard gates
# in P08 (impl). The todo!()/unimplemented!() HARD gate lives in Verification Commands above.
# Record-only: append `|| true` so a no-match (rg exit 1) cannot abort the phase under `set -e`.
rg -n "TODO|FIXME|HACK|placeholder|for now|will be implemented" src/github/parse_pr.rs src/github/mod.rs || true
```

## Success Criteria
- Compiles green; isolated boundary; signatures match pseudocode.

## Failure Recovery
Restore the modified tracked file and delete ONLY the one new file this phase created. Do NOT use
`git clean`.
```bash
git restore --staged --worktree -- src/github/mod.rs
rm -f src/github/parse_pr.rs
```

## Phase Completion Marker (`.completed/P06.md`)
Phase ID, timestamp, files changed, build result, isolation check, semantic summary.

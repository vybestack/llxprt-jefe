# Phase 08 — GitHub Client Impl (GREEN)

- **Plan ID:** `PLAN-20260624-PR-MODE`
- **Phase ID:** P08
- **Prerequisites:** `.completed/P07A.md` exists with PASS.
- **Citation discipline (finding #6):** `file:line` citations in this phase are guidance captured at planning time and may have drifted. Locate every referenced symbol BY NAME first, refresh any stale line numbers during preflight, and treat a symbol that cannot be found by name as a blocker. See Critical Reminder #6 in `00-overview.md`.

## Purpose

Implement the PR gh client methods and parse helpers so all P07 RED tests turn GREEN — replacing the
P06 TOTAL wrong-value stub bodies with real parsing/arg-building/error-mapping logic. (There is no
`todo!()` to remove: P06 stubs were already total and clippy-clean — findings #1 & #4.) All methods
stay synchronous (async wrapping happens in the dispatch layer).

## Requirements Implemented (Expanded)

### REQ-PR-006,007,009,010,012,013, NFR-001 (sync boundary, off-thread later)
- **Behavior contract:** GIVEN P07 RED tests, WHEN P08 lands, THEN parsing/arg-building/error
  mapping are correct and all tests pass; each gh invocation follows the established error idiom
  (`Command::new("gh")...map_err(NotFound→NotInstalled else NetworkError)`;
  `!status.success() → categorize_error`; else parse).

## Implementation Tasks

### `src/github/parse_pr.rs` (markers + pseudocode refs per fn)
- `build_pr_search_args` — build `gh api graphql` cursor args (`search(type: ISSUE, query, first,
  after)` + `pageInfo`), mirroring `build_issue_search_args` (parse.rs L594-621) — c002 L35-58.
- `build_pr_search_query` — translate `PrFilter` to a search-qualifier string (`repo:o/n is:pr`
  + state/author/assignee/review-requested/label/draft + query_text), mirroring
  `issue_search_query` (parse.rs L561-578) — c002 L59-73.
- `parse_pull_requests_json` → `PrListResponse` (number,title,state,author.login,updatedAt,
  headRefName,baseRefName,isDraft,reviewDecision,statusCheckRollup,assignees,labels,comments count;
  cursor/has_more via the REUSED `parse_page_info` = real `endCursor`/`hasNextPage`) — c002 L138-156.
- `parse_pull_request_detail_json` → `PullRequestDetail` (body, branches, external url, reviews,
  checks; comments left empty here — filled by the separate `list_pr_comments` call) — c002 L157-166.
- `rollup_nodes` (normalize BOTH the `gh pr view` flat array AND the `gh api graphql`
  `contexts.nodes` connection; Finding 8), `parse_pr_review`, `parse_pr_check` (TOTAL functions:
  malformed entries become displayable degraded records, never dropped; `parse_pr_check` handles
  BOTH CheckRun and StatusContext shapes), `parse_review_decision`, `parse_check_status`,
  `parse_checks_rollup`, `parse_pr_state` (state enum MERGED OR non-null `mergedAt` → Merged;
  Finding 3), `sort_pull_requests` — c002 L167-222.

### `src/github/mod.rs` (markers per method)
- `list_pull_requests` — build args, run gh, categorize/parse — c002 L22-34.
- `get_pull_request_detail` — `gh pr view <n> --json ...` (the PR `--json` set OMITS `comments`)
  then a SEPARATE `list_pr_comments` call for the first comment page. This follows the
  comments-sourcing precedent of `get_issue_detail`, which DOES include `comments` in its
  `gh issue view --json` set (mod.rs L180), parses it (L197), then OVERWRITES
  `detail.comments`/`comments_cursor`/`has_more_comments` from a distinct comments call
  (mod.rs L198-202); PR detail skips the embedded `comments` field entirely since it would only be
  overwritten — c002 L74-101.
- `list_pr_comments` — NEW PR-specific GraphQL comments fetcher: `gh api graphql` querying
  `repository(owner:,name:){ pullRequest(number:){ comments(first:$first, after:$after) { nodes{ id
  databaseId author{login} createdAt lastEditedAt body } pageInfo{ hasNextPage endCursor } totalCount
  } } }`. This is the issue `list_comments` shape with the object swapped from `issue(number:)` to
  `pullRequest(number:)` (because `repository.issue(number:)` is NULL for a PR number — P00A §2d, a
  silent-empty regression otherwise). REUSES `parse_comments_json` for nodes and `parse_page_info` for
  the cursor, returning a `CommentsResponse` (comments oldest→newest, real `endCursor`,
  `hasNextPage`) — c002 L102-107.
- `create_pr_comment` — `gh api --method POST /repos/{owner}/{repo}/issues/{n}/comments -f body=...`
  → parse created comment. CREATE uses the issues REST endpoint, which accepts a PR number (PRs are
  issues for the REST comments API), so this transport is unchanged from the issue create path — c002
  L108-114.
- `open_pull_request_in_browser` — `gh pr view <number> --repo <owner>/<name> --web`, following the
  same error idiom (`NotFound→NotInstalled` else `NetworkError`; `!status.success()→categorize_error`)
  and returning `Ok(())` on success (REQ-PR-012) — c002 L115-122.
- `build_pr_send_payload` — assemble a `PrSendPayload` (structured, owned fields mirroring
  `SendPayload`: repository, pr_number, pr_title, pr_body, pr_state, head_ref, base_ref,
  external_url, review_summary, check_summary, focused comment + author, pr_base_prompt). Pure
  assembly, NO I/O, and NO `prompt_markdown`/`work_dir`/`signature` (markdown is rendered later by
  `prs_dispatch::format_pr_prompt`; work_dir + signature come from the agent). Mirrors
  `GhClient::build_send_payload` (mod.rs L432-455) — c002 L123-136.
- reuse the comment PARSERS (`parse_comments_json`/`parse_page_info` from mod.rs/parse.rs) and the
  `CommentsResponse`/`IssueComment` types for PR comment pagination — but via the NEW
  `list_pr_comments` (`repository.pullRequest`), NOT the issue `list_comments` (`repository.issue`,
  which is NULL for a PR number; P00A §2d). PRs share the issue COMMENT NODE shape and the CREATE REST
  endpoint, but NOT the GraphQL comments object path.

## Pseudocode Traceability
- component-002 lines 22-227.

## Verification Commands

Run the COMPLETE baseline (all gates MUST pass — this is a GREEN phase, no RED exception):
```bash
cargo fmt --all --check
cargo clippy --workspace --all-targets --all-features -- -D warnings
bash scripts/check-clippy-allows.sh
cargo build --workspace --all-features --locked
cargo test --workspace --all-features --locked
# or: make ci-check
# Deferred-implementation HARD gate — SCOPED to PR-owned changes (finding #1): absence passes,
# presence fails. parse_pr.rs is NEW (scan in full); src/github/mod.rs is a SHARED modified file, so
# only flag markers THIS branch ADDED (git diff main added lines) — never pre-existing unrelated text.
DEFERRED_RE='TODO|FIXME|HACK|todo!\(|unimplemented!\(|placeholder|for now'
if [ -f src/github/parse_pr.rs ] && rg -n "$DEFERRED_RE" src/github/parse_pr.rs ; then
  echo "FAIL: residual deferred-implementation marker in new file src/github/parse_pr.rs"; exit 1
fi
if git diff main -- src/github/mod.rs | grep -E '^\+' | grep -Ev '^\+\+\+' | grep -E "$DEFERRED_RE" ; then
  echo "FAIL: deferred-implementation marker ADDED by this branch in src/github/mod.rs"; exit 1
fi
```

## Structural Verification Checklist
- [ ] All P07 RED tests GREEN; existing tests green.
- [ ] No `todo!()`/`unimplemented!()` in boundary (clippy denies both).
- [ ] Markers per fn/method.

## Semantic Verification Checklist (Mandatory)
- [ ] Error idiom consistent with existing methods (cite one method).
- [ ] `list_pr_comments` GraphQL query targets `repository.pullRequest(number:).comments` (NOT
  `repository.issue`); the issue `list_comments` is NOT reused for PR comment FETCH (Finding 1 / P00A
  §2d). `test_list_pr_comments_query_targets_pull_request_not_issue` is GREEN (cite).
- [ ] `create_pr_comment` uses the issues REST endpoint, valid for a PR number (cite).
- [ ] Malformed JSON → `GhError::ParseError` (no panic/unwrap) — cite parse fns.
- [ ] No silent None: unexpected JSON shapes yield errors, not dropped data (#37/#39).
- [ ] Methods stay sync; no I/O thread logic here.
- [ ] No clippy allow / no override; functions within limits.

## No-Placeholder / Deferred Detection
HARD inverted gate — SCOPED to PR-owned changes (finding #1): absence passes, presence fails. New
file `src/github/parse_pr.rs` is scanned in full; shared file `src/github/mod.rs` is scanned only for
markers THIS branch ADDED (`git diff main` added lines), so pre-existing unrelated text is ignored.
NOTE: this phase-local, file-scoped deferred-marker scan is a fast local guard ONLY; it does NOT
replace the global workspace-wide deferred-marker / no-placeholder gate enforced at P16
(16-e2e-quality-gate.md / P16A). Both must pass.
```bash
DEFERRED_RE='TODO|FIXME|HACK|todo!\(|unimplemented!\(|placeholder|for now'
if [ -f src/github/parse_pr.rs ] && rg -n "$DEFERRED_RE" src/github/parse_pr.rs ; then
  echo "FAIL: deferred-implementation marker present in new file src/github/parse_pr.rs"; exit 1
fi
if git diff main -- src/github/mod.rs | grep -E '^\+' | grep -Ev '^\+\+\+' | grep -E "$DEFERRED_RE" ; then
  echo "FAIL: deferred-implementation marker ADDED by this branch in src/github/mod.rs"; exit 1
fi
```

## Success Criteria
- Suite green; correct parsing/errors; no placeholders; within limits.

## Failure Recovery
- `git restore` boundary; re-implement per pseudocode; bisect P07A↔P08 if needed.

## Phase Completion Marker (`.completed/P08.md`)
Phase ID, timestamp, RED→GREEN list, clippy/fmt result, no-placeholder output, semantic summary.

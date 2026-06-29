# Phase 00A — Preflight Verification

- **Plan ID:** `PLAN-20260624-PR-MODE`
- **Phase ID:** P00A
- **Prerequisites:** Plan documents authored (`specification.md`, `analysis/*`, `plan/00-overview.md`).
- **Citation discipline (finding #6):** `file:line` citations in this phase are guidance captured at planning time and may have drifted. Locate every referenced symbol BY NAME first, refresh any stale line numbers during preflight, and treat a symbol that cannot be found by name as a blocker. See Critical Reminder #6 in `00-overview.md`.

## Purpose

This is the PLAN-TEMPLATE "Phase 0.5" preflight verification artifact — a pre-execution grounding
GATE, NOT a worker/verifier pair. It has no `P00` worker counterpart and writes NO production code,
so the paired worker→verifier coordination protocol (which begins at P01/P01A) does NOT apply here.

Verify the toolchain, `gh` CLI, target interfaces, call paths, and quality-gate baseline BEFORE any
code is written, using the CURRENT codebase signatures (the message bus and typed layout module are
present). If any check fails, this phase produces a Blocker Gate FAIL with remediation and a
required plan revision.

## Verifier Output Contract (complete — finding #3)

P00A is the standalone preflight GROUNDING gate (no `P00` worker, no production code). The
`00-overview.md` "Verifier Output Contract" is satisfied with the code-level items explicitly marked
**N/A — preflight/documentation gate** with the reason, NOT silently omitted:
1. **Structural verification** — see "Preflight Checklists" / "Structural Verification Checklist":
   every cited target file/symbol exists (or is confirmed absent for a NEW identifier), the
   toolchain + `gh` CLI are present, and the `make ci-check` baseline is green.
2. **Behavioral code-reading evidence (file:line)** — **N/A — preflight gate.** No PR production
   code exists yet; the analogous grounding is confirming the cited EXISTING anchor symbols/lines
   (e.g. `to_persisted_state`, `apply_message`, `resolve_mode_key`) resolve by name in current
   `src/`.
3. **Runtime-path reachability** — **N/A — preflight gate.** No PR runtime path exists yet; this
   gate instead confirms the INTENDED anchor symbols on the future chain are present/absent as the
   plan assumes (so the chain is buildable).
4. **Contradiction scan** — confirm no preflight finding (a missing symbol, a wrong `gh` qualifier,
   a NULL `repository.issue(number:)` for a PR number) contradicts a plan assumption; any such
   contradiction is a Blocker Gate FAIL requiring plan revision.
5. **Atomic verdict** — `Phase 00A: PASS` (plan grounded) or `Phase 00A: FAIL` (Blocker Gate) with
   remediation and the required plan revision (see Success Criteria).

## Requirements Implemented (Expanded)

### REQ-PR-NFR-001/002/003 — toolchain & boundary feasibility
- **Requirement:** PR Mode must be implementable with non-blocking gh I/O, deterministic reducers,
  and within clippy thresholds, against the current source.
- **Behavior contract:**
  - GIVEN the current repo, WHEN preflight runs, THEN all target files/symbols exist at the cited
    lines (or are confirmed absent for new identifiers), the toolchain + gh CLI are present, and the
    baseline `make ci-check` is green.
- **Why it matters:** A plan that references stale signatures (as issue15's docs do) will fail at
  execution time; this phase guarantees the plan is grounded in real, current code.

## Implementation Tasks

- **Files to create:** `.completed/P00A.md` (completion marker).
- **Files to modify:** `plan/00-overview.md` Execution Tracker (mark P00A).

## Preflight Checklists

### 1. Toolchain
```bash
cargo --version          # expect cargo 1.92.x
rustc --version          # expect rustc 1.92.x
cargo clippy --version   # expect clippy 0.1.92
cargo llvm-cov --version || echo "llvm-cov ABSENT (coverage via make ci-check fail-under-lines 30)"
```
- PASS criteria: cargo/rustc/clippy present. If `cargo llvm-cov` absent, record it; coverage runs
  through `make ci-check`. If `make ci-check` cannot compute coverage, flag as a soft blocker.

### 2. gh CLI
```bash
gh --version
gh auth status
gh pr list --limit 1 --repo <a-configured-repo> --json number    # API reachability for PRs
gh issue list --limit 1 --repo <a-configured-repo> --json number # issue path reachability (contrast §2d)
```
- PASS criteria: gh present + authenticated + `gh pr list --json` returns valid JSON. (The PR
  comment object-path — `repository.pullRequest` vs `repository.issue` — is proven separately in §2d.)

#### 2a. statusCheckRollup JSON-shape grounding (Finding 8 — SHAPE DOCUMENTATION ONLY; NO P07 wiring)
Document and verify the rollup JSON shapes that the rollup parser (component-002
`rollup_nodes`/`parse_pr_check`/`parse_check_status`, lines 167-222) must handle, so the SHAPES are
specified in the plan for later use. The rollup is a HETEROGENEOUS array mixing `CheckRun` (fields:
`name`, `status`, `conclusion`, `detailsUrl`) and `StatusContext` (fields: `context`, `state`,
`targetUrl`) entries, `__typename`-discriminated.

SCOPE BOUNDARY (finding #3): this is a PURE preflight grounding gate. It documents/verifies the two
shapes and confirms they match the component-002 shape table. It MUST NOT require committed test
fixtures, and it MUST NOT require any wiring into P07 parser tests — those are FORWARD-PHASE
deliverables owned by the github-client TDD slice (P07 authors the committed inline raw-string
fixtures and wires them into
`test_parse_status_rollup_handles_checkrun_and_statuscontext_shapes`; P07A verifies them). P00A has
NO forward-phase dependency.

- **REQUIRED — document the two shapes in the plan (binding gate, shape-only).** Confirm the
  component-002 shape table (lines 167-222) records BOTH entry shapes with the correct
  `__typename`-discriminated field sets (`name`/`status`/`conclusion`/`detailsUrl` for `CheckRun`;
  `context`/`state`/`targetUrl` for `StatusContext`) and that the parser pseudocode handles both via
  `__typename` plus a field-presence fallback (never dropping an entry). This is a documentation
  check against the pseudocode — no fixtures are authored here.
- **OPTIONAL — opportunistic live capture (informational only, NEVER a blocker).** WHEN a live PR
  that exhibits both shapes happens to be reachable, capture the real JSON to corroborate the
  documented shapes and refine field names if reality diverges:
```bash
# (a) gh pr view flat-array shape — IF a PR with BOTH GitHub-Actions checks and a legacy
#     commit-status (StatusContext) is reachable (best-effort; absence is NOT a blocker):
gh pr view <number> --repo <owner>/<name> --json statusCheckRollup \
  | tee project-plans/issue20/.completed/fixtures/pr_view_status_rollup.json || \
  echo "live pr_view rollup capture UNAVAILABLE — shape documentation in component-002 stands"
# (b) gh api graphql connection shape (contexts.nodes with __typename + inline fragments), best-effort:
gh api graphql -f query='query($o:String!,$n:String!,$p:Int!){repository(owner:$o,name:$n){
  pullRequest(number:$p){ statusCheckRollup{ contexts(first:100){ nodes{ __typename
    ... on CheckRun { name status conclusion detailsUrl }
    ... on StatusContext { context state targetUrl } } } } } } }' \
  -F o=<owner> -F n=<name> -F p=<number> \
  | tee project-plans/issue20/.completed/fixtures/graphql_status_rollup.json || \
  echo "live graphql rollup capture UNAVAILABLE — shape documentation in component-002 stands"
```
- PASS criteria: the component-002 shape table documents BOTH `CheckRun` and `StatusContext` shapes
  with field names that match the verified GitHub schema, and the parser pseudocode handles both.
  Live capture is recorded WHEN available and used only to corroborate. This preflight does NOT author
  fixtures and does NOT wire anything into P07 — it only documents/verifies the shapes (NO
  forward-phase dependency). If a live capture is obtained AND diverges from the component-002 shape
  table, record a Blocker FAIL with a plan revision to the parser pseudocode. The committed fixtures
  and their P07 test wiring are authored in P07 (see plan/07-github-client-tdd.md) and verified in
  P07A.

#### 2b. PR draft-filter search-qualifier grounding (Finding 3 — qualifier MUST be proven before P06)

The PR list filter supports an `is_draft: Option<bool>` criterion (`PrFilter`, domain-model.md L69)
that `build_pr_search_query` (component-002 L59-73) compiles into a GitHub search qualifier. The
qualifier MUST be the one that ACTUALLY works in the SAME `search(type: ISSUE, query: ...)` GraphQL
endpoint the PR list path uses (component-002 L49-56) — NOT a guessed token. This step PROVES the
exact qualifier and that it filters SERVER-SIDE (inside the search query) so cursor pagination /
`pageInfo` is preserved (no client-side post-filter that would corrupt `endCursor`/`hasNextPage`).

VERIFIED at plan time against a real repo with both draft and non-draft PRs (`cli/cli`) using the
identical GraphQL `search(type: ISSUE, query: $q)` envelope the plan emits:

```bash
# draft:true  -> ALL results isDraft=true   (server-side filter, paginated)
gh api graphql -f query='query($q:String!,$first:Int!){search(type:ISSUE,query:$q,first:$first){
  issueCount nodes{... on PullRequest{number isDraft}}}}' \
  -F q="repo:cli/cli is:pr draft:true"  -F first=10 \
  --jq '{count:.data.search.issueCount, drafts:[.data.search.nodes[].isDraft]}'
# draft:false -> ALL results isDraft=false
gh api graphql -f query='query($q:String!,$first:Int!){search(type:ISSUE,query:$q,first:$first){
  issueCount nodes{... on PullRequest{number isDraft}}}}' \
  -F q="repo:cli/cli is:pr draft:false" -F first=10 \
  --jq '{count:.data.search.issueCount, drafts:[.data.search.nodes[].isDraft]}'
# is:draft -> equivalent to draft:true (alias) but has NO negation form (so NOT used: we need
#             a symmetric Some(true)/Some(false) mapping, which draft:true/draft:false provides)
gh api graphql -f query='query($q:String!,$first:Int!){search(type:ISSUE,query:$q,first:$first){
  issueCount nodes{... on PullRequest{number isDraft}}}}' \
  -F q="repo:cli/cli is:pr is:draft"    -F first=10 \
  --jq '{count:.data.search.issueCount, drafts:[.data.search.nodes[].isDraft]}'
```

VERIFIED RESULT (cli/cli at plan time): `draft:true` → 158 PRs, every `isDraft=true`;
`draft:false` → 4106 PRs, every `isDraft=false`; `is:draft` → 158 PRs, every `isDraft=true`;
the no-qualifier control → 4264 = 158 + 4106 (the partition is exact). CONCLUSION: the canonical,
symmetric, server-side qualifier is `draft:true` / `draft:false` in the `search(query:...)` string —
exactly as component-002 `build_pr_search_query` L71 already emits. `is:draft` is only a one-sided
alias (no `is:false`), so it is NOT used. (This is qualifier syntax distinct from how the existing
`issue_search_query` builds `state:open`/`state:closed` for ISSUE state — `src/github/parse.rs`
L561-578 — but follows the same "compose qualifiers into the search-query string" pattern.)

- PASS criteria: `draft:true` returns ONLY `isDraft=true` PRs and `draft:false` returns ONLY
  `isDraft=false` PRs through the GraphQL `search(type: ISSUE, query:...)` endpoint, confirming the
  qualifier is honored SERVER-SIDE (pagination-preserving). The implementation (component-002
  `build_pr_search_query`) and the P07 fixture-backed test
  (`test_build_pr_search_query_emits_draft_qualifier`) use `draft:true`/`draft:false`. If a future
  GitHub change breaks this (the live check fails), record a Blocker FAIL and revise
  `build_pr_search_query` + its test to whatever the re-run proves correct BEFORE P06 implements the
  client. Draft filtering MUST remain server-side (in the query string); a client-side post-filter is
  NOT acceptable because it would drop rows from a page and corrupt `endCursor`/`hasNextPage`
  pagination semantics.

#### 2c. PR review-decision + checks-status search-qualifier grounding (Finding 1 — qualifiers MUST be proven before P06)

Issue #20 requires filtering PRs by status AND common review/workflow signals. The `PrFilter` adds
two aggregate signal criteria (domain-model.md PrFilter): `review_decision: ReviewDecisionFilter`
(Any/Approved/ChangesRequested/ReviewRequired/None) and `checks_status: ChecksFilter`
(Any/Success/Failing/Pending). `build_pr_search_query` (component-002 L59-73) compiles the non-`Any`
variants into GitHub search qualifiers. Like the draft qualifier (§2b), these MUST be the qualifiers
that ACTUALLY work in the SAME `search(type: ISSUE, query: ...)` GraphQL endpoint the PR list path
uses, filtering SERVER-SIDE so cursor pagination / `pageInfo` is preserved (no client-side
post-filter that would corrupt `endCursor`/`hasNextPage`).

VERIFIED qualifiers (GitHub search syntax, server-side, mirroring the §2b verification approach
against a real repo such as `cli/cli`):

```bash
# review decision -> review:approved | review:changes_requested | review:required | review:none
for rv in approved changes_requested required none; do
  gh api graphql -f query='query($q:String!,$first:Int!){search(type:ISSUE,query:$q,first:$first){
    issueCount nodes{... on PullRequest{number reviewDecision}}}}' \
    -F q="repo:cli/cli is:pr review:$rv" -F first=10 \
    --jq "{qualifier:\"review:$rv\", count:.data.search.issueCount,
           decisions:[.data.search.nodes[].reviewDecision]}"
done
# checks rollup status -> status:success | status:failure | status:pending
for st in success failure pending; do
  gh api graphql -f query='query($q:String!,$first:Int!){search(type:ISSUE,query:$q,first:$first){
    issueCount nodes{... on PullRequest{number
      statusCheckRollup{ contexts(first:1){ checkRunCount } } }}}}' \
    -F q="repo:cli/cli is:pr status:$st" -F first=10 \
    --jq "{qualifier:\"status:$st\", count:.data.search.issueCount}"
done
```

MAPPING (component-002 `build_pr_search_query` emits, omitting the qualifier entirely when the field
is `Any`):

| `PrFilter` field / variant | search qualifier emitted | omitted when |
|----------------------------|--------------------------|--------------|
| `review_decision = Approved` | `review:approved` | `review_decision = Any` |
| `review_decision = ChangesRequested` | `review:changes_requested` | |
| `review_decision = ReviewRequired` | `review:required` | |
| `review_decision = None` | `review:none` | |
| `checks_status = Success` | `status:success` | `checks_status = Any` |
| `checks_status = Failing` | `status:failure` | |
| `checks_status = Pending` | `status:pending` | |

- PASS criteria: each `review:<x>` and `status:<x>` qualifier is accepted by the GraphQL
  `search(type: ISSUE, query:...)` endpoint and filters SERVER-SIDE (pagination-preserving), so
  `build_pr_search_query` can compose them into the query string alongside `draft:*` and the existing
  `state:*`/text qualifiers while preserving `endCursor`/`hasNextPage`. The `Any` variant emits NO
  qualifier (no filtering on that signal). The implementation (component-002 `build_pr_search_query`)
  and the P07 tests (`test_build_pr_search_query_emits_review_and_checks_qualifiers`) use exactly
  these qualifiers. If a future GitHub change breaks any qualifier (the live check fails), record a
  Blocker FAIL and revise `build_pr_search_query` + its test to whatever the re-run proves correct
  BEFORE P06 implements the client. Review/checks filtering MUST remain server-side (in the query
  string); a client-side post-filter is NOT acceptable because it would drop rows from a page and
  corrupt `endCursor`/`hasNextPage` pagination semantics.

#### 2d. PR comments object-path grounding (Finding 1 — `repository.pullRequest` vs `repository.issue`)

The existing issue comment fetcher `GhClient::list_comments` (src/github/mod.rs L211-268) queries the
GraphQL path `repository(owner:,name:){ issue(number:){ comments(first, after) {...} } }`. PR Mode
adds a NEW `list_pr_comments` that fetches PR comments. This step PROVES which object path returns PR
comments, so the plan does NOT reuse `list_comments` verbatim and silently return zero comments.

VERIFIED LIVE (at plan time, against a real PR — e.g. a recent PR in this repo): for a PR NUMBER,
`repository.issue(number: N)` resolves to NULL (the node does not exist as an issue), so
`repository.issue(number: N).comments` yields NO comments. `repository.pullRequest(number: N)` exists
and `repository.pullRequest(number: N).comments` returns the PR comment timeline with the IDENTICAL
node + pageInfo shape used by the issue path.

```bash
# (a) PR number via repository.issue -> NULL pullRequest-as-issue -> comments unavailable:
gh api graphql -f query='query($o:String!,$n:String!,$num:Int!){repository(owner:$o,name:$n){
  issue(number:$num){ number comments(first:1){ totalCount } } }}' \
  -F o=<owner> -F n=<name> -F num=<pr-number> \
  --jq '{issue:.data.repository.issue}'          # EXPECT: issue == null (NOT_FOUND for a PR number)
# (b) Same PR number via repository.pullRequest -> real comments connection:
gh api graphql -f query='query($o:String!,$n:String!,$num:Int!){repository(owner:$o,name:$n){
  pullRequest(number:$num){ number comments(first:30){ totalCount
    nodes{ id databaseId author{login} createdAt lastEditedAt body }
    pageInfo{ hasNextPage endCursor } } } }}' \
  -F o=<owner> -F n=<name> -F num=<pr-number> \
  --jq '{count:.data.repository.pullRequest.comments.totalCount}'   # EXPECT: real comment count
# (c) Comment CREATE via the REST issue-comment endpoint DOES accept a PR number (PRs are issues
#     for the REST comments API) — confirm reachability (do not actually post in preflight):
gh api --method GET /repos/<owner>/<name>/issues/<pr-number>/comments --jq 'length'  # EXPECT: ok
```

- PASS criteria: query (a) returns `issue == null` for a PR number (confirming the issue path is the
  WRONG object for PRs), and query (b) returns the PR's real comment connection
  (`totalCount`/`nodes`/`pageInfo`) with the same node shape as the issue path. Therefore
  `list_pr_comments` (component-002 lines 102-107) MUST query `repository.pullRequest(number:)
  .comments(first, after) { nodes{...} pageInfo{hasNextPage endCursor} totalCount }`, REUSING the
  existing `parse_comments_json`/`parse_page_info` helpers and the `IssueComment` type — and MUST NOT
  reuse the issue `GhClient::list_comments`. Query (c) confirms the REST `/issues/{n}/comments`
  endpoint accepts a PR number, so `create_pr_comment` (REST POST) is correct and kept. If a future
  GitHub change alters this (live check (a) returns a non-null issue, or (b) fails), record a Blocker
  FAIL and revise `list_pr_comments` BEFORE P06 implements the client. P07 backs this with
  `test_list_pr_comments_query_targets_pull_request_not_issue` (asserts the emitted query string
  contains `pullRequest(` and NOT `issue(`).

### 3. Interface existence
```bash
test -f src/state/types.rs && test -f src/state/mod.rs
test -f src/input.rs && test -f src/messages.rs
test -d src/messages && test -f src/messages/issues_conversion.rs
test -f src/app_input/mod.rs && test -f src/app_input/normal.rs && test -f src/app_input/gh_async.rs
test -f src/github/mod.rs && test -f src/layout.rs
test -f src/ui/orchestration.rs && test -f src/ui/screens/issues.rs
test -d src/ui/components && test -d src/ui/screens
# new identifiers must NOT yet exist:
! rg -q "DashboardPullRequests" src/state/types.rs
! rg -q "PullRequestsState"     src/state/types.rs
! rg -q "MessageDomain::PullRequests" src/messages.rs
! rg -q "apply_prs_message"     src/state/mod.rs
```

### 4. Concrete file-level signature verification (CURRENT lines)

#### 4a. `src/state/types.rs`
```bash
rg -n "enum ScreenMode"      src/state/types.rs   # expect ~L227: Dashboard, Split, DashboardIssues
rg -n "struct AppState"      src/state/types.rs   # expect ~L247
rg -n "issues_state"         src/state/types.rs   # expect ~L277
rg -n "enum IssueFocus"      src/state/types.rs   # expect ~L285
rg -n "enum DetailSubfocus"  src/state/types.rs   # expect ~L296
rg -n "struct IssuesState"   src/state/types.rs   # expect ~L368 (field template for PullRequestsState)
```
- Confirm `ScreenMode` currently has exactly `{Dashboard, Split, DashboardIssues}` (NOT just
  Dashboard/Split — that was issue15's older snapshot).

#### 4b. `src/input.rs`
```bash
rg -n "enum InputMode"           src/input.rs   # expect ~L9: 11 variants incl Issues*
rg -n "fn input_mode_for_state"  src/input.rs   # expect ~L45
rg -n "DashboardIssues"          src/input.rs   # expect ~L64 (block to mirror)
rg -n "fn route_search_key"      src/input.rs   # expect ~L89 (reuse for PR search)
```

#### 4c. `src/messages.rs` + `src/messages/issues_conversion.rs`
```bash
rg -n "enum MessageDomain"  src/messages.rs   # expect ~L18
rg -n "enum IssuesMessage"  src/messages.rs   # expect ~L113 (PR template)
rg -n "enum AppMessage"     src/messages.rs   # expect ~L283
rg -n "fn name"             src/messages.rs   # expect ~L318
rg -n "impl From<AppEvent> for AppMessage" src/messages.rs   # expect ~L483
rg -n "from_issues_event"   src/messages.rs   # expect ~L583
rg -n "from_app_event"      src/messages/issues_conversion.rs
```

#### 4d. `src/state/mod.rs`
```bash
rg -n "fn apply_message"               src/state/mod.rs   # expect ~L342 (reducer hub)
rg -n "fn terminal_blocks"             src/state/mod.rs   # expect ~L380
rg -n "fn finalize_message"            src/state/mod.rs   # expect ~L395
rg -n "fn select_repository_by_index"  src/state/mod.rs   # expect ~L488
rg -n "reset_issues_for_repo_change"   src/state/mod.rs   # expect ~L497 (hook to mirror)
rg -n "apply_issues_message"           src/state/issues_ops.rs
```

#### 4e. `src/app_input/mod.rs` + `normal.rs` + `gh_async.rs`
```bash
rg -n "fn dispatch_app_message"        src/app_input/mod.rs   # expect ~L420
rg -n "fn apply_and_persist"           src/app_input/mod.rs   # expect ~L216
rg -n "fn handle_normal_key_event"     src/app_input/normal.rs # expect ~L85
rg -n "fn resolve_mode_key"            src/app_input/normal.rs # expect ~L296 (i/I, s/S arms)
rg -n "handle_dashboard_issues_key"    src/app_input/normal.rs # expect ~L156 (to mirror)
rg -n "fn spawn_gh_task_with_panic"    src/app_input/gh_async.rs # expect ~L11
```

#### 4f. `src/github/mod.rs` + `src/layout.rs` + `src/ui/orchestration.rs`
```bash
rg -n "struct GhClient|enum GhError|fn list_comments|fn create_comment" src/github/mod.rs
rg -n "ISSUES_SIDEBAR_WIDTH|fn issues_pane_rows|fn issues_detail_viewport_rows" src/layout.rs
rg -n "fn build_screen_element|DashboardIssues" src/ui/orchestration.rs
```

### 5. Call-path feasibility
```bash
# count match sites that must gain a PR arm:
rg -c "ScreenMode::" src/                       # all exhaustive matches to update
rg -c "AppMessage::" src/app_input/mod.rs       # dispatch sites
rg -n "match .*\.domain\(\)|match message" src/state/mod.rs   # reducer hub match
rg -n "input_mode_for_state" src/                # call sites
```
- Enumerate every exhaustive `match` over `ScreenMode`, `AppMessage`, `MessageDomain`, and
  `InputMode` that must gain a PR arm; record the file:line list for the stub phase.

### 6. Test infrastructure
```bash
rg -n "#\[cfg\(test\)\]" src/state/issues_ops.rs src/github/mod.rs   # inline test pattern present
rg -n "#\[path = " src/state/mod.rs                                  # external test module pattern
cargo test --workspace --all-features --locked 2>&1 | tail -5        # baseline green
```

### 7. Workspace quality-gate baseline (binding canonical five-command baseline)

This is the canonical, BINDING five-command baseline that every phase re-runs (declared verbatim
here so P00A grounds it once, fully). All five MUST pass before any work begins — `cargo test
--workspace --all-features --locked` is part of THIS binding block, not a separately-tailed command
(finding #4):
```bash
cargo fmt --all --check
cargo clippy --workspace --all-targets --all-features -- -D warnings
bash scripts/check-clippy-allows.sh
cargo build --workspace --all-features --locked
cargo test --workspace --all-features --locked
```
- PASS criteria: all five commands above exit 0 (baseline is GREEN before work begins). The
  superset `make ci-check` additionally runs the coverage gate (`--fail-under-lines 30`); run it
  when `cargo llvm-cov` is available (see check #1).

## Blocker Gate Decision

Record `PASS` only if checks 1–7 succeed. On any failure record `FAIL` with:
- which check failed,
- root cause,
- remediation,
- whether the plan must be revised (e.g. line numbers drifted → update `00-overview.md` integration
  map and the affected phase docs).

## Structural Verification Checklist
- [ ] All target files/dirs exist.
- [ ] All cited current signatures present at (approximately) the cited lines.
- [ ] New identifiers confirmed NOT yet present.
- [ ] Match sites requiring PR arms enumerated with file:line.

## Semantic Verification Checklist (Mandatory)
- [ ] `ScreenMode` currently includes `DashboardIssues` (current snapshot, not stale).
- [ ] Message bus present (`MessageDomain`, `AppMessage`, `apply_message`) — PR plan targets it.
- [ ] `spawn_gh_task_with_panic` is the async entry the PR loaders will reuse.
- [ ] Layout module exposes issues helpers to mirror; no need to redefine constants.
- [ ] Baseline `make ci-check` is green before work begins.

## Deferred Implementation Detection
This preflight scan is intentionally RECORD-ONLY (informational), NOT a hard inverted gate: it scans
the whole `project-plans/issue20/` tree, whose plan prose legitimately discusses words like
"placeholder" (e.g. the P03 benign-placeholder render arm) and "TODO" in meta-discussion, so a hard
gate here would false-fail. The hard inverted deferred-implementation gates (finding #6) apply to the
production-code/deliverable scans in the impl and verifier phases, not to this preflight prose scan.
```bash
rg -n "TODO|FIXME|HACK|placeholder|for now|will be implemented" project-plans/issue20/
```
(Expected: only intentional references inside plan prose, none implying unfinished plan content.)

## Success Criteria
- Blocker Gate = PASS, or a documented FAIL with remediation + plan revision applied.

## Failure Recovery
- If signatures drifted: update `00-overview.md` integration map + affected phase docs, re-run P00A.

## Phase Completion Marker (`.completed/P00A.md`)
Contents: phase ID, timestamp, toolchain/gh versions, check outputs, blocker-gate verdict,
enumerated match sites, baseline ci-check result.

---

## Appendix — Signatures the Plan Depends On

| Identifier | Kind | File | Approx line | Plan dependency |
|-----------|------|------|------|-----------------|
| `ScreenMode` | enum | src/state/types.rs | 227 | add `DashboardPullRequests` |
| `AppState` | struct | src/state/types.rs | 247 | add `prs_state` |
| `IssuesState` | struct | src/state/types.rs | 368 | field template for `PullRequestsState` |
| `InputMode` | enum | src/input.rs | 9 | add `Prs*` variants |
| `input_mode_for_state` | fn | src/input.rs | 45 | add `DashboardPullRequests` block |
| `route_search_key` | fn | src/input.rs | 89 | reuse for PR search |
| `MessageDomain` | enum | src/messages.rs | 18 | add `PullRequests` |
| `IssuesMessage` | enum | src/messages.rs | 113 | template for `PullRequestsMessage` |
| `AppMessage` | enum | src/messages.rs | 283 | add `PullRequests(..)` |
| `apply_message` | fn | src/state/mod.rs | 342 | add `PullRequests` arm |
| `select_repository_by_index` | fn | src/state/mod.rs | 488 | add `reset_prs_for_repo_change` |
| `dispatch_app_message` | fn | src/app_input/mod.rs | 420 | add `PullRequests` arms |
| `handle_normal_key_event` | fn | src/app_input/normal.rs | 85 | add PR-mode branch |
| `resolve_mode_key` | fn | src/app_input/normal.rs | 296 | add `p`/`P` arm |
| `spawn_gh_task_with_panic` | fn | src/app_input/gh_async.rs | 11 | reuse for PR loaders |
| `GhClient` / `GhError` | type | src/github/mod.rs | — | add PR methods / reuse error |
| `build_screen_element` | fn | src/ui/orchestration.rs | — | add `DashboardPullRequests` arm |

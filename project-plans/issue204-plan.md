# Issue #204 — Issues mode: close with a reason with duplicate-issue search

## Goal

Deliver the carved-out follow-up to #182 that #188 partially implemented: let
the user close an issue with a GitHub-native reason (completed / not planned /
duplicate of #N) using the GraphQL `closeIssue` mutation as the single close
path, and display the issue's `stateReason` in the list and detail views.

## Relationship to #188 and #182

- #182 shipped close + delete via `gh issue close` (REST by number).
- #188 (closed as duplicate of #204) added a close-reason chooser overlay and
  the duplicate search, layered on top of the REST path: it calls
  `gh issue close --reason` plus a separate `markIssueAsDuplicate` GraphQL
  mutation. It scoped out the read side (parsing/displaying `stateReason`) and
  left the close path on REST.
- #204 explicitly asks to (a) migrate the close path to the GraphQL
  `closeIssue` mutation so `stateReason` and `duplicateIssueId` are first-class
  in a single call, and (b) parse and display `stateReason`.

This PR completes #204: GraphQL `closeIssue` migration + read-side
`stateReason`.

## Decisions

- Exactly one code path for the `closeIssue` mutation. The existing REST
  `gh issue close --reason` path is replaced by a single GraphQL `closeIssue`
  mutation that carries `stateReason` and (for Duplicate) `duplicateIssueId`.
  The plain-close (`C`) path migrates to the same mutation with a default
  reason of `COMPLETED`, matching GitHub's own plain-close behavior.
- The `markIssueAsDuplicate` two-step mutation is removed: `closeIssue` with
  `stateReason: DUPLICATE` + `duplicateIssueId` is the single, native API for
  marking a duplicate.
- `stateReason` is a first-class read-side field. New `IssueStateReason` domain
  enum (`Completed`, `NotPlanned`, `Duplicate`) parsed from the GraphQL
  `stateReason` field and the REST `state_reason` field. `Option<>` on the
  domain types so existing fixtures default to `None` without churn beyond the
  literal addition.
- `CloseReason::Invalid` is retained for chooser parity but maps to
  `NOT_PLANNED` at the GraphQL layer (GitHub has no native "invalid" close
  reason); the display reason stays `NotPlanned`.
- The duplicate search candidates are seeded from the already-loaded issue list
  (open issues, same repo). Network fetching of closed issues for the search is
  explicitly a non-goal here; the existing #188 behavior is preserved.
- A TUI harness scenario is added/updated for the close-with-reason chooser,
  following the test-first rule for UI-visible work.

## Acceptance matrix

| ID | Actor / path | Input and boundary cases | Target | Observable success | Failure behavior / side effects | Compatibility | Evidence |
|---|---|---|---|---|---|---|---|
| A1 | Close with reason (Completed) | Focused open issue, choose Completed | GraphQL `closeIssue` boundary | `closeIssue(input:{issueId, stateReason: COMPLETED})` is built; issue transitions to Closed | MutationFailed clears pending + scoped error; no partial state | Plain `C` close now uses the same path with default COMPLETED | `build_close_issue_graphql_*` unit tests |
| A2 | Close with reason (Not planned / Invalid) | Choose Not planned or Invalid | GraphQL `closeIssue` boundary | `stateReason: NOT_PLANNED` is built; Invalid maps to NOT_PLANNED | MutationFailed clears pending + scoped error | Invalid remains a chooser option | `build_close_issue_graphql_*` unit tests |
| A3 | Close as Duplicate of #N | Choose Duplicate, search + resolve canonical number | GraphQL `closeIssue` boundary | `stateReason: DUPLICATE` + resolved `duplicateIssueId` (canonical node id) in a single mutation | Node-id resolution failure surfaces GraphQL error; close does not proceed | Single mutation replaces close+mark-duplicate two-step | `build_close_issue_graphql_duplicate` unit test |
| A4 | Plain close (`C`) | Focused open issue, plain close key | GraphQL `closeIssue` boundary | Same `closeIssue` mutation with default `COMPLETED` | MutationFailed clears pending + scoped error | Existing close keybind preserved | Reducer + dispatch parity tests |
| A5 | Parse stateReason (list, GraphQL) | Issue JSON with `stateReason: COMPLETED/NOT_PLANNED/DUPLICATE` | GitHub parse boundary | `Issue.state_reason` is `Some(reason)`; missing/null/unknown → `None` | No panic on unknown values; defaults to None | Existing parse unaffected | `parse_issues_json` state_reason tests |
| A6 | Parse stateReason (detail, REST `state_reason`) | `gh issue view --json state_reason` values | GitHub parse boundary | `IssueDetail.state_reason` is `Some(reason)`; missing/null/unknown → `None` | No panic; defaults to None | Existing detail parse unaffected | `parse_issue_detail_json` state_reason tests |
| A7 | Display stateReason in issue list | Closed issue with a reason | Issue list projection | Meta line shows reason (e.g. `CLOSED·not planned`, `CLOSED·duplicate`) for closed issues with a reason | Open issues show plain `OPEN`; closed w/o reason show `CLSD` | List layout/density unchanged | `issue_list_visible_rows` meta tests |
| A8 | Display stateReason in issue detail | Closed issue detail with a reason | Issue detail header projection | State row shows humanized reason (e.g. `CLOSED (not planned)`, `CLOSED (duplicate of #N)`) | Open/closed-without-reason unchanged | Header row count unchanged | `issue_detail_header_view` reason tests |
| A9 | Chooser reachable from list and detail | List focus + detail focus | Reducer/overlay | Close-reason chooser opens in both focus contexts | Blocked by other overlay/mutation | Matches #182 close parity | Existing `issues_tests_close_reason` |

## Explicit non-goals

- Reopening with a reason (`REOPENED`).
- Bulk close.
- Editing `stateReason` post-close without reopening.
- Network-fetched duplicate search across open+closed issues (kept as loaded-list-only, matching #188).
- Changing the property editor state-transition path (#175 coordination is structural only).
- Changing the duplicate search UX or candidate seeding.

## Bounded vertical slices

### Slice 1 — Domain + parse: `stateReason` read side (A5, A6)

- Acceptance: A5, A6.
- Owner: domain + GitHub parse boundary.
- Allowed files: `src/domain/issues.rs`, `src/github/parse.rs`, parse test files.
- RED: parse tests expect `state_reason` populated; fail (field absent).
- GREEN: `IssueStateReason` enum + parsing into `Issue`/`IssueDetail`.
- Stop condition: requires UI/state/runtime changes.

### Slice 2 — Display: stateReason in list + detail (A7, A8)

- Acceptance: A7, A8.
- Owner: UI pure projections.
- Allowed files: `src/ui/components/issue_list.rs`, `src/ui/components/issue_detail.rs`, projection tests.
- RED: projection tests expect reason in meta/header rows.
- GREEN: reason rendered for closed issues with a reason.
- Stop condition: requires state or runtime changes.

### Slice 3 — GraphQL closeIssue migration (A1–A4)

- Acceptance: A1–A4.
- Owner: GitHub lifecycle boundary + dispatch + reducer.
- Allowed files: `src/github/issue_lifecycle.rs`, `src/github/mod.rs`, `src/app_input/issues_lifecycle.rs`, `src/domain/issues.rs` (reason→graphql mapping), lifecycle tests.
- RED: GraphQL command-construction tests for each reason + duplicate.
- GREEN: single `closeIssue` mutation; plain close defaults to COMPLETED; duplicate carries `duplicateIssueId`.
- Stop condition: requires UI/state changes beyond the pending event payload.

### Slice 4 — Exact-head qualification

- Acceptance: all rows.
- Owner: repository quality gates.
- Allowed files: only in-scope fixes discovered by verification/review.
- GREEN: `make quick-check`, `make ci-check`, review triage, exact-head PR CI pass.
- Stop condition: unplanned subsystem/abstraction/dependency/tooling change, unrelated test movement, or scope budget breach.

## Expected paths and scope ledger

| Path | Layer / purpose | Acceptance | Status |
|---|---|---|---|
| `project-plans/issue204-plan.md` | Delivery plan and evidence ledger | all | Planned |
| `src/domain/issues.rs` | `IssueStateReason` enum + `CloseReason::graphql_state_reason` + `Issue`/`IssueDetail` fields | A1–A8 | Planned |
| `src/github/parse.rs` | Parse `stateReason`/`state_reason` into domain | A5, A6 | Planned |
| `src/github/issue_lifecycle.rs` | GraphQL `closeIssue` mutation builder + client method | A1–A4 | Planned |
| `src/github/mod.rs` | Re-exports | A1–A4 | Planned |
| `src/app_input/issues_lifecycle.rs` | Dispatch via GraphQL `closeIssue` | A1–A4 | Planned |
| `src/ui/components/issue_list.rs` | Display reason in meta line | A7 | Planned |
| `src/ui/components/issue_detail.rs` | Display reason in header state row | A8 | Planned |

Struct-literal updates across test fixtures are mechanical consequences of the
new domain field and are tracked in the scope ledger as in-scope mechanical
churn (not new behavior).

## Scope ledger

- Adding `state_reason: Option<IssueStateReason>` to `Issue` and `IssueDetail`
  requires updating struct literals in ~40 test/fixture files. This is
  mechanical churn from the accepted domain change, not new behavior. Will be
  committed in focused slices.

## Review counters

- Local Open Code Review: 0/2.
- Post-PR Open Code Review: 0/2.

## Verification evidence

- Base: `issue204` created from `origin/main`.
- Issue and all comments fetched with `gh issue view 204 --json ...`.
- (To be filled as slices complete.)

## Review findings and deferred work

- None yet.

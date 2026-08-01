# Issue #187 — Issues screen: show when an issue already has linked PR(s)

## Goal

Show a visible, emoji-free indicator in the Issues **list** when an issue has one
or more linked pull requests, so the user can see at a glance which issues are
already being worked on while triaging. Linked PR numbers are sourced in bulk
from the existing GraphQL issue search query via the `timelineItems`
(`CROSS_REFERENCED_EVENT`) connection — no per-issue network calls and no N+1.

## Decisions

- **Bulk GraphQL, no new transport.** Linked PR numbers are fetched by adding a
  bounded `timelineItems(first: 15, itemTypes: [CROSS_REFERENCED_EVENT])`
  selection to the shared `issue_list_node_selection()` used by BOTH the
  `search(type: ISSUE)` path and the `repository.issues(filterBy:)` path. This
  is the approach the issue recommends ("fetching `timelineItems` with a
  `first:` limit in the existing issue list GraphQL query"). It avoids N+1 and
  adds no new client method or network round-trip.
- **Domain field on the list type only.** A new `linked_pr_numbers: Vec<u64>`
  field on `Issue` carries the parsed PR numbers. It is populated only by the
  GraphQL search/repository parse path; the deprecated legacy `gh issue list
  --json` path and REST detail parse leave it empty (graceful degradation).
- **Parse, don't drop.** A `parse_linked_pr_numbers` pure function walks the
  `timelineItems.nodes`, keeps only `CrossReferencedEvent` nodes whose `source`
  is a `PullRequest`, reads `source.number`, and de-duplicates while preserving
  first-seen order. It is a TOTAL function (missing/empty timeline → empty vec).
- **Emoji-free text marker.** The list meta line gains a `linked:#N` marker
  (multiple PRs render as `linked:#1,#2`). It is emitted only when the vec is
  non-empty, matching the existing conditional meta parts (`comments`,
  `assigned:`, `[labels]`). A pure `format_linked_prs` helper keeps the marker
  construction side-effect-free and unit-testable.
- **List scope only this PR.** The issue allows "list and/or detail". The list
  is the triage surface named in the motivation. The detail view uses REST
  (`gh issue view --json`) which has no `timelineItems`; surfacing linked PRs in
  the detail would require either a new GraphQL timeline fetch (new transport)
  or a new `IssueDetail` field + hydrate-from-list-row plumbing. Both materially
  increase the blast radius (the `IssueDetail` struct-literal churn alone is
  ~13 extra files) past the hard scope budget. Detail surfacing is recorded as
  an explicit non-goal / follow-up.

## Acceptance matrix

| ID | Actor / path | Input and boundary cases | Target | Observable success | Failure behavior / side effects | Compatibility | Evidence |
|---|---|---|---|---|---|---|---|
| A1 | Parse linked PRs (single PR cross-ref) | Issue search node with one `CROSS_REFERENCED_EVENT` whose `source` is a `PullRequest` | GitHub parse boundary | `Issue.linked_pr_numbers == [#123]` | No panic; missing timeline → `[]` | Existing parse fields unchanged | `parse_linked_pr_numbers` unit test |
| A2 | Parse excludes non-PR cross-refs | `CROSS_REFERENCED_EVENT` whose `source` is an `Issue`; mixed PR+issue refs | GitHub parse boundary | Only `PullRequest` sources retained | Non-PR refs silently skipped | N/A | `parse_linked_pr_numbers` unit test |
| A3 | Parse de-duplicates | Two events referencing the same PR number | GitHub parse boundary | Single entry, first-seen order preserved | N/A | N/A | `parse_linked_pr_numbers` unit test |
| A4 | Parse degrades on missing/empty | Node with no `timelineItems`, empty `nodes`, or null `source` | GitHub parse boundary | `linked_pr_numbers == []` | No panic | N/A | `parse_linked_pr_numbers` unit test |
| A5 | GraphQL selection includes timeline | The shared node selection string | issue_query boundary | Selection contains `timelineItems(first: 15, itemTypes: [CROSS_REFERENCED_EVENT])` with `... on CrossReferencedEvent { source { ... on PullRequest { number } } }` | N/A | Both search + repository paths | node-selection assertion test |
| A6 | List meta line shows marker (one PR) | Open issue, `linked_pr_numbers=[#123]` | Issue list pure projection | Meta line contains `linked:#123` | Empty vec adds nothing | List layout/density unchanged | `build_meta_line` / `issue_list_visible_rows` test |
| A7 | List meta line shows nothing when empty | `linked_pr_numbers=[]` | Issue list pure projection | Meta line has no `linked:` part | N/A | Existing meta parts unchanged | `issue_list_visible_rows` test |
| A8 | List meta line shows multiple PRs | `linked_pr_numbers=[#1,#2]` | Issue list pure projection | Meta line contains `linked:#1,#2` | N/A | N/A | `issue_list_visible_rows` test |
| A9 | End-to-end list parse from search JSON | Full `data.search.nodes` fixture with timelineItems | GitHub parse boundary | `parse_issue_search_json` yields issues with populated `linked_pr_numbers` | Missing timeline → empty | Paginated parse unchanged | `parse_issue_search_json` test |

## Explicit non-goals

- Surfacing linked PRs in the **issue detail** view (REST-based; would need new
  transport or `IssueDetail` field + hydrate — recorded as a follow-up).
- Navigating from the indicator to the linked PR in PR mode (explicitly optional
  in the issue).
- Filtering/sorting the issue list by "has linked PR".
- Same-repository filtering of linked PR numbers (cross-repo refs are included;
  the marker is a "has linked PR(s)" hint).
- Changes to the deprecated legacy `gh issue list --json` path (stays empty).

## Bounded vertical slices

### Slice 1 — Domain + parse: linked-PR read side (A1–A5, A9)

- Acceptance: A1, A2, A3, A4, A5, A9.
- Owner: domain + GitHub parse/query boundary.
- Allowed files: `src/domain/issues.rs`, `src/github/parse.rs`,
  `src/github/issue_query.rs`, `src/github/create_issue.rs`, parse test files.
- RED: parse + node-selection tests fail (field/selection absent).
- GREEN: `linked_pr_numbers` field, `parse_linked_pr_numbers`, selection string.
- Stop condition: requires UI/state changes beyond the list projection.

### Slice 2 — Display: list marker (A6–A8)

- Acceptance: A6, A7, A8.
- Owner: UI pure projection.
- Allowed files: `src/ui/components/issue_list.rs`, projection tests.
- RED: projection tests expect `linked:` marker.
- GREEN: `format_linked_prs` + meta-line part.
- Stop condition: requires state/runtime changes.

### Slice 3 — Exact-head qualification

- Acceptance: all rows.
- Owner: repository quality gates.
- Allowed files: only in-scope fixes from verification/review.
- GREEN: `cargo xtask quick`, `cargo xtask ci`, review triage, exact-head PR CI.
- Stop condition: unplanned subsystem/abstraction/dependency/tooling change,
  unrelated test movement, or scope-budget breach.

## Expected paths and scope ledger

| Path | Layer / purpose | Acceptance | Status |
|---|---|---|---|
| `project-plans/issue187-plan.md` | Delivery plan + evidence ledger | all | Planned |
| `src/domain/issues.rs` | `linked_pr_numbers: Vec<u64>` on `Issue` | A1–A9 | Planned |
| `src/github/parse.rs` | `parse_linked_pr_numbers` + wire into `parse_issue_from_item` | A1–A4, A9 | Planned |
| `src/github/issue_query.rs` | `timelineItems` in node selection | A5 | Planned |
| `src/github/create_issue.rs` | struct-literal update (`into_list_issue`) | — | Mechanical |
| `src/ui/components/issue_list.rs` | `format_linked_prs` + meta-line marker | A6–A8 | Planned |
| ~22 `src/**` test/fixture files | struct-literal field addition | — | Mechanical |
| 4 `tests/github_client/**` files | struct-literal field addition | — | Mechanical |

Struct-literal updates across test fixtures are mechanical consequences of the
new domain field and are tracked here as in-scope mechanical churn (not new
behavior), mirroring the issue #204 precedent.

## Scope review (above 25-file target)

The change touches ~28 files. Above the 25-file target but each non-implementation
file change is a single mechanical field addition to a test fixture required by
the domain-type change. Well under the 40-file hard stop. No unplanned
subsystem, abstraction, dependency, tooling, or unrelated-test move is introduced.

## Review counters

- Local Open Code Review: 0/2.
- Post-PR Open Code Review: 0/2.

## Verification evidence

- Base: `issue187` created from `origin/main`.
- (To be filled as slices complete.)

## Review findings and deferred work

- None yet.

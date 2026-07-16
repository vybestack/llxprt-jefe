# Issue #336 — Compare GitHub timestamps as parsed instants

## Goal

Sort issue, pull-request, and PR-review lists newest-first by the instant represented by each RFC 3339 timestamp, regardless of fractional-second precision or UTC-offset spelling, while retaining the existing tie-breaker keys.

## Decisions

- The GitHub boundary owns one shared, pure RFC 3339 comparison helper used by all three list sorters.
- No date-time crate is currently present in `Cargo.toml` or `Cargo.lock`. Adding one requires separate approval, so the helper will parse the bounded RFC 3339 fields needed for comparison without changing dependencies.
- Valid timestamps sort by their UTC instant. Equivalent instants compare equal even when their offsets or fractional precision differ, so the existing sorter-specific tie-breaker decides their order.
- Valid timestamps sort before malformed or missing timestamps. Malformed timestamps retain deterministic reverse-lexicographic ordering among themselves; equal malformed timestamps use the existing sorter-specific tie-breaker. This preserves empty timestamps on the older side.
- RFC 3339 comparison accepts uppercase or lowercase `T`/`Z`, required numeric UTC offsets in `±HH:MM` form, arbitrary fractional precision, calendar-valid dates, and leap-second syntax.
- A TUI harness scenario is not applicable: this fixes pure GitHub-boundary list ordering and adds no interaction, rendering, or UI-state contract. Direct behavioral sorter tests exercise the exact data consumed by the existing list UI.

## Acceptance matrix

| ID | Actor / path | Input and boundary cases | Target | Observable success | Failure behavior / side effects | Compatibility | Evidence |
|---|---|---|---|---|---|---|---|
| A1 | Issues list sorter | Whole seconds, fractional seconds, and mixed precision | Local GitHub boundary; platform-neutral | Issues are ordered by parsed `updated_at` instant, newest first | Malformed/missing timestamps sort after valid timestamps; no side effects | Number-ascending tie-breaker unchanged | Focused `sort_issues` behavior tests |
| A2 | Pull-request list sorter | `Z`, `+00:00`, and non-zero offsets | Local GitHub boundary; platform-neutral | PRs are ordered by parsed `updated_at` instant, newest first | Malformed/missing timestamps sort after valid timestamps; no side effects | Number-ascending tie-breaker unchanged | Focused `sort_pull_requests` behavior tests |
| A3 | PR review sorter | Different offsets representing different and equal instants | Local GitHub boundary; platform-neutral | Reviews are ordered by parsed `submitted_at` instant, newest first | Malformed/missing timestamps sort after valid timestamps; attached threads remain on parents | Review-ID-descending then author-ascending tie-breakers unchanged | Focused `sort_pr_reviews` behavior tests |
| A4 | All timestamp sorters | Equivalent instants expressed with different offset/precision forms | Local GitHub boundary; platform-neutral | Parsed timestamp comparison returns equality and each existing tie-breaker determines order | No I/O, persistence, or diagnostics | Existing public sort APIs unchanged | Tie-breaker regression tests with equivalent instants |
| A5 | Shared parser/comparator | Invalid dates/times/offsets, lowercase markers, long fractions, leap second | Pure GitHub helper; platform-neutral | Accepted RFC 3339 forms compare chronologically; invalid forms use the documented deterministic fallback | Never panics; no partial state or side effects | No dependency or schema changes | Helper unit tests plus full verification |

## Explicit non-goals

- Changing newest-first ordering.
- Changing issue/PR number or review ID/author tie-breaker keys.
- Altering GitHub JSON parsing, fetches, pagination, persistence, or UI rendering.
- Introducing a general date-time API outside the GitHub boundary.
- Adding or changing dependencies.
- Reformatting unrelated timestamp display values.

## Bounded vertical slices

### Slice 1 — Behavioral RED for all affected sorters

- Acceptance: A1–A4.
- Owner: GitHub boundary sorter behavior.
- Allowed files: `src/github/tests_timestamp_sort.rs`, `src/lib.rs`.
- RED: mixed precision/offset tests fail under raw `String::cmp`.
- GREEN criterion: deferred to slice 2.
- Stop condition: evidence requires UI/state/runtime changes.

### Slice 2 — Shared parsed-instant comparison

- Acceptance: A1–A5.
- Owner: pure GitHub timestamp comparison integrated into existing sorters.
- Allowed files: `src/github/timestamp.rs`, `src/github/mod.rs`, `src/github/parse.rs`, `src/github/parse_pr.rs`, and slice-1 tests.
- RED: slice-1 failures.
- GREEN: all focused timestamp-sort tests pass, including unchanged tie-breakers and malformed fallback.
- REFACTOR: keep parsing helpers focused and dependency-free; add parser boundary tests.
- Stop condition: dependency, public API, or cross-layer changes become necessary.

### Slice 3 — Exact-head qualification

- Acceptance: all rows.
- Owner: repository quality gates.
- Allowed files: only in-scope fixes discovered by verification/review; scope ledger updated first.
- GREEN: `make quick-check`, `make ci-check`, review triage, and exact-head PR CI all pass.
- Stop condition: unplanned subsystem/public abstraction/dependency/tooling change, unrelated test movement, or scope budget breach.

## Expected paths and scope ledger

| Path | Layer / purpose | Acceptance | Status |
|---|---|---|---|
| `project-plans/issue336-plan.md` | Delivery plan and evidence ledger | A1–A5 | Planned |
| `src/github/tests_timestamp_sort.rs` | Cross-sort behavioral evidence | A1–A4 | Planned |
| `src/lib.rs` | Test-module registration only | A1–A4 | Planned |
| `src/github/timestamp.rs` | Shared pure RFC 3339 instant comparison | A1–A5 | Planned |
| `src/github/mod.rs` | Private helper module registration | A1–A5 | Planned |
| `src/github/parse.rs` | Issue sorter integration | A1, A4 | Planned |
| `src/github/parse_pr.rs` | PR and review sorter integration | A2–A4 | Planned |

Budget target: 7 files total, under 500 net changed lines. No workflow, agent-memory, dependency, quality-tool, persistence, runtime, state, or UI files.

## Scope ledger

- No discoveries beyond the accepted scope.

## Review counters

- Local Open Code Review: 2/2 attempts. Both verified-environment runs were terminated by the shell timeout without output and are not treated as successful reviews or approval.
- Post-PR Open Code Review: 0/2.

## Verification evidence

- Base: `issue336` created from `origin/main` at `c4f36f6`; initial divergence `0/0`.
- Issue and all comments fetched with `gh issue view 336 --comments`.
- Dependency research: no `chrono`, `time`, or `jiff` package in the direct manifest or lockfile.
- RED evidence: `cargo test -q github_tests_timestamp_sort -- --nocapture` failed all 3 new tests under raw string comparison, with observed orders `[4, 3, 1, 2]`, `[3, 2, 1]`, and `[PRR_3, PRR_2, PRR_4]` instead of parsed-instant order.
- Focused GREEN evidence: 3 mixed-form sorter tests and 5 parser/comparator unit tests pass.
- `make quick-check`: passed (2,095 library tests plus all integration and doctest targets).
- `make ci-check`: passed with format, policy, source-size, Clippy, 72.43% line coverage, locked build, and locked tests successful.
- Exact-head PR workflows: pending.

## Review findings and deferred work

- None yet.

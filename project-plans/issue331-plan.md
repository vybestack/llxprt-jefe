# Issue 331 Plan: Refine git-info integration tests

## Issue summary

Issue 331 reorganizes the public `git_info` integration target introduced by pull request 330 into cohesive parsing, dirty-status, and real-repository modules. It also strengthens the NUL-delimited Y-column rename regression so the assertion can only remain clean when the parser recognizes the Y-column rename and consumes its second path. The four private subprocess-timeout tests remain white-box unit tests under `src/git_info/tests.rs`.

## Acceptance matrix

| Row | Actor / launch path | Input and boundary | Target | Observable success | Observable failure | Side effects / compatibility | Behavioral proof |
| --- | --- | --- | --- | --- | --- | --- | --- |
| A1 | `cargo test --test git_info` | Existing public origin parsing and display inputs | All platforms | Parsing/display tests run from a focused parsing module with unchanged assertions and count | Missing, renamed, or failing tests | Test-only file movement; public production APIs unchanged | Inventory test names before/after and run the integration target |
| A2 | `cargo test --test git_info` | Synthetic porcelain streams, including malformed and rename/copy records | All platforms | Dirty-status tests run from a focused module with existing coverage preserved | A prior scenario disappears or behavior changes | Test-only organization except the strengthened assertion | Inventory test names before/after and run the integration target |
| A3 | Dirty-status regression test | ` R .jefe/new.md\0.jefe/old.md\0` followed by an owned ordinary record | All platforms | Result is clean only when Y-column rename recognition consumes both rename paths and parsing resumes at the next record | Missing Y-column recognition treats the second path as a record and fails dirty | No production behavior change | Strengthened `z_y_column_rename_consumes_second_path` plus a deliberate local mutant check proving it fails when Y-column detection is removed |
| A4 | `cargo test --test git_info` | Real temporary repositories, remote short-circuit, arrow filenames where supported | Unix and Windows-compatible portions | Real-repository tests and helpers run from one focused module with platform gates preserved | Temp-repository coverage is lost or platform gates drift | Existing filesystem/git side effects remain scoped to tempdirs | Run the integration target and compare test inventory/count |
| A5 | `cargo test --lib git_info::tests` and full suite | Four private timeout-helper cases | Unix white-box unit target | Exactly four timeout tests remain in `src/git_info/tests.rs` and pass | Tests move to public integration modules, disappear, or fail | Private API remains private; no visibility changes | Source inventory plus focused library tests and full verification |

## Explicit non-goals

- No production parser, git probing, cache, timeout, or display behavior changes.
- No public API or visibility changes.
- No dependency, workflow, lint, coverage, source-size, agent-memory, or configuration changes.
- No TUI behavior or TUI scenario changes; this issue affects only non-UI integration-test organization and assertions.
- No relocation or cleanup of unrelated tests.

## Vertical slices

### Slice 1: Strengthen Y-column second-path proof

- Acceptance row: A3.
- Owner / boundary: public integration tests exercising `jefe::git_info::porcelain_is_dirty`.
- Allowed path: `tests/git_info.rs` before the module split, then `tests/git_info/dirty_status.rs`.
- Test-first evidence: replace the early-dirty real-path rename case with an all-owned rename followed by another owned record; validate test effectiveness against a deliberate local parser mutant that ignores Y-column rename/copy status, then remove the mutant.
- Green criteria: the strengthened test passes against unchanged production behavior.
- Verification: focused test invocation.
- Stop condition: any production behavior change is needed.

### Slice 2: Split the public integration target

- Acceptance rows: A1, A2, A4, A5.
- Owner / boundary: Rust integration-test target organization only.
- Allowed paths: `tests/git_info.rs`, `tests/git_info/parsing.rs`, `tests/git_info/dirty_status.rs`, `tests/git_info/real_repository.rs`.
- Test-first evidence: the strengthened regression from slice 1 is established before relocation; module declarations and inventory checks prove all existing tests remain reachable.
- Green criteria: all 88 public tests (92 functions including four real-repository helpers) remain discoverable, focused modules are cohesive, and four private timeout tests remain in place.
- Verification: `cargo test --test git_info`, `cargo test --lib git_info::tests`, `make quick-check`, and `make ci-check`.
- Stop condition: moving unrelated tests, changing production code, or requiring a new shared public abstraction.

## Expected paths

| Path | Purpose | Acceptance rows |
| --- | --- | --- |
| `tests/git_info.rs` | Small integration-target root declaring three focused modules and shared test support | A1, A2, A4 |
| `tests/git_info/parsing.rs` | Origin parsing and display/list projection tests | A1 |
| `tests/git_info/dirty_status.rs` | Synthetic porcelain and dirty-marker tests, including strengthened Y-column regression | A2, A3 |
| `tests/git_info/real_repository.rs` | Real temporary-repository helpers and tests plus resolve boundaries | A4 |
| `project-plans/issue331-plan.md` | Acceptance, scope, review, and verification ledger | All |
| `src/git_info/tests.rs` | Intentionally unchanged home of four private timeout tests | A5 |

## Scope ledger

| Discovery | Disposition | Files | Approval status |
| --- | --- | --- | --- |
| Initial issue scope | Accepted | Expected paths above | Issue-authorized |

No unplanned work discovered.

## Review counters

- Pre-PR Open Code Review runs used: 2 of 2; both external OCR processes were terminated before producing output. A CodeRabbit CLI fallback was also externally terminated during review with no findings emitted. Automated PR review remains required and will be triaged in the PR loop.
- Post-PR Open Code Review runs used: 0 of 2.

## Verification evidence

- Baseline inventory: 88 public integration tests (92 functions including four helpers) and 4 private timeout unit tests before changes.
- Mainline drift at slice start: `origin/main` is 0 commits ahead; branch descends from `origin/main`.
- Focused RED/effectiveness check: strengthened Y-column test failed against a deliberate local X-column-only mutant; mutant removed with zero production diff.
- Focused GREEN tests: `cargo test --test git_info` passed all 88 tests; `cargo test --lib git_info::tests` passed all 4 private tests; baseline/current function inventories match exactly.
- `make quick-check`: passed.
- `make ci-check`: passed with explicit exit 0.
- Exact-head CI: pending.

## Deferred findings and follow-ups

None.

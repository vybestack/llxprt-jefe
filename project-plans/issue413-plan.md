# Issue 413 delivery plan — couple moved tests to source constants and close pagination boundaries

- GitHub: https://github.com/vybestack/llxprt-jefe/issues/413
- Branch: `issue413`
- Base: `origin/main` at `462cb13`
- Review counters: OCR pre-PR 1/2, OCR post-PR 0/2
- Delivery shape: one bounded PR; expected 5 changed files and fewer than 200 net changed lines.

## Summary

Keep the moved HTML entity-window test coupled to its production scan limit,
add explicit pagination overflow/zero-boundary regression coverage, and record
that `markdown_html_strip` is public only for integration-test access rather
than a supported documented API. The module will remain importable by the
existing integration target but will be hidden from generated crate docs.

## Acceptance matrix

| Row | Actor / launch path | Input and boundary | Target | Observable success | Observable failure / diagnostic | Side effects before failure | Persistence / compatibility | Behavioral evidence |
|---|---|---|---|---|---|---|---|---|
| AC-01 | `markdown_html_strip` integration target | 3-byte and 4-byte characters straddling the entity scan window | local/CI; platform-independent pure Rust | both malformed entities pass through without panic, with filler lengths derived from `MAX_ENTITY_LEN` | test target fails to compile if the constant is inaccessible; value/output mismatch reports in the test assertion | none | no persisted state; existing stripping behavior unchanged | existing `entity_window_on_multibyte_boundary_does_not_panic` test imports and uses `MAX_ENTITY_LEN` |
| AC-02 | pagination integration target | `PageToken::after_page(u32::MAX, true)` | local/CI; platform-independent pure Rust | returns `PageToken::Done` without wrapping | equality assertion reports a non-`Done` continuation | none | no persisted state; existing API and behavior unchanged | new max-page overflow regression test |
| AC-03 | pagination integration target | `ListRequestId::default()`, `from_raw(0)`, and `checked_next()` | local/CI; platform-independent pure Rust | default equals raw zero and increments to raw one | equality assertion identifies default/zero drift or off-by-one/overflow behavior | none | no persisted state; existing API and behavior unchanged | strengthened zero-boundary/default-equivalence regression test |
| AC-04 | crates.io/rustdoc consumer | generated docs for the test-exposed `markdown_html_strip` module | local/CI docs; platform-independent | module remains available to integration tests but is omitted from generated public API docs; source docs state it is not a supported stable surface | rustdoc exposes the module, or focused integration tests cannot import it | none | additive public symbols remain link-compatible; documented public API is narrowed only by hiding an unintended test seam | `cargo doc --no-deps` plus focused integration-target compilation |
| AC-05 | contributor / CI | complete candidate-head repository gate | local and GitHub CI | all required format, policy, Clippy, coverage, build, and test gates pass | failing command names the affected gate | only build artifacts under `target/` | no dependency, workflow, quality-gate, or persisted-data changes | `make ci-check` locally and required PR checks on the exact head |

## Non-goals

- No changes to pagination production behavior; the implementation already uses
  checked arithmetic and this issue adds missing characterization coverage.
- No redesign or relocation of the integration test targets created by #368.
- No stability/API changes for `ISSUE_DETAIL_JSON_FIELDS`,
  `assign_threads_to_reviews`, or any adjacent public module; those surfaces are
  outside this issue's acceptance criteria.
- No new feature flags, test-only subsystems, dependencies, workflow changes,
  quality-gate changes, or `.llxprt/` changes.
- No UI behavior or TUI scenario changes; all accepted behavior is pure library
  logic with direct integration coverage.

## Architectural decision

`markdown_html_strip` was widened only so the integration suite could exercise
its byte-level state machine. Keep the existing public path for that suite, add
`#[doc(hidden)]` at the module export, and explicitly describe the surface as
unsupported/internal in the module docs. Publish `MAX_ENTITY_LEN` through that
same hidden module so the boundary test can use the production source of truth.
This is smaller than introducing a parallel test-support module or feature and
keeps the documented crate API minimal without moving tests back into the lib
target.

## Vertical slices

### S1 — Entity-window coupling and API-surface decision

- Rows: AC-01, AC-04.
- Owner/boundary: byte-level HTML stripping module exposed through the crate
  integration-test boundary.
- Allowed files: `tests/markdown_html_strip/strip_tests.rs`,
  `src/markdown_html_strip.rs`, `src/lib.rs`.
- RED: import/use `MAX_ENTITY_LEN` in the integration test while it remains
  private; focused compilation must fail with the private-item diagnostic.
- GREEN: expose the documented constant through the hidden module, hide the
  module from rustdoc, and make the focused target pass.
- Non-goals: no scanner behavior changes and no adjacent API cleanup.
- Verification: `cargo test --test markdown_html_strip` and
  `cargo doc --no-deps` with generated-doc inspection.
- Stop for approval if this requires a new test-support abstraction, feature,
  dependency, or changes outside the allowed files.

### S2 — Pagination overflow and zero boundaries

- Rows: AC-02, AC-03.
- Owner/boundary: pagination integration contract tests.
- Allowed file: `tests/pagination/page_token_tests.rs`.
- RED evidence: the baseline lacks named max-page and raw-zero/default boundary
  cases. Because production already implements the accepted checked arithmetic,
  these are characterization-only test additions and are expected to pass when
  introduced; no production implementation is authorized for this slice.
- GREEN: both explicit boundary tests pass and would fail on wrapping or
  off-by-one regressions.
- Non-goals: no pagination source changes or adjacent pagination scenarios.
- Verification: `cargo test --test pagination`.
- Stop for approval if a production behavior change or test relocation appears
  necessary.

### S3 — Candidate-head qualification and delivery

- Row: AC-05; confirms all preceding rows.
- Owner/boundary: repository quality and PR delivery gates.
- Allowed files: plan evidence only; implementation changes require mapping to
  an earlier row or approved scope-ledger entry.
- GREEN: focused tests, rustdoc inspection, `make ci-check`, review triage, exact
  ancestry, conflict-free PR, and required exact-head CI all pass.
- Non-goals: optional hardening or cleanup after accepted behavior is complete.
- Stop for approval on any unplanned subsystem/public abstraction, workflow,
  agent-memory, quality-tool, dependency, unrelated refactor/test move, behavior
  outside the matrix, or hard-budget breach.

## Expected files

| Layer | Path | Planned change |
|---|---|---|
| Delivery record | `project-plans/issue413-plan.md` | acceptance matrix, scope, review counters, evidence |
| Library export | `src/lib.rs` | hide test-only module from generated docs |
| HTML-strip owner | `src/markdown_html_strip.rs` | record stability decision and expose scan-limit source of truth |
| HTML-strip integration test | `tests/markdown_html_strip/strip_tests.rs` | import and use `MAX_ENTITY_LEN` |
| Pagination integration test | `tests/pagination/page_token_tests.rs` | add max-page and zero/default boundary evidence |

## Scope ledger

| Date | Discovery | Disposition |
|---|---|---|
| 2026-07-25 | `markdown_html_strip` was made public solely to support the integration-test move in #368 | In scope: retain accessibility but hide it from rustdoc and state that it is unsupported/internal |
| 2026-07-25 | Pagination production code already uses `checked_add` and the requested outputs already hold | In scope: add characterization tests only; production pagination changes are explicitly excluded |
| 2026-07-25 | Issue mentions other public symbols widened in #368 but does not require their stability policy in acceptance | Defer: no adjacent API cleanup in issue #413 |

## Verification evidence

- Baseline at `462cb13`: `cargo test --test markdown_html_strip --test pagination`
  passed (13 HTML-strip tests, 9 pagination tests).
- S1 RED: `cargo test --test markdown_html_strip` failed with E0603 because
  `MAX_ENTITY_LEN` was private.
- S1/S2 GREEN: `cargo test --test markdown_html_strip --test pagination
  --all-features --locked` passed (13 HTML-strip tests, 10 pagination tests).
- Rustdoc visibility: `cargo doc --no-deps` succeeded and
  `markdown_html_strip` was absent from the generated crate index/module path.
  Existing repository-wide rustdoc-link warnings remain outside this issue.
- Fast gate: `make quick-check` passed.
- Candidate-head full gate: `RUST_TEST_THREADS=8 make ci-check` passed; the environment variable bounded test concurrency without changing the gate or skipping tests.
- Exact-head GitHub CI: pending.

## Review triage

- Local OCR run 1 reviewed all four code/test files and generated no comments.
- No Blocker-Fix, In-scope-Fix, Reject, or Defer findings are open.

## Deferred / follow-ups

None created. The adjacent `ISSUE_DETAIL_JSON_FIELDS` and
`assign_threads_to_reviews` stability question remains outside this issue.

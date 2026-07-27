# Issue 449 delivery plan — re-sync jefe CI OCR review with newer llxprt-code OCR rigor

- GitHub: https://github.com/vybestack/llxprt-jefe/issues/449
- Branch: `issue449`
- Base: `origin/main`
- Review counters: OCR pre-PR 0/2, OCR post-PR 0/2
- Delivery shape: one bounded PR; expected 3 changed files plus 1 new test
  file and 1 new dev-doc, under 1,500 net changed lines.

## Summary

jefe's `.github/workflows/ocr-review.yml` is an older port of an llxprt-code
OCR review setup. llxprt-code has since added three rigor properties that jefe
lacks, all of which make jefe's non-blocking OCR signal more honest and more
auditable:

1. **Coverage classification** — derive `coverage` ∈
   {`complete_best_effort`, `partial`, `failed`, `unknown`} from the OCR
   result.json `status` fields, so a zero-exit run that reports
   `completed_with_errors` is classified `partial` and is never summarized as
   clean.
2. **Reproducibility manifests** — write `manifest.pre.json` (scope, resolved
   refs, OCR version, redacted provider config + its sha256, worktree state)
   and `manifest.post.json` (exit code, reported statuses, coverage
   classification, artifact hashes) plus a `sha256.txt` over uploaded
   artifacts, so any CI run is independently auditable.
3. **Versioned finding-evaluation rubric** — move the hypothesis/validity/
   disposition/dedupe/comparison-eligibility rubric out of agent memory and
   into a versioned `dev-docs/code-review-process.md` linked from
   `CONTRIBUTING.md`.

The local-tooling/scope-modes surface (issue slice A4) and the pr-review
delivery process are explicitly out of scope (tracked in #451).

## Acceptance matrix

| Row | Actor / launch path | Input and boundary | Target | Observable success | Observable failure / diagnostic | Side effects before failure | Persistence / compatibility | Behavioral evidence |
|---|---|---|---|---|---|---|---|---|
| AC-01 | `tests/ocr_review_policy.rs` contract test | `ocr-review.yml` after the OCR run | local/CI; platform-independent pure Rust test over repository text | the workflow contains a coverage-classification step that maps `completed_with_errors` to `partial` and never treats a partial/unknown run as clean | contract test fails, naming the missing mapping | none | no dependency, quality-gate, or persisted-data change | `cargo test --test ocr_review_policy` |
| AC-02 | `tests/ocr_review_policy.rs` contract test | `ocr-review.yml` upload-artifact `path:` list | local/CI; platform-independent pure Rust test over repository text | the workflow uploads `manifest.pre.json`, `manifest.post.json`, and `sha256.txt` as artifacts | contract test fails, naming each missing artifact path | none | no dependency, quality-gate, or persisted-data change | `cargo test --test ocr_review_policy` |
| AC-03 | `tests/ocr_review_policy.rs` contract test | `ocr-review.yml` job graph vs `ci.yml` required jobs | local/CI; platform-independent pure Rust test over repository text | the OCR job is observational: no required-gate job in `ci.yml` depends on it | contract test fails if a `ci.yml` required job grows a `needs:` edge to the OCR job | none | no dependency, quality-gate, or persisted-data change | `cargo test --test ocr_review_policy` |
| AC-04 | `tests/ocr_review_policy.rs` contract test | `dev-docs/code-review-process.md` markdown sections | local/CI; platform-independent pure Rust test over repository text | the rubric doc defines validity (`valid`/`partial`/`invalid`/`unverifiable`), disposition (`fix`/`explain`/`defer`/`user-judgment`), and comparison-eligibility terms | contract test fails, naming the missing phrase | none | no dependency, quality-gate, or persisted-data change | `cargo test --test ocr_review_policy` |
| AC-05 | `tests/ocr_review_policy.rs` contract test | `CONTRIBUTING.md` | local/CI; platform-independent pure Rust test over repository text | the contributor entry point links to `dev-docs/code-review-process.md` | contract test fails if the link is missing | none | no dependency, quality-gate, or persisted-data change | `cargo test --test ocr_review_policy` |
| AC-06 | CI OCR run on a PR | OCR result.json with `status: completed_with_errors` and exit code 0 | GitHub Actions `OCR Review` workflow | the posted sticky summary reports an incomplete/partial coverage state, not "No findings." or a clean status | summary line omits coverage and reads clean | only build artifacts under `target/` and workflow-run artifacts | no dependency, quality-gate, or persisted-data change | workflow YAML inspection + the contract test guard |
| AC-07 | CI OCR run on a PR | any OCR run that produces parseable JSON | GitHub Actions `OCR Review` workflow | `manifest.post.json` reports a non-`unknown` coverage value whenever OCR produced parseable JSON | coverage stays `unknown` on a parseable result | only workflow-run artifacts | no dependency, quality-gate, or persisted-data change | workflow YAML inspection + the contract test guard |

## Non-goals

- No local OCR wrapper, local `make ocr-*` target, or in-repo
  `.opencodereview/rule.json` (issue slice A4; gated, requires explicit
  approval).
- No change to the pr-review delivery process (#451).
- No change to the fork-safety model, the changed-test scope guard, the
  pinned OCR version, or the infra-failure notification job's existing
  behavior beyond surfacing coverage.
- No change to CI required gates; OCR remains observational and
  non-blocking.
- No new dependency, no new quality-gate script, no `.llxprt/` change.
- No rewrite of the posting step's inline/lineless comment logic; coverage is
  surfaced in the summary block only.

## Architectural decision

jefe's OCR workflow already classifies failures into `policy_failure` vs
`infrastructure_failure` and already redacts diagnostic artifacts fail-closed.
The re-sync extends that existing structure rather than replacing it:

- **Coverage (A2)** is computed in the existing `Post OCR results`
  `actions/github-script` step (Node), right after parsing `ocr-result.json`,
  using the same `findingsFromParsed` JSON already in scope. The four-state
  classification is derived from the OCR status fields the same way
  `ocr-review-local` derives them, and surfaced as a new `Coverage:` line in
  the sticky summary. The existing `ran`/`exitCode` logic is preserved; a
  `partial`/`unknown` coverage overrides the "No findings." clean summary.
- **Manifests (A1)** are produced by one new step placed after the OCR run and
  before the existing `Redact OCR diagnostic artifacts` step, so the existing
  fail-closed redaction applies to manifest provider fields too. The manifest
  files are added to the `diagnosticArtifacts` redaction list and the upload
  `path:` list, so they travel with the existing artifact bundle.
- **Rubric (A3)** is a new `dev-docs/code-review-process.md` linked from the
  existing `Code review demand` section of `CONTRIBUTING.md`, mirroring how
  `code-review-demand.md` is already linked.

This is the smallest change that lands the three rigor properties inside
jefe's existing ownership boundaries.

## Vertical slices

### S1 — RED contract tests (Option B)

- Rows: AC-01 … AC-05.
- Owner/boundary: repository contract tests over workflow YAML + dev-docs
  markdown, mirroring `tests/coderabbit_policy.rs`.
- Allowed file: `tests/ocr_review_policy.rs`.
- RED evidence: the test file asserts coverage-classification mapping,
  manifest upload paths, the non-blocking-gate invariant, the rubric doc
  sections, and the CONTRIBUTING link — none of which exist yet, so the new
  tests fail for the intended reason.
- GREEN: deferred to S2/S3/S4; S1 commits the failing tests as the contract.
- Non-goals: no workflow or doc edits in this slice.
- Verification: `cargo test --test ocr_review_policy` (expected failures).
- Stop for approval if the contract assertions require a new test-support
  abstraction, feature, dependency, or changes outside the allowed file.

### S2 — Coverage classification (A2)

- Rows: AC-01, AC-06, AC-07.
- Owner/boundary: `Post OCR results` step in `.github/workflows/ocr-review.yml`.
- Allowed file: `.github/workflows/ocr-review.yml`.
- RED: S1 contract test AC-01.
- GREEN: derive `coverage` from the parsed OCR JSON statuses; add a
  `Coverage:` summary line; ensure a `partial`/`unknown` run is never
  summarized as "No findings." or clean; write coverage into
  `manifest.post.json` (once S3 lands).
- Non-goals: no change to inline/lineless posting or to the infra-failure
  notification logic.
- Verification: `cargo test --test ocr_review_policy` (AC-01 green) +
  `make quick-check`.
- Stop for approval if coverage derivation requires a new workflow job or a
  dependency change.

### S3 — Reproducibility manifests (A1)

- Rows: AC-02.
- Owner/boundary: new `Build OCR reproducibility manifests` step in
  `.github/workflows/ocr-review.yml`, before `Redact OCR diagnostic
  artifacts`.
- Allowed file: `.github/workflows/ocr-review.yml`.
- RED: S1 contract test AC-02.
- GREEN: add the manifest step; add `manifest.pre.json`,
  `manifest.post.json`, `sha256.txt` to the redaction list and the upload
  `path:` list; ensure the manifest provider config is redacted.
- Non-goals: no new artifact name outside the manifest triple; no change to
  the existing artifact names.
- Verification: `cargo test --test ocr_review_policy` (AC-02 green) +
  `make quick-check`.
- Stop for approval if manifests require a new action, image, or secret.

### S4 — Finding-evaluation rubric doc (A3)

- Rows: AC-04, AC-05.
- Owner/boundary: new `dev-docs/code-review-process.md` + a link line in
  `CONTRIBUTING.md`.
- Allowed files: `dev-docs/code-review-process.md`, `CONTRIBUTING.md`.
- RED: S1 contract tests AC-04, AC-05.
- GREEN: write the rubric doc with the required validity/disposition/
  comparison-eligibility terms; link it from the `Code review demand`
  section of `CONTRIBUTING.md`.
- Non-goals: no change to `code-review-demand.md`; the rubric is a sibling
  process doc, not a merge.
- Verification: `cargo test --test ocr_review_policy` (AC-04, AC-05 green) +
  `make quick-check`.
- Stop for approval if the rubric requires changes to standards, workflow,
  or quality-gate docs.

## Scope ledger

| Item | Classification | Disposition |
|---|---|---|
| Local OCR wrapper / `make ocr-*` / in-repo rule (issue slice A4) | Out of scope | Gated; requires explicit approval. Not started. |
| pr-review delivery process | Out of scope | Tracked in #451. |
| Scope-modes / lineage doc | Out of scope | Documented only if local tooling lands (A4). |

## Verification

- Fast iteration: `make quick-check`.
- Full gate before push/PR: `make ci-check` (fmt check, clippy gates,
  coverage `--fail-under-lines 30`, build, test).
- Focused contract test: `cargo test --test ocr_review_policy`.
- Existing contract regression: `cargo test --test coderabbit_policy`.

## Stopping rules

- Stop if coverage derivation, manifest generation, or the rubric requires a
  new workflow job, action, image, secret, dependency, or quality-gate change.
- Stop if the work crosses the hard scope budget (40 files or 2,500 net
  changed lines).
- Stop if a slice leaves the accepted file/ownership boundary.

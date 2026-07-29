# Issue #464 — OCR reproducibility manifest integrity and completeness

Follow-up to #449. PR #453 shipped the manifest triple and coverage
classification; this issue closes the A1 reproducibility-integrity gaps that a
functional-completeness audit (DeepThinker) identified against llxprt-code's
`ocr-review-local` wrapper.

## Status of prior work on this issue

- PR #482 (`bf0ff3d9`) fixed the demonstrated runtime blocker from the issue
  comment: the plain-shell manifest builder imported undeclared `@actions/core`.
  That is **done** and guarded by
  `ocr_manifest_builder_uses_dependency_free_workflow_commands`.
- Commit `bcb1045f` (for #449) reconciled the coverage doc: a zero-exit
  unparsable result is documented as `unknown`, not `failed`. That is **done**.

This plan covers the remaining open findings from the issue body.

## Acceptance matrix

| ID  | Slice | Boundary | Observable success | Observable failure / diagnostic | Side effects | Persistence | Proof |
|-----|-------|----------|--------------------|---------------------------------|--------------|-------------|-------|
| AC1 | S1 | Pre-run manifest step ordering | A workflow step named to write `manifest.pre.json` runs **before** `Run OpenCodeReview` and before the post/parse steps | The step must not be absent or placed after the run | Writes `manifest.pre.json` only | File lives until upload/redaction | Contract test asserts the pre-run step precedes the run step |
| AC2 | S1 | Pre-run manifest evidence: trusted base | `manifest.pre.json` records the checked-out trusted-base HEAD and base branch (distinct from the merge-base scope.base) | Field absent → contract fails | None (read-only git) | Captured at launch time | Contract test asserts `trusted_base` block with `head`/`branch` keys |
| AC3 | S1 | Pre-run manifest evidence: worktree state | `manifest.pre.json` records worktree clean flag and staged/unstaged/untracked diff hashes | Missing → contract fails | Reads git status/diff only | Snapshot at launch | Contract test asserts `worktree` block with `clean` + diff-hash keys |
| AC4 | S1 | Pre-run manifest evidence: fixed + scope arg vectors | `manifest.pre.json` records the fixed OCR control vector and the exact scope arg vector | Missing → contract fails | None | Recorded at launch | Contract test asserts `control_args` + `scope_args` fields |
| AC5 | S1 | Pre-run manifest evidence: rule hash | `manifest.pre.json` records the sha256 of the OCR rule.json used by CI | Missing → contract fails | Reads rule.json | Fingerprint at launch | Contract test asserts `rule_sha256` field |
| AC6 | S1 | Pre-run manifest evidence: comparison eligibility | `manifest.pre.json` records an explicit `comparison_eligible` boolean | Missing → contract fails | None | Machine-supported signal | Contract test asserts `comparison_eligible` field |
| AC7 | S2 | Post-manifest does not overwrite the pre-run snapshot | The post step writes `manifest.post.json` only; it does NOT rewrite `manifest.pre.json` | If it rewrites pre → contract fails | None | Pre snapshot preserved | Contract test asserts the post step body has no `writeFileSync('manifest.pre.json'` |
| AC8 | S2 | Post-manifest completeness | `manifest.post.json` records `run_id`, `parse_error`, and includes `manifest.pre.json` in its `artifacts` map | Missing → contract fails | None | Completeness | Contract test asserts fields and artifact key |
| AC9 | S3 | Doc: connectivity preflight carve-out | `dev-docs/code-review-process.md` reconciles the "no routine connectivity tests" rule with the CI bounded preflight by carving out an explicit CI exception | Doc still contradicts workflow → contract fails | None | Versioned contract | Contract test asserts the doc mentions the CI connectivity exception |
| AC10 | S3 | Doc: comparison eligibility is machine-supported | The doc's comparison-eligibility section reflects that the evidence is now recorded in `manifest.pre.json` | Claim is still unsupportable → contract fails | None | Versioned contract | Contract test asserts the doc references the recorded evidence fields |

## Non-goals

- Do **not** change A2 coverage classification (matches the wrapper exactly).
- Do **not** port local-only wrapper ergonomics (interactive polling, detached
  execution, workspace mode for uncommitted changes).
- Do **not** change OCR provider/model, finding-posting, or required-gate
  behavior.
- Do **not** add the optional "reporting checklist + impact grouping from the
  llxprt-code skill to the versioned rubric" (explicitly optional in the issue;
  deferred to avoid scope expansion).
- Do **not** re-litigate Finding 4 (coverage doc: zero-exit unparsable = unknown)
  — already reconciled in `bcb1045f`.
- Do **not** re-litigate the `@actions/core` runtime blocker — already fixed in
  PR #482.
- Do **not** run an unconditional `ocr review --preview` on every run (that
  would add provider cost); the rule hash + conditional preview hash capture the
  selected-file-set fingerprint.
- Do **not** modify `.llxprt/`.

## Vertical slices

### Slice S1 — True pre-run manifest step (AC1–AC6)

1. **Owner / boundary:** `.github/workflows/ocr-review.yml` step ordering; new
   step placed after `Configure OCR review rules` (so rule.json exists) and
   after the trusted checkout + merge-base resolution, but **before** `Run
   OpenCodeReview`. Contract test: `tests/core/ocr_workflow_contracts.rs`.
2. **RED:** add contract tests asserting the pre-run step exists before the run
   step and captures every required evidence field.
3. **GREEN:** add a `Write pre-run OCR reproducibility manifest` step (plain
   `node` heredoc, dependency-free, workflow-command warnings) that records:
   trusted base (HEAD = `git rev-parse HEAD` on the trusted checkout; branch =
   `pr-context` base_ref), scope (mode, base = merge-base, head = PR head),
   worktree state (clean flag + sha256 of staged/unstaged/untracked diffs),
   control_args (`--audience agent --format json --concurrency 2 --timeout 30`),
   scope_args (`--from BASE --to HEAD`), rule sha256, comparison_eligible, OCR
   version + redacted provider shape + redacted_config_sha256, run_id/attempt.
4. **Non-goals for S1:** no post-manifest rewrite; no doc changes.

### Slice S2 — Post-manifest completeness and pre-snapshot preservation (AC7–AC8)

1. **Owner / boundary:** the existing `Build OCR reproducibility manifests`
   step. Contract test: `tests/core/ocr_workflow_contracts.rs`.
2. **RED:** contract test asserting the post step does NOT write
   `manifest.pre.json` and that the post manifest carries `run_id`,
   `parse_error`, and `manifest.pre.json` in `artifacts`.
3. **GREEN:** remove the pre-manifest write from the post step; add
   `run_id`/`run_attempt`/`parse_error` to the post manifest; add
   `manifest.pre.json` to the post artifact-hash map (computed from the
   pre-run file on disk).

### Slice S3 — Doc reconciliation (AC9–AC10)

1. **Owner / boundary:** `dev-docs/code-review-process.md`. Contract test:
   `tests/ocr_review_policy.rs`.
2. **RED:** contract test asserting the doc carves out the CI connectivity
   preflight and reflects machine-supported comparison eligibility.
3. **GREEN:** update the "Provider and remediation discipline" and "Run
   comparison eligibility" sections.

## Expected files

- `.github/workflows/ocr-review.yml` (S1, S2)
- `tests/core/ocr_workflow_contracts.rs` (S1, S2 contract tests)
- `tests/ocr_review_policy.rs` (S3 contract tests)
- `dev-docs/code-review-process.md` (S3)
- `project-plans/issue464-plan.md` (this file)

## Scope budget

Target ≤ 25 files / ≤ 1,500 net changed lines. Mandatory scope review above
either; hard stop without approval above 40 files / 2,500 net lines.

## Scope ledger

| Date       | Item                                                          | Disposition |
|------------|---------------------------------------------------------------|-------------|
| 2026-07-27 | PR #482 fixed the `@actions/core` runtime blocker             | Done        |
| 2026-07-27 | Commit `bcb1045f` reconciled coverage doc (zero-exit unparsable = unknown) | Done |
| 2026-07-28 | Branch `issue464` created from main                           | Accepted    |
| 2026-07-28 | S1–S3 acceptance matrix shaped                                | Accepted    |

## Review counters

- OCR pre-PR: 0/2
- OCR post-PR: 0/2

## Verification

- `cargo fmt --all --check`
- `cargo clippy --workspace --all-targets --all-features -- -D warnings`
- `cargo build --workspace --all-features --locked`
- `cargo test --workspace --all-features --locked` (or `make ci-check`)
- Focused: `cargo test -p jefe --test integration core::ocr_workflow_contracts`
  and `cargo test -p jefe --test ocr_review_policy`

# Issue #464 — Restore OCR reproducibility manifest execution

## Scope

Issue #464 already owns OCR reproducibility-manifest integrity. PR #475 proved that
`Build OCR reproducibility manifests` always fails because a plain `node` heredoc
imports undeclared package `@actions/core`.

## Acceptance matrix

| # | Boundary | Success | Failure | Proof |
|---|---|---|---|---|
| AC1 | `Build OCR reproducibility manifests` plain-shell Node process | Runs using only Node built-ins and repository-declared inputs | Missing runtime package cannot abort manifest generation | Scoped repository contract test |
| AC2 | GitHub Actions diagnostics | Manifest chmod failures remain visible as Actions warnings | Diagnostic emission does not require an npm package | Contract test for dependency-free workflow command encoding |
| AC3 | Existing manifest/redaction/upload contract | Manifest triple, redaction, checksums, and upload paths are unchanged | No provider secret or unredacted artifact is uploaded | Existing OCR policy tests |
| AC4 | Exact-head OCR workflow | Manifest-build step passes after OCR result posting | Infrastructure failure remains classified by existing downstream steps | PR workflow evidence |

## Non-goals

- No OCR coverage-classification, model/provider, review-scope, or finding-posting changes.
- No new npm or Rust dependency.
- No redesign of the broader manifest schema tracked by issue #464.
- No workflow-gating change.

## Vertical slice

1. Add a contract test scoped to the manifest-build step proving it does not
   import `@actions/core` and emits dependency-free Actions warning commands.
2. Prove RED against current main.
3. Replace the single `core.warning` use with safe workflow-command emission.
4. Run focused OCR policy tests and full required verification.
5. Open an issue-linked PR and verify the exact-head OCR manifest step.

## Expected paths

- `.github/workflows/ocr-review.yml`
- `tests/core/ocr_workflow_contracts.rs`
- `project-plans/issue464-plan.md`

## Scope ledger

| Date | Item | Disposition |
|---|---|---|
| 2026-07-27 | Existing issue #464 confirmed as owner | Accepted |
| 2026-07-27 | Exact PR #475 failure added to issue #464 | Accepted |
| 2026-07-27 | Dedicated branch `issue464` created from merged main `eaba9f2` | Accepted |

## Review counters

- OCR pre-PR: 0/2 (one bounded GLM review completed; no OCR run requested)
- OCR post-PR: 0/2

## Review triage

- **Blocker—Fix:** none.
- **In-scope—Fix:** none. The dependency-free warning command escapes `%`, CR,
  and LF before writing to the GitHub Actions command channel.
- **Reject:** broader YAML/dependency restructuring is unnecessary for the
  two-line runtime defect and would expand issue scope.
- **Defer:** broader manifest-schema completeness remains tracked by issue #464.

## Verification evidence

- RED: focused integration contract failed because the manifest step imported
  unavailable `@actions/core`.
- GREEN: focused contract passed after replacement with built-in Node output.
- All 19 scoped OCR workflow contracts passed.
- All 7 repository OCR policy tests passed.
- `cargo fmt --all --check`, policy checks, strict Clippy, and complexity checks
  passed in the full `cargo xtask ci` attempt.
- Full local coverage/test completion is blocked by the pre-existing native
  `tests/psmux_attach.rs` terminal-input assertion; this workflow-only diff does
  not touch that route. Required exact-head Linux and Windows CI will provide
  authoritative full-suite evidence.

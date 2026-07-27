# pr-review delivery process

This document defines Jefe's **pr-review delivery process**: how a reviewer
turns review *signals* (the OCR CLI and CodeRabbit) plus **independent source
analysis** into one accountable, cross-verified review artifact, posted under
the reviewer's own account for **traceability**. It is the delivery layer on
top of the signals; it does not replace either.

This process is a Rust-tailored port of the review-delivery process used in
the llxprt-code project. It mirrors how CodeRabbit reports (a holistic
**verdict**, severity-tiered findings, and an explicit
blocking-vs-suggestion split), which the team already reads, so the output is
low-friction to consume.

## Related documents

- [CodeRabbit review-demand policy](./code-review-demand.md) — when Jefe asks
  for a CodeRabbit review and how the allowance is measured.
- [Coding Standards](./standards/coding-standards.md) — the Rust quality
  baseline (Result/Option, no unwrap/expect, no `unsafe`, lint/complexity
  thresholds) that the findings rubric below applies.
- [Architecture Standards](./standards/architecture.md) — ownership
  boundaries, unidirectional data flow, the pure-views projection pattern.
- [Testing and Quality](./standards/testing-and-quality.md) — TDD, test
  layers, the verification suite.

## What the pr-review process is

A pr-review is a **manual review posted by the reviewer's own identity**, not
an anonymous bot dump. It is produced by combining:

1. **An OCR CLI pass** over the change (Jefe's CI OCR workflow provides the
   reproducible signal; see the sibling OCR issue for manifests, coverage
   classification, and the versioned rubric).
2. **Independent source analysis.** The reviewer reads the change, the
   contracts, and the tests themselves — the OCR pass is a hypothesis
   generator, not the verdict.
3. **Cross-verification of every finding against the current source.** Each
   finding is re-checked against the file/symbol/line at the reviewed head
   before it is posted. Findings that do not hold against the current source
   are dropped or corrected. This cross-verification is the defining property
   that distinguishes a pr-review from a raw OCR dump.
4. **A holistic verdict** (fully fixed / partially fixed / not fixed, with a
   concise justification) plus **guardrail confirmations**.
5. **Severity-tiered findings** with a blocking-vs-suggestion split at the
   end.

The review is posted under the reviewer's own account so there is a single,
accountable identity for every posted review.

## How to perform a pr-review

1. Confirm the change has passed the required local gate (`make ci-check`)
   on the exact head being reviewed, and that the PR head SHA matches.
2. Read the issue acceptance matrix, the non-goals, and the scope ledger.
3. Run (or read) the OCR pass over the change. Note the OCR coverage state
   (`complete_best_effort` / `partial` / `failed` / `unknown`) once the
   sibling OCR issue lands coverage classification; today, treat the OCR run
   as best-effort and say so in the verdict.
4. Perform independent source analysis. For each OCR finding and each
   concern the reviewer sees independently: re-open the file at the reviewed
   head, confirm the path/symbol/line, and confirm the finding still holds.
5. Apply the Rust findings rubric below to classify each finding.
6. Run the Rust guardrail-confirmation checklist (next section).
7. Post the review using the template below, under the reviewer's own
   account, with the OCR coverage state reflected in the verdict.

## Rust findings rubric

Jefe is a Rust project. Findings are evaluated against the project's quality
baseline, not against a generic style guide:

- **Error handling:** prefer `Result`/`Option` and typed errors. No
  `.unwrap()`/`.expect()` in production paths. Never silently discard an
  error. Absence is `Option`; fallible operations are `Result`.
- **Safety:** `unsafe` is forbidden (`Cargo.toml` sets
  `unsafe_code = "forbid"`).
- **Determinism:** keep state transitions deterministic and side effects at
  boundary modules (runtime, persistence, GitHub). The pure-views projection
  pattern must be preserved.
- **Architecture:** respect the ownership boundaries and the dependency DAG
  in [Architecture Standards](./standards/architecture.md). Flag any
  unidirectional-data-flow violation or contract leak.
- **Fail-fast preference:** prefer fail-fast over defense-in-depth. Do not
  add layers of if/then guards, fallbacks, or error swallows to hedge
  against possible upstream bugs — find and fix the actual bug instead.
  (Defensive handling remains acceptable for genuinely external/unpredictable
  inputs: third-party data formats, network I/O, OS-level variance, untrusted
  input parsing.)
- **TDD / coverage:** flag missing behavioral tests for new behavior, and
  tests that assert mock call counts instead of observable behavior.

Each finding is a hypothesis until cross-verified. The versioned
finding-evaluation rubric (hypothesis -> verify -> validity -> disposition ->
dedupe, plus local-vs-PR comparison-eligibility) is owned by the sibling OCR
issue (#449, slice A3) and will live in its own section of this document once
landed; this section defines the Rust-specific evaluation criteria only.

## Rust guardrail-confirmation checklist

A pr-review must explicitly confirm these guardrails, because they are the
mechanically-checkable invariants that keep Jefe's quality floor from
regressing. Each item names the repo command or file that makes it
mechanically checkable. Confirm each one in the verdict's guardrail block.

- `cargo fmt --all --check` is clean.
- `clippy --workspace --all-targets --all-features -- -D warnings` is clean.
- **No new clippy allows** — `scripts/check-clippy-allows.sh` passes (there
  is no exception ledger; a new `#[allow(clippy::...)]` /
  `#[expect(clippy::...)]` / `cfg_attr(..., allow(clippy::...))` is a
  blocker).
- No lint/complexity rule weakening — no increase to any threshold in
  `clippy.toml` or `.github/clippy/clippy.toml`, no severity downgrade, no
  new exclusion from `scripts/check-source-file-size.sh` or the complexity
  gates.
- No source/test exclusion — no new `ignores`/path filter that excludes
  first-party `src` or `tests` from review, lint, or coverage.
- `.llxprt/` untouched — version-controlled project memories, settings, and
  skills must not be deleted, removed, or modified by the change.
- `make ci-check` is green on the exact reviewed head.

If any guardrail is violated, the verdict must say so and the violation is
treated as a blocking finding regardless of severity tier.

## Review template

Use the following CodeRabbit-style structure, adapted to Rust. The preamble
states provenance and traceability; the body delivers a holistic verdict,
tiered findings, and guardrail confirmations; the tail separates blocking
items from suggestions.

```markdown
_Review generated by an automated code-review pass (OCR CLI + independent
source analysis), posted under my account for traceability. Findings
cross-verified against the source at the reviewed head. OCR coverage state:
<complete_best_effort | partial | failed | unknown | best-effort (pre-#449)>._

## Overall verdict

<fully fixed / partially fixed / not fixed> — <one- or two-sentence
justification referencing the issue acceptance matrix>.

I also confirmed:
- `cargo fmt --all --check` clean
- `clippy ... -D warnings` clean
- no new clippy allows (`scripts/check-clippy-allows.sh` passes)
- no lint/complexity rule weakening
- no source/test exclusion
- `.llxprt/` untouched
- `make ci-check` green on head <SHA>

## Findings

Each finding uses the heading shape `### N. [Severity] <one-line title> —
path/to/file.rs:LINE`, where `[Severity]` is one of the tiers below.

### 1. [High] <one-line title> — `path/to/file.rs:LINE`
<root-cause explanation referencing the Rust rubric: Result/Option, no
unwrap/expect, no unsafe, deterministic transitions, fail-fast, architecture,
TDD.>
<minimal fix: a diff, a test suggestion, or a concrete instruction.>

### 2. [Medium] <one-line title> — `path/to/file.rs:LINE`
...

### 3. [Low] <one-line title> — `path/to/file.rs:LINE`
...

### 4. [Nit] <one-line title> — `path/to/file.rs:LINE`
...
```

Severity tiers map to the existing CodeRabbit scheme:

- **High** — correctness, security, data-loss, acceptance, architecture, or
  mandatory-gate failure. Blocking.
- **Medium** — maintainability required to implement the accepted behavior
  safely. Blocking unless explicitly waived.
- **Low** — valid improvement; non-blocking. A suggestion.
- **Nit** — style or preference; non-blocking. A suggestion.

The review ends by distinguishing **blocking** items (High, and Medium unless
waived, plus any guardrail violation) from **suggestions** (Low, Nit). A
reviewer suggestion is not, by itself, scope authorization — see the
[issue-delivery workflow](./workflow/ISSUE-DELIVERY.md) finding triage
(Blocker-Fix / In-scope-Fix / Reject / Defer).

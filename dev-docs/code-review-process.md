# OpenCodeReview finding-evaluation process

This document is the versioned contract for how OpenCodeReview (OCR)
findings are evaluated, dispositioned, and compared across runs in this
repository. It is the review-process sibling of the
[CodeRabbit review-demand policy](code-review-demand.md): the demand policy
governs *when* a review runs; this document governs *how a reviewer treats
its output*. The invariants below are enforced mechanically by
`tests/ocr_review_policy.rs`.

OCR is an observational, non-blocking review signal. A green OCR run is
coverage, not approval; a red OCR finding is a hypothesis, not a verdict.

## Scope and roles

OCR runs in CI on pull requests (`.github/workflows/ocr-review.yml`) and may
run locally where a contributor has configured a provider. Both surfaces are
governed by this process. The CI workflow is observational: it never gates a
merge and is intentionally absent from the required-gate chain of `ci.yml`.

## Finding validity

Treat every OCR finding as a hypothesis. Before acting on it:

1. Re-read the current source, tests, and contracts at the cited location.
2. Verify the path, symbol, line range, and any quoted existing code against
   the actual file at the reviewed head.
3. Classify the finding's validity:
   - **valid** — the cited code exists as described and the concern holds.
   - **partial** — the location or concern is partly right but imprecise
     (wrong line range, misidentified symbol, or an over-broad claim that
     contains a real kernel).
   - **invalid** — the cited code does not exist, the concern is
     contradicted by the source, or the suggestion would break accepted
     behavior.
   - **unverifiable** — there is not enough context in the finding or the
     repository to confirm or refute the claim.

Do not act on a finding whose validity you have not classified. An
unverifiable finding is not valid by default.

## Finding disposition

Record one disposition for every finding, grounded in its validity:

- **fix** — the finding is valid (or partially valid with a real kernel) and
  the fix is in scope for the current work; implement it and verify.
- **explain** / **dismiss** — the finding is invalid or already covered;
  record the source-backed reason it is dismissed rather than silently
  dropping it.
- **defer** — the finding is valid but outside the current work's scope;
  record it as a follow-up rather than expanding scope.
- **user judgment** — the finding raises a scope, architecture, or
  intentional-design question that needs a maintainer decision; surface it
  rather than resolving it unilaterally.

Deduplicate by factual root cause, not by prose. One root cause affecting
several locations should have one representative finding with every affected
path or symbol listed as members; do not emit semantically equivalent
rephrasings. Do not silently discard Low findings; low-value or invalid
findings may be dismissed concisely with evidence.

## Coverage honesty

A review's coverage state must be reported honestly. OCR coverage is one of:

- **complete_best_effort** — OCR reports success across reviewed files. This
  is usable but, because upstream manifest support is incomplete, it is a
  best-effort signal rather than a recall guarantee.
- **partial** — OCR reports `completed_with_errors` for at least one file.
  Findings may still be useful, but the review did not complete cleanly.
- **failed** — the OCR command failed (nonzero exit) or produced no parseable
  output.
- **unknown** — coverage could not be determined from the result.

A `partial`, `unknown`, or `failed` run is never summarized as clean, even
when it reports zero findings. The CI sticky summary surfaces the coverage
state on every run.

## Run comparison eligibility

Two OCR runs are only comparable when their inputs match. Do not contrast a
local run with a CI run, or one run with another, as if they measure the
same thing unless all of the following match:

- OCR version and executable lineage.
- The exact reviewed range (base/head SHAs) or commit.
- The selected file set (same include/exclude/rule resolution).
- Provider, model, protocol, and account generation.
- Concurrency, rules, and redacted configuration (compare via the
  `redacted_config_sha256` in `manifest.pre.json`).
- Worktree state (the manifest records HEAD, branch, and diff hashes).

When any of these differ, treat the runs as independent observations, not as
a before/after measurement. The reproducibility manifests
(`manifest.pre.json`, `manifest.post.json`, `sha256.txt`) exist so this
eligibility can be checked after the fact.

## Provider and remediation discipline

- Do not switch providers, edit OCR configuration, or run routine
  connectivity tests as part of an ordinary review.
- Do not retry a review automatically or resume a range/commit review with a
  different provider lineage; a manual provider transition is a new run with
  recorded lineage.
- Never loosen lint or complexity rules, add suppression directives, exclude
  source or tests from analysis, or modify `.llxprt/` as review remediation.
- OCR is capped per issue/PR effort per the bounded-review rules in
  [`dev-docs/workflow/ISSUE-DELIVERY.md`](workflow/ISSUE-DELIVERY.md).

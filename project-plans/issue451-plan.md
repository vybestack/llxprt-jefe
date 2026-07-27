# Issue #451 — Port llxprt-code pr-review GitHub Action to jefe

## Problem

Port the **`pr-review.yml`** GitHub Action from llxprt-code into jefe. This is
the walkthrough/summary review pipeline that runs the `llxprt` CLI to generate
a per-file map, group into themes, synthesize a walkthrough + release notes +
sequence diagram + related items + pre-merge checks, and post the result as a
PR comment. It is gated on mergeability, a linked-issue requirement, and an
exact-SHA head fetch.

## Source surface (llxprt-code)

- `.github/workflows/pr-review.yml` — main workflow (trigger
  `pull_request_target`, mergeability gate, issue-link requirement, diff
  artifact generation, llxprt CLI invocation via the orchestrator, comment
  posting).
- `.github/workflows/_pr-mergeability-gate.yml` — reusable read-only gate
  (polls `pulls.get` mergeability, fails open after bounded polling).
- `scripts/pr-review-walkthrough.mjs` — orchestrator (map -> group ->
  synthesis -> pre-merge pipeline, concurrency limiter, JSON extraction,
  renderer, magnitude).
- `scripts/pr-review-prompts.mjs` — prompt builders (untrusted-data framing).
- `scripts/pr-review-llm-helpers.mjs` — parse-error retry + artifact saving.
- `scripts/pr-review-artifacts.mjs` — artifact reader + context builder.
- `scripts/ci-quota-check.js` — API quota check + key selection.

## Jefe adaptations (required differences)

1. **LLM config mapping.** llxprt-code uses `vars.OPENAI_BASE_URL`,
   `vars.LLXPRT_DEFAULT_MODEL`, `vars.LLXPRT_DEFAULT_PROVIDER`,
   `vars.LLXPRT_STRONG_MODEL`, `secrets[vars.KEY_VAR_NAME]`. jefe has none of
   these. jefe's existing OCR workflow uses `vars.OCR_LLM_URL`,
   `vars.OCR_LLM_MODEL`, `secrets.OCR_LLM_AUTH_TOKEN`. The port maps jefe's
   existing config into the llxprt CLI's expected env:
   - `OPENAI_BASE_URL` <- `vars.OCR_LLM_URL`
   - `LLXPRT_DEFAULT_MODEL` / `LLXPRT_STRONG_MODEL` <- `vars.OCR_LLM_MODEL`
   - `LLXPRT_DEFAULT_PROVIDER` <- `openai` (the stepfun endpoint is
     OpenAI-compatible; jefe's `OCR_LLM_USE_ANTHROPIC=false` confirms this)
   - `OPENAI_API_KEY` <- `secrets.OCR_LLM_AUTH_TOKEN`
   The workflow reads these via jefe's existing vars so no new repo secrets
   or vars are introduced.

2. **Quota check.** `ci-quota-check.js` targets the Synthetic provider quota
   API and is only meaningful when `KEY_VAR_NAME` contains `SYNTHETIC`. jefe
   does not use Synthetic. The port keeps the script (single key path, CR/LF
   guard) but the workflow step is retained for parity; with jefe's config it
   takes the "not Synthetic, use primary key" branch.

3. **CI workflow name.** `Capture CI status` step queries
   `ci.yml` runs for the head SHA. jefe's CI workflow is also `ci.yml`, so no
   change needed.

4. **Comment tag / planner issue.** `<!-- llxprt-walkthrough -->` stays.
   Planner issue reference (`#2256`) is llxprt-code-specific; replaced with a
   reference to this issue (#451) in the footer so the artifact is traceable
   to jefe.

5. **PR template sections.** `DEFAULT_PR_TEMPLATE_SECTIONS` in prompts is
   llxprt-code's (TLDR, Dive Deeper, Reviewer Test Plan, Testing Matrix,
   Linked issues / bugs). Adapted to jefe's PR template: Summary,
   Pre-push checklist, Testing notes, Reviewers / Assignees.

6. **`countPackages` layout.** llxprt-code is a monorepo under `packages/`;
   the magnitude + sequence-diagram heuristics key off `packages/`. jefe is a
   single-crate Rust repo (`src/`, `tests/`). The heuristic is adapted to
   count top-level Rust module groups instead of npm packages.

7. **No `package.json`.** jefe has no `package.json` and is not a Node
   project. The `.mjs` scripts are self-contained (Node built-ins only), so
   they run under `node` without a manifest. `node --eval` in the workflow
   provides `require`/`import` regardless of package type, so the
   context-builder step is unchanged.

8. **Commenter action pin.** `thollander/actions-comment-pull-request@v3`
   pin retained at its ratcheted SHA.

## Non-goals

- Changing jefe's existing CodeRabbit or OCR workflows.
- Adding new repo variables or secrets. The port reuses jefe's existing
  `OCR_LLM_*` config.
- Modifying `.llxprt/`, Cargo manifests, Rust source, or the quality gates.
- Auto-merging or status-gating the PR (the workflow is observational; it
  posts a comment).
- Porting the `ocr-review-local` wrapper (that is sibling issue #449).

## Acceptance matrix

| # | Behavior | Evidence | Type |
|---|----------|----------|------|
| AC1 | A `pr-review.yml` workflow exists, triggered on `pull_request_target` (opened/reopened/synchronize/ready_for_review/edited), with a mergeability gate and concurrency group. | `.github/workflows/pr-review.yml` present; actionlint clean. | In-scope |
| AC2 | The mergeability gate reusable workflow is ported verbatim (bounded `pulls.get` polling, fail-open, stale-head skip). | `.github/workflows/_pr-mergeability-gate.yml` present; actionlint clean. | In-scope |
| AC3 | The workflow requires a linked issue; a PR with no linked issue is returned to draft with a comment and no review runs. | `issue_gate` step + `should_review` gating in `pr-review.yml`. | In-scope |
| AC4 | The workflow fetches the PR head at the exact event SHA (fork-safe), computes merge-base, and aborts on a stale head. | `fetch_head` step + SHA equality check. | In-scope |
| AC5 | The orchestrator scripts (`pr-review-walkthrough.mjs`, `-prompts.mjs`, `-llm-helpers.mjs`, `-artifacts.mjs`, `ci-quota-check.js`) are ported and run under `node` with no syntax errors and no package.json dependency. | `node --check` on each script. | In-scope |
| AC6 | The LLM config maps jefe's existing `OCR_LLM_*` vars/secret to the llxprt CLI's expected env; no new repo var/secret is introduced. | env block in `pr-review.yml`. | In-scope |
| AC7 | Prompts and magnitude/sequence heuristics are adapted to jefe's Rust layout (src/tests) and jefe's PR template sections. | `DEFAULT_PR_TEMPLATE_SECTIONS` + layout heuristics in the scripts. | In-scope |
| AC8 | The workflow posts a walkthrough comment (with a fallback comment on any failure) and never leaks secrets/stderr into the comment. | comment-posting steps + fallback step + stderr redirection in `pr-review.yml`. | In-scope |
| AC9 | No Rust source, Cargo manifest, quality-gate, or existing workflow is changed. | `git diff` scope. | In-scope |
| AC10 | `make ci-check` remains green. | Local + CI. | In-scope |

## Vertical slices

Single coherent slice: the workflow + reusable gate + 5 scripts together form
one delivered behavior (an automated walkthrough review posted on PRs). They
cannot be split into independently testable behaviors. Verification is:
actionlint on both workflows, `node --check` on all scripts, and `make
ci-check` confirming the Rust gates are unaffected.

## Expected paths / files

- `.github/workflows/pr-review.yml` (new)
- `.github/workflows/_pr-mergeability-gate.yml` (new)
- `scripts/pr-review-walkthrough.mjs` (new)
- `scripts/pr-review-prompts.mjs` (new)
- `scripts/pr-review-llm-helpers.mjs` (new)
- `scripts/pr-review-artifacts.mjs` (new)
- `scripts/ci-quota-check.js` (new)
- `project-plans/issue451-plan.md` (new — this plan)

8 files, no Rust changes.

## Verification

- `actionlint .github/workflows/pr-review.yml`
- `actionlint .github/workflows/_pr-mergeability-gate.yml`
- `node --check scripts/pr-review-walkthrough.mjs` (and each script)
- `make ci-check` (Rust gates unaffected; confirms no regression)

## Scope ledger

| Date | Item | Disposition |
|------|------|-------------|
| 2026-07-26 | Initial scope: port the pr-review GitHub Action + scripts. | Accepted |
| 2026-07-26 | Process-doc + contract-test interpretation (first attempt). | Rejected — maintainer clarified the intent is the Action port. PR #455 closed. |

## Review counters (OCR)

- Local OCR runs before PR: 0 / 2
- OCR runs after PR opened: 0 / 2

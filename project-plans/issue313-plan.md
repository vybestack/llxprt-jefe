# Issue 313: bounded fresh issue delivery prompts

## Issue and decision

Issue: https://github.com/vybestack/llxprt-jefe/issues/313

Fresh Send Issue launches currently append an open-ended implementation and review loop to the issue prompt. The replacement will be one concise, runtime-neutral launch instruction that points agents to `dev-docs/workflow/ISSUE-DELIVERY.md` and states the policy's essential scope, review, exact-head, and stopping guardrails. The generated `.jefe/issue-prompt.md` remains owned by the existing issue formatter; the canonical workflow is referenced from the launch instruction rather than copied into that file.

The issue and its single automated planning comment are decision-complete: they require the documented four review dispositions (`Blocker-Fix`, `In-scope-Fix`, `Reject`, and `Defer`), the documented file/line thresholds, and the two-local/two-PR OCR cap. No prompt-generation, pull-request, runtime-command, workflow, dependency, or quality-policy change is needed.

## Acceptance matrix

| ID | Actor / launch path | Input and boundary | Observable success | Failure / diagnostic | Side effects before failure | Persistence / compatibility | Behavioral evidence |
| --- | --- | --- | --- | --- | --- | --- | --- |
| A1 | LLxprt fresh Send Issue | Issue prompt path plus persisted LLxprt mode flags, including optional `--yolo` and stale `--continue` | Launch receives the bounded issue contract after `-i`; existing flags are preserved except `--continue` | Existing launch diagnostics remain authoritative | No new side effects | Existing fresh/non-resuming behavior remains | Unit tests compare runtime contracts and preserve LLxprt flags |
| A2 | Code Puppy fresh Send Issue | Issue prompt path and any stale runtime flags | Launch receives the same bounded issue contract as its sole positional prompt | Existing launch diagnostics remain authoritative | No new side effects | Existing positional-prompt behavior remains | Unit tests compare runtime contracts and assert one positional argument |
| A3 | Either fresh Send Issue runtime | Delivery instruction is constructed | Instruction references the canonical policy; requires acceptance shaping, non-goals, bounded slices, expected paths, scope ledger, approval stops, documented thresholds, four-way review triage, the two-local/two-PR OCR cap, exact-head readiness, and successful stopping | A missing contract clause fails a focused semantic assertion | None | Runtime-neutral string contract | Focused semantic unit tests, not a giant exact-string assertion |
| A4 | Either fresh Send Issue runtime | Review produces a valid suggestion outside accepted scope | Contract permits `Reject` or `Defer` and does not require implementing every actionable suggestion | Semantic negative assertion detects the superseded open-ended command | None | Canonical review policy remains authoritative | Unit test rejects the old "address every actionable finding" wording |
| A5 | LLxprt or Code Puppy fresh Send PR | Pull-request prompt path | Existing PR instruction remains exactly unchanged | Unit regression identifies any accidental issue-contract append | None | Existing PR launch behavior remains | Existing exact PR prompt tests |
| A6 | Issue prompt formatter | Issue facts, focused comment, and configured issue base prompt | No canonical workflow text is embedded in `.jefe/issue-prompt.md`; launch instruction owns the policy reference | Existing formatter tests remain authoritative | Existing prompt write only | Prompt format remains unchanged | No formatter production change; full regression suite |

## Explicit non-goals

- Weakening architecture, lint, complexity, source-size, safety, coverage, cross-platform, TDD, or CI requirements.
- Embedding the canonical workflow text in `.jefe/issue-prompt.md` or changing issue prompt formatting.
- Adding automatic issue decomposition, a mechanical scope checker, or a new public abstraction.
- Changing pull-request fresh prompts, runtime command construction, launch flags, persistence, or orchestration.
- Changing `.llxprt/`, `.code_puppy/`, `.github/`, dependencies, quality gates, or workflow policy.
- Expanding the canonical policy itself; this issue only makes fresh issue launches reference and summarize it.

## Vertical slice

### Slice 1: bounded runtime-neutral Send Issue contract

- Acceptance: A1-A6.
- Owner: fresh-prompt launch-signature adapter.
- Allowed paths: `src/app_input/fresh_prompt.rs`, the end-to-end launch assertion in `src/app_input/issue_send_modal_tests.rs`, and this issue plan.
- RED: replace the giant exact-string test with semantic contract tests that require every accepted guardrail and reject the superseded open-ended review command; run the focused test target and confirm failure against the old constant.
- GREEN: replace `ISSUE_DELIVERY_WORKFLOW` with the concise canonical-policy reference and required bounded clauses.
- Refactor: keep semantic groups in focused tests and preserve existing runtime/PR regression coverage.
- Verification: focused fresh-prompt tests, `make quick-check`, then exact-head `make ci-check`.
- Stop for approval: any need to change the issue formatter, PR behavior, runtime command layer, canonical workflow policy, public API, dependency/tooling, or files outside the allowed paths.

## Expected paths by layer

- Launch adapter and unit behavior: `src/app_input/fresh_prompt.rs`.
- Send Issue integration assertion: `src/app_input/issue_send_modal_tests.rs`.
- Planning, scope, review counters, and evidence: `project-plans/issue313-plan.md`.
- Canonical policy: `dev-docs/workflow/ISSUE-DELIVERY.md` is read-only for this issue.
- Prompt formatter: `src/app_input/issues_dispatch.rs` is read-only; no workflow text is added to generated issue facts/comments.

## Test-first sequence

1. Replace the exact full-string workflow assertion with focused semantic assertions for policy reference, acceptance shaping, scope-expansion stops and thresholds, review dispositions/cap, and exact-head successful stopping.
2. Add a negative assertion that the old command to address every actionable finding is absent.
3. Run the focused library tests and record RED against the existing open-ended instruction.
4. Replace only `ISSUE_DELIVERY_WORKFLOW` with the bounded runtime-neutral contract.
5. Run the focused tests to GREEN and retain existing LLxprt, Code Puppy, and PR regression tests.
6. Run `make quick-check`, inspect the diff and scope counts, then run exact-head `make ci-check`.
7. Use review runs only on a stable verified checkpoint and triage each finding under the canonical four dispositions.

## Scope ledger

| Discovered item | Disposition | Acceptance / reason |
| --- | --- | --- |
| Existing test asserts the entire old instruction as one exact string | Planned | A3 explicitly requires semantic tests instead |
| Existing negative assertion forbids `OCR`, while the new contract must state the OCR cap | Planned | A3 requires two local and two PR OCR reviews |
| Existing issue formatter can include a configured issue base prompt under `## Instructions` | Reject for this issue | Pre-existing user-configured issue content; the requested workflow remains separate and no formatter change is authorized |
| Pull-request prompt uses the same adapter but no issue workflow suffix | Preserve | A5; exact existing tests protect it |
| Send Issue integration test asserted superseded operational clauses | In-scope fix | A1 and A3; update it to assert the canonical reference and bounded scope/review contract |
| Canonical policy contains broader operational detail than the launch string | Preserve by reference | Concision and single-policy ownership are explicit requirements |

## Review counters

- Local Open Code Review runs: 1 / 2
- Post-PR Open Code Review runs: 1 / 2

## Verification evidence

- RED: `cargo test --bin jefe issue_delivery_workflow -- --nocapture` failed all four new semantic tests against the old contract, first identifying the missing canonical-policy reference, scope stop, review dispositions, and exact-head rule.
- Focused GREEN: `cargo test --bin jefe issue_delivery_workflow -- --nocapture` passes all 4 contract tests.
- Fresh-prompt regression GREEN: `cargo test --bin jefe fresh_prompt -- --nocapture` passes all 12 tests, including identical runtime contracts, LLxprt flag preservation, Code Puppy positional prompting, and unchanged PR prompts.
- `make quick-check` initially exposed the related Send Issue integration assertion's superseded operational wording; after the in-scope semantic update, the command passes all format, check, unit, integration, and doctest targets.
- Review-finding RED: the focused exact-head test failed until completion explicitly required resolving all `Blocker-Fix` and `In-scope-Fix` findings; it and all 12 fresh-prompt tests pass after the prompt update.
- Exact-head `make ci-check` passes format, policy, source-size, both Clippy gates, 72.77% line coverage, locked build, and all locked tests; it passed again after rebasing onto current `origin/main`.
- PR CI passes on reviewed head `2431c3d`, including native Windows, coverage, build, tests, and Open Code Review; the PR is conflict-free with correct main ancestry.

## Review findings and dispositions

| Reviewer | Finding | Disposition |
| --- | --- | --- |
| Open Code Review | Restore a giant exact-string assertion because semantic clause tests permit textual drift | Reject: issue 313 explicitly requires semantic verification without one giant exact-string assertion; focused positive groups plus negative superseded-wording coverage protect the accepted contract while allowing harmless prose edits |
| Open Code Review | Retain the removed operational workflow inline in case the canonical policy is unavailable | Reject: the issue explicitly requires a concise policy reference and avoiding duplicated workflow text; fresh issue agents run in the prepared repository where the versioned policy path is available |
| Open Code Review | Exact-head completion could classify findings without resolving accepted blocker and in-scope fixes | In-scope fix: added an explicit resolution requirement and test-first semantic coverage |
| Post-PR Open Code Review | Add direct unit proof that the composed issue instruction includes the workflow suffix | Reject as already covered: `issue_send_forces_pass_continue_false_on_launch_signature` exercises the public Send Issue composition and asserts representative bounded clauses, while the runtime-equality test proves both launch paths receive that composed instruction |

## Deferred findings / follow-ups

None.

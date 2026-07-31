# Issue 532 — Fresh Send Issue delivery contract: repo-neutral & scope-focused

## Links

- Issue: https://github.com/vybestack/llxprt-jefe/issues/532

## Problem

`ISSUE_DELIVERY_WORKFLOW` in `src/app_input/fresh_prompt.rs` is appended to
every fresh Send Issue prompt Jefe sends to an agent. Two defects:

1. It opens with "Follow the canonical bounded issue-delivery policy in
   dev-docs/workflow/ISSUE-DELIVERY.md". That path exists only inside the jefe
   repo; target repos (e.g. llxprt-code) do not contain it, so agents are told
   to follow a file that is absent.
2. It hard-codes numeric budgets — "25 files or 1,500 net changed lines",
   "40 files or 2,500 net changed lines", "mandatory scope review above either
   threshold", "hard scope budget" — which makes agents optimize for file/line
   counts rather than actual issue scope.

## Decision-complete acceptance matrix

| ID | Actor / path | Inputs & boundary | Observable success | Observable failure / diagnostic | Side effects before failure | Persistence / compat | Behavioral test |
|----|--------------|-------------------|--------------------|---------------------------------|----------------------------|----------------------|-----------------|
| A1 | Fresh Send Issue → `fresh_prompt_instruction(Issue, ..)` | Any issue body, any repo | Instruction ends with a delivery contract that is self-contained (no jefe-only doc path) and scope-focused | Contract still referencing `dev-docs/workflow/ISSUE-DELIVERY.md` → focused unit test fails | None | Runtime-neutral; no behavior change to PR prompts | `fresh_prompt::issue_delivery_workflow_*` |
| A2 | Same | — | Contract contains NO numeric file/line budgets and NO "hard scope budget" | Presence of "net changed lines"/"hard scope budget" → negative assertion fails | None | — | negative assertions in scope-guardrail test |
| A3 | Same | — | Contract still requires acceptance shaping + stop-for-approval guardrails (unplanned subsystem, public abstraction, workflow/agent-memory/quality-tool/dependency change, unrelated refactor/test move, out-of-scope behavior) | Missing guardrail → assertion fails | None | — | `issue_delivery_workflow_stops_unplanned_scope_expansion` |
| A4 | Same | — | Contract still defines four-way review triage + OCR cap + standards-preservation closer | Missing clause → assertion fails | None | — | `issue_delivery_workflow_bounds_and_triages_review`, completion test |
| A5 | Issue send modal → `prepare_issue_launch_signature` | Fixture issue agent | Produced instruction asserts the new (repo-neutral, count-free) contract | Old-string assertions fail | None | — | `issue_send_modal_tests` |

## Non-goals

- Do NOT change the jefe-internal `dev-docs/workflow/ISSUE-DELIVERY.md` doc — it
  is not injected and remains jefe's own process reference.
- Do NOT change PR-prompt behavior (`FreshPromptKind::PullRequest` path).
- Do NOT remove the OCR review cap, review triage, or the standards-preservation
  closer (harmful to drop; out of scope).
- Do NOT change compaction/truncation thresholds or the tmux sizing logic beyond
  updating the now-stale "~1.5 KB" comment if the appendix shrinks.

## Vertical slice

Single slice: rewrite `ISSUE_DELIVERY_WORKFLOW` + its assertions.

1. RED: replace the four `fresh_prompt` contract tests and the one
   `issue_send_modal_tests` assertion with the new contract (scope-focus +
   negative assertions that the doc path and counts are gone). Run focused
   tests → fail.
2. GREEN: rewrite `ISSUE_DELIVERY_WORKFLOW` to the repo-neutral, count-free,
   scope-focused contract. Run focused tests → pass.
3. Update the stale "~1.5 KB appendix" doc comments to reflect the new size.
4. Full verification: `make ci-check`.

## Expected files

- `src/app_input/fresh_prompt.rs` (constant + tests + sizing comments)
- `src/app_input/issue_send_modal_tests.rs` (one assertion block)
- `project-plans/issue532-plan.md` (this plan)

## Scope ledger

- Decision: drop "scope ledger is clean" from the *injected* contract because a
  scope ledger is a jefe-plan artifact absent in target repos (same class as
  the removed doc reference); it created an unsatisfiable completion gate.
  Keep the OCR cap because it is a harmless *limit* (trivially satisfied when
  OCR is unused), not an unsatisfiable gate.
- No other behavior changes.

## Review counters

- OCR: 0 local / 0 PR so far (cap 2/2).

## Verification evidence

- RED: `cargo test --bin jefe issue_delivery_workflow` — 2 failed (missing
  `acceptance criteria`, `outside the issue's scope`) against the old constant.
- GREEN: `cargo test --bin jefe issue_delivery_workflow` — 4 passed; modal
  test `issue_send_projects_fresh_operation_and_prompt` passed.
- Appendix size: 1320 bytes (~1.3 KB), down from ~1.5 KB; sizing comments updated.
- `cargo fmt --all --check`: clean.
- `cargo clippy --workspace --all-targets --all-features -- -D warnings`: clean.
- `cargo build --workspace --all-features --locked`: clean.
- `cargo test --workspace --all-features --locked -- --test-threads=1`: all bins
  green, 0 failed.

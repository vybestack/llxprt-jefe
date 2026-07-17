# Issue 351 delivery plan

## Issue

- GitHub: https://github.com/vybestack/llxprt-jefe/issues/351
- Branch: `issue351`
- Base: `origin/main` at `248d968`
- Reported behavior: switching the Issues repository selection to a repository with no open issues can print a background-thread panic from `generational-box` line 145 (`Option::unwrap`); the same panic can occur while the Issues screen is idle.
- Reproduction evidence: the unmodified base emitted the reported panic from the issue-list background task while the Issues screen remained active. The task currently reads and mutates iocraft `State<AppState>` inside `smol::unblock`, while the root component renders and polls the same state on the executor thread.

## Acceptance matrix

| ID | Actor / launch path | Inputs and boundary cases | Target | Observable success | Observable failure / diagnostics | Permitted side effects | Persistence / compatibility | Proof |
| --- | --- | --- | --- | --- | --- | --- | --- | --- |
| A1 | User enters Issues and moves repository focus from a repository with issue rows to a repository whose GitHub issue query returns zero rows | first request may still be in flight; second response is empty; stale first response may finish before or after the empty response | Local Unix TUI; platform-independent state/task contract | selected repository remains the second repository, Issues shows `No issues found`, and no thread panic is printed | GitHub command failures remain an Issues error and/or existing Errors entry; no raw panic text | read-only `gh api graphql`; normal selected-repository persistence | deterministic TUI scenario with a fail-closed `gh` shim; focused async-task test |
| A2 | User leaves an empty Issues screen idle through later render/poll cycles | no rows, no selected issue, no active filter | Local Unix TUI; platform-independent state/task contract | empty screen remains responsive and exits normally without a `generational-box` panic | recoverable task failures continue through existing typed failure events | periodic render tick only | no schema change | TUI scenario waits after `No issues found`, captures the screen, and exits |
| A3 | A background GitHub task completes after its owning iocraft component/state has been dropped (shutdown/replacement race) | both success and panic-handler completion paths | all platforms | task does not dereference stale iocraft state and therefore does not panic | task result is safely discarded because there is no live owner to receive it | completed GitHub I/O only; no stale mutation | existing public behavior unchanged while owner is live | mock-terminal regression test drops the component before releasing a blocked task and asserts the task exits cleanly |

## Non-goals

- No new global panic-catching or panic-recovery subsystem. The existing Errors screen already records typed operational failures; recovering an arbitrary iocraft runtime panic safely is a separate architectural feature.
- No persistence-schema, dependency, workflow, `.llxprt/`, `.code_puppy/`, or quality-gate changes.
- No refactor of all 32 GitHub dispatch routes in this issue. The issue-list route establishes the approved UI-owned delivery boundary; broader migration is deferred.
- No change to issue filtering, pagination, repository navigation, or visible empty-state wording.
- No network-backed required test.

## Planned vertical slice

### Slice S1: lifecycle-safe background GitHub task delivery

- Acceptance rows: A1-A3.
- Architecture owner: the root app-shell lifecycle plus `app_input::gh_async`, the established boundary for blocking GitHub I/O.
- Integration boundary: the worker performs only blocking I/O and constructs a typed delivery. An iocraft async handler owned by the root component applies that delivery to `State<AppState>`; dropping the root drops the only consumer, so late results never poll stale state.
- Approved architecture decision: the user approved a root-owned typed event/delivery queue on 2026-07-17 and clarified that event queues are acceptable project architecture.
- Allowed production files:
  - `src/main.rs` for the shared delivery-handle slot in `AppContext`
  - `src/app_shell.rs` for the root-owned async handler
  - `src/app_input/mod.rs` for typed delivery dispatch/re-exports
  - `src/app_input/gh_async.rs` for the typed worker/result boundary
  - `src/app_input/issues_list_dispatch.rs` for issue-list result construction and UI delivery
- Allowed evidence/support files:
  - `dev-docs/tmux-scenarios/issues-empty-repository.json`
  - `scripts/issue351-gh-shim.sh`
  - `scripts/issue351-run-scenario.sh`
  - this plan
- RED:
  1. Add a mock-terminal test that starts a blocked background task, drops its state owner, releases the task, and requires clean completion.
  2. Add the deterministic TUI scenario before production changes and run it on the base implementation.
- GREEN:
  - issue-list workers no longer receive or mutate iocraft state;
  - typed success and panic deliveries are applied only by the root-owned async handler;
  - empty repository scenario shows `No issues found`, survives idle renders, exits normally, and shim audit contains only expected read-only queries;
  - focused tests and `make quick-check` pass.
- REFACTOR: keep the queue typed and lifecycle-owned; avoid stringly typed callbacks or route-specific state access in the worker.
- Verification:
  - focused `cargo test` for `gh_async`;
  - `scripts/issue351-run-scenario.sh`;
  - `make quick-check`;
  - `make ci-check` at exact candidate head.
- Stop conditions: caller API redesign, a new dispatcher/event queue, dependency changes, edits outside the allowed paths, or scope above the workflow budget.

## Expected paths by layer

| Layer | Paths | Reason |
| --- | --- | --- |
| Root lifecycle wiring | `src/main.rs`, `src/app_shell.rs` | own the typed delivery handler for exactly the component lifetime |
| App-input delivery boundary | `src/app_input/mod.rs`, `src/app_input/gh_async.rs`, `src/app_input/issues_list_dispatch.rs` | keep blocking work state-free and apply typed issue-list results on the root handler |
| End-to-end UI evidence | `dev-docs/tmux-scenarios/issues-empty-repository.json` | prove navigation, empty state, idle stability, and clean exit |
| Deterministic fixture boundary | `scripts/issue351-gh-shim.sh`, `scripts/issue351-run-scenario.sh` | isolate GitHub/config state and audit allowed operations |
| Delivery record | `project-plans/issue351-plan.md` | acceptance, scope, review, and verification ledger |

## Scope ledger

| Discovery | Disposition | Rationale / follow-up |
| --- | --- | --- |
| Issue comment requests copyable panic diagnostics | Defer | Issue #292 already supplied the Errors screen for typed operational errors. Arbitrary framework panic recovery would be a new subsystem and does not prevent this stale-state panic. Consider a dedicated follow-up for process-level crash reporting if still desired after this fix. |
| `gh_async` runs caller work, including state mutation, inside `smol::unblock` | Approved in-scope architecture change for issue-list dispatch | Introduce a root-owned typed delivery handler and migrate issue-list fetches first. The user explicitly approved this event-queue boundary; migration of the remaining routes is deferred. |
| Live `llxprt-luther` now has open issues | Test fixture decision | Required proof uses a fail-closed shim with an explicitly empty second repository; no test depends on live repository contents. |
| Unkeyed dynamic selectable-list children | Reject for this issue | The captured panic originates inside the GitHub background task, not a list component hook. Existing list parity remains unchanged. |
| OCR: scenario build and harness lacked outer timeouts | In-scope—Fix | Added a portable Python-backed timeout wrapper because Python is already a required scenario dependency and macOS does not provide GNU `timeout` by default. |
| OCR: move `catch_unwind` outside `smol::unblock` | Reject | `catch_unwind` encloses the blocking closure, so a work panic is converted before it can escape the blocking executor. Moving the catch outside would require the panic to cross the `unblock` future boundary first, weakening containment. |
| OCR: delivery without an installed handler was silent | In-scope—Fix | Added a debug diagnostic for the unreachable-before-first-render/shutdown discard path; late results remain intentionally unapplied after owner loss. |

## Review counters

- Pre-PR Open Code Review: 2 / 2 (both invocations were terminated by signal 15 without output; no findings available to triage)
- Post-PR Open Code Review: 0 / 2

## Verification evidence

| Candidate head | Command | Result |
| --- | --- | --- |
| `248d968` | live-repository TUI reproduction | RED: issue-list background task emitted the reported generational-box panic |
| `248d968` | `scripts/issue351-run-scenario.sh` | Fixture baseline passes the visible empty-state flow; lifecycle RED is supplied by the focused async-owner test |
| working tree | focused lifecycle test | PASS: 1 passed; late delivery was not applied after owner drop |
| working tree | `scripts/issue351-run-scenario.sh` | PASS: 11 steps; audited empty-repository flow remained stable and panic-free |
| working tree | `make quick-check` | PASS |
| working tree | `make ci-check` | PASS |

## Deferred findings and follow-ups

- Process-level panic capture/copy UX: deferred as a separate feature requiring an explicit recovery and ownership design; no follow-up issue created yet.

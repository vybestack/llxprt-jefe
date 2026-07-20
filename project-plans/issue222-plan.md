# Issue 222 delivery plan: terminal and vterminal launch

## Decision

Deliver both requested launch paths in one bounded change.

- `F10` opens the default embedded shell for the selected running local agent.
- `F8` opens a native external terminal for the selected local agent.
- The embedded shell is a temporary `jefe-shell` tmux window in the agent's existing session. The existing single attached viewer follows the session's selected window, so no second PTY viewer or process manager is introduced and the agent continues running in its original window.
- The embedded dashboard keeps the repository sidebar and outer bars, replaces the agent list plus preview with the shell terminal, focuses terminal input, and resizes the existing viewer to the expanded shell viewport.
- `F11` closes the embedded shell window and restores the normal dashboard. Exiting the shell naturally closes its tmux window; bounded background observation restores dashboard state. Application exit also closes an active temporary shell window.
- External launch is fire-and-forget. It opens a native terminal in the agent work directory, scrubs Jefe's tmux client environment, and does not track the terminal process.
- Local agents are supported. Remote repositories produce an explicit warning without side effects because opening a local native terminal in a remote filesystem and managing a remote auxiliary shell are separate SSH product contracts absent from this issue.

This preserves the runtime's single-viewer invariant and extends existing tmux orchestration rather than adding parallel viewer/session infrastructure.

## Acceptance matrix

| ID | Actor / launch path | Input and boundary | Success behavior | Failure behavior / diagnostic | Side effects before failure | Persistence / compatibility | Evidence |
| --- | --- | --- | --- | --- | --- | --- | --- |
| A1 | Dashboard user presses `F10` | Selected agent is running in a local repository | A temporary shell window opens in that agent's work directory; agent window remains running; shell occupies all workspace except repository sidebar and outer bars; terminal input is focused | Runtime command failure leaves dashboard unchanged and sets a visible warning | No app-state transition until runtime open succeeds | Overlay state is runtime-only and is not persisted | Runtime command tests, reducer/input tests, TUI scenario |
| A2 | Dashboard user presses `F10` | No selected agent, dead agent, or remote repository | No shell/window is created | Visible actionable warning explains the missing/running/local requirement | None | No persistence change | Dispatch tests |
| A3 | User interacts with embedded shell | Shell overlay active | Keys and paste use the existing PTY forwarding path; expanded PTY geometry matches expanded render area | Write/resize errors follow existing runtime diagnostics | Existing PTY only | Existing terminal behavior remains compatible | Projection/layout tests, TUI scenario |
| A4 | User presses `F11` in embedded shell | Shell overlay active | Only the temporary shell window is killed; normal dashboard geometry and agent window are restored; agent remains running | Failure keeps overlay state and displays warning so the user can retry or exit naturally | No agent-session kill | Runtime-only state | Runtime and dispatch tests, TUI scenario |
| A5 | User types `exit` / shell exits | Shell overlay active | Tmux closes the temporary window and selects the agent window; bounded observation clears overlay state and restores normal geometry | Observation errors do not mark the agent dead; warning is surfaced when actionable | No agent lifecycle mutation | Runtime-only state | Runtime liveness tests, TUI scenario |
| A6 | User presses `F10` repeatedly | Shell already active | No duplicate shell window is created | Existing overlay remains usable | None beyond first open | Compatible/idempotent | Runtime command test |
| A7 | User presses `F8` | Any selected local agent with an existing work directory | Native terminal is launched with that work directory and without `TMUX`, `TMUX_PANE`, or `TMUX_TMPDIR`; Jefe remains active | Missing emulator, invalid directory, or spawn failure sets visible warning | No state/persistence mutation except warning | Fire-and-forget; no process lifecycle tracking | Cross-platform launch-plan tests, dispatch tests |
| A8 | User presses `F8` | No selected agent or remote repository | No process is spawned | Visible actionable warning | None | No persistence change | Dispatch tests |
| A9 | Platform launch | macOS / Linux / Windows | macOS opens Terminal.app (or `JEFE_TERMINAL` app override); Linux launches `JEFE_TERMINAL` or a discovered common emulator inheriting the work directory; Windows launches `JEFE_TERMINAL`, Windows Terminal, or a new-console command shell | No supported terminal yields an actionable override message | None | No new dependency; argv remains structural and shell-free where possible | Platform plan tests and native Windows CI |
| A10 | Jefe quits while overlay active | Temporary shell window exists | Temporary shell window is closed without killing the agent session | Cleanup failure does not block Jefe shutdown | Best-effort temporary-window cleanup only | Agent sessions retain existing lifecycle semantics | Cleanup decision/unit test |

## Explicit non-goals

- Multiple simultaneous attached PTY viewers.
- A second shell-session manager or durable auxiliary-shell persistence.
- Showing the agent and shell terminals concurrently.
- Remote auxiliary shell or native-terminal-over-SSH support.
- Guaranteeing a new tab in every terminal emulator; native tab/window behavior is emulator-controlled and best effort.
- Tracking, terminating, or persisting externally launched terminal processes.
- Config schema, dependency, workflow, quality-gate, `.llxprt/`, or `.code_puppy/` changes.
- Changes to agent lifecycle, launch signatures, scrollback semantics, or existing terminal selection behavior beyond selecting the correct geometry while the shell overlay is active.

## Vertical slices

### Slice 1: Embedded shell runtime contract

- Rows: A1, A4, A5, A6, A10.
- Owner: runtime tmux/psmux boundary.
- RED: structural command tests and manager tests for open/idempotence/close/existence without affecting agent identity.
- GREEN: temporary named window operations exposed through the existing `RuntimeManager`; local-only validation; no second viewer.
- Allowed paths: `src/runtime/commands.rs` or a focused sibling, `src/runtime/manager.rs`, `src/runtime/stub_manager.rs`, `src/runtime/mod.rs`, focused tests.
- Stop if psmux requires a separate process manager or a new dependency.

### Slice 2: Overlay state, layout, and input

- Rows: A1-A6, A10.
- Owners: deterministic state/message layer, app-input orchestration, pure layout/UI projection.
- RED: add the required TUI scenario first; reducer/event conversion/key-routing/layout/dashboard projection tests fail for missing behavior.
- GREEN: runtime-only overlay state, `F10`/`F11` dispatch, expanded layout, existing terminal forwarding, bounded shell-window observation and shutdown cleanup.
- Allowed paths: `src/state/`, `src/messages*`, `src/app_input/`, `src/app_shell.rs`, `src/layout.rs`, `src/ui/screens/dashboard.rs`, `src/ui/components/keybind_bar.rs`, scenario/script support.
- Stop if the design needs a second viewer or persisted schema.

### Slice 3: External terminal boundary

- Rows: A7-A9.
- Owners: runtime process boundary and app-input orchestration.
- RED: platform launch-plan, environment-scrub, selected-agent/remote/failure dispatch, key-routing, and key-hint tests.
- GREEN: typed launch plan plus thin spawn boundary; `F8` dispatch and warning behavior.
- Allowed paths: focused `src/runtime/external_terminal.rs`, `src/runtime/mod.rs`, `src/app_input/`, keybind/help tests.
- Stop if support requires shell-string interpolation, terminal-emulator dependencies, or remote SSH expansion.

## Expected paths

- Plan: `project-plans/issue222-plan.md`.
- Runtime: `src/runtime/manager.rs`, `src/runtime/commands.rs` or focused new shell-window module, `src/runtime/stub_manager.rs`, `src/runtime/external_terminal.rs`, `src/runtime/mod.rs` and focused tests.
- Message/state/input: `src/state/events.rs`, `src/state/types.rs`, `src/state/mod.rs`, `src/messages.rs`, `src/messages/event_conversion.rs`, `src/app_input/normal.rs`, `src/app_input/mod.rs` and focused tests.
- Layout/UI/shell: `src/layout.rs`, `src/ui/screens/dashboard.rs`, `src/ui/components/keybind_bar.rs`, `src/app_shell.rs` and focused tests.
- TUI evidence: one scenario and bounded runner/shim only if required for deterministic agent setup.

Target is at most 25 files and 1,500 net changed lines. A mandatory scope review occurs before continuing if either target is exceeded; work stops without explicit approval above 40 files or 2,500 net lines.

## Scope ledger

| Discovery | Classification | Disposition |
| --- | --- | --- |
| The runtime owns one attached viewer | In scope architectural constraint | Reuse one viewer and switch tmux windows |
| External terminal behavior differs by OS/emulator | In scope platform boundary | Typed per-platform plans; best-effort window/tab |
| Remote work directories are not local paths | Out of scope expansion | Explicit warning; defer SSH terminal contract |
| Deep-thinker design task did not complete after two bounded synchronous attempts | Process evidence | Continue with repository-evidenced single-viewer design as requested |

Mandatory scope review: remediation expanded the change to 28 files and 1,560 net lines (421 tracked additions, 213 tracked deletions, and 1,352 lines in new files). Every added path maps to accepted rows: `runtime/liveness.rs` prevents the shell window masking agent death (A5), `mouse_routing.rs` and its existing test map terminal coordinates to overlay geometry (A3), `ui/modals/help.rs` documents accepted shortcuts, and `scripts/issue222-run-scenario.sh` supplies deterministic TUI evidence. The change remains below the hard stop of 40 files / 2,500 net lines. No unapproved subsystem, dependency, workflow, quality-tool, or persistence change was added.

## Review counters

- Pre-PR Open Code Review: 2 / 2 attempted; both tool runs terminated without review output.
- Independent Rust review: 1 completed; actionable findings remediated and the stale Clippy finding rejected with exact locked-Clippy evidence.
- Post-PR Open Code Review: 0 / 2.

## Verification evidence

- Focused shell-window structural tests: pass (Unix and psmux argument/environment contracts).
- Focused liveness parser tests: pass; only live window index zero counts as agent liveness.
- External-terminal structural tests: 14 pass.
- Full library unit suite: 2,109 pass.
- Full application unit suite: 728 pass.
- Deterministic real-tmux scenario: 26 steps pass via `scripts/issue222-run-scenario.sh`.
- Source-file-size gate: pass.
- Exact locked Clippy gate: pass.
- Full `make ci-check`: pass on the candidate head after review remediation (format, policy, source size, Clippy/complexity, coverage, locked build, full tests, and doctests).

## Deferred findings / follow-ups

- Remote embedded/external shells: create a follow-up only if review or user demand establishes the SSH UX contract.

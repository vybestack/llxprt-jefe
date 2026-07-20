# Issue 361 delivery plan: persistent embedded shells and terminal manager

## Issue and decisions

- GitHub: https://github.com/vybestack/llxprt-jefe/issues/361
- Branches: `issue361` for PR A; `issue361-manager` stacked on PR A for PR B.
- Base: current `origin/main` at `74b2f7360c85d70567f5768fe05d91806ce3969f`.
- Delivery: one issue, two stacked PRs because the accepted behavior crosses state, runtime, orchestration, startup, UI, and harness ownership.
- Keys: F12 hides a focused shell; F10 resumes a hidden shell from Dashboard and closes a visible shell; F7 opens the manager; Ctrl-k closes the selected managed shell; Enter focuses it; Esc/F12 leaves the manager.
- Runtime ground truth is tmux/psmux shell windows. Application inventory and view state are runtime-only and are not persisted.
- Manager previews are throttled targeted captures, not a second live viewer.
- One fixed `jefe-shell` window per agent is the structural accumulation limit.

## Acceptance matrix

| ID | Actor / boundary | Success | Failure and side-effect ordering | Proof |
| --- | --- | --- | --- | --- |
| A1 | F12 in visible focused shell | Intercept before PTY; select agent window 0; retain shell process; restore prior dashboard focus/layout and resize | Select failure leaves overlay unchanged and warns; select attempt is the only side effect | routing/reducer/runtime tests and real-tmux survival marker |
| A2 | F10 on Dashboard with hidden shell | Resume exact shell with cwd/process/history/content; no duplicate | Existing attach/open warnings; runtime operation precedes state | runtime create-or-select test and hide/resume scenario |
| A3 | F10 in visible shell | Kill only `jefe-shell`; agent remains Running; restore launch view | Kill failure keeps overlay and warns | existing and extended scenario/tests |
| A4 | `exit` in visible shell | Natural disappearance removes state/inventory and restores launch view | Probe errors warn/retry and never mark agent Dead | observer tests and real-tmux exit scenario |
| A5 | hidden shell exits | Batch reconciliation removes inventory entry without disrupting current view | Probe failure retains entry and retries | reconciler tests and scenario |
| A6 | F7 manager entry | List all shells above and selected reduced preview below; useful empty state | Capture failure cannot create a second viewer or stale inventory | render tests and manager scenario |
| A7 | Enter on Running owner in manager | Attach owner, resume exact shell, and return to manager on hide/exit | Failed attach clears generation-guarded pending focus and warns | reducer/orchestration tests and two-agent scenario |
| A8 | Ctrl-k in manager | Close selected shell only; keep agent; update list/preview | Failure keeps entry, warns, and forces reconciliation | dispatch/runtime tests and scenario |
| A9 | agent kill/delete | Session teardown removes visible/hidden shell and inventory | Existing agent failure behavior remains authoritative | lifecycle tests |
| A10 | graceful quit with N shells | Close all Jefe-created shell windows without killing agent sessions | Best effort; failures logged and do not block quit | shutdown tests |
| A11 | restart after crash | Adopt known shells, select window 0 for each, kill unknown shell windows | Probe errors warn/retry; do not mark agents Dead | startup tests and restart scenario |
| A12 | naturally dead owner | Manager lists shell as unavailable and close-only | Resume attempt warns without side effects | manager state/dispatch tests |
| A13 | footer/help/chrome | Focused shell documents F10 close/F12 hide; F7 manager documented; no dead F12 hint | Stale text fails tests | component/help tests and scenarios |
| A14 | Windows/psmux | F12 is Jefe-owned while shell visible; all commands are structured with bounded fallback | Typed runtime errors use warning paths | command structural tests and native Windows CI |

## Invariants

1. Preserve the single attached viewer; no parallel PTY viewer or process manager.
2. Runtime side effects complete before deterministic state transitions.
3. Shell inventory, overlay visibility, manager selection, and pending focus are runtime-only.
4. Agent liveness derives only from window 0; shells never mask agent death.
5. Non-intercepted focused-shell keys continue to reach the PTY; F12 is intercepted on all platforms.
6. Whenever a shell is not visible, its session current window is window 0.
7. Close kills only `jefe-shell`, never the agent session.
8. At most one shell per agent.
9. Graceful quit cleans every tracked shell; restart reconciles runtime ground truth.
10. Reducers contain no I/O; side effects stay in runtime/app-input/startup boundaries.

## PR A: hide/resume and lifecycle safety

### Slice A1: F12 hide, F10 resume, and visible natural exit

- Acceptance: A1-A4 and focused-shell portions of A13-A14.
- Scenario-first RED: update `dev-docs/tmux-scenarios/agent-shell-overlay.json` to prove marker survives F12 hide and F10 resume, F10 still closes, and a recreated shell closes via typed `exit`.
- Unit RED: route F12 to hide while preserving F11 PTY forwarding; runtime select-window-0 structural tests; state hide behavior.
- GREEN owner paths: `src/app_shell_key_routing.rs`, `src/app_input/shell_overlay.rs`, `src/runtime/shell_window.rs`, runtime trait/stub, shell state/events/messages, footer/help/chrome, scenario.
- Stop if this requires persistence, a new viewer, or changing agent lifecycle.

### Slice A2: inventory, reconciliation, startup, and shutdown

- Acceptance: A5, A9-A12 plus inventory portions of A1-A4.
- RED: pure inventory transition tests; runtime list-window tests; hidden-exit reconciliation; startup adoption/orphan/window-0 normalization; close-all shutdown tests; restart scenario first.
- GREEN: typed runtime-only inventory; batched runtime observation with bounded psmux fallback; startup reconciliation; immediate lifecycle updates with reconciliation backstop; close-all shutdown.
- Stop if batching requires a new generic process subsystem, persistence, or crossing the PR hard budget.

## PR B: terminal manager and cross-agent focus

### Slice B1: manager list, static preview, and close

- Acceptance: A6, A8, A12-A14.
- Scenario-first RED: new terminal-manager scenario and runner.
- Unit RED: screen projection, navigation, empty state, close dispatch, Help/footer, targeted preview capture.
- GREEN: F7 entry, typed manager state/input mode, list above, static selected preview below, Ctrl-k close, Esc/F12 return.

### Slice B2: manager-to-shell focus

- Acceptance: A7.
- RED: generation-guarded pending-focus reducer/orchestration tests and two-agent scenario.
- GREEN: select owner, request existing background attach, resume only after the expected agent attaches, and preserve manager return target.

## Expected paths

- State: `src/state/types.rs`, `src/state/shell_overlay_ops.rs`, terminal-manager state/reducer modules, `src/state/events.rs`, `src/state/mod.rs`.
- Messages: `src/messages.rs`, `src/messages/names.rs`, `src/messages/event_conversion.rs`.
- Input/app shell: `src/app_input/shell_overlay.rs`, terminal-manager input module, `src/app_input/normal.rs`, `src/app_input/mod.rs`, `src/app_shell_key_routing.rs`, `src/app_shell.rs`.
- Runtime/startup: `src/runtime/shell_window.rs` and tests, `src/runtime/manager.rs`, `src/runtime/stub_manager.rs`, `src/runtime/mod.rs`, `src/app_init.rs`.
- UI: terminal manager screen, screen registration/orchestration, keybind bar, terminal view, Help, and layout only if required.
- Evidence: shell scenario update, manager/restart scenarios and runners, this plan.

Combined target: approximately 26-32 files and 1,500-2,200 net changed lines, split so each PR targets no more than 25 files / 1,500 net lines. Stop without approval above 40 files / 2,500 net lines for either PR.

## Explicit non-goals

- Multiple shells per agent or remote-repository shells.
- Persisting shell inventory or visible/hidden state.
- A second live attached viewer in the manager.
- Configurable keybindings, mouse support, or close confirmation in v1.
- Resuming a shell whose owner is not Running.
- Changes to external-terminal F8, agent launch signatures, or agent liveness semantics.

## Scope ledger

| Discovery | Classification | Disposition |
| --- | --- | --- |
| Existing `open_shell_window` already selects an existing `jefe-shell` | In-scope design evidence | Reuse for F10 resume; do not add duplicate shell creation |
| Viewer and captures follow multiplexer current window | Blocker-Fix invariant | Every hide/adoption must select window 0 before state says shell is hidden |
| F12 currently reaches psmux prefix in overlay mode | In-scope cross-platform bug | Intercept in overlay-first route and cover structurally |
| Existing #356 overlaps hide/resume | Resolved duplicate | Closed as superseded by #361 |

## Review counters

- PR A local pre-PR Open Code Review: 1 / 2 attempted; the CLI was terminated without output, so no findings were produced.
- PR A post-PR Open Code Review: 0 / 2.
- PR B local pre-PR Open Code Review: 0 / 2.
- PR B post-PR Open Code Review: 0 / 2.

## Verification evidence

| Candidate | Command | Result |
| --- | --- | --- |
| base | current main and issue/architecture analysis | COMPLETE at `74b2f736`; scenario/tests were authored before production changes, but the interrupted session did not retain runnable RED command output, so the evidence gap is recorded rather than reconstructed |
| PR A candidate | focused runtime/state/startup/UI tests | PASS: shell inventory 15, shell overlay 20, binary routing 1, Help 15, keybind bar 12 |
| PR A candidate | `scripts/issue222-run-scenario.sh` | PASS: 50 real-tmux steps; F12 hide preserved the marker, dashboard navigation remained usable, F10 resumed the same shell, F10 destroyed it, and typed `exit` restored Dashboard |
| PR A candidate | `make quick-check` | PASS: all workspace tests and doctests |
| PR A candidate | `make ci-check` | PASS: fmt, allow/source-size gates, clippy, coverage >=30%, locked build, tests |
| PR A candidate | scope review | APPROVED by user for issue #361 split delivery; PR A is 25 implementation/test/scenario files and 1 delivery record, 1,985 net lines. This exceeds the 25-file/1,500-line target only by the required delivery record and lifecycle coverage, remains below the 40-file/2,500-line hard stop, and introduces no dependency/workflow/persistence subsystem. |

## Review findings and deferred follow-ups

- PR A pre-PR OCR attempt produced no artifact because the CLI process was terminated; exact-head local gates and manual source review remain green.

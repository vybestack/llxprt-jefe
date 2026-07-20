# Issue 355 delivery plan: macOS-safe embedded-shell close shortcut

## Issue and decision

- GitHub: https://github.com/vybestack/llxprt-jefe/issues/355
- Branch: `issue355`
- Base: `origin/main` at `fb0aa0a`
- Decision: reuse `F10` as an embedded-shell toggle. In normal Dashboard input, `F10` keeps opening the embedded shell. While the shell overlay is active and owns terminal input, the existing overlay-first route intercepts `F10` before PTY forwarding and invokes the existing close orchestration. The old `F11` event is no longer a Jefe shortcut and is forwarded to the shell like other non-intercepted keys.
- Rationale: `F10` is already the discoverable open binding, has no second action while the overlay is active, and is delivered by supported platforms including default macOS configurations. This changes only the close trigger, not shell creation, destruction, or lifecycle ownership.

## Acceptance matrix

| ID | Actor / launch path | Inputs and boundary cases | Target | Observable success | Observable failure / diagnostics | Permitted side effects before failure | Persistence / compatibility | Proof |
| --- | --- | --- | --- | --- | --- | --- | --- | --- |
| A1 | User with an active embedded shell presses `F10` | Overlay is active, terminal input is focused, and the temporary shell window exists; modifier handling remains consistent with the existing function-key dispatch | All supported platforms, including default macOS | `F10` is consumed by Jefe before PTY forwarding and invokes the existing shell-close path | Runtime close failure keeps the overlay active and reports the existing warning, allowing retry or natural shell exit | Existing runtime close attempt and warning only | Overlay remains runtime-only; no schema or persisted shortcut change | focused input-routing unit test plus real-tmux scenario that opens and closes with `F10` while shell terminal focus is active |
| A2 | Dashboard user presses `F10` with no shell overlay active | Dashboard normal input, terminal not focused, selected running local agent remains subject to existing validation | All supported platforms | Existing behavior opens one embedded shell; the same documented key therefore acts as a context-sensitive toggle | Existing missing-selection, attachment, remote, and runtime-open warnings remain unchanged | Existing shell-open side effects only | Backward-compatible open behavior; no persistence change | routing regression test for the open binding plus real-tmux open step |
| A3 | User presses the former `F11` binding while the shell overlay is active | Shell terminal owns input | All supported platforms | Jefe does not close the overlay; `F11` follows the existing non-intercepted PTY forwarding path | Existing PTY write diagnostics apply | PTY input write only | Deliberately removes the unreliable Jefe shortcut; shell input compatibility is restored | focused input-routing boundary test proves only `F10` selects close; root routing evidence shows all other overlay keys are forwarded |
| A4 | Existing shell-close orchestration completes after `F10` interception | Runtime close succeeds or fails | All supported platforms | Success removes only the temporary `jefe-shell` window, leaves the agent running, restores dashboard focus/layout, and resizes the viewer to dashboard geometry | Failure leaves overlay state/focus/layout intact and surfaces the existing warning | No agent-session kill; no state transition before runtime close succeeds | Existing issue #222 lifecycle and runtime-only state are unchanged | existing reducer lifecycle tests plus the end-to-end scenario; focused routing test connects `F10` to the unchanged close path |
| A5 | User reads active-shell footer or Help | Overlay active for footer; Help opened from Dashboard for reference | All supported terminals | Active-shell footer says `F10 close shell`; Help documents `F10` as open/close toggle and no longer advertises `F11` | No runtime behavior; stale text is a test failure | None | User-facing documentation matches dispatch | keybind-bar render test and pure Help-content test |

## Explicit non-goals

- No configurable keybinding system or persistence/schema change.
- No changes to external-terminal `F8` behavior.
- No changes to how the shell window is created, observed, destroyed, or cleaned up.
- No changes to agent lifecycle, tmux/psmux command construction, PTY encoding, focus restoration, or layout computation.
- No platform-specific key translation or macOS settings manipulation.
- No dependency, workflow, quality-tool, `.llxprt/`, `.code_puppy/`, or `.github/` changes.
- No refactor or relocation of unrelated input, UI, runtime, or lifecycle tests beyond the explicitly approved source-size gate remediation recorded in the scope ledger.

## Bounded vertical slices

### Slice S1: context-sensitive `F10` routing and lifecycle preservation

- Acceptance rows: A1-A4.
- Architecture owner: existing `app_input::shell_overlay` orchestration at the established root overlay-first integration boundary.
- Allowed files: `src/app_input/shell_overlay.rs`; comments that describe this exact event in `src/messages.rs` and `src/state/events.rs`; `src/app_input/normal.rs` only to remove the now-stale statement that Jefe avoids `F11` globally.
- RED:
  1. Update the TUI scenario first so it opens with `F10`, requires the active-shell `F10` close hint, closes with `F10`, and observes restored dashboard focus.
  2. Add a focused routing test requiring `F10` to select close and the former `F11` key to remain non-intercepted.
  3. Run the focused test and scenario against the unmodified dispatch and record their intended failures.
- GREEN: switch only the close predicate to `F10`; retain overlay-first root ordering and all existing close/runtime/reducer code; focused tests and scenario pass.
- Explicit non-goals: no new input subsystem, public abstraction, route reordering, or runtime/state behavior.
- Verification: focused Rust test, existing shell-overlay state tests, `scripts/issue222-run-scenario.sh`, then `make quick-check`.
- Stop conditions: a new public contract, root route redesign, runtime/state changes beyond stale comments, or any unlisted production path.

### Slice S2: discoverable and consistent shortcut text

- Acceptance row: A5.
- Architecture owner: existing UI projection/component and Help projection.
- Allowed files: `src/ui/components/keybind_bar.rs`, `src/ui/modals/help.rs`, and the same TUI scenario.
- RED: component-render and Help-content assertions require the `F10` close/toggle text and reject stale `F11` close documentation.
- GREEN: active-shell footer and Help match actual dispatch; no other shortcut text changes.
- Explicit non-goals: no footer layout redesign, modal redesign, or broader documentation cleanup.
- Verification: focused UI tests and the TUI scenario, then `make quick-check`.
- Stop conditions: fitting the text requires layout architecture changes or edits outside the allowed paths.

## Expected paths by layer

| Layer | Paths | Acceptance mapping |
| --- | --- | --- |
| Delivery record | `project-plans/issue355-plan.md` | all rows and workflow evidence |
| TUI evidence | `dev-docs/tmux-scenarios/agent-shell-overlay.json` | A1, A2, A4, A5 |
| Input orchestration and test | `src/app_input/shell_overlay.rs` | A1-A4 |
| Exact stale code documentation | `src/messages.rs`, `src/state/events.rs`, `src/app_input/normal.rs` | A1, A3; ensure internal shortcut descriptions do not contradict dispatch |
| Active-shell footer | `src/ui/components/keybind_bar.rs` | A5 |
| Help projection | `src/ui/modals/help.rs` | A5 |
| Approved quality-gate remediation | `src/app_shell.rs`, `src/app_shell_workers.rs` | unblock exact-head verification without changing behavior |

Planned scope is 10 files and fewer than 300 net changed lines. The pull-request target remains at most 25 files / 1,500 net lines. Perform a mandatory scope review above either target and stop without explicit approval above 40 files / 2,500 net lines.

## Scope ledger

| Discovery | Classification | Disposition |
| --- | --- | --- |
| `F10` already opens the shell only from normal, unfocused Dashboard input | In-scope design evidence | Reuse it as a context-sensitive toggle while the overlay-first route owns all active-shell keys |
| Overlay routing executes before normal mode and terminal-capture forwarding | In-scope architecture constraint | Keep the root route unchanged; test the close predicate and prove the focused flow end to end |
| Existing close lifecycle reducer tests already cover focus restoration, clearing agent identity, and idempotence | In-scope existing evidence | Retain them and connect the new key to the unchanged close orchestration; do not duplicate state tests |
| The issue #222 runner and scenario already provide deterministic shell setup | In-scope existing evidence | Update only the scenario key/hint expectations; do not add another runner or fixture subsystem |
| `F11` remains mentioned in historical issue #222 planning evidence | Reject as implementation scope | Historical delivery records describe the behavior delivered at that time and are not user-facing current shortcut documentation |
| Current `origin/main` already has `src/app_shell.rs` at 1,009 lines, above the mandatory 1,000-line source-size limit | Blocker—Fix | User approved remediation before PR creation. Move the existing non-blocking PTY dirty-check helper into the established `app_shell_workers` cache/worker boundary; preserve behavior and bring `app_shell.rs` below the hard limit without moving tests or weakening the gate. |

Current scope after approved blocker remediation: 10 files (9 tracked modifications plus this plan), well below the target and hard budgets.

## Review counters

- Pre-PR Open Code Review: 1 / 2.
- Post-PR Open Code Review: 0 / 2.

## Verification evidence

| Candidate | Command | Result |
| --- | --- | --- |
| base behavior plus RED test | `cargo test --bin jefe f10_is_the_only_shell_overlay_close_shortcut` | RED as intended: unresolved close-route helper before implementation |
| base behavior plus RED scenario | `scripts/issue222-run-scenario.sh` | RED as intended at step 18: active shell did not advertise `F10 close shell` |
| working tree | focused input, footer, Help, and shell-overlay lifecycle tests | PASS: close route 1, footer 1, Help 1, existing lifecycle 5 |
| working tree | `scripts/issue222-run-scenario.sh` | PASS: 28 real-tmux steps; `F10` opened and closed the terminal-focused shell and restored the dashboard |
| working tree | `make quick-check` | PASS: format/check and all test targets (including 2,191 library and 725 binary tests) |
| working tree | approved source-size remediation plus focused tests | PASS: `src/app_shell.rs` is 994 lines; format, compile, close route, footer, Help, and 17 shell tests pass |
| working tree | `scripts/issue222-run-scenario.sh` after review remediation | PASS: 30 real-tmux steps, including active-overlay F11 non-close proof; artifacts at `target/tmux-harness/issue222-lKKo2Z` |
| working tree | `make ci-check` after remediation | PASS: format, clippy policy, source-size gate, clippy, coverage, locked build, full tests, and doctests |

## Review findings and deferred follow-ups

- Blocker—Fix resolved: the approved extraction moved the existing non-blocking PTY dirty check into `app_shell_workers`, reduced `src/app_shell.rs` from 1,009 to 994 lines, and preserved behavior.
- Reject: Open Code Review claimed `handle_shell_shortcut_key` would reopen instead of close an active overlay. Root dispatch routes every active-overlay key through `route_shell_overlay_key` and returns before `handle_pre_mode_shortcut`; the real-tmux scenario proves F10 closes while terminal input owns focus.
- Reject: Open Code Review suggested changing the pre-existing `(120, 40)` resize fallback. The fallback is unchanged by this issue, outside A1-A5, and changing its error semantics would widen scope without evidence of a regression.
- In-scope-Fix: CodeRabbit requested route-level proof that F11 is not consumed by the active overlay. The real-tmux scenario now sends F11 while the shell owns input, verifies the active-shell footer remains visible, and then closes with F10.

## Summary

Fixes #245 — mouse-wheel scrolling, scrollbar dragging, and scroll-key input (PageUp/PageDown/Home/End) were broken when running llxprt-code inside a Jefe-managed terminal, after the copy/selection (#197 / PR #218) and Code Puppy scrollback (#198 / PR #219, #221) changes.

All three input paths were unconditionally intercepted by Jefe's scrollback viewport / selection state machine regardless of agent kind, starving llxprt-code (a full TUI that handles its own scrolling via SGR mouse reporting) of the events it needed.

## Root cause

The scrollback/selection interception code used only `mouse_reporting_active` and `shift_held` to decide ownership. But **both** Code Puppy and llxprt-code advertise SGR mouse reporting, so that flag alone cannot distinguish them. The distinguishing predicate is `is_kennel_mode()`:

- **Code Puppy (kennel)** uses Jefe's tmux-history scrollback viewport — that is the entire point of the #198 design. Jefe **should** intercept the wheel and scroll keys for these sessions.
- **llxprt-code (non-kennel)** is a full TUI application with its own scrollback and mouse handling. It **must** receive wheel events, scrollbar drags, and scroll-key events directly over the PTY.

The previous code intercepted all three input paths unconditionally, so llxprt-code never received them.

## Fix

Gate every scrollback/selection interception path on `is_kennel_mode()`. Non-kennel agents (llxprt) now receive their events natively; kennel (Code Puppy) behavior is preserved exactly (#197 + #198).

### Layer changes

- **input** (`src/input.rs`): `should_intercept_for_scrollback` gains a `kennel_mode` param and returns `None` for non-kennel agents, forwarding all scroll keys (PageUp/PageDown/Home/End/arrows) to the PTY. `app_input/mod.rs` reads `is_kennel_mode()` once and threads it through.
- **mouse_routing** (`src/mouse_routing.rs`): the wheel pre-check in `route_terminal_gesture` is gated by a new pure helper `wheel_intercept_active_for_agent(kennel_mode, shift_held)` so non-kennel wheel events fall through to the gesture state machine, which forwards them to reporting children. The two per-event state reads are merged into a single lock acquisition.
- **selection/gesture** (`src/selection/gesture.rs`): the pure gesture-ownership state machine gains a `kennel_mode` field on `GestureEvent` and a new `PtyOwned` gesture state. For a non-kennel reporting child, an unmodified left gesture (down/drag/up) forwards to the PTY immediately and latches `PtyOwned` — so **scrollbar drags and clicks** reach llxprt. Shift+drag still does Jefe text selection for both kinds. Kennel (Code Puppy) reporting left-gestures keep the #197 `Pending` behavior (drag = Jefe selection, pure click = PTY replay) **unchanged**. A non-left event (wheel/right/middle) while `PtyOwned` resets to Idle (nothing is buffered, so no flush is needed).

### Behavioral contract

| Agent | Wheel | Scrollbar drag (left) | Shift+drag | Scroll keys | Text selection |
|---|---|---|---|---|---|
| Code Puppy (kennel) | Jefe scrollback (#198) | Jefe selection (#197) | Jefe selection | Jefe scrollback | Yes |
| llxprt (non-kennel) | Forwards to PTY | Forwards to PTY | Jefe selection | Forwards to PTY | Yes (shift+drag) |

## Testing (TDD)

- Pure truth-table tests for `wheel_intercept_active_for_agent` (kennel/shift/wheel/over-pane matrix).
- 8 `should_intercept_for_scrollback` tests asserting non-kennel forwards all scroll keys to the PTY.
- 8 gesture-state tests covering the non-kennel `PtyOwned` lifecycle: down/drag/up forward to PTY, shift still selects, non-reporting still selects, kennel still `Pending`, and wheel/other-button while `PtyOwned` resets to Idle.
- A routing-layer integration assertion that a non-kennel wheel forwards to the PTY via the gesture machine.
- A TUI scenario (`dev-docs/tmux-scenarios/llxprt-terminal-scroll.json`) documents the end-to-end PageUp path.

All existing #197 (Code Puppy selection) and #198 (Code Puppy scrollback) tests are preserved and pass unchanged.

## Verification

- `cargo fmt --all --check` — pass
- `CLIPPY_CONF_DIR=.github/clippy cargo clippy --workspace --all-targets --all-features -- -D warnings` — pass (zero warnings; no lint suppressions added)
- `cargo build --workspace --all-features --locked` — pass
- `cargo test --workspace --all-features --locked` — 1602 lib + 264 integration tests pass
- Coverage: 72.29% (floor 30%)

closes #245

# Issue #245 — Mouse scrolling and scrollbar selection are broken in llxprt-code

## Problem

After the copy/selection (#197 / PR #218) and Code Puppy scrollback (#198 /
PR #219, #221) changes, mouse-wheel scrolling and scrollbar interaction are
broken when running **llxprt-code** inside a Jefe-managed terminal.

### Observed
- Mouse-wheel input no longer scrolls llxprt-code.
- The scrollbar always triggers text selection instead of scrolling.

### Expected (from the issue)
- Mouse-wheel scrolls the llxprt-code terminal normally.
- Scrollbar drag/click scrolls rather than initiating text selection.
- Text selection remains available without intercepting ordinary scrolling.
- **Both** Code Puppy's Jefe-managed scrollback **and** llxprt-code's native
  scrolling must work.

## Root cause

`src/mouse_routing.rs::route_terminal_gesture` contains an **unconditional**
wheel-interception pre-check (introduced by #198):

```rust
if !shift_held && is_wheel_event(mouse_event) && is_event_over_terminal_pane(mouse_event) {
    refresh_terminal_scroll_geometry_from_ctx(ctx, app_state);
    if let Some(scroll_evt) = wheel_to_terminal_scroll_event(mouse_event) {
        let mut state = app_state.write();
        *state = std::mem::take(&mut *state).apply(scroll_evt);
    }
    return true; // ALWAYS consumed by Jefe scrollback — never reaches the PTY
}
```

This steals **every** non-shift wheel event over the focused terminal pane for
Jefe's scrollback viewport, **regardless of agent kind**. The pure gesture
state machine (`selection/gesture.rs::process_wheel`) already correctly
forwards wheel events to a mouse-reporting child, but the router's pre-check
short-circuits before the state machine ever sees the wheel event.

### Why this breaks llxprt-code but not Code Puppy

- **Code Puppy** (kennel / `AgentKind::CodePuppy`) is a line-oriented agent
  whose tmux pane retains history. Issue #198 built a Jefe-side scrollback
  viewport (capture-pane history + live snapshot) specifically for it. Jefe
  **should** intercept the wheel for these sessions.
- **llxprt-code** (`AgentKind::Llxprt`) is a full TUI application that
  advertises SGR mouse reporting and handles its **own** scrolling — it has
  its own scrollback and expects to receive wheel events over the PTY. Jefe
  intercepting the wheel starves it.

Both kinds set `mouse_reporting_active() == true`, so the reporting flag alone
cannot distinguish them. The distinguishing predicate is `is_kennel_mode()`
(selected agent is Code Puppy).

A parallel unconditional intercept exists in the **keyboard** path:
`app_input::try_intercept_terminal_scrollback` →
`input::should_intercept_for_scrollback` intercepts PageUp/PageDown/Home/End
for the Jefe scrollback in `InputMode::TerminalCapture` regardless of agent
kind. This steals llxprt-code's keyboard scroll keys the same way.

## Fix design

Gate **both** the wheel and the keyboard scrollback interception on
`is_kennel_mode()`. When the focused terminal belongs to a non-kennel agent
(llxprt), the events flow through the existing gesture/forwarding path so the
child receives them natively.

### Layer responsibilities (respect existing boundaries)

| Layer   | Change                                                                 |
|---------|------------------------------------------------------------------------|
| Input   | `should_intercept_for_scrollback` gains a `kennel_mode` param; returns `None` for non-kennel so keys forward to the PTY. Pure + unit-tested. |
| App input | `try_intercept_terminal_scrollback` reads `is_kennel_mode()` once and threads it into the pure helper. |
| Mouse routing | The wheel pre-check in `route_terminal_gesture` only intercepts when `is_kennel_mode()`; otherwise the wheel flows through the gesture state machine (which already forwards to reporting children). |
| State   | No new state; reuses existing `is_kennel_mode()` selector. |

No new AppState fields, no persistence changes, no UI changes.

### Behavioral contract

1. **Kennel (Code Puppy) + wheel over terminal** → Jefe scrollback viewport
   moves (unchanged #198 behavior).
2. **Non-kennel (llxprt) + wheel over terminal, reporting active** → wheel
   forwards to the PTY (llxprt-code scrolls itself).
3. **Non-kennel (llxprt) + wheel, NOT reporting** → falls through to app-level
   detail scroll (unchanged).
4. **Shift+wheel (any agent)** → host passthrough (unchanged #197 behavior).
5. **Kennel (Code Puppy) + PageUp/PageDown/Home/End** → Jefe scrollback
   (unchanged #198 behavior).
6. **Non-kennel (llxprt) + PageUp/PageDown/Home/End** → forwarded to the PTY.
7. Text selection (left-button drag / shift-drag) remains unchanged for both
   agent kinds.

## Test-first plan (RED → GREEN → REFACTOR)

### Pure unit tests (state-machine / helper level)

- `selection/gesture_tests.rs` already covers wheel-forward-to-reporting; no
  change needed there (the state machine is already correct).
- `input.rs` tests: extend `should_intercept_for_scrollback` tests to assert
  that for `kennel_mode == false` the helper returns `None` for PageUp /
  PageDown / Home / End / arrows (events must forward to the PTY). Keep the
  existing `kennel_mode == true` assertions.
- `mouse_routing_tests.rs`: add tests proving the wheel pre-check is gated on
  kennel mode — for a non-kennel focused terminal the wheel is NOT consumed
  by the Jefe scrollback (the scroll offset does not change). Use the
  `focused_terminal_state(AgentKind::Llxprt)` helper.

### Integration test

- `tests/runtime/terminal_focus_routing.rs`: add a test asserting that for a
  non-kennel reporting terminal, a wheel event produces a `ForwardToPty`
  gesture action (the state machine path), and for a kennel terminal the
  wheel is consumed by the scrollback (offset changes).

### TUI harness scenario

- Add `dev-docs/tmux-scenarios/llxprt-terminal-scroll.json`: focus the
  terminal on an llxprt agent, produce output, send a wheel scroll, and
  assert the child received it (via a capture / expect on scrolled content).
  This proves the end-to-end path. (Soft assert mode, like the existing
  scrollback scenario.)

## Verification

`make ci-check` (fmt, clippy gates, coverage ≥30, build, test).

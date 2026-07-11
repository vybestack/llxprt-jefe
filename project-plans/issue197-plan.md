# Plan: Fix terminal text selection and copy for Code Puppy sessions (#197)

## Problem

Inside the embedded TerminalView, drag-to-select and copy do not work when a
Code Puppy agent is running, even though the same Code Puppy build supports
native drag selection when run in a normal terminal.

Three root causes in `src/mouse_routing.rs` + `src/selection/geometry.rs`:

1. **Shift-drag is dropped entirely.** `handle_fullscreen_mouse` returns
   immediately when `SHIFT` is held, assuming the host terminal will paint its
   own selection. But the host terminal sees Jefe's rendered canvas, not the
   nested PTY grid, so nothing happens — no highlight, no copy.

2. **Mouse-reporting agents (Code Puppy) swallow drags.** When the child has
   mouse reporting on, `forward_to_pty_if_in_terminal` forwards drag/up to the
   PTY, so Jefe never starts its own selection. Code Puppy "normally relies on
   ordinary terminal selection," so this prevents the expected behavior.

3. **Geometry gap when focused.** `pane_at` returns `None` for the terminal
   region when `terminal_input_enabled` is true. So even when forwarding is
   skipped (e.g. no mouse reporting), `resolve_selection_point` cannot resolve
   a `TerminalView` selection and no selection begins.

## Design

Introduce an explicit **mouse routing policy** so Jefe picks the right
behavior per agent, rather than a single host-terminal assumption.

### New pure policy module: `src/app_input/terminal_mouse_policy.rs`

A single pure, iocraft-free, side-effect-free function:

```rust
/// How a mouse gesture over the focused terminal should be routed.
pub enum TerminalMouseRouting {
    /// Jefe paints a selection over the snapshot and copies on release.
    AppSelection,
    /// Forward the gesture to the child PTY (when mouse reporting is active).
    ForwardToPty,
}

/// Resolve the routing policy for a mouse gesture over the focused terminal.
///
/// Inputs are pure booleans (no runtime lock), so the decision is fully
/// unit-testable.
#[must_use]
pub fn route_terminal_mouse(
    agent_is_kennel: bool,   // selected agent is Code Puppy
    shift_held: bool,        // SHIFT modifier on the gesture
    mouse_reporting_active: bool, // child currently reports mouse
) -> TerminalMouseRouting
```

Decision table (the documented contract):

| agent kennel | shift | reporting | routing        | rationale                                             |
|-------------:|:-----:|:---------:|----------------|-------------------------------------------------------|
| any          | yes   | any       | AppSelection   | Shift-drag must not be a no-op (#197).                |
| no (LLxprt)  | no    | yes       | ForwardToPty   | preserve existing LLxprt mouse behavior.              |
| no (LLxprt)  | no    | no        | AppSelection   | nothing to forward; select over snapshot.             |
| yes (CP)     | no    | yes       | AppSelection   | CP relies on terminal selection; Jefe owns the grid.  |
| yes (CP)     | no    | no        | AppSelection   | select over snapshot.                                 |

Rationale captured in the enum/fn docs: Code Puppy advertises mouse reporting
for transient menus, but its primary interaction model is ordinary terminal
selection which Jefe now owns for the embedded view. LLxprt genuinely drives
mouse reporting and must keep PTY forwarding. Shift always means "I want a
host/Jefe selection," mirroring standard terminal semantics.

### `handle_fullscreen_mouse` rewrite

Replace the top of `handle_fullscreen_mouse` so:

- It reads the selected agent's kennel flag and `mouse_reporting_active()`
  under the existing short-lived ctx + state guards.
- It calls `route_terminal_mouse(...)` to get a `TerminalMouseRouting`.
- On `AppSelection`: proceed straight to the selection handlers (begin/update/
  finalize), passing a new `force_terminal_selectable` so the geometry can
  resolve `TerminalView` even while focused.
- On `ForwardToPty`: keep the existing `forward_to_pty_if_in_terminal` path.

This removes the blanket early `return` for SHIFT. Shift-drag now routes to
`AppSelection` (and the SGR encoder already suppresses Shift forwarding in
`mouse_event_to_bytes`, so the two layers stay consistent).

### Geometry: resolve `TerminalView` while focused, when selecting

`dashboard_pane_at` currently returns `None` for the terminal region when
`terminal_input_enabled`. Add a parameter (or a focused-but-selectable flag)
so that, when the routing decision is `AppSelection`, `pane_at` resolves
`SelectablePane::TerminalView` with the same geometry used when unfocused.

Concretely, thread a `terminal_selectable: bool` through `pane_at` /
`dashboard_pane_at` (distinct from `terminal_input_enabled`). The mouse router
sets `terminal_selectable = matches!(routing, AppSelection)`. Existing call
sites pass `terminal_selectable == !terminal_input_enabled` to preserve their
behavior verbatim.

### `resolve_selection_point` already copies from the snapshot

`finalize_and_copy_selection` builds content via `pane_content_lines(...)` →
`terminal_lines(snapshot)`, which already reads the rendered snapshot cells
(Unicode + wrapped rows) — so the copied text matches the TerminalView as long
as the snapshot is captured. No change needed there; the issue's
"Unicode/wrapped" criterion is satisfied by reusing the snapshot projection.

## Tests (TDD: RED → GREEN)

### RED — pure policy (`src/app_input/terminal_mouse_policy.rs` inline tests)

Assert every row of the decision table, including: shift-drag is never
`ForwardToPty`, kennel agents never forward unmodified, LLxprt forwards when
reporting.

### RED — geometry (`src/selection/tests.rs`)

New test: with `terminal_selectable = true`, a coordinate in the terminal
region resolves to `SelectablePane::TerminalView` (the focused case that
previously returned `None`). Keep the existing
`pane_at_dashboard_terminal_focused_returns_none_in_terminal_region` semantics
for the `terminal_input_enabled && !terminal_selectable` case (forward path).

### RED — mouse routing integration

A focused-TerminalView + Code Puppy agent + `mouse_reporting_active` gesture
must begin/update/finalize a `TerminalView` selection (not forward, not no-op).
Cover:
- Code Puppy + plain drag → selection begins.
- Code Puppy + shift-drag → selection begins (was no-op).
- LLxprt + mouse reporting + plain drag → forwarded (existing behavior kept).
- LLxprt + no reporting + plain drag → app selection.

Because `handle_fullscreen_mouse` is iocraft-hook-bound, the testable seam is
the pure policy + geometry; a thin extraction of the decision into a pure
helper keeps the hook closure minimal and unit-testable, mirroring the existing
`next_wheel_scroll_offset` pattern.

### TUI harness scenario (`dev-docs/tmux-scenarios/kennel-terminal-select.json`)

Exercise selection/copy in Kennel mode: launch, enter terminal focus, open the
host copy/selection path. Because the harness is keyboard/tmux-driven and
OSC 52 copy cannot be asserted deterministically across machines, the scenario
documents the Kennel-mode selection path and captures the focused terminal
screen. It is intentionally a local/manual scenario (not a hard CI gate),
matching the existing `scratch-pr-mode.json` / `actions-mode.json` policy for
scenarios that depend on per-machine state.

## Files

- NEW `src/app_input/terminal_mouse_policy.rs` — pure routing policy + tests.
- `src/app_input/mod.rs` — register the new module.
- `src/mouse_routing.rs` — rewrite the top of `handle_fullscreen_mouse`,
  thread `terminal_selectable`.
- `src/selection/geometry.rs` — `pane_at` / `dashboard_pane_at` accept
  `terminal_selectable`.
- `src/selection/tests.rs` — focused-terminal-selectable geometry test.
- `tests/runtime/terminal_focus_routing.rs` — keep existing LLxprt forwarding
  covered; add Code Puppy selection-not-forwarded coverage.
- NEW `dev-docs/tmux-scenarios/kennel-terminal-select.json`.

## Out of scope

- Changing how LLxprt forwards mouse events (existing behavior preserved).
- Multi-pane cross-terminal selection.
- tmux copy-mode workflow (the issue says it is not a practical path).

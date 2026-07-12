# Issue #198 — Usable scrollback for embedded Code Puppy sessions

## Problem

Jefe renders only the **current** alacritty terminal snapshot (the live PTY
grid) for an embedded agent. There is no user-facing history: wheel/trackpad
scrolling and Page Up/Down do not expose prior output, even though the
underlying tmux pane retains it (proven by working Page Up/Down in tmux copy
mode). tmux copy mode is not an acceptable UX (no usable selection/copy through
Jefe, and it leaks raw tmux into the embedded-terminal experience).

The goal: Code Puppy scrollback "just works" inside Jefe via a proper Jefe
embedded-terminal history viewport.

## Design overview

Introduce a **history viewport** for the embedded terminal pane. It composes a
retained history tail (captured from tmux) with the live PTY snapshot, and
applies a bounded scroll offset owned by the state layer. Scrolling pauses
follow-tail; returning to the bottom resumes live follow. A subtle position /
follow indicator renders when not at the bottom.

### Layer responsibilities (respect existing module boundaries)

| Layer          | Owns (this feature)                                                              |
|----------------|----------------------------------------------------------------------------------|
| Runtime        | Retrieve retained tmux history without shelling out every frame (cached).       |
| State          | Bounded scroll offset + follow-tail policy for the terminal pane (deterministic).|
| Input          | Wheel + Page Up/Down move the viewport (intercept before PTY forward).          |
| UI             | Pure viewport projection (history+live windowing) + follow indicator rendering. |
| Selection      | Coordinate text selection over the scrolled viewport (history-aware content).  |

Runtime owns capture; state owns policy; UI renders a projection. This matches
the pure-views pattern and the DAG.

---

## Runtime layer — retained history capture

### New tmux command helper

`capture-pane -p -S -<N> -E -` captures lines from history. Extend
`src/runtime/commands.rs` with a function that captures a bounded history tail
(e.g. last N lines including the visible pane) for a session name. Use
`-S -<history_lines>` to start N lines before the top of the visible pane and
`-E -` to end at the bottom of the visible pane (current line). This returns
plain text lines (no styles), which is acceptable for scrollback history (the
live snapshot already provides styled current content).

Note: `capture_session_output` (dead-pane crash text) already exists and uses
`capture-pane -p`. The new path is history-aware and bounded.

### `RuntimeManager` trait extension

Add a method to retrieve retained history lines for the currently attached
session, returning `Option<Vec<String>>` (plain text rows). Implementations:

- **`TmuxRuntimeManager`**: shell out to `capture-pane` (bounded), memoized per
  render-dirty cycle so it is **not** invoked on every render frame. The cache
  invalidates when `take_dirty()` returns true (new PTY data arrived). The
  cache stores the raw captured lines + the captured-at dimensions.
- **`StubRuntimeManager`**: return `None` (no PTY) — sufficient for unit tests;
  the offset/policy logic is tested purely in the state layer without a PTY.

### Caching strategy (acceptance: "without shelling out on every render frame")

- The history is re-captured only when the viewer reports dirty (new output) or
  the session changes. On a clean (non-dirty) render frame the cached lines are
  reused. This mirrors the existing `take_dirty` event-driven render gate.
- Bound: cap retained history at a fixed number of lines (e.g. 2000, matching
  the harness `history_limit`). Older history is dropped. This bounds memory
  and render cost.

---

## State layer — bounded scroll offset + follow-tail policy

### New fields on `AppState` (runtime-only, never persisted — like `selection`)

```
pub terminal_history_offset: Option<usize>,  // None = follow-tail (live)
```

- `None` (default) means **follow-tail**: render the live snapshot at the
  bottom (current behavior). No indicator.
- `Some(n)` means the viewport is scrolled back `n` lines from the bottom.
  Follow-tail is paused. A follow indicator renders.

### Deterministic policy helpers (pure, in `src/state/`)

Pure functions (unit-testable, no I/O):

- `terminal_scroll_up(offset, history_lines, viewport_rows, step) -> Option<usize>`
- `terminal_scroll_down(offset, history_lines, viewport_rows, step) -> Option<usize>`
  (returns `None` when the viewport reaches the bottom → resume follow-tail by
  clearing the offset to `None`).
- `terminal_scroll_page_up / page_down` variants (step = viewport_rows).
- `terminal_at_bottom(offset, history_lines, viewport_rows) -> bool`
- `terminal_follow_indicator(offset, history_lines, viewport_rows) -> Option<FollowIndicator>`
  — returns the indicator label/position when scrolled back, `None` when
  following.

These are the deterministic reducer helpers. The reducer applies them on new
`AppEvent` variants; the app-shell input layer translates keys/wheel into
those events.

### Why state owns this

The architecture standard says the app-state/event layer owns state
transitions and the policy must be deterministic + unit-testable. Putting the
offset in `AppState` lets the pure projection and the reducer both reason about
it without I/O, and lets selection coordinate with the same offset.

---

## Input layer — intercept scroll/keys before PTY forwarding

### Current flow (context)

When `terminal_focused && pane_focus == Terminal`, `resolve_input_mode`
returns `InputMode::TerminalCapture` and **all** keys are forwarded to the PTY
via `forward_key_to_pty`. Mouse wheel events, when mouse reporting is active,
are forwarded to the PTY via `forward_to_pty_if_in_terminal`.

### New behavior

Add an interception **before** PTY forwarding, only when the terminal pane is
focused:

- **Page Up / Page Down / Up / Down (when scrolled back)** → move the Jefe
  history viewport (dispatch the new state events), do NOT forward to PTY.
- **Wheel Up / Wheel Down over the terminal pane** → move the viewport by one
  line (mouse_routing intercepts before `forward_to_pty_if_in_terminal`).
  Shift+wheel keeps the existing host-terminal bypass.
- **End / Ctrl-End / mouse-click-at-bottom / any keystroke that resumes
  follow** → clear offset back to `None` (follow-tail resumes). The simplest
  contract: pressing End, or scrolling/clicking to the bottom, resumes follow;
  **new output while scrolled back does NOT jump the viewport** (the offset is
  sticky until the user returns to bottom).

Implementation seam: in the key handler, when `InputMode` would be
`TerminalCapture`, first check if the key is a scroll-control key and dispatch
the corresponding `AppEvent` instead of forwarding to PTY. Add a small
`should_intercept_for_scrollback(key_event) -> Option<AppEvent>` helper (pure,
unit-testable) that the app-shell key path consults. Same for the mouse path:
`mouse_routing::handle_fullscreen_mouse` checks the terminal-focus + scrollback
case before `forward_to_pty_if_in_terminal`.

**Important interaction with mouse reporting**: when the child has mouse
reporting active (e.g. Code Puppy's TUI), wheel events currently go to the PTY.
For scrollback to "just work," wheel-up over the terminal pane must move the
Jefe viewport, not the child. This is the crux of the issue. The design: wheel
events over the terminal pane move the Jefe viewport (intercept), while clicks
and drags still go to the PTY when the child reports mouse mode (so the child
UI stays interactive). This is the Code-Puppy-specific routing the issue
explicitly permits. Shift+wheel still bypasses to the host terminal.

---

## UI layer — pure viewport projection + indicator

### Pure projection (iocraft-free, `src/terminal_view.rs` or a new pure module)

`build_terminal_viewport(...)` takes:
- live `TerminalSnapshot` (styled grid),
- retained history lines (`Vec<String>`),
- `offset: Option<usize>`,
- `viewport_rows`, `viewport_cols`,

and returns a windowed `TerminalSnapshot`-like projection: the rows that should
be painted (history rows above + live rows, windowed by the offset) plus an
optional follow indicator descriptor. When `offset == None`, the projection is
just the bottom `viewport_rows` of history+live (i.e. the live follow view).

The existing `TerminalGrid` then paints this projected snapshot unchanged.
History rows carry a default style (plain text); live rows keep their styles.

### Follow indicator

When `offset.is_some()`, render a subtle indicator (e.g. a dim one-line banner
like "scrollback: N lines up — End/↓ to follow") at the top or bottom of the
terminal pane chrome. This is a render-only concern in `TerminalView`; the
decision (whether + what text) is computed by the pure projection so it is
unit-testable. No emoji (display-and-ui policy).

---

## Selection layer — history-aware content projection

`mouse_routing::finalize_and_copy_selection` builds pane content via
`pane_content_lines`. For the `TerminalView` pane, extend the content source to
include retained history lines above the live snapshot rows, accounting for the
current offset so selection coordinates map to the correct content line. The
selection geometry (`selection::geometry`) must treat the terminal pane as
`history_lines + live_rows` tall when computing content coordinates.

Coordinate the offset: `scroll_offset_for_pane` returns the terminal history
offset for the `TerminalView` pane (same pattern as detail panes).

---

## Test plan (TDD — RED → GREEN → REFACTOR)

### State layer (pure, unit)

- Scroll up from follow-tail sets an offset; scroll down to bottom clears it
  (resumes follow).
- Scroll up/down clamps to `[0, max]`; max derived from history + live lines −
  viewport rows.
- New output while scrolled back does not move the offset (sticky).
- Page up/down step by viewport rows.
- Follow indicator descriptor correct when scrolled back; `None` when
  following.

### Runtime layer (unit + integration)

- `capture-pane -S` command builder produces correct argv (unit, pure parser).
- History cache returns cached lines on a non-dirty frame (no second shell-out)
  — unit with a stub that counts calls.
- `StubRuntimeManager` returns `None` for history (contract test).

### Input layer (unit)

- `should_intercept_for_scrollback(PageUp) -> Some(scroll event)`; PageDown,
  Up/Down-when-scrolled, End similar.
- Non-scroll keys return `None` (forward to PTY as before).
- Mouse wheel over terminal pane dispatches viewport scroll before PTY
  forwarding.

### UI projection (pure, unit)

- `build_terminal_viewport` windows history+live correctly for given offset +
  viewport size.
- Offset `None` → bottom viewport (follow).
- Indicator descriptor present iff scrolled back.

### Selection (unit)

- Selection over history maps content coords with the offset accounted for.

### TUI harness scenario (RED first)

Create `dev-docs/tmux-scenarios/terminal-scrollback.json` — a manual/scratch
scenario (not a CI gate, like `scratch-pr-mode.json`) that:
1. Launches jefe, focuses the terminal (requires a configured agent — so this
   is a developer-machine scratch scenario).
2. Produces output exceeding the pane height.
3. Scrolls up (PageUp / wheel), asserts history is visible.
4. Scrolls back to bottom, asserts follow resumes.

The scenario JSON is created **first** to prove the RED intent, then the
behavior is implemented. Mark it as a non-CI scratch scenario in the harness
docs (deterministic agent output cannot be guaranteed across machines).

---

## Acceptance criteria mapping

- ✅ Runtime/session APIs retrieve retained tmux history without shelling out
  every render frame → cached `capture-pane -S` path.
- ✅ State owns a bounded scroll offset / follow-tail policy; runtime owns
  history capture → `AppState.terminal_history_offset` + pure policy helpers.
- ✅ Wheel and Page Up/Down covered for Code Puppy sessions → input
  interception before PTY forwarding.
- ✅ Live output, resize, agent switching, detach/reattach, returning to bottom
  tested → state + runtime unit tests + scenario.
- ✅ Selection/copy over scrolled history behaviorally tested → selection
  history-aware content projection tests.
- ✅ TUI harness scenario proves wheel or key scrollback in Kennel mode →
  `terminal-scrollback.json`.

---

## Implementation order (for the implementing subagent)

1. **Runtime**: `capture_pane_history` command + `RuntimeManager` trait method +
   `TmuxRuntimeManager` cache + stub. Unit tests first (RED).
2. **State**: `terminal_history_offset` field + pure policy helpers. Unit tests
   first (RED).
3. **UI projection**: pure `build_terminal_viewport` + indicator. Unit tests
   first (RED). Wire into `TerminalView` + dashboard props.
4. **Input**: `should_intercept_for_scrollback` + mouse interception. Unit
   tests first (RED). Wire into app-shell key path + `mouse_routing`.
5. **Selection**: history-aware content for `TerminalView` pane. Tests first.
6. **TUI scenario**: create `terminal-scrollback.json` (RED), document as
   scratch/non-CI.
7. Full verification: `make ci-check`.

## Constraints (must follow)

- TDD mandatory (RED → GREEN → REFACTOR). No production code before a failing
  test for the behavior.
- Pure-views pattern: viewport windowing + indicator decision live in an
  iocraft-free `#[must_use]` function.
- No `unwrap`/`expect` in production paths; typed errors / Result / Option.
- Files < 1000 lines (warn 750); functions < 60 lines; cognitive complexity <
  15.
- No new clippy allows, no lint/complexity threshold increases, no suppression
  directives.
- No emoji in UI.
- Runtime-only state (`terminal_history_offset`) is never persisted (like
  `selection`, `quit_sequence`).

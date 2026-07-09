# Display and UI Standards

This document defines the emoji-free policy, the pure-projection discipline for
display logic, the screen/component structure, the keybind-footer and help-modal
conventions, and the theme/UX rules for Jefe. It absorbs and supersedes section 8
of the former `dev-docs/project-standards.md` and the Theme/Visual Standards of
`docs/project-standards.md`.

Sibling standards:

- [Architecture Standards](./architecture.md)
- [Coding Standards](./coding-standards.md)
- [Testing and Quality](./testing-and-quality.md)
- [Persistence and Runtime](./persistence-and-runtime.md)

---

## Emoji-Free Policy

**No pictographic emojis anywhere in the UI.** This keeps rendering deterministic
across terminals and avoids width-measurement issues.

- Textual symbols and box-drawing/checkbox characters are fine and are used
  throughout: `│` (box-drawing borders), `✓` (checkmark), `×` (cross/multiply),
  `→` (arrow), `⌥` (option key in hint strings), `⬜`/checkbox glyphs.
- The codebase measures terminal cell widths with the `unicode-width` crate
  (a direct dependency in `Cargo.toml`). Pictographic emojis are frequently
  double-width and render inconsistently across terminal emulators, which breaks
  the deterministic grid layout the UI depends on. Textual symbols have
  well-defined widths.
- This policy applies to documentation too: do not add pictographic emojis to
  markdown docs, hint strings, or status text.

---

## Pure Projections for Display Logic

Display logic that computes **what to render** must live in pure, iocraft-free
functions so it is unit-testable without a terminal. This is the pure-views
pattern, documented in full in [Architecture Standards](./architecture.md).

Two canonical examples live in the UI layer:

- **`src/ui/components/keybind_bar.rs`** — `keybind_hints_for(screen_mode,
  terminal_focused) -> &'static str` is a pure `#[must_use]` function. The
  iocraft `KeybindBar` component just renders its return value.
- **`src/text_box_view.rs`** — `build_text_box_view(text, byte_cursor,
  viewport_rows, content_width) -> TextBoxView` is a pure, iocraft-free
  projection. The `ui::components::text_box` component consumes it.

When you add display-deciding logic (viewport windowing, caret placement,
filtering/sorting for display, hint construction), extract it into a pure
function. Do not bake it into an iocraft component.

---

## Screen and Component Structure

The UI is organized into three directories under `src/ui/`:

| Directory     | Contents                                                                                      |
|---------------|-----------------------------------------------------------------------------------------------|
| `screens/`    | Screen-level layouts: `dashboard`, `split`, `issues`, `pull_requests`, `new_agent`, `new_repository`. |
| `components/` | Reusable components: `sidebar`, `agent_list`, `terminal_view`, `preview`, `status_bar`, `keybind_bar`, `text_box`, `issue_list`, `issue_detail`, `pr_list`, `pr_detail`, filter controls, choosers, `scrollable_text`. |
| `modals/`     | Modal overlays: `help`, `confirm`.                                                            |

### Component contract

Components receive `Props` containing a cloned `AppState` snapshot and
`ThemeColors`, and return element trees. Components:

- do not mutate `AppState`,
- do not call `PtyManager` directly (PTY interaction flows through the root
  component's event handler),
- receive owned/cloned data, never references into `AppState`.

### The render cycle

The root `App` component uses iocraft hooks: `use_state` for `AppState`/
`ThemeManager`/render-tick, `use_future` for the ~30fps PTY poll timer,
`use_terminal_events` for keyboard/mouse/resize dispatch. On each render, `App`
clones the `AppState` snapshot, extracts PTY data for the active agent, and
passes both to the active screen component as props.

### The message/conversion pattern

UI intent flows through the unidirectional pipeline (see
[Architecture Standards](./architecture.md)):

```text
AppEvent -> AppMessage -> AppState::apply_message -> render
```

The conversion seam is `src/messages/event_conversion.rs`. The UI keeps
producing the historical `AppEvent` facade; reducers route through typed domain
messages.

### Screen modes

`ScreenMode` (`src/state/types.rs`) enumerates the active screen:

- `Dashboard` (default) — repositories, agents, terminal, preview.
- `Split` — compact cross-agent operational view.
- `DashboardIssues` — issues list/detail with filter and search.
- `DashboardPullRequests` — PR list/detail with filter, search, merge.

---

## Keybind Footer Convention

The bottom `KeybindBar` (`src/ui/components/keybind_bar.rs`) shows
context-sensitive keyboard hints via the pure
`keybind_hints_for(screen_mode, terminal_focused)` function.

- Each `ScreenMode` has its own hint string. The hint text is the single source
  of truth for what shortcuts are available in that mode.
- When the terminal is focused (`terminal_focused == true`), the bar shows only
  `F12 unfocus` regardless of screen mode, because all other keys are forwarded
  to the PTY.
- The bar renders inverted: theme foreground as background, theme background as
  text.

Current hint strings (authoritative source: `keybind_hints_for`):

- **Dashboard**: navigate, pane switch, terminal focus, active-only toggle,
  option-key agent shortcuts, new agent/repo, delete, kill, restart, relaunch,
  reorder, split, help, quit.
- **Split**: select, grab, move, back, help, quit.
- **DashboardIssues**: navigate, open detail, new issue, filter, search, cycle
  focus, reply, send-to-agent, edit, comment, exit issues, back.
- **DashboardPullRequests**: navigate, open detail, filter, search, cycle focus,
  reply, send-to-agent, edit, comment, open in browser, merge, exit, back.

When you add or change a shortcut, update both the key dispatch (in the root
event handler) and the matching hint string in `keybind_hints_for`.

---

## Help Modal Convention

`?`, `h`, or `F1` opens the help modal (`src/ui/modals/help.rs`) — a scrollable
keyboard reference. The modal lists the available shortcuts for the current
context. `Esc` or `?`/`h`/`F1` again closes it.

---

## Theme and UX

### Mandatory Defaults

- The default theme is **Green Screen**: `#6a9955` foreground on `#000000`
  background.
- `#00ff00` (bright green) is reserved for high-emphasis elements only: the
  running-status indicator and focused borders. It must not be used as
  general-purpose text color.
- `#4a7035` is the dim/muted color for secondary text, inactive elements, and
  de-emphasized content.
- All shipped themes must have `"kind": "dark"`. No light themes. No bright
  default palettes.

### Theme Color Slots

Every theme JSON must define all color slots in the theme file format (see
[`docs/technical-overview.md`](../../docs/technical-overview.md) for the full
slot list). Missing slots fall back to green-screen values, which may produce
visual inconsistency in non-green themes. Theme authors must populate every
slot.

Key slots:

| Slot             | Green Screen value | Use                              |
|------------------|--------------------|----------------------------------|
| `background`     | `#000000`          | App background.                  |
| `foreground`     | `#6a9955`          | Default text.                    |
| `bright_foreground` | `#00ff00`       | High-emphasis (running, focused).|
| `dim_foreground` | `#4a7035`          | Dim/muted secondary text.        |
| `border`         | `#6a9955`          | Default borders.                 |
| `border_focused` | `#00ff00`          | Focused-pane borders.            |
| `status_running` | `#00ff00`          | Running agent status.            |

### Terminal View Colors

The embedded terminal view remaps ANSI default/named colors to the active
theme's palette. Explicit 256-color and RGB colors set by the child process are
passed through unmodified. Only the 16 named ANSI colors and the logical
Foreground/Background/Cursor colors follow the theme.

### Theme Loading and Fallback

Theme loading, selection, and fallback are owned by the theme layer
(`src/theme/`). See [Persistence and Runtime](./persistence-and-runtime.md) for
how the active theme slug is persisted. Invariants:

- Green Screen is always the first embedded theme and the startup default.
- All embedded themes are dark. `kind: "dark"` is the only supported value.
- Serde deserialization ignores unknown JSON keys, enabling forward-compatible
  theme files.
- External themes loaded from disk never replace an embedded theme with the same
  slug.
- `ResolvedColors::from_theme(None)` always returns green-screen values. There
  is no code path where a component renders without color information.

### Terminal Focus Semantics

Terminal focus (capture mode) is toggled by `F12` or `t`. It is unconditional —
it works in all screen modes — and it is reversible: pressing `F12`/`t` again
unfocuses. When focused, the keybind bar shows only `F12 unfocus`, and all keys
(except the unfocus toggle) are forwarded to the PTY as raw bytes.

Keyboard behavior must remain explicit and predictable. Focus state is part of
`AppState` and transitions through the reducer; it is never implicit.

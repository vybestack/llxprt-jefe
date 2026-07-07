# Architecture Standards

This document defines the module boundaries, the unidirectional data flow, the
pure-views projection pattern, and the dependency-direction rules for Jefe. It
consolidates and supersedes the architectural sections of the former
`dev-docs/project-standards.md` and `dev-docs/RULES.md`. For deeper background
on each module see [`docs/technical-overview.md`](../../docs/technical-overview.md).

Sibling standards:

- [Coding Standards](./coding-standards.md)
- [Testing and Quality](./testing-and-quality.md)
- [Display and UI](./display-and-ui.md)
- [Persistence and Runtime](./persistence-and-runtime.md)

---

## Module Boundaries

Jefe is a single-binary Rust TUI application. Every module owns one concern and
exposes it through a typed contract. Side effects live at boundary modules; the
core logic stays deterministic and unit-testable.

| Layer          | Owns                                                                                   | Must not do                                   |
|----------------|----------------------------------------------------------------------------------------|-----------------------------------------------|
| UI             | Render state, emit user intent                                                         | Mutate state, call the runtime directly       |
| App state/event| State transitions (the deterministic reducer)                                          | I/O, process spawning, theme loading          |
| Runtime        | tmux/PTY orchestration, Alacritty terminal model                                       | Own application state, persist to disk        |
| Persistence    | File I/O, schema/version validation, atomic writes, safe fallback                      | Reach into state internals, spawn processes   |
| Theme          | Theme parsing, selection, fallback                                                     | Render UI, know about PTY internals           |

These boundaries are normative. "Must" and "must not" are requirements. Do not
bypass a boundary with a convenience call, and do not create parallel
architecture variants (`*_v2`, `new_*`) unless explicitly approved.

### Boundary details

- **UI layer** (`src/ui/`) renders state and captures intent. Components receive
  `Props` with a cloned `AppState` snapshot and `ThemeColors`, and return element
  trees. Components never mutate `AppState` or call `PtyManager` directly. PTY
  interaction flows through the root component's event handler.
- **App state/event layer** (`src/state/`) owns all state transitions. State
  mutation happens only through the reducer entry point (see below). The state
  layer does not own `PtyManager` or `ThemeManager`; it references PTY slots by
  index only.
- **Runtime layer** (`src/runtime/`, the PTY manager) owns tmux/PTY behavior.
  Runtime failures are captured as `Result` and never crash the app process.
- **Persistence layer** (`src/persistence/`) owns file I/O and schema handling.
  See [Persistence and Runtime](./persistence-and-runtime.md).
- **Theme layer** (`src/theme/`) owns theme loading, selection, and fallback.
  See [Display and UI](./display-and-ui.md).

---

## The Unidirectional Data Flow

Jefe follows an Elm-like unidirectional flow. Understanding this flow is
required before changing any state or rendering code.

```text
raw terminal input
        │
        ▼
   AppEvent                 (src/state/types.rs — exhaustive input enum)
        │  From<AppEvent> for AppMessage
        ▼
   AppMessage               (src/messages.rs — typed domain message bus)
        │  AppState::apply_message
        ▼
   AppState (next)          (src/state/mod.rs — deterministic reducer)
        │  render path clones a snapshot
        ▼
   iocraft component tree   (src/ui/ — pure render of the snapshot)
```

1. **Raw terminal input** arrives from iocraft's event loop (keyboard, mouse,
   resize).
2. **`AppEvent`** (`src/state/types.rs`) is the exhaustive low-level input enum.
   Each character input is `Char(char)`, not a generic blob.
3. **Global-shortcut seam.** A small number of inputs are handled at the
   app-shell/global-shortcut layer *before* the typed-message dispatch:
   - `F12`/`t` terminal-focus toggle (`handle_f12_toggle` in `src/app_input/`).
   - Option/Alt-digit agent jump shortcuts (`handle_global_shortcut_key` /
     `jump_to_shortcut_agent`).
   These apply directly to `AppState` and return early. They are kept narrow
   and intentionally few; **all other keyboard input** flows through the typed
   message pipeline below.
4. **`AppMessage`** (`src/messages.rs`) is the typed domain message bus, split
   into domain channels (`UiNavigationMessage`, `ModalMessage`,
   `RepositoryAgentMessage`, `RuntimeMessage`, `PersistenceMessage`,
   `ThemeMessage`, `IssuesMessage`, `PullRequestsMessage`, `SystemMessage`). The
   conversion seam lives in **`src/messages/event_conversion.rs`** — this is
   where low-level `AppEvent` values are routed into the smallest relevant
   domain message enum. New behavior must be added to the smallest domain
   message enum, not to app-shell-specific branching.
5. **`AppState::apply_message`** (`src/state/mod.rs`) is the deterministic
   reducer. It takes `self` by value, routes the message to the domain-specific
   `apply_*` handler, and returns the next state. Transitions are deterministic:
   given the same state and message, the next state is fixed.
6. **Render** clones the `AppState` snapshot, extracts any PTY data for the
   active agent, and passes both to the active screen component as props. The
   next render cycle picks up the new state.

There is no event queue, no async event bus, no pub/sub. Events are processed
inline in the terminal event callback, which is appropriate because all event
handling is fast (microseconds); the only potentially slow operations (tmux
commands) are called directly by the runtime, not deferred through the message
bus.

### Why the conversion seam exists

The UI keeps producing the historical `AppEvent` facade, while reducers and
dispatch code route through typed domain messages. `event_conversion.rs` is the
single place where the two worlds meet. Keeping it isolated means the domain
message enums can grow with the domain (issues, pull requests) without the
`AppEvent` enum becoming a god-object, and the reducer stays readable because it
dispatches on typed domains rather than a flat input enum.

---

## The Pure-Views Pattern

This is the most important architectural discipline in Jefe, and historically it
was tribal knowledge. It is now written down so that drift like PR #132 (which
grew `types.rs`/`tests.rs` past 1000 lines, baked scrolling into the iocraft
screen instead of a pure view, and added a 335-line reducer) does not recur.

### The problem

iocraft components are declarative and side-effect-free, but they are not
unit-testable: they return element trees that depend on the iocraft runtime, and
they carry `Color`/`Color` types that pull the whole iocraft crate into the test
binary. When display logic — viewport windowing, caret placement, line
splitting, truncation — lives inside an iocraft component, it can only be tested
by spinning up a real terminal. That makes the logic hard to test, hard to
reason about, and tempting to grow without bounds (per-keystroke
caret-following in the reducer, multi-hundred-line screen files).

### The pattern

Extract the display-deciding logic into an **iocraft-free, side-effect-free
projection function** that takes raw data plus viewport dimensions and returns a
plain data structure. The iocraft component then only renders that projection.

The canonical example is **`src/text_box_view.rs`**:

```rust
// src/text_box_view.rs — iocraft-free, no Color, no Props, no hooks.
#[must_use]
pub fn build_text_box_view(
    text: &str,
    byte_cursor: usize,
    viewport_rows: usize,
    content_width: usize,
) -> TextBoxView { ... }
```

`build_text_box_view` takes the raw `(text, byte_cursor)` plus a viewport size
and returns a `TextBoxView` — a fixed-size projection of display rows with an
optional caret cell per row. It carries no iocraft types, no `Color`, no
`Props`. Its module doc states the contract explicitly:

> This module is iocraft-free and side-effect-free: it turns the raw
> composer/editor `(text, byte_cursor)` plus a viewport size into a fixed
> window of display rows with an optional caret cell. The UI component
> (`ui::components::text_box`) consumes the projection and renders exactly
> `viewport_rows` rows — the reducer never needs to follow the caret per
> keystroke because the editable text owns its own local viewport invariant.

The matching iocraft component (`src/ui/components/text_box.rs`) is then thin:

```rust
// src/ui/components/text_box.rs — only renders the projection.
let view = build_text_box_view(&props.text, props.byte_cursor,
                               props.viewport_rows, props.content_width);
// ...iterate view.rows, render the caret cell as reverse-video.
```

### Why it works

- **The projection is pure.** No iocraft dependency, no `Color`, no runtime. It
  is a plain function from `(data, dimensions)` to a data structure.
- **It is trivially unit-testable.** `text_box_view.rs` has a `#[cfg(test)] mod
  tests` block that covers empty text, caret-following past the viewport,
  multibyte safety, trailing-newline semantics, and zero-width edge cases — all
  without a terminal.
- **It keeps the reducer lean.** Because the projection derives its own viewport
  from the caret (no stored scroll state), the reducer does not need to track
  per-keystroke caret-following. State stays focused on domain transitions.
- **It keeps files under control.** The pure module is small and cohesive; the
  component stays a thin renderer. This is how we keep files under the 1000-line
  hard limit and the 60-line function budget (see
  [Coding Standards](./coding-standards.md)).

### When to apply it

Apply the pure-views pattern whenever a component needs to compute what to
render — viewport windowing, caret placement, line wrapping, truncation,
filtering/sorting of a list for display, hint-string construction. The same
discipline already exists in:

- **`src/ui/components/keybind_bar.rs`** — `keybind_hints_for(screen_mode,
  terminal_focused)` is a pure `#[must_use]` function returning a `&'static str`
  hint for each screen mode. The iocraft `KeybindBar` component just renders it.
  See [Display and UI](./display-and-ui.md).
- **`src/presenter/`** — `format_elapsed`, `status_icon`, `status_label`,
  `todo_icon`, `truncate` are pure `#[must_use]` display-formatting functions
  with no UI-crate dependency.

### Discipline

- Keep projection modules **iocraft-free** (no `use iocraft::prelude::*`, no
  `Color`, no `Props`).
- Keep projection functions **`#[must_use]`** and side-effect-free.
- Keep files under **1000 lines** (`scripts/check-source-file-size.sh`
  `HARD_LIMIT=1000`; `WARN_LIMIT=750`) and functions under **60 lines**
  (`clippy.toml` `too-many-lines-threshold = 60`).
- Keep cognitive complexity under **15** (`clippy.toml`
  `cognitive-complexity-threshold = 15`).
- Do not bake scrolling/caret-following into the iocraft screen. Derive the
  viewport inside the pure projection.

---

## Dependency Direction DAG

Dependency direction is acyclic and enforced by convention and review. The
"depends on" arrow always points downward in the table below; no module may
import a module beneath it.

```
main.rs ──> state/ ──> domain/ (models only)
main.rs ──> runtime/ (PTY manager)
main.rs ──> theme/
main.rs ──> ui/ ──> presenter/ ──> domain/
ui/     ──> theme/ (for ResolvedColors)
state/  ──> messages/ ──> domain/
persistence/ ──> domain/
```

| Module            | May depend on (project-internal)                          |
|-------------------|-----------------------------------------------------------|
| `domain/`         | Nothing project-internal.                                 |
| `messages/`       | `domain/`, `state/` (for `AppEvent`/types only).          |
| `events/`         | Nothing project-internal.                                 |
| `presenter/`      | `domain/` only.                                           |
| `theme/`          | Nothing project-internal (uses iocraft types for `Color`).|
| `persistence/`    | `domain/` only.                                           |
| `runtime/`        | Nothing project-internal (uses iocraft types for `Color`).|
| `state/`          | `domain/`, `messages/`.                                   |
| `ui/`             | `domain/`, `presenter/`, `theme/`, pure-view modules.     |
| `main.rs`         | Wires everything together.                                |

Invariants:

- `domain/` depends on nothing project-internal.
- UI components must never call `PtyManager` methods. PTY interaction flows
  through the root component's event handler.
- `AppState` references PTY slots by index only; it never owns `PtyManager`.
- Do not break the DAG with a convenience import. If a module needs a type from
  a forbidden direction, move the type down (usually into `domain/`).

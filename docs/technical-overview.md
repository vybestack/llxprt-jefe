# Jefe — Technical Specification

## Architecture Overview

Jefe is a single-binary Rust TUI application that orchestrates AI coding agent processes via tmux and renders their terminal output through an embedded Alacritty terminal emulator model. The architecture separates concerns into distinct modules with clear boundaries: data models, application state management, event handling, PTY session management, theme resolution, presentation formatting, and UI rendering.

The application runs on a single async executor (`smol`), with blocking PTY I/O delegated to dedicated OS threads. There is no background task scheduler, no database, and no network server. All persistence uses flat JSON files. All process management uses tmux as the session backend.

### Published workbench and screen instances

Startup composes shipped, local, and package definitions into one immutable
`PublishedWorkbench`. Every route resolves an open `ScreenIdentity` through that
registry. `ScreenId` is not the routing vocabulary: it contains only the seven
residual adapters for Repositories, Issues, Pull Requests, Actions, Errors,
Terminals, and Settings. Dashboard is the shipped open identity
`core.dashboard` and uses the same descriptor, layout, control projection,
input, overlay, and navigation runtime as local and package screens.

`NavState` owns the current and suspended `ScreenInstance` values. Each instance
owns its focus, presentation, typed relationships, overlays, and provider-panel
state. Process-unique screen and panel instance IDs prevent stale completion or
provider delivery from mutating another visit to the same definition. Push
suspends the exact owner, Back restores it, and Replace or Back disposes the
removed owner only after typed deactivation effects are staged atomically.

Public panel behavior is selected solely by the closed nine-value `ControlKind`
and sealed host factories. The Dashboard's PTY panel is a private host-only
capability rendered through `TerminalView`; local and package definitions cannot
name it. Help, Search, generic confirmation, and provider confirmation are
instance-owned declared overlays, and their typed projections are shared by
rendering, input, scrolling, copying, and selection geometry.

### Agent definitions and launch plans

Jefe models every coding agent as an `AgentTypeId` plus typed values validated by
a shipped `AgentDefinition`. Definitions declare fields, supported operations and
targets, executable candidates, probes, required capabilities, and structural
argument emitters. There is no product enum in active state or runtime routing.

A launch resolves and probes a candidate, finalizes package wrappers and the
physical executable fingerprint, and produces one immutable `AgentLaunchPlan`.
Authorization and typed preflight validate that evidence before tmux, SSH, or a
local process receives the plan. Persistence stores generic values and a
canonical `LaunchSignatureV1`; startup reattaches when the persisted signature
and live session evidence match. For local sessions, it also tolerates
definition-hash-only drift when values, target, prior binding, and stable session
identity remain compatible. That exception uses a reattach-only runtime operation
which cannot spawn or probe the agent executable; the running binding retains its
prior launch signature as process provenance. Remote definition drift remains
inconsistent.

```
┌──────────────────────────────────────────────────────────────────┐
│                         main.rs (entry)                          │
│  Creates PtyManager, mounts App component, runs event loop       │
└──────────────────────┬───────────────────────────────────────────┘
                       │
        ┌──────────────┼──────────────────┐
        ▼              ▼                  ▼
   ┌─────────┐   ┌──────────┐     ┌────────────┐
   │ app.rs  │   │ pty/     │     │ theme/     │
   │ AppState│   │ PtyMgr   │     │ ThemeMgr   │
   │ events  │   │ tmux     │     │ loader     │
   └────┬────┘   │ alacritty│     │ definition │
        │        └──────────┘     └────────────┘
        ▼
   ┌──────────────────────────────────────┐
   │              ui/                      │
   │  screens/  components/  modals/       │
   │  (iocraft declarative components)     │
   └──────────────────────────────────────┘
        │
        ▼
   ┌──────────────┐    ┌──────────────┐
   │ presenter/   │    │ data/        │
   │ format.rs    │    │ models.rs    │
   │ (display fmt)│    │ (core types) │
   └──────────────┘    └──────────────┘
```

---

## Module Boundaries

### `data/` — Core Domain Types

**Responsibility**: Define the canonical data structures that represent the application domain. No behavior, no I/O, no UI coupling.

**Contents**:

- `models.rs` — `Repository`, `Agent`, `AgentStatus`, `TodoItem`, `TodoStatus`, `OutputLine`, `OutputKind`, `ToolStatus`. All types derive `Serialize`/`Deserialize` for JSON persistence and `Clone`/`Debug` for state management.
- `mock.rs` — Test fixture generator producing realistic sample data. Used exclusively in tests and prototype bootstrapping. Absent from production paths.

**Invariants**:

- No `use` of `iocraft`, `crossterm`, or any UI crate.
- No file I/O. No process spawning. No network.
- Every struct field is explicitly typed. No `HashMap<String, serde_json::Value>` or equivalent weak typing.

### `events/` — Event Definitions

**Responsibility**: Define the complete set of discrete application events. Events are value types that flow from input handling into `AppState::handle_event`.

**Contents**:

- `bus.rs` — The `AppEvent` enum. Every user action that mutates application state has a named variant. Character input is `Char(char)`, not a generic blob.

**Invariants**:

- Events carry no side effects. They are data.
- The enum is exhaustive: every keyboard shortcut documented in the functional spec maps to exactly one variant.
- No async, no channels, no callbacks.

### `state/` — Application State Machine

**Responsibility**: Reduce typed messages into deterministic application state
transitions. `AppState` is the cloneable aggregate, while screen-scoped state is
owned by the exact current or suspended `ScreenInstance` rather than by a flat
Dashboard/modal authority.

**Contents**:

- `AppState` — aggregate domain state, immutable published workbench reference,
  and staged effects.
- `NavState` / `ScreenInstance` — sole route stack and exact-instance focus,
  presentation, relationships, overlays, activation, generation, and panel state.
- `ScreenOverlayState` — declared Help, Search, generic confirmation, and provider
  confirmation presentation for one instance.
- Pure reducers for residual adapter state and typed provider request/context
  transitions.

**Invariants**:

- `AppState` is `Clone`; render reads a snapshot and does not mutate it.
- Screen changes go through the navigation reducer. Failed declaration,
  activation, lifecycle, or effect staging restores the prior complete state.
- Two instances of one definition never share selection, viewport, draft, focus,
  overlay, provider panel, or generation state.
- `AppState` does not own process, PTY, or theme-manager handles; side effects run
  only after a transition commits.

### `pty/` — PTY Session Management

**Responsibility**: Create, attach, detach, resize, snapshot, and destroy tmux-backed terminal sessions. Translate between the Alacritty terminal model and Jefe's renderable cell grid.

**Key Types**:

- `PtyManager` — the singleton manager. Holds a `Vec<AgentSession>` (one per agent), a single `AttachedViewer` (the currently-displayed tmux attachment), and shared resize/color state.
- `AgentSession` — metadata for one tmux session: work_dir, profile, mode, tmux session name.
- `AttachedViewer` — one live `tmux attach-session` child process: PTY master, writer, child killer, Alacritty `Term` model, alive flag, reader thread handle.
- `TerminalSnapshot` — the renderable output: a `Vec<Vec<TerminalCell>>` grid with per-cell character, foreground color, background color, bold, and underline.
- `TerminalColorDefaults` — the theme-derived default colors for ANSI palette remapping.

**Session Lifecycle**:

1. `add_session(work_dir, profile, mode)` → kills any stale tmux session with the same name, runs `tmux new-session -d -s jefe-{idx} -c {dir} llxprt [--profile-load profile] [mode flags]`, returns slot index.
2. `ensure_attached(idx)` → if not already attached to slot `idx`, tears down the existing viewer (kill child → join reader thread with 500ms timeout → clear index), verifies target tmux session exists (re-creates if needed), spawns new `tmux attach-session` child via `portable-pty`, starts reader thread feeding bytes into `alacritty_terminal::vte::ansi::Processor`.
3. `terminal_snapshot(idx)` → calls `ensure_attached`, locks the `Term`, extracts renderable content, maps cell colors through `resolve_color` with theme defaults, handles DIM/INVERSE/HIDDEN/selection/cursor flags, returns `TerminalSnapshot`.
4. `kill_session(idx)` → runs `tmux kill-session`, tears down attached viewer if current.
5. `relaunch_session(idx)` → kills and re-creates the tmux session from stored metadata, clears error state, re-attaches if current.

**Threading Model**:

- One reader thread per attached viewer, running a blocking `read()` loop on the PTY master's reader fd.
- The reader thread holds `Arc<Mutex<Term>>` and `Arc<Mutex<Processor>>`, advancing the terminal model on each read.
- The main thread (render path) locks `Term` briefly to extract snapshots. Lock contention is minimal because snapshot extraction is fast (cell iteration, no allocation-heavy transforms).
- `PtyManager` fields are wrapped in `Mutex` for interior mutability. There is no `RwLock`; contention is low enough that `Mutex` suffices.

**Resize Protocol**:

- Host terminal resize → `PtyManager::resize_all(rows, cols)` → resizes the PTY master (`PtySize`) and the Alacritty `Term` model.
- Layout dimensions account for UI chrome: outer bars (2 rows), terminal widget chrome (3 rows, 2 cols for borders/header).

**Input Forwarding**:

- `write_input(idx, &[u8])` → ensures attached, writes raw bytes to the PTY writer.
- `key_event_to_bytes(KeyEvent) -> Option<Vec<u8>>` — translates iocraft key events to PTY byte sequences (Ctrl+letter → ASCII control char, arrows → CSI sequences, Alt → ESC prefix).
- `mouse_event_to_bytes(FullscreenMouseEvent) -> Option<Vec<u8>>` — translates to xterm SGR mouse reporting format (`ESC [ < Cb ; Cx ; Cy M|m`). Only left-button events are forwarded to avoid noisy middle/right button artifacts.
- Mouse forwarding is conditional: only active when the child app has enabled terminal mouse reporting (`TermMode::MOUSE_MODE | SGR_MOUSE | UTF8_MOUSE`).

**Color Resolution**:

- `TerminalColorDefaults` is set from the active theme on every render cycle.
- ANSI indexed colors 0–15 are remapped through `themed_ansi_color()` to match the theme palette. Colors 16–231 use the standard xterm 6×6×6 cube. Colors 232–255 use the standard grayscale ramp.
- Named colors (`Foreground`, `Background`, `Cursor`, `Dim*`, `Bright*`) resolve through `resolve_named_color()` with theme defaults as fallback.
- Cells with the `DIM` flag override foreground to the theme's dim color. `INVERSE` swaps fg/bg. Selection overrides both to selection colors.

**Invariants**:

- `PtyManager` never panics. All tmux failures are captured as `Result<(), String>` or logged to stderr.
- A single attached viewer exists at any time. There is no multi-viewer mode.
- tmux fork failures trigger exactly one server reset retry before propagating the error.
- Drop impl kills the attached viewer and all tmux sessions.

### `theme/` — Theme System

**Responsibility**: Load, store, and resolve visual themes. Provide color values to all UI components and to the PTY color mapper.

**Components**:

- `definition.rs` — `ThemeDefinition` (name, slug, colors), `ThemeColors` (all hex color strings), `ResolvedColors` (pre-parsed iocraft `Color` values with green-screen fallbacks).
- `manager.rs` — `ThemeManager`: holds the list of available themes and the active slug. Provides `active()`, `colors()`, `set_active(slug)`, `load_external(dir)`.
- `loader.rs` — `load_embedded_themes()` (compile-time `include_str!` of JSON files), `load_themes_from_dir(path)` (runtime filesystem scan for `.json` files).

**Theme File Format**:

Themes are JSON files with this structure:

```json
{
  "name": "Green Screen",
  "slug": "green-screen",
  "kind": "dark",
  "colors": {
    "background": "#000000",
    "foreground": "#6a9955",
    "bright_foreground": "#00ff00",
    "dim_foreground": "#4a7035",
    "muted": "#3a5945",
    "border": "#6a9955",
    "border_focused": "#00ff00",
    "panel_bg": "#000000",
    "panel_header_fg": "#6a9955",
    "selection_fg": "#000000",
    "selection_bg": "#6a9955",
    "scrollbar_thumb": "#6a9955",
    "scrollbar_track": "#1a3318",
    "status_running": "#00ff00",
    "status_completed": "#6a9955",
    "status_error": "#6a9955",
    "status_waiting": "#6a9955",
    "status_paused": "#4a7035",
    "status_queued": "#3a5945",
    "accent_primary": "#6a9955",
    "accent_warning": "#6a9955",
    "accent_error": "#6a9955",
    "accent_success": "#00ff00",
    "diff_added_bg": "#...",
    "diff_added_fg": "#...",
    "diff_removed_bg": "#...",
    "diff_removed_fg": "#...",
    "input_bg": "#000000",
    "input_fg": "#6a9955",
    "input_placeholder": "#3b7a3b"
  }
}
```

**Color Resolution Flow**:

1. `ThemeManager::colors()` returns `&ThemeColors` (raw hex strings).
2. UI components call `ResolvedColors::from_theme(Some(&colors))` to get pre-parsed iocraft `Color` values with green-screen fallbacks for any missing/unparseable values.
3. PTY rendering calls `PtyManager::set_color_defaults()` with `TerminalColorDefaults` derived from the active theme's hex values via `to_rgb()`.

**Invariants**:

- Green Screen is always the first embedded theme and the startup default.
- All embedded themes are dark. `kind: "dark"` is the only supported value.
- Serde deserialization ignores unknown JSON keys, enabling forward-compatible theme files.
- External themes loaded from disk never replace an embedded theme with the same slug.
- `ResolvedColors::from_theme(None)` always returns green-screen values. There is no code path where a component renders without color information.

### `presenter/` — Display Formatting

**Responsibility**: Transform raw data model values into display-ready strings. Pure functions, no state, no I/O.

**Contents**:

- `format.rs` — `format_elapsed(secs) -> String` (HH:MM:SS), `status_icon(AgentStatus) -> char`, `status_label(AgentStatus) -> &str`, `todo_icon(TodoStatus) -> char`, `truncate(s, max_len) -> String`.

**Invariants**:

- All functions are `#[must_use]` and side-effect-free.
- No allocation beyond the returned String.
- No dependency on UI crates, PTY, or theme.

### `ui/` — Component Tree

**Responsibility**: Declare the visual structure using iocraft's component model. Components receive `Props` containing a cloned `AppState` snapshot and `ThemeColors`, and return element trees. Components do not mutate application state.

**Structure**:

```
ui/
├── mod.rs
├── orchestration.rs       — residual-seven adapters or shared ProviderScreen
├── screens/               — explicit residual adapters only
├── components/
│   ├── provider_screen.rs — shared published-definition renderer
│   ├── host_control_overlay.rs — shared declared overlay rendering
│   ├── terminal_view.rs   — private host PTY cell-grid renderer
│   ├── status_bar.rs      — shared top-bar statistics
│   └── keybind_bar.rs     — projected context shortcuts
```

Dashboard has no whole-screen renderer. Its five declared panels are projected
through `ProviderScreen`; unbound host models and exact provider-backed records
converge at the same closed host-control projection boundary.

**Rendering Model**:

- The root snapshots `AppState`; components never mutate it.
- `ui::orchestration` selects one of the seven explicit residual adapters or the
  shared `ProviderScreen` for every open definition, including Dashboard.
- Provider and host models become the same closed control projection before
  drawing. The private terminal model is the only host-only exception and still
  uses the descriptor's resolved panel rectangle.
- Declared overlays project above screen content through shared typed controls.

**Layout Authority**:

- `workbench::resolve_layout` computes one `ResolvedLayout` from the published
  descriptor and exact `ScreenInstanceId`.
- Rendering, mouse routing, keyboard page capacity, selection, wrapping, overlay
  geometry, and PTY resize consume that same snapshot.
- Missing or stale visible-panel geometry is a typed refusal; consumers do not
  reconstruct Dashboard arithmetic or use a fallback rectangle.
- Outer chrome: 2 rows (status bar + keybind bar).
- Terminal widget chrome: 3 rows (top border + header + bottom border), 2 cols (left/right border).
- Effective render size respects fullscreen mode. Non-fullscreen mode subtracts 2 rows and 2 cols to avoid host-terminal scroll/wrap artifacts.

---

## Data Model

### Entity Relationships

```
Repository 1──* Agent
    │                │
    ├── name         ├── id (UUID)
    ├── slug         ├── display_id ("#42")
    ├── base_dir     ├── name
    ├── default_profile ├── description
    └── agents[]     ├── work_dir
                     ├── profile
                     ├── mode
                     ├── pty_slot (Option<usize>)
                     ├── status (AgentStatus enum)
                     ├── started_at (DateTime<Utc>)
                     ├── elapsed_secs
                     ├── token_in / token_out
                     ├── cost_usd
                     ├── todos: Vec<TodoItem>
                     └── recent_output: Vec<OutputLine>
```

### Key Design Decisions

- **No Task entity.** The original three-level hierarchy (Repository → Task → Agent) was simplified. Each agent IS the unit of work. Agent name and description carry the semantic role of a task.
- **PTY slot is an index, not a handle.** Agents reference their tmux session by slot index (`Option<usize>`). `None` means no PTY is allocated. This decouples the data model from PTY lifecycle management.
- **Status is not persisted.** Agent lifecycle status is re-derived from tmux session liveness on startup. The state file stores agent definitions; runtime status is ephemeral.
- **Display IDs are sequential.** `#1`, `#2`, etc., generated from the global agent count at creation time. Not guaranteed unique across deletions; used for human readability only.
- **Working directory derivation.** Agent work_dir defaults to `{repo.base_dir}/{name-slug}` where slug is lowercased, space-to-dash, alphanumeric-only. User can override.

---

## Persistence Files

### Settings: `~/.config/jefe/settings.json`

Purpose: User preferences that are not repository/agent-specific.

```json
{
  "active_theme": "green-screen",
  "window_preferences": {},
  "default_profile_overrides": {}
}
```

Written on theme change and preference mutations. Read once on startup.

### State: `~/.local/share/jefe/state.json`

Purpose: The complete set of repository and agent definitions.

```json
{
  "repositories": [
    {
      "name": "llxprt-code",
      "slug": "llxprt-code",
      "base_dir": "/Users/alice/projects/llxprt-code",
      "default_profile": "default",
      "agents": [
        {
          "id": "550e8400-e29b-41d4-a716-446655440000",
          "display_id": "#1",
          "name": "Fix ACP socket timeout",
          "description": "...",
          "work_dir": "/Users/alice/projects/llxprt-code/fix-acp-socket-timeout",
          "profile": "default",
          "mode": "--yolo --continue",
          "pty_slot": null,
          "status": "Running",
          "started_at": "2026-02-12T16:00:00Z",
          "token_in": 0,
          "token_out": 0,
          "cost_usd": 0.0,
          "todos": [],
          "recent_output": [],
          "elapsed_secs": 0
        }
      ]
    }
  ]
}
```

Written on every mutation: agent created, edited, deleted; repository created, edited, deleted. Read once on startup with tmux reconciliation.

### Reconciliation on Startup

1. Load `state.json`.
2. For each agent with status `Running`, check `tmux has-session -t jefe-{slot}`.
3. If session is missing, set status to `Dead`.
4. If session exists, leave status as `Running` and allow the render loop to attach on selection.

No other state recovery is performed. Agents in `Completed`, `Errored`, `Paused`, `Waiting`, or `Queued` states retain those states as-is. `Dead` agents remain `Dead` until explicitly relaunched.

---

## Runtime Responsibilities

### Event Dispatch (main.rs)

The root component's `use_terminal_events` hook receives all terminal events and dispatches them:

1. **Resize** → `PtyManager::resize_all`.
2. **Mouse** (when terminal focused) → coordinate translation from screen-space to pane-local, clamped to terminal bounds, forwarded as SGR mouse bytes if child has mouse reporting enabled.
3. **Key F12** → toggle terminal focus (unconditional, works in all modes).
4. **Key in form screen** → map to form events (NextField, PrevField, SubmitForm, Back, Backspace, Char).
5. **Key when terminal focused** → forward to PTY as raw bytes.
6. **Key in normal mode** → map to `AppEvent`, dispatch to `AppState::handle_event`.

### PTY Lifecycle Coordination

When a new agent form submits:
1. `AppState::submit_form()` creates the `Agent` struct with `pty_slot: None`.
2. The event handler in `main.rs` detects the `NewAgent → Dashboard` screen transition.
3. It calls `PtyManager::add_session(work_dir, profile, mode)` which creates the tmux session.
4. On success, it writes the returned slot index back into `agent.pty_slot`.

When kill is requested:
1. The event handler calls `PtyManager::kill_session(slot)` to destroy the tmux session.
2. `AppState::handle_event(KillAgent)` sets status to `Dead`.

When relaunch is requested:
1. If the agent has a `pty_slot`, `PtyManager::relaunch_session(slot)` kills and re-creates the tmux session.
2. If no slot exists, `PtyManager::add_session` creates a new one and the slot is assigned.
3. `AppState::handle_event(RelaunchAgent)` sets status to `Running` and resets timestamps.

### Liveness Polling

On every render cycle, the root component checks all agents with `status == Running`:
- For each, if `pty_slot` is `Some(slot)` and `PtyManager::is_alive(slot)` returns false, status is set to `Dead`.
- This check only writes to `AppState` when changes are detected, avoiding infinite render loops.
- `is_alive` checks the attached viewer's alive flag for the current slot, or calls `tmux has-session` for non-attached slots.

---

## Event Model

Events in Jefe are synchronous and single-threaded from the application logic perspective:

1. `TerminalEvent` arrives from iocraft's event loop (keyboard, mouse, resize).
2. The event is translated into a canonical `Chord`, the current state is projected into an ordered context stack, and the immutable action/binding snapshot resolves exactly one result.
3. A `Dispatch` result runs its closed `HandlerKey`, which produces the smallest typed message (or, at an explicit boundary, a runtime operation). `Unavailable` runs nothing, `ForwardToPty` writes raw bytes to the child, and `Unbound` does nothing.
4. `AppState` applies the typed message.
5. If PTY side effects are needed (kill, relaunch, add session), they happen at the boundary after the state transition.
6. The next render cycle picks up the new state.

### Action composition, availability, dispatch, and explain

**Composition.** Compiled default actions and bindings are merged with
`keymap.<context>.<action>` overrides from schema-2 settings, where an empty
list is an explicit unbind and an absent entry inherits. The merged candidate is
validated as a whole: chord grammar, duplicate chords in one context, implicit
shadows, protected-binding violations, and resource bounds. Any failure rejects
the entire candidate with a `KEY-E401` diagnostic and keeps both the previous
snapshot and the previous settings bytes, so a bad edit cannot half-apply.

**Availability.** Availability is computed once per composed snapshot and
carries a correlation so a late completion that no longer matches is ignored. An
unavailable action still resolves, still appears in Help, footer, menus, and the
Keys editor, and exposes one byte-identical reason string everywhere — but runs
no handler and produces no effect.

**Dispatch.** Keyboard and mouse share the path. A clickable surface carries the
action ID assigned to its hit target and goes through the same availability check
and the same handler as the keyboard chord, so the two cannot diverge. Raw editor
text insertion and terminal passthrough are deliberately not actions.

**Explain.** `jefe explain binding CHORD [--context ID]` composes the same
snapshot offline. It prints the normalized chord, the contexts searched in
order, the winning resolution, availability and reason, shadowed bindings, and
provenance. It starts no TUI, contacts no provider, probes nothing, and writes
nothing; exit is `0` when resolved, `2` when invalid or unresolved, and `64` for
a usage error, so a broken keymap remains diagnosable when the app is unusable.

There is no event queue, no async event bus, no pub/sub. Events are processed inline in the terminal event callback. This is appropriate because all event handling is fast (microseconds) — the only potentially slow operations (tmux commands) are called directly, not deferred.

---

## Plugin Packages

A package is a directory `<root>/<plugin-id>/<canonical-semver>/` containing a
closed `plugin.json`. Packages are found in ordered roots — the executable's own
prefix, the platform package-manager prefixes, then `<config>/plugins/installed`
— and deduplicated by physical identity so one package reached through several
paths is one row.

Discovery, validation and composition are **provider-free**: a manifest is data,
every rule in it is checked by pure code, and nothing starts a provider. Whether
a package could run here is reported as a reason, never discovered by trying.

Installation takes a `tar.gz` or, with `--developer`, an unpacked directory.
Everything is validated in memory, staged privately with normalized modes, then
committed by one atomic rename into the user root. Versions live side by side;
nothing is ever overwritten, and there is no update or network command.

A package is **disabled until explicitly trusted**. Trust records the exact
version and configuration through the lossless settings writer and states what
it means: the provider will run unsandboxed as the operator's OS user. Because
packages are composed while the session builds its workbench, trust takes
effect at the next start.

### Atomic startup authority

Normal startup first composes a private `PublishedWorkbench` candidate from the
effective Settings, retained package inventory and exact selection, validated
agents, actions, keys and contexts, screens, panels, controls, schemas, routes,
relationships, static availability, provider/tool descriptors, and provider
Ready metadata. The aggregate is data-only and immutable after commit.

Selected persistent providers are required only when their validated package
contributes nonempty provider configuration or declarations. They start in a
deterministic private transaction and must all complete Configure and Ready.
Any static or provider failure reaps all recorded candidate descendants,
preserves durable bytes, publishes no aggregate, and reports provider-free
recovery. Selected one-shot and declaration-empty providers start no startup
process.

Success returns exactly one `StartupCommit`: an `Arc<PublishedWorkbench>` plus a
`ProviderCoordinator`. The coordinator owns only live process/session,
supervisor, request-counter, and health handles; it does not duplicate static
catalogs or Ready publication metadata. The composition root consumes the
commit before constructing `AppState`, renderer/input services, runtime PTYs,
or the TUI. Every declaration consumer receives the same aggregate identity.
Structural Settings or package edits therefore take effect only after restart.

Provider health and action availability are generation-bound runtime overlays.
They may mark committed declarations unavailable, but cannot mutate, replace,
or promote the immutable declaration graph. A post-Ready provider crash keeps
the same aggregate and renders the declared unavailable/error state without a
fallback selection.

Initial PTY geometry is also post-commit. The runtime remains pending until the
first observed terminal size has produced a committed resolved frame; that
frame's nonzero dimensions are then committed once before durable session
restoration or any spawn/attach/resize operation.

## Non-Functional Requirements

### Performance

- PTY polling at ~30fps (33ms interval). Terminal snapshot extraction completes in under 1ms for typical terminal sizes.
- Layout computation is O(1) — fixed arithmetic on terminal dimensions.
- Agent list rendering is O(n) in agent count. Split view is O(n) in running agent count.
- Theme color resolution is O(1) per cell via `ResolvedColors` pre-computation.
- Reader thread processes PTY bytes in 4KB chunks. Alacritty parser advances synchronously under lock.

### Memory

- One `alacritty_terminal::Term` model per attached viewer (not per agent). Typical memory: ~200KB for an 80×24 terminal with scrollback.
- `AppState` clone per render cycle. Shallow clone — `Vec<Repository>` with `Vec<Agent>`. Typical memory: negligible.
- tmux sessions consume OS resources independently. Jefe's memory footprint does not scale with agent count beyond the session metadata vectors.

### Reliability

- `unsafe_code = "forbid"` in Cargo.toml. No unsafe Rust anywhere in the codebase.
- All PTY operations return `Result`. Failures are logged to stderr and do not crash the application.
- tmux fork failures trigger one automatic server reset retry.
- Lock poisoning is handled gracefully (all `Mutex::lock()` calls use `.ok()` or `.map_err()`, never `.unwrap()`).
- Reader thread join uses a 500ms bounded timeout to prevent indefinite hangs on viewer teardown.

### Compatibility

- Requires a tmux-compatible multiplexer installed and resolvable on `PATH`:
  upstream `tmux` on macOS/Linux, or native `psmux` (3.3.7+) on Windows.
- Native Windows uses the ConPTY pseudo-console and psmux on a private
  `-L` namespace. WSL, Cygwin, MSYS2, Git Bash, and Docker tmux are rejected.
- Requires `llxprt` (or configured agent CLI) installed and available on `PATH`.
- Terminal must support alternate screen mode and 256-color or RGB color.
- Supported on macOS, Linux, and native Windows (x86_64). See
  [Windows support](windows-support.md) for the native Windows architecture.
- Minimum Rust version: edition 2024 (Rust 1.85+).

### Extensibility Points

- **External themes**: Drop JSON files in `$JEFE_THEME_DIR`.
- **ACP sideband**: When implemented, connects via Unix domain sockets. Orthogonal to PTY rendering. Enriches the preview pane and enables structured todo/tool-call display.
- **Agent CLI**: The tmux session command is `llxprt [--profile-load {profile}] {mode_flags}`. Substituting a different CLI requires changing the session spawn command.
- **Persistence backend**: Settings and state are flat JSON files. The persistence layer is isolated from the state machine, enabling future migration to other formats without touching `AppState` or UI code.

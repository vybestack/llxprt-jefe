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

### Definition-driven agent authority

Agent identity and configuration are generic domain data: an `AgentTypeId` plus
a schema-validated `TypedMap`. Shipped `AgentDefinition` values own fields,
defaults, operation/target support, candidate declarations, the identity probe,
a declared minimum version, and argv emitters. A flag emitter carries its own
argv token; nothing outside the emitter decides what a flag emits. Application, state, persistence, and runtime
code must not branch on product identity or reconstruct product-specific
configuration.

The launch pipeline is one-way:

`definition -> candidate -> probe evidence -> immutable AgentLaunchPlan -> authorization -> preflight -> runtime`

Runtime managers execute the immutable plan; they never rediscover executables
or rebuild argv from an agent name. Schema-1 aliases are interpreted only by
the migration boundary. Unknown active schema-2 type IDs fail closed.

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
- **JSP wire-boundary layer** (`src/jsp/`) owns the external JSP/1 snapshot
  parser. It depends only on `domain::observation` (transport-neutral semantic
  values), the standard library, and existing `serde`/`serde_json`. Private
  closed wire DTOs convert to domain types only after complete validation.
  The parser performs no I/O or logging and returns typed errors without
  echoing payload values. See `dev-docs/jsp/v1/specification.md`.

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
3. **Action-registry resolution seam.** Raw platform events are translated to a
   canonical `Chord`, the current state is projected to an ordered context
   stack, and one immutable snapshot answers what the input means. There is no
   parallel shortcut table: dispatch, Help, the keybind footer, menus, the Keys
   editor, mouse activation, and `jefe explain binding` all read the same
   snapshot. Resolved handler keys then produce the smallest typed message, so
   input still enters the pipeline below rather than bypassing it.
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

## The Action Registry

Every key and every clickable action resolves through **one immutable
action/binding snapshot**. This is a boundary rule, not a convenience.

### One-resolution invariant

For a given input and application state there is exactly one resolution:
`Dispatch`, `Unavailable`, `ForwardToPty`, or `Unbound`. Consequences:

- No second dispatch table, shortcut map, help string table, or footer string
  table may exist. If a surface needs to know what a key does, it asks the
  snapshot.
- An `Unavailable` resolution runs no handler and produces no effect, and the
  reason string it exposes is byte-identical in dispatch notices, Help, the
  footer, menus, and the Keys editor.
- Availability is computed once per composed snapshot. A completion whose
  correlation no longer matches the current snapshot is ignored rather than
  applied late.
- A candidate registry is published atomically. A grammar error, a duplicate
  chord in one context, an implicit shadow, a protected-binding violation, or a
  resource-bound breach (`KEY-E401`) rejects the **whole** candidate and retains
  the previous snapshot and the previous settings bytes.
- Protected actions (emergency exit, leave terminal, Back) cannot be unbound or
  shadowed, and their reachability is validated for macOS and Linux.

### Dependency direction

```text
platform event ──> input translation ──> domain::Chord
AppState ─────────> action_context ────> ordered ContextStack
(Chord, ContextStack) ──> domain::ActionRegistrySnapshot::resolve  (pure)
        └──> HandlerKey ──> app_input handlers ──> smallest typed message
settings (schema 2) ──> candidate composition/validation ──> snapshot
snapshot + context ──> action_projection ──> Help / footer / menu / Keys
layout hit target ──> ActionId ──> the same availability + handler path
```

The registry values and the resolver live in `domain/` and depend on nothing
project-internal: no state, persistence, UI, runtime, or harness imports. State
depends on the registry, never the reverse. `HandlerKey` is a closed enum, never
a closure, service handle, or generic payload, which is what keeps the snapshot
a pure value that persistence and the offline `jefe explain binding` CLI can
compose without a running TUI.

Raw text insertion in an editor and raw PTY forwarding in terminal capture are
deliberately *not* actions. They stay raw so remapping can never change what a
child process receives.

## One geometry authority

`layout` is the sole geometry authority. The implementation lives in the
I/O-free `workbench/` module and is re-exported by `src/layout.rs`, so there is
exactly one `resolve_layout` and exactly one place a consumer looks for
rectangles.

```text
ScreenId ──> workbench::ScreenRegistry ──> ScreenDescriptor  (compiled, validated)
(descriptor, outer Rect, PanelState) ──> resolve_layout  (pure, checked u32)
        └──> ResolvedLayout { screen_instance, panels, too_small }
                 ├──> renderers        (chrome / content rectangles)
                 ├──> mouse routing    (hit regions)
                 ├──> selection + wrap (content rectangles)
                 └──> PTY resize       (pty_content_rect, never zero)
```

The snapshot is computed once per size or state change and carries a
`ScreenInstanceId`, so a consumer can prove it read the geometry the renderer
used rather than deriving its own. Screen-specific geometry arithmetic and
independent terminal-size reads are not permitted in consumers: a panel's
position is whatever the snapshot says it is.

`workbench/` depends on nothing project-internal except the shared typed-value
and diagnostic contracts in `domain/` and `persistence/diagnostic`. It performs
no I/O, holds no state, and imports no terminal, rendering, runtime, or harness
types, which is what lets the allocation algorithm be swept exhaustively as pure
arithmetic.

## Screen discovery, lowering, and publication

A screen may be compiled into the executable or authored by a user, and both end
as the same internal `ScreenDescriptor`. Ownership of each step is fixed:

| Step | Owner | Rule |
|---|---|---|
| Where definitions live | `persistence::paths::ResolvedPaths::definitions` | The sole discovery root. Nothing else names a definitions directory. |
| Which files are candidates, and their bytes | `persistence::screen_files` | The only enumeration of that directory and the only read of a definition. No recursion, no symlink traversal, no hidden file, no extension alias, no non-UTF-8 name; canonical path order; bounded before parse. |
| What the external syntax is | `workbench::screen_file` | The closed grammar, with spans. Objects deny unknown fields and every enumerated value is a Rust enum. |
| What the declared bounds are | `workbench::screen_file_bounds`, `workbench::screen_file_shape` | Checked, never clamped; the measured value is reported. |
| External to internal | `workbench::screen_lowering::lower_screen` | The single crossing. It copies and resolves; it supplies no semantic default. Nothing external survives it. |
| Which panel types and actions exist | `workbench::panel_types`, `domain::default_action_inventory` | Immutable registries. A definition resolves against them and can never extend them. |
| Which owners are active | `persistence::settings_publish::PublishedWorkbenchSettings::enabled_screens` | Read before lowering; a dormant definition is never lowered. |
| Composing and refusing | `workbench::compose` | All-or-nothing. One unusable enabled definition refuses the whole candidate registry. |
| Publishing | `workbench::publish_screen_registry` | Exactly once, at startup, before anything renders. |
| Requesting all of the above | `startup_screens::compose_and_publish` | The only caller that turns paths plus settings into a published registry. |

Three properties are normative:

- **The workbench never performs I/O.** Discovery and reading live in
  `persistence/`; parsing, validating, lowering, and composing are pure
  functions over text and values.
- **No external screen syntax survives publication.** The registry holds
  internal descriptors only, so the external grammar can change without
  reaching a renderer or a resolver.
- **A definition cannot request an effect.** It may name a panel type from the
  compiled registry and an action from the compiled inventory, and nothing else.
  `pty-terminal` is deliberately absent from what a definition may name.

Screen *description* is open — `ScreenIdentity` is either a compiled `ScreenId`
or a validated `local.*` identity — while screen *routing* stays the closed
`ScreenId` enum, so every routable screen still has a compiled renderer and an
exhaustive match.

## Route and navigation ownership

`state::navigation::NavState` is the sole runtime authority for which screen the
session is on. There is no screen field anywhere else, and nothing assigns a
screen: every change goes through `state::navigation::reduce_navigation`, which
is pure — it takes the navigation state, the registry, and one message, and
returns the next state plus what the caller must do about the instances that
entered or left. It performs no I/O and stages no effect of its own.

Three verbs on `AppState` are the only way in, and each is one call into the
reducer:

| Verb | Meaning |
|---|---|
| `enter_screen` | Open a screen, suspending the current one so Back returns to it. |
| `switch_screen` | Take the place of the current screen without stacking on it. |
| `leave_screen` | Go back if there is somewhere to go back to, otherwise go home. |

The following are normative:

- **A target is constructed before anything is suspended or disposed.** A
  refused navigation returns the state it was given; there is no partially
  applied navigation and no half-mutated stack.
- **Routes are declared, not asserted.** A `RouteDeclaration` is derived from a
  screen descriptor's `route` and `activation` (`workbench::route`), so a route
  and the screen it reaches cannot drift apart, and a route no descriptor
  declares cannot be navigated to.
- **Activations are closed and non-secret.** `ActivationValue` mirrors
  `ActivationKind` exactly; there is no secret variant, no nested map, and no
  generic payload. Field count, serialized size, and identifier length are
  enforced at construction, so an over-large activation cannot be held at all.
  Every refusal is `NAV-E001` and names only identifiers the program declared —
  never a value the caller supplied.
- **Instance identity is never reused.** Two visits to one screen are two
  instances, so the second never inherits the first's pending answers. A request
  computed against an instance that is no longer current is refused as stale
  rather than acted on.
- **Generations decide what is still wanted.** Work is answered only when its
  correlation names the live instance's screen and activation generations
  (`NavState::answers_live_work`). A suspended instance's generations are not
  live, restoring it makes them live again, and a disposed instance's
  generations never return because generations only move forward.
- **The stack is bounded at 32 and is never persisted.**
- **Rooting a session is total.** Both the route and the initial focus come from
  compiled tables (`workbench::screens::route_of`, `::initial_focus`), so
  starting a session has no failure mode at the moment it is needed. Those
  tables duplicate what the descriptors declare, and
  `route_agrees_with_every_descriptor` and `initial_focus_agrees_with_every_descriptor`
  are what keep the two from drifting; a screen that drifts fails those tests.

`ScreenInstance` currently carries the descriptor-declared panel focus as
navigation's own record. The per-mode focus fields (`pane_focus`,
`issues_state.issue_focus`, and their siblings) remain the authority for what is
actually focused; relocating them into the instance is a separate cutover.

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

- **`src/action_projection.rs`** — the Help, keybind-footer, menu, and Keys-editor
  text are pure `#[must_use]` projections of the immutable action/binding
  snapshot for the current context. `keybind_bar.rs` and `modals/help.rs` are
  thin renderers of those projections and hold no shortcut text of their own.
  See [Display and UI](./display-and-ui.md).

### Discipline

- Keep projection modules **iocraft-free** (no `use iocraft::prelude::*`, no
  `Color`, no `Props`).
- Keep projection functions **`#[must_use]`** and side-effect-free.
- Keep files under **1000 lines** (`cargo xtask check source-size`
  `HARD_LIMIT=1000`; `WARN_LIMIT=750`) and functions under **60 lines**
  (`clippy.toml` `too-many-lines-threshold = 60`).
- Keep cognitive complexity under **15** (`clippy.toml`
  `cognitive-complexity-threshold = 15`).
- Do not bake scrolling/caret-following into the iocraft screen. Derive the
  viewport inside the pure projection.

---

## Plugin package ownership

Plugin packages are discovered, validated, and composed in a **provider-free
static phase**. Nothing in that phase starts a provider process; a manifest is
data, and every rule in it is checked by pure domain code.

| Layer | Owns | Must not do |
|---|---|---|
| `domain/plugin/` | closed manifest schema, `PluginId`, `PackageCoordinate`, declaration validation, `PLG-Ennn` codes | touch the filesystem, resolve a root |
| `persistence/plugin_roots` | the ordered low-to-high root list and each root's writability | read a manifest |
| `persistence/plugin_inventory` | the physical scan, alias collapse, ambiguity, and the snapshot the UI projects | interpret declarations itself |
| `persistence/plugin_archive` | archive and developer-directory validation | write to the installed tree |
| `persistence/plugin_install` | staging, mode normalization, atomic commit | validate schema rules |
| `plugin_command` | the provider-free `jefe plugin` verbs | start a process |
| `state/plugins_editor` | the pure Plugins projection | scan, install, or write |

The inventory is scanned **once** per session, at the boundary that already owns
path resolution, and travels as plain snapshot rows. Neither the state layer nor
the UI rescans, so what the Settings section shows and what the session composed
are one moment rather than two that can disagree.

A package's identity is its directory names. A manifest that declares a
different identity is rejected rather than believed, because the directory is
what the roots enumerate and what settings key off.

## Action-provider ownership

A package's actions are executed by a **provider process**. The static phase
above still starts nothing; every process belongs to exactly one owner, and that
owner is never the state layer.

| Layer | Owns | Must not do |
|---|---|---|
| `runtime/provider/{framing,protocol,dto,...}` | the closed JSONL wire: framing bounds, envelope/payload DTOs, lifecycle order, progress monotonicity | perform I/O, spawn, or hold state |
| `runtime/provider/composition` | the pure decision of which trusted package contributes which action, its runtime descriptor, and its startup candidate | start a process or read settings |
| `runtime/provider/supervisor` | one one-shot lifecycle: spawn, pipes, drains, timeouts, staged shutdown, reap | outlive its invocation or expose a handle |
| `runtime/provider/persistent` | resident candidates and their atomic all-or-nothing publication | auto-restart a candidate |
| `runtime/provider/coordinator` | the published catalog, the request-id counter, and the persistent supervisor for the session | live in `AppState` |
| `startup_providers` | the single place a provider process may start | run from `build_persistence` |
| `state/provider_requests` | handle-free request, progress, terminal and confirmation state | own a process, pipe, timer, or thread |
| `services/provider_effect_worker` | translating one supervisor result into typed reducer messages | execute on the input or render thread |

Two rules carry most of the weight:

**Provider processes start in exactly one place.** `startup_providers` runs from
the TUI startup path only. `build_persistence` scans packages but never starts
one, which is what keeps `jefe config` and recovery provider-free even when a
selected package declares a provider that would hang.

**A provider effect runs only after the transition commits and the state guard
is released.** Dispatch stages a closed `ProviderEffect`; the background worker
executes it off the UI thread and routes typed messages back through the
reducer. A result for a superseded generation is ignored by the reducer, not
filtered by the worker.

Provider actions join the **same** `ActionRegistrySnapshot` as compiled actions,
under the closed `HandlerKey::ProviderAction`. There is no second registry, so
the reason an action cannot run is one string with one owner: a refused keybind,
the Help package section, and any provider surface quote the same bytes. A
package that fails to publish leaves its actions visible and unavailable rather
than partially runnable.

## Dependency Direction DAG

Dependency direction should be acyclic and is enforced by convention and review.
The "depends on" arrow points downward; modules should only import from layers
below their own. When a module needs a type from a forbidden direction, move the
type down (usually into `domain/`).

```text
main.rs ──> state/ ──> domain/ (models only)
main.rs ──> runtime/ (PTY manager)
main.rs ──> theme/
main.rs ──> ui/ ──> theme/ (for ResolvedColors)
ui/     ──> text_box_view/ (pure projection)
state/  ──> messages/ ──> domain/
action_context/ ──> domain/ (state snapshot to ordered context stack)
action_projection/ ──> domain/ (snapshot to Help/footer/menu/Keys text)
persistence/ ──> domain/ (keymap override composition)
persistence/screen_files ──> workbench/ids (the member grammar it enumerates by)
workbench/ ──> domain/ (typed values), persistence/diagnostic (codes and bounds)
startup_screens/ ──> persistence/, workbench/ (discovery, composition, publication)
state/screen_relationships ──> workbench/ (the declared coupling a screen carries)
jsp/    ──> domain/ (transport-neutral observation values)
```

| Module              | May depend on (project-internal)                          |
|---------------------|-----------------------------------------------------------|
| `domain/`           | Nothing project-internal.                                 |
| `messages/`         | `domain/`, `state/` (types only — see known coupling).    |
| `theme/`            | Nothing project-internal (uses iocraft types for `Color`).|
| `persistence/`      | `domain/`; `workbench/ids` for the definition-file grammar.|
| `workbench/`        | `domain/`, `persistence/diagnostic`. No I/O, no state.     |
| `startup_screens/`  | `persistence/`, `workbench/` (the composition boundary).   |
| `runtime/`          | Nothing project-internal (uses iocraft types for `Color`).|
| `state/`            | `domain/`, `messages/`.                                   |
| `text_box_view/`    | Nothing project-internal (pure projection).               |
| `action_projection/`| `domain/` only (pure projection of the snapshot).         |
| `action_context/`   | `domain/`, `state/` types (pure context derivation).      |
| `jsp/`              | `domain/observation` only (external wire boundary).       |
| `ui/`               | `domain/`, `theme/`, `text_box_view/`, other pure views.  |
| `main.rs`           | Wires everything together.                                |

Invariants:

- `domain/` depends on nothing project-internal.
- The action registry resolves exactly one result per input, and no surface may
  keep a private copy of a binding, hint, or help string.
- UI components must never call `PtyManager` methods. PTY interaction flows
  through the root component's event handler.
- `AppState` references PTY slots by index only; it never owns `PtyManager`.
- Do not break the DAG with a convenience import. If a module needs a type from
  a forbidden direction, move the type down (usually into `domain/`).

### Known coupling: `state/` ↔ `messages/`

The dependency between `state/` and `messages/` is bidirectional today:
`state/` imports domain message types from `messages/`, and `messages/`
(in particular `event_conversion.rs`) imports the `AppEvent` input enum and a
few display-state types (`EditorTarget`, `InlineState`, `ReadOnlyHintKind`)
from `state/`. This is a known coupling, not a desired pattern — the ideal
resolution is to move the shared input types (`AppEvent` and the display-state
enums) into a lower layer (e.g. `events/` or `domain/`) so that `messages/`
no longer depends on `state/`. New code should avoid deepening this coupling;
prefer adding new domain message variants in `messages/` and consuming them in
`state/`, not the reverse.

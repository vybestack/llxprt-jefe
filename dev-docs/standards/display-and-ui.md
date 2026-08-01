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

- **`src/action_projection.rs`** — the footer, Help, menu, and Keys-editor text
  are pure `#[must_use]` projections of the immutable action/binding snapshot
  for the current context. `ui::components::keybind_bar` and `ui::modals::help`
  render those projections and own no shortcut text.
- **`src/text_box_view.rs`** — `build_text_box_view(text, byte_cursor,
  viewport_rows, content_width) -> TextBoxView` is a pure, iocraft-free
  projection. The `ui::components::text_box` component consumes it.

When you add display-deciding logic (viewport windowing, caret placement,
filtering/sorting for display, hint construction), extract it into a pure
function. Do not bake it into an iocraft component.

---

### Generated agent forms

Agent create/edit forms are projected from the selected `AgentDefinition`.
Field order, labels, defaults, scope, signature participation, and operation/
target availability come from the definition rather than product-specific UI
branches. Form results are consumed exactly once into a typed map. Reducers do
not create directories, probe processes, install packages, or mark an agent
running; those effects remain behind app-input/runtime boundaries and occur
only after validation. Unsupported operation/target cells keep Create disabled
and must produce zero state, persistence, filesystem, SSH, tmux, or spawn effects.

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

### Screens

`ScreenId` (`src/workbench/ids.rs`, re-exported from `crate::state`) names the
active screen. Its identity is the stable namespaced string returned by
`as_str`, not the variant's position, so persistence and descriptors agree on
one vocabulary and reordering the enum cannot change which screen a restored
session opens on. `ScreenId::from_stable` is the only way an external value
becomes a screen identity.

| `ScreenId` | Stable identity | Screen |
|---|---|---|
| `Dashboard` (default) | `core.dashboard` | Repositories, agents over the embedded terminal, preview |
| `Repositories` | `core.repositories` | Compact cross-agent repository list under its filter band |
| `Issues` | `github.issues` | Issue list/detail with filter and search |
| `PullRequests` | `github.pull-requests` | PR list/detail with filter, search, merge |
| `Actions` | `github.actions` | Workflow-run list/detail |
| `Errors` | `core.errors` | Error ring buffer list/detail |
| `Terminals` | `core.terminals` | Terminal Manager: shell list with a read-only preview |

`ScreenId` answers *which* screen is active and nothing else. What a screen
*contains* is its descriptor's business — see the next section.

### Screen descriptors and the layout resolver

`src/workbench/` is the sole definition of every screen's structure and the sole
implementation of geometry. It is I/O-free: it owns no state, touches no
terminal, and knows nothing about rendering, so it is exercised exhaustively as
pure data.

- `screens.rs` compiles one `ScreenDescriptor` per screen: its panels, which are
  focusable, which are required, the focus order, the layout tree, and any
  declared relationships. `shipped-screen-definition-parity.json` is the golden
  that must move with it. A screen may also be lowered from a user definition
  file (see "User screen definitions" below); either way the descriptor is the
  only description of a screen's structure.
- `validate.rs` enforces the structural invariants: each panel appears exactly
  once in `panels` and exactly once in the layout; each focusable panel appears
  exactly once in the focus order; initial focus is focusable; a required panel
  never sits under a collapsible child; split children number 2–8; nesting stays
  within 8. A malformed compiled descriptor fails at startup and in tests, never
  at render time.
- `allocate.rs` is the whole sizing algorithm on one axis, isolated so it can be
  swept exhaustively. In order: charge the split's declared gap per adjacent
  visible pair; while the visible minima do not fit, hide one collapsible child
  chosen by `(collapse_priority ascending, depth_first_index descending)`; clamp
  fixed sizes into `[min, max]`; give weighted children their minima, then share
  the rest by `floor(remaining * weight / sum_weight)`; pin any child that
  reaches its maximum and redistribute what it gave back; hand out the remainder
  one cell at a time in declaration order. All interior arithmetic is checked
  `u32` and overflow is a typed error, never a panic.
- `resolve.rs` walks the tree into one immutable `ResolvedLayout`. Every
  geometry consumer — renderer, mouse routing, selection, wrapping, scrolling,
  PTY resize — reads that snapshot, so no two can disagree about where a panel
  is. `src/layout.rs` re-exports it, so `layout` stays the one place a consumer
  looks for geometry.

Guarantees the snapshot carries:

- rectangles along an axis are contiguous and non-overlapping;
- a hidden panel has no hit region, no content rectangle, and no PTY region;
- `pty_content_rect` never yields a zero-area rectangle, which is why no caller
  needs its own `.max(1)` guard — a required PTY panel that cannot keep a
  nonzero rectangle makes the screen too small instead;
- when the required panels do not fit, exactly the first required focusable
  panel in descriptor focus order is visible, with a `TooSmall { needed,
  available }` notice, so Back and exit stay reachable;
- `repair_focus` advances cyclically from the prior focus to the first visible
  focusable panel, starting from the descriptor's initial focus when there is no
  prior focus. The workspace screens cycle through the repository sidebar but
  open on their list, so those are genuinely different panels.

Legacy persisted screen values are translated once, by name, in
`src/workbench/migration.rs`. That module is the only place the legacy
vocabulary is named; an unrecognised value warns and selects the compiled
initial screen rather than costing the user the rest of their restored state.

### User screen definitions

A user may add a screen by placing a definition in the definitions directory
that `jefe config path` reports. Discovery is exact: one direct regular file per
screen, named `<member>.screen.toml`, where `member` matches
`[a-z][a-z0-9-]{0,62}` and is also the screen's identity — `review.screen.toml`
declares `local.review` and nothing else. Subdirectories, symbolic links, hidden
files, extension aliases, and non-UTF-8 names are not candidates. Files are read
in canonical path order and bounded before parsing, the directory holds at most
64 candidates, and a file whose identity changes between being listed and being
opened is refused rather than read.

A definition takes effect only when settings enable it:

```toml
[workbench]
enabled_screens = ["local.review"]
```

#### Grammar

```text
ScreenFile = { screen_schema: 1, id: "local.<member>", title, route,
               activation: [Field; 0..32], initial_focus, focus_order: [PanelId],
               panels: [Panel; 1..16], layout: Layout,
               relationships: [Relationship; 0..64], bindings: [BindingRef; 0..256] }
Field      = { name, type: "boolean" | "optional-boolean" | "string" | "integer"
                       | "enum" | "path" | "string-list",
               values: [String]  # present exactly for `enum` }
Panel      = { id, type, config: TypedMap, focusable: bool, required: bool,
               ports: [Port; 0..32] }
Port       = { id, direction: "input" | "output", type_id: "<name>@<version>",
               required: bool, retained: bool }
Layout     = { type: "leaf", panel }
           | { type: "split", axis: "horizontal" | "vertical", children: [Child; 2..8] }
Child      = { node: Layout, size: { fixed: 1..65535 } | { weight: 1..65535 },
               min: 1..65535, max?: 1..65535, collapsible: bool,
               collapse_priority?: i32  # present exactly when collapsible }
Relationship = { kind: "scope", source: PortRef, target: PortRef }
             | { kind: "master-detail", source, target,
                 activation: "immediate" | "explicit",
                 empty: "show-none" | "show-all" | "retain" }
             | { kind: "session-target", source, target,
                 empty: "detach" | "retain" }
BindingRef = { context: ContextId, action: ActionId }
PortRef    = "<panel>.<port>"
```

Every object is closed: an unknown field, a duplicate key, a value outside a
closed enumeration, or a missing field is a rejection. Nothing that carries
meaning is optional, so `focusable`, `required`, and `collapsible` must be
written rather than defaulted. There is no secret field kind, and `pty-terminal`
is not a panel type a definition may name.

Panel and port identifiers may not contain `.`, because a port is named again as
`<panel>.<port>` and that reference is split on its first separator; an
identifier containing one would be unreachable or ambiguous with a different
pair.

`type` and `bindings` resolve against the compiled panel-type registry and the
compiled action inventory. A definition can request what the program already
has; it can never introduce a renderer, an action, or an effect.

A definition whose owner settings do not enable is parsed but never lowered, so
a screen nobody enabled resolves nothing against those registries.

#### Bounds

| Subject | Limit |
|---|---|
| File | 1,048,576 bytes |
| Data nesting (tables) / layout nesting | 16 / 8 |
| Map entries / array elements | 256 / 1,024 |
| String / identifier / path bytes | 262,144 / 128 / 4,096 |
| Screens in the registry, and candidates in the definitions directory | 64 |
| Panels per screen | 1–16 |
| Ports per panel | 32 |
| Split children | 2–8 |
| Relationships per screen, follow-ups per transition | 64 |
| Activation fields / bindings per screen | 32 / 256 |

Bounds are checked, not clamped: the value that broke the rule is reported, so
an author sees what to remove instead of silently losing the tail of a list.

#### Relationship rules and policies

Relationships join ports within one screen and are the only way one panel
influences another. The graph must be same-screen, output to input, exactly
type-and-version matched, acyclic across panels, driven by at most one incoming
edge per input, and must not declare two edges of one kind from one source port
or one source panel.

Propagation runs in declaration order inside one committed transition, never
moves focus, and is computed in full before it is committed — a transition that
would exceed the follow-up bound is abandoned with `SCR-E301` and no partial
state. A follow-up is work an edge does, including staging a selection that
moves no port; the publication that started the transition is not one. `immediate` edges move the target at once; `explicit` edges stage the
selection until the declared activation action fires. When a source becomes
absent, a target that did not declare `retained` clears regardless of policy,
and a retained target follows its own: `show-none` clears, `show-all` sets the
typed all-value, `retain` keeps the prior value, and `detach` clears the session
attachment.

The shipped `github.issues` and `github.pull-requests` screens declare this
coupling themselves — an `immediate`, `show-none` master-detail edge from the
list's `selection` output to the detail's `subject` input — so the shipped and
authored paths run the same engine.

#### States and recovery

| State | What is shown |
|---|---|
| NORMAL / FOCUSED | The composed screens; focus and status are textual and clipping is grapheme-safe. |
| UNAVAILABLE | A definition that settings do not enable is simply absent from the registry. |
| ERROR | An enabled definition that is unusable refuses the whole candidate registry before anything renders: `SCR-E301` plus `CFG-E005` (ownership or duplicate) or `CFG-E006` (reference or bound), naming the file and the rule. |
| DIRTY | Not applicable: there is no draft and no editor. |
| RECOVERY | A dormant invalid definition is reported with `CFG-W004`, omitted from the registry, and left byte-for-byte unchanged. Fix or disable the named file and restart. |
| SMALL | A lowered screen uses the standard collapse ordering and `TooSmall` fallback; there is no second geometry engine. |

---

## Keybind Footer Convention

The bottom `KeybindBar` (`src/ui/components/keybind_bar.rs`) shows
context-sensitive hints projected from the immutable action/binding snapshot for
the current context. There is no hand-maintained hint string.

- The hints are derived from the actions actually bound and available in the
  current context, so a rebound or unbound action changes the footer with no
  separate edit.
- An unavailable action is still shown, with the same reason string the
  dispatch notice, Help, menus, and the Keys editor use.
- When the terminal is focused, the bar shows only the protected leave-terminal
  binding, because every other key is forwarded to the PTY.
- The bar renders inverted: theme foreground as background, theme background as
  text.

Adding or changing a shortcut means changing the registry inventory. Do not add
a parallel hint table; the generated inventory-completeness gate fails when a
dispatch row and its projected rows disagree.

---

## Help Modal Convention

The help modal (`src/ui/modals/help.rs`) is a scrollable keyboard reference
projected from the same snapshot as the footer, so the two can never drift. It
lists the actions available in the current context with their effective chords
and, for unavailable actions, the shared reason text. The modal owns only the
viewport and scroll math.

The Keys editor (`,` -> `core.open-keys`) edits the same bindings the modal
displays; unbind writes an explicit empty list and reset removes the override so
the compiled default is inherited again.

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

Terminal focus (capture mode) is reversible and explicit. While capture is
active the registry intercepts only:

- the protected emergency exit (`core.emergency-exit`, `Ctrl+Q`);
- the protected leave-capture action (`core.leave-terminal`, `F12`);
- the scrollback controls (`PageUp`, `PageDown`, `Home`, `End`, `Up`, `Down`)
  under the existing scrollback routing conditions.

Everything else resolves to `ForwardToPty` and is written to the child process
byte-for-byte by the existing encoder. Forwarded input is deliberately not an
action, so no rebinding can change what a child process receives, and the
harness asserts those bytes separately from the resolution.

Protected bindings cannot be unbound or shadowed by an override, and their
reachability is validated for macOS and Linux, so a bad keymap can never trap a
user inside terminal capture.

Keyboard behavior must remain explicit and predictable. Focus state is part of
`AppState` and transitions through the reducer; it is never implicit.

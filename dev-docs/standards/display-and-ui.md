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
| `Settings` | `core.settings` | Section list beside General, Appearance, or Diagnostics |

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

Every declared identifier matches the workbench identifier grammar — lowercase
letters and digits in hyphen-separated groups — rather than the wider grammar the
compiled identifier types happen to accept. Panel and port identifiers may not
contain `.` on top of that, because a port is named again as `<panel>.<port>` and
that reference is split on its first separator; an identifier containing one
would be unreachable or ambiguous with a different pair. A route may carry dotted
labels, since it is namespaced.

`type` and `bindings` resolve against the compiled panel-type registry and the
compiled action inventory. A definition can request what the program already
has; it can never introduce a renderer, an action, or an effect.

A definition whose owner settings do not enable is parsed but never lowered, so
a screen nobody enabled resolves nothing against those registries.

`activation` and `bindings` are lowered onto the descriptor even though nothing
draws or dispatches them yet. Navigation builds a route declaration from a
screen's `route` plus its activation schema, and the Keys editor reads the
actions a screen requests; both read the composed registry, so a consumer that
had to re-read the file would be a second parser for a grammar that has one. An
activation field declares a *shape*, never a value, and there is no secret kind.

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
moves no port; the publication that started the transition is not one.
`immediate` edges move the target at once; `explicit` edges stage the selection
until the declared activation action fires. When a source becomes
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
| NORMAL / FOCUSED | **Not applicable to a user-defined screen yet.** A definition is discovered, lowered, validated, composed, and layout-resolvable, but it has no route, so nothing can navigate to it and no frame contains it. Routing and activation are the navigation capability's; the compiled screens' normal and focused states are unchanged and covered by their own parity suites. |
| UNAVAILABLE | A definition that settings do not enable is simply absent from the registry. |
| ERROR | An enabled definition that is unusable refuses the whole candidate registry before anything renders: `SCR-E301` plus `CFG-E005` (ownership or duplicate) or `CFG-E006` (reference or bound), naming the file and the rule. |
| DIRTY | Not applicable: there is no draft and no editor. |
| RECOVERY | A dormant invalid definition is reported with `CFG-W004`, omitted from the registry, and left byte-for-byte unchanged. Fix or disable the named file and restart. |
| SMALL | A lowered screen resolves through the standard collapse ordering and `TooSmall` fallback; there is no second geometry engine. Proven on the resolved layout rather than on a frame, for the same reason NORMAL is not applicable. |

---

## Back Precedence and the Dirty Guard

Back means the same thing on every screen. One press unwinds exactly one layer,
and the order is stated once, in `state::navigation_unwind::BackLayer::PRECEDENCE`:

1. host confirmation modal
2. dirty guard
3. chooser
4. editor or composer
5. search input
6. filter controls
7. non-dirty overlay
8. focused panel transient
9. navigation stack

A screen reports which of those it has open (`AppState::open_back_layers`) and
the shared resolver decides (`AppState::back_resolution`); the order lives in one
place rather than in each screen. Only when nothing local is open does Back reach
navigation — leaving to the screen beneath, or home when there is nothing
beneath, or doing nothing when it is already home.

The per-mode key chains in `app_input` have not yet been converted to ask the
resolver, so today it states and enforces the order for everything that consults
it rather than being the only decider. New Back handling must go through it; the
existing chains are being migrated and already agree with the order above.

`Ctrl-Q` is the protected exit. It is never an alias for Back, it is never
consumed by a layer, and it stays visible at every geometry — including inside
the dirty guard.

### Dirty guard

Leaving a screen that holds unsaved work raises the host guard rather than
navigating. The guard traps focus and restores it:

- `Tab` cycles Save, Discard, Cancel; `Esc` is Cancel.
- **Save** runs the owner's declared save and navigates only on a matching
  successful completion. The guard never saves anything itself; the screen
  holding the draft declares what saving means.
- **Discard** abandons the draft, tells the owner to restore its base, and then
  performs the navigation that was held back.
- **Cancel** keeps the draft, drops the pending navigation, and restores the
  exact focus the guard interrupted.
- A failed save keeps the user with their work: the draft, the screen, and the
  pending navigation all survive, and the guard re-offers Retry, Discard, and
  Cancel with the redacted reason shown.
- When a draft has nowhere to save to, Save is shown disabled with its reason
  and only Discard and Cancel act. The reason is text, never colour alone.

At reduced geometry the guard stacks its choices one per line and keeps the
protected exit visible:

```text
DIRTY                          RECOVERY
+ Unsaved changes ----------+ + Save failed -------------+
|>>Save  Discard  Cancel    | | draft retained           |
| Tab moves; Esc cancels    | |>>Retry  Discard  Cancel  |
+---------------------------+ +---------------------------+
```

```text
SMALL
+Unsaved?-------+
|>>Save         |
| Discard       |
| Cancel        |
| Ctrl-Q Exit   |
+---------------+
```

A refused navigation leaves the current screen exactly as it was and reports
`NAV-E001`; the reason names the rule that was broken, never the value that
broke it.

---

## The Settings Screen

`core.settings` is a full screen, not a modal, because the sections it will grow
to hold need list and detail space. It is opened with `,` from anywhere.

### Keys

| Key | Effect |
|---|---|
| `,` | Open Settings |
| `j` / `k` / Up / Down | Move the selection in the focused pane |
| Tab / Shift-Tab | Move focus between the section list and the detail pane |
| Enter | Apply the focused row, open what it leads to, confirm a reload, or take the offered recovery |
| Space | Flip what the focused row holds, when it holds something that flips |
| `K` / Alt-Up | Move the focused screen one place earlier |
| `J` / Alt-Down | Move the focused screen one place later |
| Delete | Bind nothing to the focused action |
| a | Wait for one more chord to add to the focused action |
| Left / Right | Step the recovery choices, or move the same selection |
| `s` | Save |
| `S` | Save and exit |
| `r` | Return the focused row's setting to its compiled default |
| `q` / `Esc` | Back, or withdraw a reload that is waiting to be confirmed |
| `?` / F1 | Help |
| Ctrl-Q | Protected exit, never aliased to Back and never taken by a capture |

Enter and Space are separate actions. Enter opens what a row leads to — a
screen's layout tree, an action's chord capture — and Space changes what the row
holds. Binding both to one action would make "open this screen's layout" and
"stop composing this screen" the same keystroke.

### Sections

- **General** reports the settings and state paths, the platform, and whether
  `--config` isolated the session, and selects the start screen. Everything
  except the start screen is read-only.
- **Appearance** selects the theme and toggles applying the Jefe theme to
  embedded agent output. A theme the document names but the manager cannot
  resolve is listed as `unavailable: not installed` and cannot be selected: it
  is never silently substituted.
- **Agent Types** lists every agent type the inventory declares, with the
  enablement the candidate document describes and the status the startup probe
  observed: `Compatible`, `Incompatible` with the probe's own reason,
  `Not found`, or the probe's error code and reason. Enablement may be drafted
  for a type this machine cannot run — what is installed is a fact about now,
  and what the document offers is a decision that outlives it. A document naming
  an owner no definition declares keeps its bytes and has no row: the editor has
  nothing true to say about a type it cannot name, describe, or probe.
- **Screens** lists every screen the registry composed, in the order the
  document asks for, with the layout override's composition status. A compiled
  screen is always composed, so its membership is read-only and says so rather
  than offering a toggle nothing reads. Enter opens the layout tree editor.
- **Keys** lists every action in every context it is declared for, with the
  chords the candidate describes. A protected control is read-only and carries
  the registry's exact reason. Enter captures exactly the next chord.
- **Diagnostics** is read-only. It reports each diagnostic's code, severity,
  path, redacted detail, and correction, sorted error, warning, info and then by
  path, span, and code. A diagnostic never carries a value from the document.

A section's rows are windowed around the selection and each is fitted to one
line. The Keys section lists hundreds of rows, and a pane that drew all of them
would put most of the list where `j` could reach it and nothing could show it —
and a wrapped row would push the notice line and the keybind bar off the bottom,
hiding the very reasons a long row was trying to explain. The window reports how
many rows sit above and below it.

### The layout tree editor

Enter on a screen opens its layout as a tree. The editor holds text, not a
tree: a half-typed size or a split with one child is a normal moment in an edit,
and only a complete tree the descriptor validator accepts reaches the draft.

| Key | Effect |
|---|---|
| `j` / `k` / Down / Up | Move between siblings |
| `h` / Left | Move to the parent node |
| `l` / Right | Move to the first child |
| `a` | Add a leaf, choosing from the panels this screen declares but does not place |
| `x` | Remove the selected child, when the descriptor's invariants survive it |
| `e` | Edit the selected child's axis, size, minimum, maximum, collapsibility, and collapse order |
| `H` / `V` | Wrap the selected node in a horizontal or vertical split |
| Enter | Apply the dialog, or apply the whole tree to the draft |
| Esc | Cancel the node dialog, or abandon the whole edit |
| `r` | Remove this screen's override entirely |
| `q` | Back |
| Ctrl-Q | Protected exit |

The add chooser offers only panels the tree does not already place, because a
layout must place each declared panel exactly once. That is what makes the
chooser closed rather than free text, and it is also what stops a document from
growing the identifier table. A node dialog that does not parse stays open with
its reason; a structural change the validator refuses leaves the tree exactly as
it was and reports the validator's own words on the screen's notice line.

### Draft behaviour

Every edit goes into a draft bound to the exact bytes the screen opened on.
Nothing active changes until a save succeeds:

- A theme edit previews immediately, and the preview remembers the theme it
  replaced. Cancel, Discard, a confirmed reload, and a failed save all restore
  that exact theme; a successful save adopts the previewed one.
- An edit that puts every value back where it started leaves nothing unsaved,
  including the way each value was written: rewriting a value that is already
  there does not change its quoting.
- Save is blocked while the document carries a validation error. The Diagnostics
  section carries the count and the first error, and no write is attempted.
- A save that finds the file changed keeps both the file and the draft, and
  offers Reload, Export, and Retry. A save that fails to write keeps the draft
  and offers Retry, Export, and Discard.
- Reload rebuilds the draft from the exact bytes now on disk. While the draft
  has unsaved work it asks first; Enter reloads and Esc keeps the draft.
- Export writes the draft to a contained path beside the settings document,
  named after the draft's own digest, and refuses to replace an existing file.
  The base, hash, and dirty status are unchanged either way.
- A saved change that only takes effect at startup displays exactly
  `Restart Jefe to apply structural changes`. Nothing hot reloads and nothing
  restarts itself. Every registry leaf is structural: agent enablement, screen
  membership and order, layout overrides, and key bindings are all read once
  while a session builds a registry.
- A document that publishes but that a registry owner refuses is not the same as
  a document that cannot be read. Its values stay on screen, because a screen
  that fell back to the file on disk would report a chord conflict while showing
  the binding that does not conflict, leaving the user to correct something they
  cannot see. Save stays blocked either way.
- The editors never start a provider, probe anything, or change an active
  registry. Toggling an agent type, reordering a screen, previewing a layout,
  and rebinding a key all change the draft and nothing else.

### Distinct states

```text
NORMAL                         FOCUSED
+ Settings -----------------+ + Settings -----------------+
| General                   | |>>Appearance               |
| Appearance                | | Theme: green-screen      |
| Diagnostics (2)           | | s Save                   |
+---------------------------+ +---------------------------+
```

```text
UNAVAILABLE                    ERROR
+ Appearance ---------------+ + Settings -----------------+
| Theme missing-theme       | | theme has wrong type     |
| unavailable: not installed| | CFG-E003 Save blocked    |
+---------------------------+ +---------------------------+
```

```text
DIRTY                          RECOVERY
+ Save changes? ------------+ + External edit detected --+
|>>Save  Discard  Cancel    | | disk and draft preserved |
+---------------------------+ |>>Reload Export Retry     |
                               +---------------------------+
```

```text
SMALL
+Settings--------+
|>>Appearance    |
| theme: green   |
| ! 1 error      |
| q Back Ctrl-Q  |
+----------------+
```

Every state is keyboard reachable and every marker is text: selection is `>>`,
the active choice is `*`, and unavailability says so in words.

The Agent Types, Screens, and Keys editors each carry the same seven states, and
each is exercised separately — a shared scenario would not prove that an editor
reports its own unavailability and its own refusals:

```text
UNAVAILABLE (Agent Types)      UNAVAILABLE (Screens)
+ Agent Types --------------+ + Screens ------------------+
|>>Claude Code: [x] Not found| |>>Dashboard: [x]          |
| Codex CLI: [x] Not found  | | compiled screens are     |
|                           | | always composed          |
+---------------------------+ +---------------------------+
```

```text
ERROR (Keys)                   ERROR (Screens/Layout)
+ Keys ---------------------+ + Layout -------------------+
|>>actions Focus search: d  | | split H                  |
| Diagnostics (1)  KEY-E401 | | declares panel but never |
+---------------------------+ | places it in the layout  |
                               +---------------------------+
```

The 21 scenarios under `dev-docs/tmux-scenarios/settings-*.json` drive these
states through the real TUI. Each reaches its state by keystroke alone, because
the harness launches jefe with its own isolated `--config` directory and a
scenario cannot seed a settings document.

---

## The Plugins Section

The Plugins section is a pure presenter over the immutable inventory snapshot
bound when the screen opens. It never scans a root, installs, writes, or starts
a provider, so a rescan finishing underneath the screen cannot make the list
move while the operator is choosing from it.

Every state is a distinct **text** status, never a colour alone:

| State | Status text | Second line |
|---|---|---|
| Installed | `installed` | — |
| No binary for this host | `Unsupported platform` | `no binary for <triple>` |
| Ambiguous | `Ambiguous PLG-E501` | `N physical package paths` |
| Unreadable | `unavailable` | the reason it cannot be read |

An installed package defaults to **not** trusted — deliberately the opposite of
an agent type — so one nobody has enabled never looks ready to run. A package
that cannot be selected, because it is ambiguous or unreadable, never renders as
trusted and offers no toggle: there is no single thing to trust and trusting it
could not take effect.

Granting trust states the consequence rather than implying it: the provider runs
unsandboxed as the operator's own OS user. The recovery state for a broken
selected package states the process count explicitly, because the point of that
state is that nothing ran.

## Generated Plugin Configuration

The Plugins section generates configuration rows from the immutable manifest of
the exact installed package version selected by the Settings draft. It does not
scan packages or choose a version independently. Boolean, string, integer,
finite-number, enum, path, string-list, and secret-reference declarations use
the existing Settings draft, editor, dirty guard, and expected-hash writer.

The pure projection owns visibility and display decisions. It shows labels,
descriptions, defaults, inclusive bounds, enum choices, list uniqueness, and
restart metadata; invalid active values have an adjacent diagnostic and also
contribute to the Save-blocking summary. A hidden row is omitted. Required
fields are required only while visible, but a present hidden value must remain
typed and valid. Configuration owned by an absent or disabled package is dormant:
its exact bytes remain in the lossless document and the absent owner does not
validate it.

Secret-reference controls display only the environment-variable name and whether
that variable is set. The durable and draft value is exactly an environment
reference; resolved secret bytes never enter a projection, editor seed,
diagnostic, effective-settings view, export, migration preview, or panel model.

## Host-Rendered Provider Panels

A package screen is rendered from the same published `ScreenRegistry` and
`ResolvedLayout` as a compiled screen. Each package panel binding records the
exact selected package owner and manifest declaration; the UI never infers an
owner from an identifier. Provider snapshots are data only. The host exclusively
owns iocraft elements, focus, selection repair, scrolling, wrapping, form drafts,
confirmation, links, theme, accessibility, mouse/key translation, Back, and the
small-terminal presentation.

The closed host primitives are list, detail, form, status, progress, empty, and
error. Activating and unavailable panels render explicit text rather than an
empty rectangle. A failed panel may retain the last complete accepted model, but
it is marked literally `stale`. A candidate snapshot is projected only after the
panel reducer has accepted the entire model; a rejected candidate never appears
partially.

Input becomes one closed semantic event only after the host validates the live
screen/panel instance, process and panel generations, accepted revision,
manifest event declaration, referenced item/field/action/token, argument schema,
and enabled affordance. Invalid, stale, undeclared, or disabled input emits zero
provider and host effects. Providers never receive raw keys, mouse events,
iocraft objects, focus or scroll instructions, or arbitrary host effects.

Panel lifecycle follows navigation. Enter declares and activates; pushing another
screen suspends; Back disposes the departed screen and resumes the retained panel
with a fresh activation generation; replacement disposes with a replace reason.
Only bounded host-local focus/scroll/selection/form state survives suspension.
Panel models, lifecycle, revisions, generations, and host-local state are never
persisted.

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

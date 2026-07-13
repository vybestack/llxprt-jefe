# Configurable Workbench Architecture

## Status

Design discussion only. This document proposes terminology and a target architecture; it is not an implementation plan or compatibility commitment.

## Executive recommendation

Jefe should evolve from a closed collection of hard-coded screens and agent kinds into a **typed, registry-backed terminal workbench**.

The target should be:

- declarative where composition is involved;
- strongly typed inside the application;
- rendered through host-owned reusable controls;
- explicit about panel relationships and focus;
- driven by stable actions and contextual keymaps;
- extensible through versioned declarative plugins first;
- process-isolated when executable plugin behavior is necessary;
- incremental, retaining the tested reducers, controls, and runtime boundaries already present.

The current Dashboard should become the built-in default configuration rather than being discarded.

## Proposed terminology

| Term | Meaning |
|---|---|
| **Screen** | A named workspace such as `core.dashboard` or `github.pull-requests`. |
| **Layout tree** | A declarative tree of row, column, and stack nodes that allocates geometry. |
| **Region** | A named slot in the layout with sizing, minimum-size, and collapse policies. |
| **Panel type** | A registered interactive implementation such as `core.repositories` or `core.terminal`. |
| **Panel instance** | One configured use of a panel type in a screen region. |
| **Control** | A reusable host-rendered primitive such as a selectable list, detail document, form, or terminal surface. |
| **Action** | A stable, namespaced user intent such as `core.agent.kill`. |
| **Relationship** | A declared data or navigation connection between panel instances. |
| **Focus graph** | Directional and sequential focus relationships between panel instances. |
| **Context** | A typed predicate over the active screen, focused panel/control, input mode, overlay, and selection. |
| **Keymap** | Ordered contextual bindings from normalized key chords or sequences to actions. |
| **Contribution** | A plugin-provided panel, action, screen, adapter, keymap, or provider. |
| **Agent adapter** | A descriptor and translator from semantic agent launch intent to a validated process specification. |
| **Semantic capability** | A product-neutral feature such as model selection, autonomy, resume, profile, or sandboxing. |

Use **view** or **projection** for pure display models. Use **panel** for an interactive unit placed on a screen.

## Current architecture

### Closed screen and mode model

The active screen is represented by the closed `ScreenMode` enum in `src/state/types.rs`, with Dashboard, Split, Issues, Pull Requests, and Actions variants. `src/ui/orchestration.rs` exhaustively matches that enum and constructs a concrete screen component.

`InputMode` in `src/input.rs` is a second closed hierarchy containing navigation, terminal capture, forms, modals, and domain-specific Issues/PR/Actions modes.

Adding a screen therefore affects rendering, input routing, focus, selection geometry, help, key hints, and often layout calculations.

### Hard-coded and duplicated layout topology

The Dashboard topology is embedded in `src/ui/screens/dashboard.rs`: status bar, repository sidebar, center agents/terminal stack, Preview, and key-hint bar. Width and height assumptions are encoded in `src/layout.rs`, including a fixed 22-column repository sidebar, a 36-column Preview, and a roughly 25/75 agents-to-terminal center split.

The same topology is represented separately for:

- component rendering;
- layout arithmetic;
- PTY geometry;
- mouse hit testing and text selection;
- focus navigation;
- selectable pane identity.

`src/selection/text.rs` contains a closed `SelectablePane` enum, while `src/selection/layout_descriptor.rs` describes screen-specific geometry again. A configurable architecture must replace these parallel representations with one resolved geometry tree consumed by every subsystem.

### Existing reusable controls

The codebase already has useful control-level abstractions:

- `SelectableList` in `src/ui/components/selectable_list.rs`;
- `DetailPane` in `src/ui/components/detail_pane.rs`;
- `TextBox`;
- `ScrollableText`;
- `FilterBar`;
- `Sidebar`;
- `StatusBar`;
- `TerminalView`;
- chooser and modal components.

Issues, PRs, and Actions already reuse list/detail controls. The missing abstraction is not another widget toolkit. It is a panel contract that composes controls, consumes typed inputs, maintains bounded state, and emits registered actions.

### Implicit panel relationships

The Dashboard manually wires this chain in `src/ui/screens/dashboard.rs`:

```text
repository selection
    -> scopes agents
        -> drives Preview
        -> selects the terminal session
```

Issues, PRs, and Actions independently implement repository-scoped master-detail behavior. Their domain reducers contain important behavior, including request identities and stale-response guards. That business logic should remain typed and domain-specific, while selection, scope, activation, and focus wiring become reusable relationship concepts.

### Hard-coded keymaps

Key dispatch is compiled into ordered handlers in `src/app_shell.rs` and `src/app_input`. Help and footer hints are separately maintained in `src/ui/modals/help.rs` and `src/ui/components/keybind_bar.rs`.

That creates multiple sources of truth. Actions and resolved key bindings should generate dispatch, help, and contextual key hints.

### Product-specific agent model

`AgentKind` in `src/domain/mod.rs` is a closed enum for Code Puppy and LLxprt. Detection, form fields, persistence, runtime command construction, and remote resolution branch on it.

Shared and product-specific concepts are mixed together: model, YOLO/autonomy, continuation, quick resume, profile, sandboxing, debug, and raw mode flags. Command construction is properly centralized in `src/runtime/commands.rs`, but the supported product set is closed.

## Target architecture

## Screens and layout

A screen contains:

- a layout tree;
- named panel instances;
- typed relationships;
- a focus graph;
- screen-level key bindings.

A conceptual layout model is:

```rust
pub enum LayoutNode {
    Region(RegionSpec),
    Split(SplitSpec),
    Stack(StackSpec),
}

pub struct RegionSpec {
    pub id: RegionId,
    pub panel: PanelInstanceId,
    pub size: SizeRule,
    pub min: CellSize,
    pub collapse_priority: Option<u16>,
}

pub enum SizeRule {
    Fixed(u16),
    Fraction(u16),
    Fill,
}
```

Use integer weights rather than persisted floating-point percentages.

A single resolver should produce rectangles by region and panel ID. Rendering, mouse hit testing, text selection, focus derivation, viewport calculations, and PTY resizing should all consume that result.

The terminal is a privileged panel whose PTY dimensions derive from its resolved rectangle after subtracting its own chrome. It should not infer dimensions from Dashboard constants.

## Panel contract

A conceptual built-in panel contract is:

```rust
pub trait PanelController: Send {
    fn descriptor(&self) -> &PanelDescriptor;

    fn project(
        &self,
        context: &PanelReadContext,
    ) -> Result<PanelView, PanelError>;

    fn reduce(
        &self,
        state: &mut PanelState,
        action: &ActionInvocation,
    ) -> Result<Vec<EffectRequest>, PanelError>;
}
```

The host should render a constrained set of views:

```rust
pub enum PanelView {
    SelectableList(SelectableListModel),
    Detail(DetailModel),
    Form(FormModel),
    Terminal(TerminalPanelModel),
    Composite(CompositeControlModel),
    Empty(EmptyPanelModel),
}
```

External plugins should not return arbitrary iocraft elements or receive mutable `AppState`, PTY handles, persistence managers, or runtime internals. They should return validated panel models rendered by host-owned controls.

Built-in domain state should remain typed. The extensibility layer should not turn the application into maps of unchecked JSON values.

## Relationships

Relationships connect typed panel ports:

```rust
pub enum RelationshipKind {
    Scope,
    MasterDetail,
    Reveal,
    SessionTarget,
    DataInput,
}
```

Examples:

```text
repositories.selection --scope--> agents.repository
agents.selection --master-detail/live--> preview.agent
agents.selection --session-target--> terminal.agent
pulls.selection --master-detail/activate--> detail.pull-request
```

Master-detail relationships should support explicit policies:

- `selection-changed` for immediate updates;
- `activate` for Enter-driven detail activation;
- `live` for continuous projection;
- `retain-on-refresh` for preserving visible detail during refresh.

Ports declare data types, and configuration validation rejects incompatible links.

Focus should remain separate from data relationships. Explicit directional edges can override deterministic geometric focus derivation.

## Actions and keymaps

All user-facing behavior should receive a stable action ID:

```text
core.app.quit
core.screen.open
core.panel.focus-next
core.agent.create
core.agent.kill
core.terminal.toggle-capture
github.pr.open-detail
github.pr.reply
```

An action descriptor supplies a title, description, category, valid arguments, valid contexts, and discoverability metadata.

Resolution precedence should be explicit:

1. protected host bindings;
2. active overlay or modal;
3. focused control;
4. focused panel;
5. active screen;
6. global bindings.

Context conditions should be parsed into a small typed predicate AST, not evaluated as scripts.

At least one terminal-capture escape must remain valid so a user cannot configure Jefe into an unrecoverable state. F12 can remain protected initially.

Help and footer hints should be generated from action descriptors and resolved active bindings.

## Representative configuration

### Default Dashboard

```toml
schema_version = 2

[ui]
default_screen = "core.dashboard"

[ui.screens."core.dashboard"]
title = "Dashboard"

[ui.screens."core.dashboard".layout]
type = "rows"
children = ["status", "workspace", "keys"]

[ui.screens."core.dashboard".regions.status]
panel = "status"
size = { fixed = 1 }

[ui.screens."core.dashboard".regions.workspace]
type = "columns"
children = ["repositories", "center", "preview"]

[ui.screens."core.dashboard".regions.repositories]
panel = "repositories"
size = { fixed = 22 }
min = { cols = 12, rows = 3 }
collapse_priority = 30

[ui.screens."core.dashboard".regions.center]
type = "rows"
children = ["agents", "terminal"]
size = { fill = true }

[ui.screens."core.dashboard".regions.agents]
panel = "agents"
size = { fraction = 1 }
min = { cols = 20, rows = 3 }

[ui.screens."core.dashboard".regions.terminal]
panel = "terminal"
size = { fraction = 3 }
min = { cols = 20, rows = 5 }

[ui.screens."core.dashboard".regions.preview]
panel = "preview"
size = { fixed = 36 }
min = { cols = 20, rows = 5 }
collapse_priority = 10

[ui.screens."core.dashboard".regions.keys]
panel = "key-hints"
size = { fixed = 1 }

[ui.screens."core.dashboard".panels.repositories]
type = "core.repositories"

[ui.screens."core.dashboard".panels.agents]
type = "core.agents"

[ui.screens."core.dashboard".panels.terminal]
type = "core.terminal"

[ui.screens."core.dashboard".panels.preview]
type = "core.agent-preview"

[ui.screens."core.dashboard".panels.status]
type = "core.status"

[ui.screens."core.dashboard".panels.key-hints]
type = "core.key-hints"
```

### Relationships

```toml
[[ui.screens."core.dashboard".relationships]]
kind = "scope"
source = "repositories.selection"
target = "agents.repository"
empty = "show-none"

[[ui.screens."core.dashboard".relationships]]
kind = "master-detail"
source = "agents.selection"
target = "preview.agent"
activation = "selection-changed"

[[ui.screens."core.dashboard".relationships]]
kind = "session-target"
source = "agents.selection"
target = "terminal.agent"
activation = "selection-changed"
```

### Keymaps

```toml
[keymaps]
sequence_timeout_ms = 1000
conflict_policy = "error"

[[keymaps.bindings]]
id = "quit"
keys = "ctrl+q"
action = "core.app.quit"
when = "input.navigation && !terminal.capture"
scope = "global"

[[keymaps.bindings]]
id = "terminal-capture"
keys = "f12"
action = "core.terminal.toggle-capture"
when = "panel.exists(core.terminal)"
scope = "global"
protected = true

[[keymaps.bindings]]
id = "open-pr-detail"
keys = "enter"
action = "github.pr.open-detail"
when = "screen == github.pull-requests && panel == pulls && selection.present"
scope = "panel"
```

Bindings need IDs so users can explicitly replace defaults without relying on ambiguous order.

## Agent adapters

Normalize user intent rather than CLI spelling:

```rust
pub enum AutonomyPolicy {
    Guided,
    Unrestricted,
}

pub enum ResumePolicy {
    Fresh,
    ContinueConversation,
    LatestCheckpoint,
}

pub enum ModelChoice {
    AdapterDefault,
    Named(String),
}

pub enum SandboxPolicy {
    Disabled,
    Enabled(SandboxConfig),
}
```

An adapter declares supported variants and maps them to product-specific arguments. For example:

- LLxprt unrestricted autonomy becomes `--yolo`;
- Code Puppy unrestricted autonomy becomes `--yolo true`;
- LLxprt continuation becomes `--continue`;
- Code Puppy checkpoint resume becomes `--quick-resume <work_dir>`.

The final adapter output should remain argv-based:

```rust
pub struct ProcessSpec {
    pub executable: ExecutableRef,
    pub args: Vec<OsString>,
    pub env: Vec<(OsString, OsString)>,
    pub cwd: PathBuf,
}
```

Never make arbitrary shell templates the adapter language.

A representative adapter definition is:

```toml
adapter_schema_version = 1
id = "code-puppy.cli"
display_name = "Code Puppy"
executable = "code-puppy"
probe = { type = "path-executable" }

[capabilities]
model = "optional-text"
autonomy = ["guided", "unrestricted"]
resume = ["fresh", "latest-checkpoint"]
prompt = ["positional"]
interactive = true
sandbox = false
profile = false

[launch]
base_args = ["-i"]

[launch.model]
args = ["--model", "{value}"]

[launch.autonomy.unrestricted]
args = ["--yolo", "true"]

[launch.autonomy.guided]
args = ["--yolo", "false"]

[launch.resume.latest-checkpoint]
args = ["--quick-resume", "{work_dir}"]
```

LLxprt and Code Puppy should initially remain Rust implementations behind the adapter interface. Declarative definitions should replace them only after parity tests prove the grammar can express all current local and remote behavior safely.

## Plugin architecture

Use two tiers.

### Declarative plugins

These can contribute:

- screens and layouts;
- panel declarations;
- actions;
- default keymaps;
- agent adapters;
- panels backed by host providers.

This should be the first external plugin tier.

### Process plugins

When executable functionality is required, use a supervised child process with a versioned JSON-lines protocol over standard input/output. A process plugin may provide data or action behavior but should not receive unrestricted host internals.

Avoid Rust dynamic libraries because Rust has no stable plugin ABI, dependency coupling is severe, crashes compromise the host, and in-process code bypasses capability boundaries.

A representative manifest is:

```toml
manifest_version = 1
id = "code-puppy"
name = "Code Puppy Integration"
version = "1.2.0"
requires_jefe = ">=0.1,<0.2"
mode = "declarative"

[permissions]
process = ["code-puppy"]
network = false
filesystem = ["agent-workdir:read-write"]
terminal_input = false

[[contributions.agent_adapters]]
id = "code-puppy.cli"
definition = "adapters/code-puppy.toml"

[[contributions.panels]]
id = "code-puppy.checkpoints"
kind = "provider-backed-list"
provider = "checkpoint-list"

[[contributions.actions]]
id = "code-puppy.resume-checkpoint"
title = "Resume checkpoint"
provider = "resume-checkpoint"
```

Plugin enablement and grants must be stored separately so a package update cannot silently expand its permissions.

A process plugin still has its operating-system process permissions unless Jefe introduces an OS sandbox. The UI and documentation must state that limitation honestly.

## Registries and boundaries

Startup should resolve contributions in this order:

1. built-in actions, panels, screens, and adapters;
2. declarative plugin manifests;
3. enabled process-provider handshakes;
4. user overrides;
5. full cross-reference validation;
6. immutable registry snapshot.

Recommended boundaries:

- **Domain:** stable IDs, semantic capabilities, relationship types, and descriptors; no iocraft, filesystem, process, or plugin transport.
- **Application:** action dispatch, relationship propagation, screen session state, reducers, and effect planning.
- **UI adapter:** maps resolved layouts and `PanelView` models to host controls; emits actions only.
- **Runtime adapters:** tmux/PTY, GitHub, persistence, plugin transport, and executable agent adapters.
- **Composition root:** discovery, construction, validation, and dependency injection.

Do not put a mutable global registry in `AppState`.

Panel and action handlers should request bounded effects:

```rust
pub enum EffectRequest {
    PersistState,
    PersistSettings,
    Runtime(RuntimeRequest),
    Provider(ProviderRequest),
    OpenExternalUrl(ValidatedUrl),
    ClipboardWrite(String),
}
```

The shell executes effects and returns typed completion messages carrying request and scope IDs. This preserves the stale-response protections already used by PR and Issue flows.

## Validation and security

Configuration should be parsed into version-specific DTOs, migrated, validated, and only then converted into domain models.

Validation should cover:

1. syntax and size limits;
2. schema compatibility;
3. ID grammar and namespace ownership;
4. duplicates;
5. plugin dependency constraints;
6. layout tree structure and region references;
7. relationship port compatibility;
8. focus graph references and reachability;
9. key parsing, conflicts, and protected bindings;
10. adapter capability and option schemas;
11. permission grants;
12. construction of an immutable resolved registry.

Security invariants include:

- never interpolate plugin data into shell commands;
- preserve argv arrays until the final process boundary;
- route remote serialization through one audited escaping implementation;
- give plugin panels redacted query snapshots, not all application state;
- bound and sanitize plugin text before terminal rendering;
- reject path traversal in config includes and plugin resources;
- store no plaintext secrets in application state;
- keep terminal input ownership built-in initially;
- ensure a protected terminal escape remains available.

Settings, operational state, plugin manifests, adapter definitions, and process protocols should have independent schema/protocol versions.

## Migration approach

Every stage should begin with behavioral and contract tests. Existing lint, complexity, and source-size rules should not be weakened.

### Stage 1: stable actions and keymap metadata

- Add action IDs and descriptors behind existing event behavior.
- Preserve current key precedence, including terminal capture.
- Generate help and footer hints from resolved bindings.

### Stage 2: unified layout resolution

- Represent the current Dashboard as the built-in default layout.
- Prove parity for normal and degenerate terminal sizes.
- Move rendering, hit testing, selection, and PTY sizing to one geometry result.

### Stage 3: panel instances and relationships

- Wrap existing Repository, Agent, Preview, Terminal, list, and detail components.
- Preserve current reducers and controls.
- Introduce typed relationship ports and focus edges.

### Stage 4: built-in screen migration

- Migrate Dashboard, Issues, PRs, Actions, and repository management to registered screen definitions.
- Preserve modal, filter, chooser, focus-restoration, and mouse behavior through TUI harness tests.

### Stage 5: semantic agent adapters

- Put LLxprt and Code Puppy behind the adapter interface.
- Prove exact local and remote argv/environment parity.
- Migrate persisted product-specific fields into semantic launch configuration plus validated adapter options.

### Stage 6: declarative plugins

- Add deterministic discovery, namespacing, validation, permissions, and contributions.
- Prove that a test plugin can add a screen, panel, action, keymap, and adapter without changing core enums.

### Stage 7: process providers if required

- Add supervised process transport only for demonstrated use cases.
- Test handshake, protocol mismatch, permission denial, timeout, cancellation, oversized messages, malformed responses, crashes, and stale replies.

## Major design decisions to settle

1. **Panel expressiveness:** host-owned controls versus arbitrary drawing. Recommendation: host-owned controls and constrained panel models.
2. **Responsive layout:** collapse, stack, or fail. Recommendation: deterministic collapse priorities followed by a host-provided focused single-panel fallback.
3. **Screen overrides:** mutate contributed screens or derive new ones. Recommendation: permit user layout/panel/keymap overrides while preserving immutable contribution descriptors and provenance.
4. **Panel state lifetime:** screen-local, shared by type, or repository-scoped. Recommendation: screen-instance state by default with explicit repository-scoped preferences.
5. **Master-detail activation:** immediate preview or Enter activation. Recommendation: support both as explicit policies.
6. **Protected keys:** recommendation is at least one terminal escape and one emergency application exit path.
7. **Agent advanced arguments:** retain a clearly marked argv escape hatch for compatibility, while keeping semantic options authoritative.
8. **Executable UI plugins:** recommendation is not initially; begin with declarative and provider-backed panels.
9. **Plugin trust:** explicit enablement and grants first, with optional hash pinning; do not imply that process plugins are sandboxed unless an OS sandbox exists.
10. **Hot reload:** startup-only initially because live registry replacement complicates focused panels, running providers, and persisted state.

## Architectural position

Jefe already has valuable foundations: typed reducers, pure projections, shared controls, explicit runtime and persistence traits, atomic writes, and careful request scoping. The missing abstraction is at the composition level.

The proposed architecture makes composition configurable without making the core untyped:

- declarative screens and layout trees;
- named panel instances rendered through host controls;
- explicit typed relationships and focus graphs;
- stable actions as the common currency for input, help, and plugins;
- contextual user-configurable keymaps;
- semantic agent capabilities translated by adapters;
- declarative plugins first and supervised process plugins where needed;
- one resolved geometry model for rendering, hit testing, selection, and PTY sizing;
- strict schema, namespace, permission, and compatibility validation;
- incremental migration protected by behavioral, contract, integration, and TUI harness tests.

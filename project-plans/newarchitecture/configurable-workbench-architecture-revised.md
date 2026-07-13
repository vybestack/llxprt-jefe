# Configurable Workbench Architecture — Revised Discussion

## Core distinction

The architecture should keep three concepts separate:

1. **Agent type definitions** — LLxprt, Code Puppy, Claude Code, Codex CLI, and similar agent CLIs.
2. **Workbench definitions** — screens, layouts, panel instances, constrained relationships, actions, focus, and keymaps.
3. **Functional plugins** — optional installed functionality such as a Git merger.

An agent CLI is not a general plugin.

## Decisions established in the discussion

- Every workspace screen is resolved dynamically from a definition rather than a closed Rust screen enum.
- Jefe ships the complete standard screen set matching current behavior.
- Agent/CLI types are data-driven definitions, not general plugins.
- Relationships are deliberately constrained rather than exposed as an arbitrary graph.
- Panel state is local to a screen instance by default.
- Master-detail supports immediate preview and explicit activation.
- Plugin and definition loading occurs only at startup; there is no hot reload.
- Loading is transactional: if any enabled plugin or configured definition cannot load or validate, Jefe aborts startup rather than partially loading.
- Protected recovery actions must use platform-appropriate bindings for macOS, Linux, and eventually Windows.
- Users explicitly choose which executable plugins to install and enable. Jefe does not need elaborate untrusted-plugin infrastructure.

## Agent type definitions

An agent type definition describes how Jefe detects, presents, configures, and launches an agent CLI.

For example:

```text
Code Puppy
├── executable: code-puppy
├── repository defaults
│   └── default model
├── agent fields
│   ├── model
│   ├── yolo
│   └── quick resume
├── detection
└── argument mapping
```

Likewise:

```text
LLxprt
├── executable: llxprt
├── repository defaults
│   └── default profile
├── agent fields
│   ├── profile
│   ├── yolo
│   ├── continue
│   ├── sandbox
│   └── debug
├── detection
└── argument/environment mapping
```

Conceptually:

```rust
pub struct AgentTypeDefinition {
    pub id: AgentTypeId,
    pub display_name: String,
    pub probe: AgentProbe,
    pub repository_fields: Vec<AgentFieldDefinition>,
    pub agent_fields: Vec<AgentFieldDefinition>,
    pub launch: AgentLaunchDefinition,
}
```

The definition determines:

- whether the CLI is available locally or remotely;
- which defaults appear on repository configuration screens;
- which fields appear on create/edit-agent screens;
- field defaults, choices, and validation;
- how field values map to individual argv elements and environment entries;
- which values participate in the launch signature.

This replaces product-specific branching currently spread across `AgentKind` in `src/domain/mod.rs`, `src/agent_detection.rs`, agent form projection, repository defaults, agent creation, and `src/runtime/commands.rs`.

### Common and product-specific fields

Do not force every agent into one universal lowest-common-denominator capability schema.

Some concepts are genuinely common:

- working directory;
- executable detection;
- prompt delivery;
- possibly autonomy;
- possibly resume behavior.

Others are product-specific:

- Code Puppy quick-resume behavior;
- LLxprt profile loading;
- Codex approval and sandbox choices;
- Claude Code permission choices.

The model should therefore be:

```text
small set of genuinely common launch fields
+
namespaced typed fields supplied by the agent definition
```

The UI can present equivalent concepts consistently where they really align, but an agent definition may expose capabilities that other agents do not have.

### Raw/advanced arguments

The earlier raw-argument idea was intended as an escape hatch when an agent CLI adds a new option before Jefe's definition supports it.

The simpler recommendation is: **do not add raw arguments initially**.

A raw field would create ambiguity:

- extra arguments could conflict with generated arguments;
- Jefe could not completely validate the launch;
- the form would no longer fully represent the process configuration;
- launch-signature comparisons would be less meaningful;
- local and remote parity would be harder to prove;
- free-form string splitting would introduce avoidable parsing and quoting problems.

Users should instead update or locally extend the relevant agent definition with a typed field.

If real usage later proves this too restrictive, Jefe could add an explicitly supported `additional_args: Vec<OsString>` capability for selected agent definitions. It must remain an argv array, never a shell command or whitespace-split string.

## Definition-driven screens

Every screen should resolve through a `ScreenDefinition`:

```rust
pub struct ScreenDefinition {
    pub id: ScreenId,
    pub title: String,
    pub layout: LayoutNode,
    pub panels: Vec<PanelInstanceDefinition>,
    pub relationships: Vec<RelationshipSpec>,
    pub focus: FocusPolicy,
    pub bindings: Vec<BindingDefinition>,
}

pub struct ScreenInstance {
    pub id: ScreenInstanceId,
    pub definition_id: ScreenId,
    pub panel_states: BTreeMap<PanelInstanceId, PanelState>,
    pub focused_panel: Option<PanelInstanceId>,
}
```

Jefe should ship built-in definitions corresponding to all current workspace screens:

- Dashboard;
- repository management, currently called Split;
- Issues;
- Pull Requests;
- Actions.

Potential IDs are:

```text
core.dashboard
core.repositories
github.issues
github.pull-requests
github.actions
```

“Dynamic” means Jefe resolves a screen definition from an immutable startup registry instead of matching a closed `ScreenMode` enum. It does not mean arbitrary scripts, arbitrary iocraft trees, or runtime hot replacement.

Built-in screen definitions may reference strongly typed Rust panel controllers. Definition-driven composition does not require converting existing PR, Issue, terminal, or agent reducers into untyped maps.

### Modals

Forms, confirmations, choosers, and help overlays should retain modal lifecycle semantics:

- focus capture;
- confirmation or cancellation;
- restoration of previous focus;
- blocking lower-priority input.

They may become descriptor-driven later, but they should not be treated as ordinary workspace panels merely because both render rectangles.

## Panel state lifetime

Panel state should be keyed by screen instance and panel instance:

```text
(ScreenInstanceId, PanelInstanceId) -> PanelState
```

Two instances of the same screen definition therefore have independent:

- selection;
- scroll position;
- filters;
- drafts;
- activated detail;
- focus;
- local loading and error presentation.

Shared domain data remains outside panel-local state:

- repositories;
- agents;
- fetched PR records;
- runtime sessions;
- appropriate provider caches.

This separates application data from how a particular panel instance currently presents it.

## Constrained relationships

Jefe should not expose a generic data graph. Start with only the relationship kinds required by current behavior:

```rust
pub enum RelationshipSpec {
    Scope {
        source: SelectionOutputPort,
        target: ScopeInputPort,
        empty: EmptyScopePolicy,
    },
    MasterDetail {
        source: SelectionOutputPort,
        target: DetailInputPort,
        activation: DetailActivation,
    },
    SessionTarget {
        source: SelectionOutputPort,
        target: SessionInputPort,
    },
}

pub enum DetailActivation {
    ImmediatePreview,
    ExplicitActivation,
}
```

Examples:

```text
repositories.selection --Scope----------> agents.repository
agents.selection       --MasterDetail----> preview.agent
agents.selection       --SessionTarget---> terminal.agent

repositories.selection --Scope----------> pulls.repository
pulls.selection        --MasterDetail----> pull-detail.pull
```

### Cardinality and topology rules

1. **At most one incoming relationship per target port.** A PR detail panel cannot be controlled by two PR lists.
2. **At most one outgoing relationship per source and relationship kind.** A PR list cannot control two PR detail panels.
3. **Cross-kind use is permitted.** Agent selection may control one Preview through `MasterDetail` and one terminal through `SessionTarget`; this is deliberate behavior, not arbitrary fan-out.
4. **Endpoints must belong to the same screen instance.** Persistent cross-screen relationships are prohibited.
5. **Port types must match exactly.** A `Selection<AgentId>` cannot connect to `DetailInput<PullRequestId>`.
6. **The relationship dependency structure must be acyclic.** No relationship may feed back into an ancestor.
7. **Focus edges remain separate from data relationships.** Data flow does not implicitly define keyboard navigation.
8. **Panels declare their available ports.** Configuration cannot invent arbitrary source or target ports.

If a future demonstrated use case needs more complex topology, add a named and typed coordinator for that case rather than weakening the relationship model into a general graph.

## Master-detail behavior

Both required behaviors fit one relationship with an activation policy.

### Immediate preview

Selection movement immediately updates the target:

```text
Agent selection changes
    -> Preview changes
```

### Explicit activation

Selection movement does not change the activated detail. Enter or another bound action activates the current selection:

```text
PR list selection changes
    -> current detail remains

User invokes Activate
    -> selected PR becomes active detail
    -> detail may receive focus
```

Domain reducers still own loading, request IDs, stale-response rejection, mutations, and errors.

## Functional plugins

A general plugin adds optional product functionality. A Git merger is a representative example.

Initially, plugins may contribute:

### Declarative contributions

- screen definitions;
- provider-backed panel descriptors using host-owned controls;
- action descriptors;
- default contextual key bindings;
- action or menu placement metadata.

### Executable contributions

- typed action handlers;
- typed query/data providers for plugin-backed panels.

Agent type definitions are intentionally a separate mechanism.

## What “executable UI plugin” should mean

Jefe should not allow a plugin executable to:

- draw arbitrary terminal UI;
- return iocraft components;
- own keyboard routing or focus;
- mutate `AppState` directly;
- own PTY input;
- access persistence or internal service objects directly.

That would effectively introduce a second UI runtime and an unstable rendering/input contract.

Instead, support **executable behavior providers with host-rendered UI**.

A plugin may return constrained view models:

```rust
pub enum PluginPanelView {
    List(ListModel),
    Detail(DetailModel),
    Form(FormModel),
    Status(StatusModel),
    Empty(EmptyModel),
}
```

Jefe renders those models with standard controls and retains ownership of focus, key handling, dialogs, state transitions, and effects.

### Git merger example

A merger plugin could contribute:

- a `git-merger.merge` action;
- an optional status/detail panel;
- an optional merger screen;
- default bindings or action placement metadata.

When invoked, Jefe owns the confirmation dialog and sends typed context:

```rust
pub struct MergeRequest {
    pub request_id: RequestId,
    pub repository: RepositoryRef,
    pub pull_request: PullRequestRef,
    pub strategy: MergeStrategy,
    pub expected_head_oid: GitOid,
}
```

The flow is:

```text
User invokes Merge
    -> Jefe shows confirmation
    -> Jefe starts the installed merger executable
    -> Jefe writes a typed request to stdin
    -> plugin performs Git/GitHub work
    -> plugin writes progress/result messages to stdout
    -> Jefe applies the typed result and refreshes PR data
```

A simple action should use a one-shot process. A persistent provider process is warranted only when a plugin panel needs repeated queries or streaming progress.

The plugin does not control the PR detail panel or receive all application state. It receives only the repository and PR context declared by its action contract.

## Plugin trust model

The trust model should be straightforward:

> Enabling an executable plugin means the user trusts that executable to run with the same operating-system account and ambient permissions as Jefe.

Initially, Jefe does not need:

- a capability broker;
- permission grants;
- hash pinning;
- signatures;
- an OS sandbox;
- elaborate untrusted-component infrastructure.

The process boundary is still useful for engineering reasons:

- no Rust ABI coupling;
- plugin crashes are isolated from the Jefe process;
- versioned request/response contracts;
- lifecycle and cancellation;
- bounded output;
- implementation in any language.

It is not a security sandbox.

Basic correctness protections still apply:

- plugin resource paths must remain inside the plugin package;
- protocol versions are validated;
- input and output sizes are bounded;
- Jefe does not interpolate plugin data into shell commands;
- Jefe owns ordinary confirmations for destructive actions.

A trusted plugin may itself invoke Git, `gh`, or a shell. Jefe should not pretend to police the internals of software the user deliberately installed and enabled.

## Transactional startup

The entire configured workbench should be composed as one transaction:

1. Register Jefe's built-in screens, panels, actions, keymaps, and agent types in a candidate registry.
2. Read enabled plugins in deterministic order.
3. Parse every enabled plugin manifest and referenced resource.
4. Parse user screen, keymap, and agent definitions.
5. Validate versions, IDs, namespaces, paths, duplicates, and dependencies.
6. Add every contribution to the unpublished candidate registry.
7. Validate every screen layout, panel reference, relationship cardinality, port type, cycle rule, focus reference, action argument, key conflict, and protected binding.
8. Verify required plugin executables and complete required startup handshakes.
9. Construct the initial screen instance and panel state.
10. Publish one immutable registry and start the TUI.

Any failure before publication aborts startup with actionable diagnostics. Jefe must not:

- silently skip a broken enabled plugin;
- automatically disable it;
- partially register a plugin package;
- start with a different workbench than the user configured.

If startup has already launched provider processes and a later step fails, Jefe terminates those providers before exiting.

### Runtime plugin failure

Transactional loading governs startup composition. A plugin may still fail after startup.

In that case:

- the registry remains immutable;
- provider-backed panels show a host-rendered unavailable state;
- plugin actions fail clearly;
- Jefe may expose an explicit restart/retry action;
- Jefe does not silently unload the plugin or rewrite configuration.

## No hot reload

Plugin installation, enablement, disabling, screen-definition changes, keymap changes, and agent-definition changes take effect after restart.

This avoids premature complexity around:

- replacing a focused panel;
- migrating panel-local state;
- unloading provider processes;
- changing actions during pending input sequences;
- changing terminal geometry during capture;
- reconciling persisted state with a replacement schema.

## Platform-aware protected keys

Protect semantic recovery actions rather than one universal key literal:

```rust
pub enum ProtectedAction {
    LeaveTerminalCapture,
    EmergencyExit,
}
```

At startup, Jefe selects a tested binding set for the active platform and terminal capabilities. Validation proves that every reachable terminal-capture context has an available `LeaveTerminalCapture` binding and that an emergency exit path remains reachable.

F12 is a reasonable current default, but it should not become the cross-platform invariant. macOS function-key settings, Linux terminal variants, Windows terminal encodings, and modifier reporting may differ.

User and plugin keymaps cannot shadow or remove the selected protected recovery bindings. Conflict diagnostics should report the normalized chord and platform scope.

The exact Windows/Linux/macOS chord matrix should be chosen from crossterm event tests on supported terminal environments rather than guessed in advance.

## Registries and boundaries

The immutable startup snapshot can contain separate registries:

```rust
pub struct WorkbenchRegistry {
    pub screens: ScreenRegistry,
    pub panels: PanelTypeRegistry,
    pub actions: ActionRegistry,
    pub agent_types: AgentTypeRegistry,
    pub providers: ProviderRegistry,
    pub keymaps: KeymapRegistry,
}
```

Agent types and plugins have distinct source contracts even though their resolved definitions participate in one validated workbench snapshot.

Recommended boundaries:

- **Domain:** typed IDs, agent values, action contracts, relationship specifications, and launch intent; no iocraft, filesystem, subprocess, or plugin transport.
- **Application:** screen-instance lifecycle, panel state, relationship propagation, action dispatch, reducers, and typed effect requests.
- **UI adapter:** layout resolution and rendering of host-owned controls.
- **Runtime adapters:** tmux/PTY, GitHub, persistence, agent process launch, and plugin process transport.
- **Composition root:** discovery, construction, complete validation, provider startup, and publication of the immutable registry.

Do not place a mutable registry in `AppState`.

## Revised architecture summary

```text
Jefe
├── Workbench definitions
│   ├── screens
│   ├── layouts
│   ├── panel instances
│   ├── constrained relationships
│   ├── focus rules
│   └── keymaps
│
├── Built-in panel types
│   ├── repositories
│   ├── agents
│   ├── terminal
│   ├── Preview
│   ├── Issues
│   ├── PR list/detail
│   └── Actions
│
├── Agent type definitions
│   ├── LLxprt
│   ├── Code Puppy
│   ├── Claude Code
│   └── Codex CLI
│
└── General plugins
    ├── Git merger
    ├── provider-backed panels
    ├── actions
    └── optional screens
```

The three systems resolve into one immutable startup registry, but they remain conceptually and contractually distinct.

## Remaining decisions

1. **Agent definition location:** recommended approach is built-ins plus files in a dedicated agent-definition directory, separate from general plugins.
2. **Agent mapping grammar:** catalog LLxprt, Code Puppy, Claude Code, and Codex CLI arguments before finalizing the smallest typed field-to-argv/environment grammar. Do not design a general command language.
3. **Plugin process lifetime:** use one-shot processes for actions and persistent providers only for panels requiring repeated queries or streaming progress.
4. **User screen customization:** derive or override a screen definition while retaining an immutable shipped base and its provenance.
5. **Runtime provider recovery:** provide explicit restart/retry, with automatic retry only for clearly idempotent data queries if later desired.
6. **Platform recovery bindings:** select the exact chord matrix through real crossterm tests on supported terminal environments.
7. **Public plugin panel scope:** initially expose standard list, detail, form, status, and empty models. Keep arbitrary drawing, terminal ownership, and direct domain mutation unsupported.

## Final position

Agent CLIs are configuration-driven runtime types. Plugins are optional product functionality. Screens are definition-driven workbench composition. These are adjacent systems, but they should not be collapsed into one universal plugin mechanism.

The result remains strongly typed and deliberately constrained while allowing every standard and user-defined screen to be composed dynamically at startup.

!# Configurable Workbench Architecture Readiness Review

## Verdict

The architectural direction is clean and appropriately constrained, but the complete design is **not yet implementation-ready**.

Proceed with:

1. a contract-definition and behavioral-characterization phase;
2. a deliberately smaller staged v1;
3. executable functional plugins only after the public provider protocol and compatibility kit are proven.

The strongest decisions should remain:

- immutable transactional startup registries;
- typed IDs, actions, messages, routes, ports, and effects;
- definition-driven screens with shipped standard definitions;
- one resolved geometry model;
- host-owned controls and rendering;
- constrained same-screen relationships;
- typed cross-screen navigation;
- screen-instance-local panel state;
- one serialized state store rather than an event bus;
- sparse user-authored `settings.toml`;
- argv-preserving agent launch definitions;
- trusted out-of-process plugin providers;
- no hot reload initially.

The major remaining work is to turn illustrative examples into one normative set of public contracts.

## Will the architecture be clean and maintainable?

Yes, if implemented incrementally and if public boundaries are defined before coding.

It improves maintainability by replacing several duplicated structures:

- closed `ScreenMode` rendering;
- screen-specific layout arithmetic;
- separate hit-testing geometry;
- screen-specific key match trees;
- separately maintained help and key hints;
- hard-coded agent kinds;
- direct mutations from UI, async closures, and runtime polling.

The target gives each concern one owner:

- screen composition belongs to screen definitions;
- geometry belongs to the layout resolver;
- behavior identity belongs to actions;
- key choice belongs to contextual keymaps;
- state transitions belong to reducers;
- I/O belongs to effects and adapters;
- live process state belongs to runtime managers;
- panel-local interaction belongs to screen instances;
- user policy belongs to `settings.toml`;
- operational records belong to `state.json`.

It will become unmaintainable if attempted as a single rewrite of screens, state, settings, agents, and plugins. Compatibility adapters and screen-by-screen migration are essential.

## Will it be flexible and extensible?

Yes, but it is deliberately a **workbench-composition system**, not an arbitrary UI framework or workflow scripting engine.

Users will be able to:

- add screen definitions at startup;
- enable, disable, order, and select the initial screen;
- customize supported layouts;
- instantiate registered panel types;
- connect compatible panel ports with supported relationships;
- choose immediate or explicit master-detail activation;
- remap and unbind ordinary actions by context;
- invoke typed cross-screen routes;
- add agent CLI definitions with typed fields and argv/environment mappings;
- install functional plugins that contribute actions, routes, screens, and provider-backed panels;
- configure plugins through namespaced schema-defined values;
- use the Settings UI or edit the same master TOML directly.

Users will not be able to:

- inject arbitrary iocraft components;
- draw directly into the terminal;
- own raw terminal input or PTYs;
- mutate application state directly;
- subscribe to internal reducer messages;
- create arbitrary relationship graphs or cross-screen data edges;
- inject shell templates through agent definitions;
- hot-reload registries;
- create loops, branches, schedules, and arbitrary workflow automation merely through screen configuration.

Those are appropriate v1 boundaries. A user can compose functionality already exposed by built-in panels or installed providers. Creating genuinely new behavior requires writing a provider executable against the SDK.

## Important correction: static composition is not a workflow engine

The architecture supports:

- workspaces;
- screens;
- panels;
- typed relationships;
- actions;
- routes;
- contextual bindings.

It does not yet support:

- conditional action pipelines;
- loops;
- scheduled jobs;
- branching workflows;
- compensation/rollback flows;
- arbitrary cross-screen dataflow.

Documentation should call this **workbench composition**, not user-defined workflow automation. A workflow engine can be designed separately if demonstrated use cases justify it.

## Contradictions to resolve

### Plugin trust

Earlier examples included permissions, grants, and optional hash pinning. The revised design uses a simpler and more honest trust model:

> Enabling an executable plugin means trusting it to run as the user.

V1 should not present unenforced manifest permissions as a security boundary. Operational requirements may be displayed as metadata, but they are not a sandbox.

### Artifact formats

The design examples currently use competing formats:

- TOML and JSON plugin manifests;
- TOML and JSON screen definitions;
- TOML and JSON agent definitions;
- several layout grammars.

Choose one format per artifact:

- TOML for user-authored master settings, screens, and agent definitions;
- JSON for machine-oriented plugin manifests;
- JSON Lines for process protocol messages.

Do not support duplicate authoring formats in v1.

### Namespaces

Examples use inconsistent IDs such as:

```text
core.issues.open
github.issues
code-puppy.cli
core.code-puppy
```

Define one ownership model:

- `core.*` — Jefe core;
- `github.*` — shipped first-party GitHub functionality;
- `local.*` — user-authored definitions;
- `<plugin-id>.*` — plugin-owned contributions.

IDs need a fixed grammar, case policy, length limit, reserved prefixes, and qualification rules.

### Layout grammar

Examples currently alternate among:

- Region/Split/Stack;
- rows/columns with separate regions;
- inline axis/children trees.

Select one recursive grammar, likely:

```rust
enum LayoutNode {
    Split {
        axis: Axis,
        children: Vec<LayoutChild>,
    },
    Leaf {
        region_id: RegionId,
        panel: PanelInstanceId,
    },
}
```

Each child carries fixed/fill/weight/min/max/collapse policy. Add `Stack` only when a concrete tab/stack use case is specified.

### Relationship cardinality

One incoming relationship per target port is valuable because it gives the target one controller.

The previous “one outgoing relationship per source and kind” rule is probably too restrictive. Deterministic bounded fan-out is safe:

```text
one source selection
    -> compatible detail panel A
    -> compatible detail panel B
```

Retain:

- one incoming controller per target port;
- exact port-type compatibility;
- same-screen endpoints;
- acyclic propagation;
- bounded total edge count and propagation steps.

### Plugin outcomes

Plugins must never emit internal `AppMessage` variants. Internal messages are implementation details.

Public outcomes should be stable host contracts such as:

```text
Navigate(validated route)
Refresh(resource reference)
ShowNotice
ReplacePanelModel
RequestConfirmation
```

The host translates those outcomes into private messages.

### Fail-fast recovery

Transactional fail-fast startup is appropriate because configuration selects executable plugins and defines the workbench. Silent fallback would run a different application than the user requested.

However, users need a guaranteed provider-free recovery path:

```text
jefe config path
jefe config validate
jefe config show-effective
jefe config edit
```

These commands must work without starting enabled providers.

## Contracts that must be defined before implementation

## 1. Extension taxonomy and identity

Define precisely:

- workbench definition;
- agent type definition;
- declarative extension bundle;
- functional plugin;
- provider;
- panel type;
- panel instance;
- action;
- context;
- route;
- capability.

Also define:

- canonical ID grammar;
- namespace ownership;
- duplicate and override policy;
- provenance from shipped/plugin/user layers;
- effective-value explanation.

## 2. Artifact formats and compatibility

For every persisted or public artifact, define:

- canonical format;
- machine-readable schema;
- schema version;
- compatibility rules;
- unknown field behavior;
- migration ownership;
- downgrade behavior;
- resource and nesting limits.

Version axes should be independent where appropriate:

- settings schema;
- screen schema;
- agent-definition schema;
- plugin manifest schema;
- host API;
- provider protocol;
- panel model schema;
- persisted state schema.

## 3. Screen and layout contract

Define:

- canonical `ScreenDefinition`;
- panel-instance and region uniqueness;
- sizing rules;
- impossible-layout validation;
- deterministic collapse ordering;
- tiny-terminal behavior;
- hidden-panel focus repair;
- single-panel fallback;
- terminal minimum viewport;
- resolved chrome/content rectangles;
- overlay and modal ownership.

`ResolvedLayout` must be the sole geometry source for:

- rendering;
- mouse routing;
- text selection;
- focus derivation;
- scrolling/viewports;
- PTY resizing.

## 4. Public panel SDK

The current internal component props are not public SDK models. They include internal state, theme, and selection types.

Define transport-neutral models for v1:

- list;
- detail/document;
- form;
- status/progress;
- empty/error.

Reserve terminal panels as built-in-only initially.

Each model needs:

- stable item identities;
- selection behavior;
- loading/error representation;
- pagination policy;
- text wrapping and limits;
- action affordances;
- labels/descriptions;
- validation errors;
- maximum model sizes;
- full-snapshot revision semantics.

Panel lifecycle must define construction, activation, projection, UI events, deactivation, and disposal.

## 5. Panel state

Define:

- state identity by panel type and state schema version;
- initialization and reset;
- screen-instance ownership;
- serialization eligibility;
- migration;
- invalid-state fallback;
- maximum size;
- boundary between panel state, domain cache, request state, and provider state.

Built-in panel state remains typed Rust. External state uses schema-validated tagged values, not unrestricted JSON maps throughout the store.

## 6. Typed ports and relationships

Define stable type identity across Rust and manifests, including:

- direction;
- required/optional/null behavior;
- retained value;
- update timing;
- activation semantics;
- version compatibility.

Specify `Scope`, `MasterDetail`, and `SessionTarget` behavior for:

- empty selection;
- deleted entities;
- refresh retention;
- loading and errors;
- focus side effects;
- propagation ordering.

Relationship propagation must be reducer follow-up, not callbacks or an event bus.

## 7. Actions and contexts

Define `ActionDescriptor` with:

- stable ID and owner;
- label and description;
- category;
- discoverability;
- argument schema;
- context predicate;
- applicability;
- visibility;
- availability and reason codes;
- destructive/confirmation metadata;
- handler kind.

Define `ActionInvocation` with validated arguments, source, screen/panel instance identity, and correlation ID where needed.

The context predicate language must be a small typed AST, not scripts.

## 8. Key grammar

Define:

- canonical key names and modifiers;
- Shift/case behavior;
- function and navigation keys;
- macOS/Linux aliases;
- unsupported terminal events;
- display format;
- sequence delimiter and maximum length;
- prefix ambiguity and timeout;
- cancellation and consumption;
- text-entry behavior;
- terminal-capture behavior;
- context precedence;
- explicit parent/child shadow syntax;
- default/plugin/user conflict handling;
- protected recovery validation.

Help and hints must use the same resolved binding and availability data as dispatch.

## 9. Routes, navigation, and dirty state

Define:

- distinction between `ScreenId` and `RouteId`;
- typed built-in activations;
- plugin activation schemas;
- push, replace, reuse, and Back semantics;
- navigation stack limit and overflow;
- root-screen behavior;
- local unwind precedence;
- route activation failure;
- screen-instance freshness/restoration;
- subscription behavior for suspended screens;
- activation generation and stale results.

Establish one dirty-state protocol for panels and overlays:

```text
Clean
Dirty { reason, can_save, can_discard }
```

Navigation, Back, quit, provider failure, and route replacement should all use the same host-owned Save/Discard/Cancel policy.

## 10. Master configuration

Define merge behavior for every category:

- scalar replacement;
- map merging;
- list replacement or ID-keyed patching;
- deletion/reset sentinel;
- explicit disable;
- inheritance;
- screen derivation versus patching;
- behavior when shipped bases change;
- dormant plugin data;
- unknown-field severity;
- source provenance.

Lossless editing is a v1 dependency if the Settings UI edits hand-authored TOML. The implementation must prove comment/order/unknown-table preservation and external-edit conflict handling.

## 11. Plugin package and process protocol

Before executable plugins ship, define:

- canonical package tree;
- manifest schema;
- platform/architecture naming;
- deterministic executable selection;
- install/enable/update/remove/rollback transactions;
- startup handshake;
- one-shot versus persistent provider modes;
- protocol envelope;
- request and generation IDs;
- structured errors;
- timeouts;
- cancellation;
- malformed/unknown/oversized messages;
- backpressure;
- EOF/crash behavior;
- shutdown and cleanup;
- bounded panel models;
- plugin config and migration.

The manifest remains authoritative for contribution identity. A provider handshake cannot dynamically register undeclared functionality.

## 12. Agent-definition grammar

Define:

- deterministic discovery;
- built-in and user namespace rules;
- field ID/type/scope/default/validation;
- repository-level and agent-level fields;
- conditional visibility;
- persistence representation;
- launch-signature participation;
- executable probes;
- local and remote availability.

The field-to-launch grammar must cover:

- fixed arguments;
- flags;
- flag/value pairs;
- repeated values;
- positional values;
- explicit boolean spellings;
- enum mappings;
- environment entries;
- ordering;
- prompt delivery.

No shell templates or arbitrary setup scripts.

Separate `AgentLaunchPlan` from transport execution. Local execution preserves argv directly. SSH uses exactly one audited serializer and explicit remote resolution policy.

LLxprt and Code Puppy local/remote behavior require exact golden parity tests before declarative definitions become authoritative.

## 13. Store and effects

Define:

- serialized queue behavior;
- reentrancy policy;
- queue capacity and backpressure;
- bounded synchronous follow-ups;
- reducer transition ordering;
- effect IDs and ownership;
- cancellation and retry;
- idempotency classification;
- stale-result semantic keys;
- subscription lifecycle;
- provider generations;
- persistence revisions;
- panic/error containment;
- shutdown.

Public actions and plugin outcomes translate to private messages at host adapters. Internal messages remain closed.

## 14. Diagnostics and introspection

Users cannot realistically author extensions without strong diagnostics.

Define structured diagnostics with:

- stable code;
- severity;
- source path and span;
- owner/provenance;
- effective source chain;
- suggested correction;
- secret redaction.

Provide introspection:

```text
jefe config validate
jefe config show-effective
jefe explain binding ...
jefe explain action ...
jefe explain screen ...
jefe plugin list
jefe agent-type list
```

Users should be able to determine:

- which definition won;
- where an effective value came from;
- why an action is unavailable;
- which binding won;
- which plugin contributed a screen or panel;
- why a provider failed;
- how a layout resolved.

## 15. Limits and compatibility

Specify maximums for:

- enabled plugins;
- screens;
- panels per screen;
- layout depth;
- relationships;
- bindings and sequence length;
- navigation depth;
- panel items/text/model bytes;
- protocol line size;
- queued requests;
- provider concurrency;
- progress update frequency;
- diagnostics retention;
- cache size;
- subscription rates.

Define additive versus breaking API evolution and a compatibility matrix for host API, manifest, protocol, panel models, and schemas.

## Smallest coherent v1

### 1. Stable actions and contextual keymaps

- typed action IDs and descriptors;
- context snapshots and availability;
- chord normalization;
- conflict validation;
- protected recovery;
- generated help and hints;
- preservation of existing terminal input precedence.

### 2. One geometry model and registered built-in panels

- one canonical layout grammar;
- one `ResolvedLayout`;
- current Dashboard represented as a built-in definition;
- rendering, hit testing, selection, focus, and PTY sizing consume the same geometry;
- existing controls remain implementation details behind panel adapters.

### 3. Typed navigation and screen instances

- built-in route activations;
- bounded navigation stack;
- local unwind and Back;
- dirty-state guard;
- independent panel state per screen instance;
- migrated shipped screens with behavioral parity.

### 4. Sparse lossless `settings.toml`

- versioned parser;
- precise merge and provenance;
- provider-free validate/show-effective/path commands;
- lossless edits and external-edit conflict detection;
- durable atomic save;
- Settings UI initially for theme, screen enable/order/start, and keys.

Advanced recursive layout editing can remain hand-edited initially.

### 5. Declarative user screens

Users may compose screens from registered built-in panels, supported relationships, focus rules, actions, and layouts.

Ship schemas, examples, and source-aware validation.

### 6. Agent-definition seam

Define and prove the minimal typed field/probe/argv/env grammar against LLxprt and Code Puppy. Keep their Rust launch adapters authoritative until parity is demonstrated.

### 7. Defer executable providers initially

Reserve clean public panel/action/route DTOs, but do not ship the executable plugin protocol until a real plugin—such as a Git merger—passes an external compatibility kit.

A package that only contributes screens/layout/key defaults should be called an **extension bundle**, not an executable plugin.

## What is deliberately deferred

- hot reload;
- dynamic Rust libraries;
- arbitrary drawing;
- plugin PTY ownership;
- global input interception;
- arbitrary graph relationships;
- a generic event bus;
- workflow scripting/DAGs;
- online plugin marketplace;
- signatures and automatic updates;
- security sandbox/capability broker;
- automatic provider restart loops;
- cross-screen relationships;
- persisted arbitrary panel state/navigation stacks;
- visual recursive layout designer;
- raw agent arguments;
- OS keychain support beyond an extensible secret-reference type;
- Windows support until runtime/input behavior is implemented and tested.

## Author tooling required for real extensibility

Before claiming that non-core users can extend Jefe, ship:

- machine-readable schemas;
- source-aware validation;
- effective-config and provenance inspection;
- versioned SDK/reference DTOs that exclude internal reducer types;
- complete screen, keymap, agent, and provider examples;
- compatibility test runner;
- golden protocol fixtures;
- API compatibility policy;
- packaging and rollback documentation;
- explicit resource limits;
- host/API/schema support matrix.

Example set should include:

- custom Dashboard layout;
- custom review screen;
- key override;
- local agent type;
- remote-capable agent type;
- one-shot action provider;
- persistent list/detail provider;
- plugin configuration migration;
- failure and recovery behavior.

## Go criteria

Begin staged implementation only after:

- one normative specification supersedes contradictory examples;
- taxonomy, namespaces, IDs, and provenance are fixed;
- screen/layout grammar and geometry invariants are fixed;
- public panel models and state ownership are fixed;
- ports and relationship semantics are fixed;
- action/context/availability/argument contracts are fixed;
- key chord/sequence/conflict semantics are fixed;
- route/navigation/dirty-state behavior is fixed;
- settings merge/lossless/durability behavior is fixed;
- agent launch grammar proves current local and remote parity;
- store/message/effect/subscription ordering is fixed;
- persistence ownership and migrations are fixed;
- diagnostics and resource limits are specified;
- characterization and TUI harness tests protect current behavior.

Before executable plugins ship, additionally require:

- canonical package and manifest schemas;
- complete provider protocol lifecycle;
- exact-version install/update/rollback;
- plugin configuration and migration;
- compatibility matrix;
- SDK, examples, and external compatibility kit.

## Final assessment

The design can become clean, maintainable, flexible, and extensible.

Its constraints are mostly strengths:

- users customize composition without breaking host invariants;
- plugins add behavior without receiving internal state or UI objects;
- agent CLIs become data-driven without shell-template injection;
- keymaps become customizable without losing recovery paths;
- screens become dynamic without hot-reload complexity;
- state remains typed and deterministic rather than becoming a generic JSON graph.

The architecture is not yet ready to freeze as a public SDK. The remaining work is mostly specification work at compatibility boundaries, not a need to replace the core direction.

The recommended path is a smaller v1 that establishes actions, keymaps, layout, screen instances, typed navigation, lossless configuration, declarative user screens, and agent-definition seams. Executable plugins should follow only after the provider protocol is proven by a real plugin and an external compatibility kit.

# State Management Architecture

## Recommendation

Move Jefe incrementally toward a **typed, unidirectional application loop**:

```text
input/external observation
    -> action or typed message
    -> deterministic reducer
    -> committed state
    -> explicit effects
    -> asynchronous completion message
    -> reducer
    -> render
```

This is broadly Elm/Redux-style, adapted to Rust and Jefe's existing typed domain reducers.

Do **not** introduce a generic event bus. Jefe needs one serialized typed dispatch queue, composed reducers, explicit effects, and explicit subscriptions—not string topics, arbitrary subscribers, or distributed mutation.

At startup, publish immutable `Arc<WorkbenchRegistry>` and `Arc<EffectiveConfig>`. Run one logical store whose internal state is partitioned by ownership. UI, render code, timers, GitHub tasks, runtime callbacks, and plugin providers must not mutate state directly.

## Current state is distributed

Although `AppState` is described as the source of truth, meaningful state currently lives in several places.

### `AppState`

`src/state/types.rs` combines:

- repositories and agents;
- selected repository and agent;
- installed agent kinds;
- screen identity and pane focus;
- modal, split, and grab state;
- Issues state;
- Pull Request state;
- Actions state;
- mouse selection;
- terminal snapshots, scroll, and gesture state;
- help scrolling;
- terminal viewport caches;
- a mirror of theme settings.

This is one structure, but not one coherent ownership domain. It mixes durable entities, screen-local state, transient input state, asynchronous request state, and runtime observations.

### Root iocraft hooks

`src/app_shell.rs` owns additional hook state outside `AppState`:

- quit state;
- the `AppState` hook itself;
- render ticks;
- help scroll;
- initialization flags;
- runtime-session restoration state;
- attach scheduler;
- paste-related Enter suppression.

Some of this is duplicated. Help scroll, for example, exists in hook state and in `AppState` for selection projection.

### Infrastructure context

`src/main.rs` constructs a coarse `Arc<Mutex<AppContext>>` containing:

- persistence;
- theme manager;
- tmux runtime manager;
- GitHub client.

Unrelated infrastructure services therefore share one lock.

### Process-global caches

There are additional global caches or capability snapshots:

- installed agent detection uses `OnceLock`;
- Git metadata uses a global mutex cache;
- markdown rendering uses a global render cache.

Caches are not necessarily wrong, but mutable or startup-dependent registries should not become globals as the workbench becomes configurable.

## Current event/message model

### `AppEvent`

`src/state/events.rs` contains a large historical `AppEvent` enum spanning:

- navigation;
- focus;
- screen changes;
- forms and modals;
- repository and agent operations;
- runtime lifecycle;
- persistence;
- themes;
- terminal scrollback;
- Issues, PRs, and Actions.

### `AppMessage`

`src/messages.rs` already has the better direction. `AppMessage` is partitioned into typed domains:

- UI navigation;
- modal;
- repository/agent;
- runtime;
- persistence;
- theme;
- Issues;
- Pull Requests;
- Actions;
- system.

`MessageDomain` and `MessageRoute` provide routing and tracing metadata.

### Duplicate vocabularies

`AppState::apply(AppEvent)` immediately converts the event to `AppMessage`, and input dispatch performs similar conversion. New behavior may therefore require:

- an `AppEvent` variant;
- an `AppMessage` domain variant;
- conversion arms;
- route/name metadata;
- dispatcher logic;
- reducer logic.

The long-term model should retire `AppEvent` after compatibility migration and retain one typed message vocabulary for reducers.

## Current reducer and orchestration behavior

`AppState::apply_message` in `src/state/mod.rs` is a central deterministic reducer. It routes messages to domain operations and then runs `finalize_message`, which repairs cross-cutting invariants such as repository agent IDs and selection validity.

That deterministic core is worth preserving.

The problem is that not every transition goes through it.

`src/app_input` currently combines:

- input routing;
- reducer dispatch;
- effect interpretation;
- GitHub orchestration;
- runtime calls;
- persistence;
- direct state mutations.

`dispatch_app_message` special-cases runtime, Issues, PR, and Actions behavior. `apply_and_persist` applies a transition and then saves state, while some other paths directly mutate fields before saving.

The effects already exist, but they are implicit Rust control flow rather than values returned by reducers.

## Current asynchronous state

Issues, PRs, and Actions already use a strong pattern:

- monotonic request IDs;
- typed pending descriptors;
- repository/entity/filter scope;
- completion validation against request ID and semantic scope;
- stale-response rejection.

This should remain central to the target architecture.

The weakness is that detached GitHub tasks receive a state hook and can mutate state from completion closures. Cancellation is generally logical—late results are ignored—rather than physical.

The target should have async tasks return typed completion messages to one store queue. Tasks must never hold mutable store handles.

## Current runtime state

`RuntimeManager` is already a useful infrastructure boundary. `TmuxRuntimeManager` owns:

- active sessions;
- attached viewer;
- output generations;
- history cache;
- dimensions;
- tmux option state;
- runtime liveness information.

`AppState` separately stores agent runtime binding and status. Startup reconciles persisted state with tmux/PID observations.

The ownership rule should be explicit:

- tmux/runtime manager is authoritative for live external process/session state;
- the store holds immutable observations needed for rendering and decisions;
- persisted state holds only stable recovery identifiers that are useful after restart;
- terminal snapshots, dirty flags, viewer objects, and history caches are never persisted.

## Target store structure

Use a private root assembly state:

```rust
struct StoreState {
    domain: DomainState,
    workbench: WorkbenchState,
    overlays: OverlayState,
    requests: RequestState,
    runtime: RuntimeCache,
    settings: SettingsState,
    plugins: PluginState,
    diagnostics: DiagnosticsState,
}
```

This root is not an invitation for every reducer to mutate everything.

- The store is private.
- Reducers receive only their state slice.
- Selectors expose read models.
- Reducer contexts contain explicit immutable dependencies.
- Effects receive immutable payloads, never `&mut StoreState`.

## State ownership by category

### Domain state

`DomainState` owns durable application entities:

```rust
struct DomainState {
    repositories: BTreeMap<RepositoryId, Repository>,
    agents: BTreeMap<AgentId, Agent>,
    github: GithubEntityCache,
    repository_preferences: BTreeMap<RepositoryId, RepositoryPreferences>,
}
```

Use typed IDs across async and navigation boundaries. Indexes are projections, not identities.

Avoid duplicated authoritative data. If repository-to-agent indexing is needed, derive it or update it as part of one domain transaction rather than maintaining unrelated mutable truths.

### Workbench state

```rust
struct WorkbenchState {
    navigation: NavigationStack,
    active: ScreenInstanceId,
    instances: BTreeMap<ScreenInstanceId, ScreenInstance>,
}

struct ScreenInstance {
    definition_id: ScreenId,
    activation: RouteActivation,
    focused_panel: Option<PanelInstanceId>,
    panel_states: BTreeMap<PanelInstanceId, PanelState>,
}
```

Panel-local state includes:

- selection;
- scrolling;
- filter drafts;
- activated detail;
- local viewport state;
- local notices and presentation errors;
- panel focus/subfocus.

Two instances of the same screen definition receive independent panel state.

Shared repositories, agents, PR records, and runtime sessions remain outside screen-local state.

### Overlay and input state

`OverlayState` owns transient host interaction:

- active modal or modal stack;
- confirmation state;
- chooser/form/composer draft;
- paste-related suppression;
- key-sequence state such as `qqq`;
- mouse gesture and text selection;
- one authoritative help scroll value.

Terminal capture remains a host input policy rather than an ordinary plugin context.

### Request state

Use typed slots:

```rust
struct RequestSlot<K> {
    request_id: RequestId,
    key: K,
    status: RequestStatus,
    cancellation: Option<CancellationId>,
}
```

The semantic key might contain:

- screen instance;
- panel instance;
- repository;
- Issue or PR number;
- filter/cursor;
- plugin provider generation.

Correctness always relies on matching request ID and semantic key. Physical cancellation is best effort because cancellation races.

### Runtime cache

`RuntimeCache` stores observations needed by reducers and rendering:

- observed session status by agent;
- currently attached agent;
- latest immutable terminal snapshot or generation reference;
- capabilities;
- runtime errors.

Process handles, tmux clients, pipes, viewer objects, and history implementations stay in runtime adapters.

### Settings state

```rust
struct SettingsState {
    effective: Arc<EffectiveConfig>,
    draft: Option<SettingsDraft>,
    pending_restart: bool,
}
```

The running effective configuration and workbench registry are immutable.

`SettingsDraft` owns:

- base document revision/hash;
- lossless TOML document;
- edits;
- candidate validation results;
- restart-required status.

Editing Settings never mutates the running registry piecemeal. Structural changes become effective after validation, save, and restart.

### Plugin state

The store may track:

- provider health;
- provider generation;
- pending plugin request IDs;
- bounded progress;
- host-rendered plugin view models;
- typed plugin failures.

Child processes, stdin/stdout pipes, buffers, and process supervision stay in the plugin runtime adapter.

A provider generation is part of request identity so a response from an old process cannot satisfy a request issued to a restarted process.

### Diagnostics state

Track user-visible, bounded diagnostics:

- configuration errors;
- provider health notices;
- persistence failures;
- retryable runtime failures;
- source locations where available.

Do not use diagnostics state as an unbounded in-memory log.

## Persisted versus ephemeral state

### Persist in `settings.toml`

User-authored policy and sparse overrides:

- theme;
- start screen and screen order;
- screen/layout overrides;
- keymap overrides;
- enabled exact plugin versions;
- plugin configuration;
- enabled/referenced agent definitions;
- repository/agent creation defaults.

### Persist in `state.json`

Application-owned durable records and explicitly selected UX restoration state:

- repositories;
- concrete agents;
- stable runtime recovery identifiers;
- per-repository preferences;
- selected IDs if restoration is a deliberate feature;
- named/versioned subsets of screen state if later required.

### Ephemeral by default

- active modal and chooser;
- unsaved composer text unless explicit draft restoration is added;
- loading and errors;
- request IDs;
- mouse gestures;
- terminal snapshots and history;
- render ticks;
- attach scheduler;
- process handles;
- plugin progress;
- navigation stack and panel state unless a concrete restoration requirement justifies persistence.

## Terminology

Use these terms consistently.

### Raw input

A terminal key, mouse, paste, or resize event.

### Action

A stable user-invokable semantic operation with validated arguments:

```text
core.navigation.back
core.pull_requests.open-linked-issue
com.example.git-merger.merge
```

Actions are exposed to keymaps, help, menus, Settings, and plugins.

### Message

A closed typed value delivered to a reducer:

```rust
NavigationMsg::Activate(RouteActivation)
GithubMsg::PrDetailCompleted { request, result }
RuntimeMsg::AttachFailed { agent_id, error }
```

### Effect or command

A typed request for boundary work:

```rust
Effect::Github(FetchPrDetail { ... })
Effect::Runtime(AttachAgent { ... })
Effect::Plugin(InvokeAction { ... })
Effect::Persist(CommitState { ... })
```

“Command” may name the domain payload. `Effect` is the top-level executable enum.

### External event

An observation from a timer or adapter. The adapter translates it into a typed message before it reaches the store.

Do not use “event” as another interchangeable name for actions, messages, effects, and observations.

## Reducer contract

Conceptually:

```rust
struct Transition<M, E> {
    messages: Vec<M>,
    effects: Vec<E>,
}

fn reduce(
    state: &mut SliceState,
    message: SliceMessage,
    context: &ReduceContext<'_>,
) -> Transition<Message, Effect>;
```

The root reducer:

1. routes the message to one owning slice;
2. applies the deterministic transition;
3. propagates constrained same-screen relationships where appropriate;
4. runs targeted invariant normalization;
5. commits state;
6. processes bounded synchronous follow-up messages;
7. releases store access;
8. runs effects.

Effects never execute while the store is locked or while a reducer has mutable state access.

## Why not an event bus

A generic event bus would make the architecture worse.

It introduces:

- string/topic coupling;
- unclear ownership;
- hidden subscribers;
- ambiguous ordering;
- difficult replay and testing;
- weak exhaustiveness;
- harder stale-response correlation;
- opportunities for plugins to couple to internal implementation details.

Jefe does not need publish/subscribe for core state. It needs one typed dispatch queue.

`MessageDomain` can remain useful tracing metadata. It should not become an event-bus topic system.

## Why not actors for core state

Runtime and plugin process supervisors may internally use actor-like loops. That does not mean every state slice should become an actor.

Actorizing repositories, navigation, panels, requests, settings, and runtime observations would distribute invariants across mailboxes and make ordering harder to reason about.

Core UI/domain state should remain one serialized logical store with composed reducers.

## End-to-end input flow

```text
Crossterm KeyEvent
    -> protected host input policy
    -> derive ContextSnapshot
    -> KeymapRegistry resolves ActionInvocation
    -> ActionRegistry validates and creates Message
    -> store reducer commits state and returns Effects
    -> render observes committed state
    -> effect executor performs I/O
    -> executor dispatches completion Message
    -> reducer checks request ID and semantic key
    -> state updates
    -> render
```

Protected host input policy handles before keymap lookup:

- terminal-capture escape;
- PTY forwarding;
- paste framing;
- host terminal scrollback interception.

## GitHub request example

Activating a PR list item dispatches a typed message. The reducer:

1. records the activated PR;
2. allocates a `RequestId`;
3. stores a pending request keyed by screen, panel, repository, and PR;
4. returns:

```rust
Effect::Github(FetchPrDetail {
    screen_instance,
    panel_instance,
    repository,
    pr_number,
    request_id,
})
```

The GitHub executor performs I/O and dispatches:

```rust
GithubMsg::PrDetailCompleted {
    key,
    request_id,
    result,
}
```

The reducer accepts it only when both request ID and semantic key still match. Otherwise it is a deterministic no-op.

## Navigation example

The action:

```text
core.pull_requests.open-linked-issue
```

produces a typed route activation:

```rust
NavigationMsg::Push(
    BuiltInRouteActivation::Issues {
        repository_id,
        issue_number,
    },
)
```

The navigation reducer:

1. validates the route against the immutable registry;
2. suspends the current `ScreenInstance` on a bounded stack;
3. constructs a fresh Issues instance from its definition;
4. applies typed activation values;
5. focuses the definition's initial panel;
6. returns the required GitHub fetch effects.

Back restores the exact prior PR screen instance and its panel-local state.

Cross-screen navigation is never represented as a panel relationship.

## Plugin action example

The merger action opens a host-owned confirmation. Confirm dispatches a message. The reducer records pending plugin state and emits:

```rust
Effect::Plugin(InvokeAction {
    plugin_id,
    action_id,
    request_id,
    provider_generation,
    typed_context,
})
```

The plugin executor invokes the trusted provider, validates its protocol response, and dispatches progress or completion messages.

Reducers reject responses with the wrong request ID or provider generation. Success may emit a core PR-refresh effect.

The plugin never receives:

- mutable store state;
- key routing;
- persistence access;
- iocraft components;
- PTY ownership;
- the complete master configuration.

## Registry access

Build a mutable candidate registry only during startup:

```text
core contributions
    -> enabled plugins
    -> user screen/agent definitions
    -> sparse overrides
    -> complete validation
    -> provider handshakes
    -> initial screen construction
    -> publish immutable registry
```

Inject immutable references:

```rust
Arc<WorkbenchRegistry>
Arc<EffectiveConfig>
```

Do not put mutable registries in `StoreState`, global `OnceLock`s, or a service locator. There is no runtime registration or hot reload.

## Effects and infrastructure

Replace the coarse `Arc<Mutex<AppContext>>` with narrower adapters/executors.

The store has one serialized dispatch queue. Reducers never hold infrastructure locks. Effect execution occurs after state commit and outside store access.

Adapters include:

- GitHub executor;
- runtime/tmux executor;
- persistence writer;
- plugin process executor;
- clipboard/open-URL boundary;
- timer/subscription supervisor.

Each adapter may manage its own narrow synchronization internally.

## Subscriptions

Model recurring observations explicitly:

```rust
SubscriptionId::PtyDirty(agent_id)
SubscriptionId::AgentLiveness(agent_id)
SubscriptionId::RuntimeAttachment
SubscriptionId::PluginStream(plugin_id, generation)
SubscriptionId::TerminalResize
```

Starting or stopping relevant screen/runtime state starts or stops subscriptions. Timers and streams dispatch messages; they do not mutate store state directly.

Preserve nonblocking runtime snapshot reads, but publish observations into `RuntimeCache` instead of coupling snapshot acquisition to render-time mutation.

## Persistence writer

Operational state should use one serialized writer with monotonic revisions.

```rust
Effect::Persist(CommitState {
    revision,
    snapshot,
})
```

Older completion must not overwrite or report success for a newer revision. Coalescing may skip intermediate snapshots when a newer full snapshot is queued.

For `settings.toml`:

- keep a lossless TOML document and loaded hash/revision;
- reread before saving;
- reject external-edit conflicts;
- validate the complete candidate;
- use a unique same-directory temporary file;
- sync and atomically rename;
- sync the parent directory where supported.

`state.json` and `settings.toml` are separate transactions. If one operation conceptually affects both, define ordering and recovery explicitly rather than claiming cross-file atomicity.

## Migration approach

### 1. Characterize existing behavior

Preserve with tests:

- key to reducer behavior;
- terminal capture and scrollback;
- stale GitHub request guards;
- focus restoration;
- runtime reconciliation;
- persistence boundaries;
- TUI harness behavior.

### 2. Introduce actions and effects behind current behavior

Add stable `ActionId`, contextual resolution, and typed `Effect` values without immediately rewriting every reducer.

### 3. Add the serialized store queue

Adapt current domain reducers. In tests, record effects rather than executing adapters.

Prove:

- deterministic transitions;
- effects run only after commit;
- no reducer invokes adapters;
- synchronous follow-up processing is bounded.

### 4. Route async completions through messages

Move GitHub, runtime, and persistence completion callbacks onto the store queue. Remove mutable `HookState<AppState>` from async APIs.

### 5. Add typed navigation

Introduce route activations and a navigation stack while existing screens remain built-in definitions.

### 6. Extract screen and panel state

Migrate one screen at a time into `ScreenInstance` and panel-state slices. Test independent state for two instances of the same definition.

### 7. Add immutable config and registries

Resolve the master configuration and plugin contributions transactionally, then inject immutable snapshots.

### 8. Migrate agent definitions and plugins

Replace hard-coded agent-kind branching incrementally. Add plugin providers only after the action/effect/store seam is stable.

## Anti-patterns to avoid

- No generic event bus.
- No direct mutation from UI, render code, timers, subscriptions, or async callbacks.
- No `HookState<AppState>` in application or effect APIs.
- No effects while the store or reducer state is locked.
- No untyped global `HashMap<String, Value>` state model.
- No mutable global registries or service locator.
- No partial plugin registration or hot reload.
- No persistence after every ephemeral message.
- No fixed shared temporary filename with concurrent writers.
- No list indexes as identities across async or navigation boundaries.
- No duplicated authoritative state between hooks, store, runtime manager, and settings manager.
- No plugin whole-store snapshots, direct persistence, arbitrary drawing, global input capture, or PTY ownership.

## Final position

Jefe should use messaging, but not an event bus.

The desired architecture is:

- one typed serialized message queue;
- one logical store partitioned by ownership;
- composed deterministic reducers;
- stable user/plugin actions resolved by contextual keymaps;
- explicit typed effects for I/O;
- explicit subscriptions for recurring observations;
- async completions returned as typed messages;
- request IDs plus semantic keys for stale-result safety;
- immutable startup configuration and registries;
- one authoritative owner for every piece of state.

This builds directly on the strongest parts of the current code while eliminating distributed direct mutation and duplicated state authority.

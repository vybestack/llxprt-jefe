# Plugin Authoring, Cross-Screen Navigation, and Keymaps

## Summary

The architecture should use three related but separate mechanisms:

1. **Agent type definitions** describe agent CLIs such as LLxprt, Code Puppy, Claude Code, and Codex CLI.
2. **Workbench definitions** describe screens, layouts, panels, actions, focus, contextual bindings, and constrained same-screen relationships.
3. **Functional plugins** add optional behavior such as a Git merger through registered actions, routes, provider-backed panels, and trusted executable providers.

Cross-screen behavior should not be modeled as a panel relationship. It should be a stable action that produces a typed screen activation. For example, `i` on a loaded PR detail would be the shipped default binding for `core.pull_requests.open_linked_issue`, which navigates to an Issues screen activation carrying a repository ID and issue number.

Keys are defaults attached to actions, not behavioral identities. This aligns directly with open issue [#185](https://github.com/vybestack/llxprt-jefe/issues/185), which requests configurable key layouts.

## Current key and event architecture

### Input entry and precedence

Terminal input enters through `handle_terminal_event` in `src/app_shell.rs`. Resize, fullscreen mouse, paste, and key events have separate paths. Key events ultimately reach `handle_key_event`.

The current effective precedence in `handle_key_event` is:

1. Ignore release events.
2. Suppress synthetic Enter associated with paste.
3. Handle F12 before modal or screen routing.
4. Handle global agent shortcuts, including Alt/Option+1–9.
5. Resolve `InputMode`.
6. Apply special Ctrl-C terminal passthrough behavior.
7. If terminal capture is active, intercept host scrollback keys and forward remaining keys to the PTY.
8. Dispatch active modal, confirm, theme picker, search, or form input.
9. Dispatch normal and screen-specific input.

macOS Option+digit may arrive as symbols such as `¡`, `™`, or `£`; the current code already normalizes those aliases in `src/app_input/normal.rs`.

### Input modes

`input_mode_for_state` in `src/input.rs` establishes ownership:

- modal input wins first;
- Issues resolves inline editor, chooser, search, filter, then normal mode;
- PRs resolves inline editor, choosers, search, filter, then normal mode;
- Actions resolves search, filter, then normal mode;
- terminal capture is considered after those higher-priority modes.

This priority is sound and should be retained as a context stack rather than another collection of ordered key-match functions.

### Screen-specific key dispatch

Normal dispatch is an ordered chain in `src/app_input/normal.rs`. Issues, PRs, and Actions each have separate pure key resolvers.

Current Dashboard bindings include:

```text
i -> enter Issues
p -> enter Pull Requests
g -> enter Actions
s -> enter Split/repository management
```

Inside Issues, `i` means refocus the issue list. Inside PRs, `p` means refocus the PR list. That reuse is valid because the contexts differ.

Screen wrappers currently consume unmatched keys so Dashboard or destructive actions do not leak into Issues, PRs, or Actions. A contextual resolver must preserve that ownership rule.

### Terminal capture

Terminal capture is special. While captured, almost all keys go to the child PTY except:

- F12 host escape;
- host scrollback interception;
- paste framing and synthetic-Enter handling.

Terminal capture should remain a host input policy rather than becoming an ordinary plugin key context. Plugins should not intercept arbitrary terminal input or replace the host escape.

### Events and messages

Input currently produces the historical `AppEvent` facade. Dispatch converts events into domain-scoped `AppMessage` variants for navigation, modals, repository/agent behavior, runtime, persistence, themes, Issues, PRs, Actions, and system behavior.

`dispatch_app_message` in `src/app_input/mod.rs` separates reducer-only transitions from orchestration that performs asynchronous work.

Existing Issue and PR loading already carries repository scope, entity number, and monotonic request IDs. Completion handlers reject stale responses when scope or request identity no longer matches. Typed cross-screen activation should reuse this pattern and add screen-instance/activation identity rather than inventing a separate concurrency mechanism.

### Duplicated key knowledge

There is no action registry today. Key behavior is duplicated across:

- hard-coded key-to-event match arms;
- `keybind_hints_for` in `src/ui/components/keybind_bar.rs`;
- help content in `src/ui/modals/help.rs`;
- literal prompts embedded in detail and chooser content.

This is why configurable keymaps should be built around stable actions. Input dispatch, help, bottom-bar hints, Settings, and plugin action placement can all use the same action metadata and resolved binding.

## Existing configurable-key issue

Open issue [#185 — Configurable key layouts + versioned settings.json; turn theme dialog into a Settings dialog](https://github.com/vybestack/llxprt-jefe/issues/185) requests:

- current bindings as defaults;
- user mappings from keys to functions/commands;
- partial overrides falling back to defaults;
- same-context conflict validation;
- centralized lookup used by handlers and key hints;
- versioned user-editable settings;
- Keys and Theme in a Settings dialog;
- parse, merge, conflict, and migration tests.

There is no open, closed, or merged PR implementing issue 185.

Related merged precursors include:

- [PR #63](https://github.com/vybestack/llxprt-jefe/pull/63), which added isolated `--config` directories;
- [PR #114](https://github.com/vybestack/llxprt-jefe/pull/114), which demonstrates the current multi-site work required for one Issues binding;
- [PR #130](https://github.com/vybestack/llxprt-jefe/pull/130), which centralized quit policy around Ctrl-Q and `qqq` but still hard-coded the binding.

Issue 185 should be implemented as part of the action registry, not as a lookup table that maps keys directly to `AppEvent` variant names.

## Actions and contextual keymaps

### Stable actions

Keys should resolve to stable action IDs:

```rust
pub struct ActionId(pub String);
pub struct ContextId(pub String);
pub struct RouteId(pub String);

pub struct ActionDefinition {
    pub id: ActionId,
    pub label: String,
    pub description: String,
    pub contexts: Vec<ContextId>,
    pub default_bindings: Vec<Binding>,
    pub availability: AvailabilityId,
    pub handler: ActionHandlerId,
}

pub enum ActionEffect {
    None,
    Message(AppMessage),
    Navigate(ScreenActivation),
    Back,
}
```

Examples:

```text
Dashboard i  -> core.issues.open
Dashboard p  -> core.pull_requests.open
PR detail i  -> core.pull_requests.open_linked_issue
Issues i     -> core.issues.refocus_list
PRs p        -> core.pull_requests.refocus_list
```

The same chord can map to different actions in disjoint contexts. Tests and reducers should assert action/effect semantics rather than keyboard letters.

### Context resolution

Build a context stack from current state, most specific first:

1. protected host input policy;
2. active modal/editor/chooser/search/filter;
3. focused control or panel;
4. active screen;
5. application-wide context.

Resolve one chord against that stack and stop at the first applicable action. Text-entry contexts consume printable input before ordinary action lookup except for explicitly registered editor commands.

### Generated help and hints

The action registry and effective keymap should generate:

- contextual bottom-bar hints;
- Settings > Keys rows;
- global and contextual help;
- command/action menus;
- plugin action listings;
- conflict diagnostics.

Content should refer to an action ID and ask the host to render the current binding rather than embedding text such as “Press c.”

### User settings

A representative settings shape is:

```json
{
  "version": 2,
  "keymap": {
    "core.dashboard": {
      "core.issues.open": ["i"],
      "core.pull_requests.open": ["p"]
    },
    "core.pull_requests.detail": {
      "core.pull_requests.open_linked_issue": ["i"]
    }
  }
}
```

Resolution order:

1. shipped defaults;
2. plugin defaults;
3. user overrides.

An override replaces the bindings for one action in one context. Omitted actions retain defaults.

Conflict rules:

- same chord and same effective context for two actions is an error;
- the same chord in disjoint contexts is valid;
- parent/child context shadowing must be explicit rather than accidental;
- plugin defaults cannot silently replace shipped defaults;
- user changes validate as a complete candidate before atomic persistence.

Unknown action IDs may be retained as dormant settings for temporarily unavailable plugins, but they should produce a warning while unavailable.

### Platform-aware bindings

Normalize input into a canonical `KeyChord` while retaining platform aliases for parsing and display.

Reject bindings that the current platform/terminal cannot reliably report. Protected recovery semantics include:

```rust
pub enum ProtectedAction {
    LeaveTerminalCapture,
    EmergencyExit,
}
```

F12 can remain the current macOS/Linux default for leaving terminal capture, but the invariant is that a tested recovery chord exists, not that F12 is universal. Windows, Linux, and macOS binding sets can differ based on actual crossterm behavior.

Host paste mechanics and scrollback interception are policies rather than user/plugin-remappable actions.

## Cross-screen navigation

### Routes, not relationships

Panel relationships remain constrained and same-screen only. Cross-screen behavior uses actions and typed route activation.

A screen definition declares its activation schema:

```rust
pub struct ScreenDefinition {
    pub id: RouteId,
    pub title: String,
    pub activation: ActivationSchema,
    pub panels: Vec<PanelDefinition>,
    pub relationships: Vec<SameScreenRelationship>,
    pub actions: Vec<ActionId>,
}
```

Built-in routes should use typed activation enums:

```rust
pub enum ScreenActivation {
    Dashboard(DashboardActivation),
    Issues(IssuesActivation),
    PullRequests(PullRequestsActivation),
    Actions(ActionsActivation),
    Settings(SettingsActivation),
    Plugin(ValidatedPluginActivation),
}

pub enum IssuesActivation {
    Browse {
        repository: Option<RepositoryId>,
    },
    SelectIssue {
        repository: RepositoryId,
        issue_number: IssueNumber,
    },
}

pub enum PullRequestsActivation {
    Browse {
        repository: Option<RepositoryId>,
    },
    SelectPullRequest {
        repository: RepositoryId,
        pr_number: PullRequestNumber,
    },
}
```

Built-in routes should not use an untyped string/value parameter map.

Plugin routes declare a small manifest schema with required and optional values of known types such as string, integer, boolean, repository ID, issue number, or PR number. The host validates those values before creating `ValidatedPluginActivation`.

### PR detail to linked issue

The stable action is:

```text
core.pull_requests.open_linked_issue
```

Its shipped default binding in PR detail context may be `i`.

When exactly one linked issue is available, the action yields:

```rust
ActionEffect::Navigate(ScreenActivation::Issues(
    IssuesActivation::SelectIssue {
        repository: linked.repository_id,
        issue_number: linked.issue_number,
    },
))
```

Before changing screens, navigation validates:

1. the Issues route exists and is enabled;
2. the repository exists in Jefe;
3. the configured GitHub owner/repository matches the link target;
4. the issue number is valid;
5. no unsaved modal/editor state needs confirmation.

After validation:

1. push the current PR screen instance onto a bounded navigation stack;
2. activate an Issues instance immediately in a loading state;
3. select the target repository;
4. fetch the issue by exact number rather than relying on current list pagination or filters;
5. show loaded detail and select its list row if present;
6. if filtered out, retain a pinned out-of-filter row and offer to clear the filter rather than silently changing persisted filters.

The request carries screen-instance ID, repository ID, issue number, activation generation, and request ID. Late responses are ignored when any identity no longer matches.

### Back navigation

Replace the current one-level “exit to Dashboard” assumption with a bounded navigation stack of screen instances.

`Back` behavior should be:

1. unwind local UI first—composer, chooser, search, filter, or detail subfocus;
2. if nothing local remains to unwind, pop the prior screen instance;
3. restore its panel selection, scroll, filter, activated detail, and focus;
4. refresh only if its freshness policy requires it;
5. if the stack is empty, use the root screen policy, normally Dashboard or no-op.

Quit remains a distinct action.

This supports:

```text
PR detail
  -> linked Issue
      -> Back
          -> exact prior PR detail and subfocus
```

### Linked-issue edge cases

- **No linked issue:** hide or disable the action; a stale invocation shows a non-blocking notice without navigation.
- **One linked issue:** navigate directly.
- **Several linked issues:** show a host-rendered chooser with repository, number, and title; do not guess.
- **Ambiguous textual references:** prefer explicit GitHub linkage/closing metadata rather than every `#123` in prose.
- **Cross-repository link:** allow only when the target resolves to a configured Jefe repository.
- **Missing configured repository:** show a clear error and do not partially switch screens.
- **Deleted/private/inaccessible issue:** activate the validated route and show a retryable fetch error with Back available.
- **Filter excludes target:** pin the exact target without silently mutating persisted filters.
- **Async race:** reject stale responses by screen instance and activation generation.

## Functional plugin authoring

### Package structure

A Git merger package could be distributed as:

```text
jefe-git-merger/
├── plugin.json
├── bin/
│   ├── jefe-git-merger-darwin-arm64
│   ├── jefe-git-merger-darwin-x86_64
│   ├── jefe-git-merger-linux-x86_64
│   └── jefe-git-merger-linux-aarch64
├── screens/
│   └── merge.json
├── assets/
│   └── README.txt
└── LICENSE
```

Agent type definitions are not included. They use a separate loader and directory.

### Manifest

A representative `plugin.json` is:

```json
{
  "manifest_version": 1,
  "id": "com.example.git-merger",
  "version": "1.0.0",
  "host_api": "1",
  "executable": {
    "darwin-arm64": "bin/jefe-git-merger-darwin-arm64",
    "darwin-x86_64": "bin/jefe-git-merger-darwin-x86_64",
    "linux-x86_64": "bin/jefe-git-merger-linux-x86_64",
    "linux-aarch64": "bin/jefe-git-merger-linux-aarch64"
  },
  "actions": [
    {
      "id": "com.example.git-merger.merge",
      "label": "Merge branch",
      "contexts": [
        "core.dashboard.repository",
        "com.example.git-merger.screen"
      ],
      "default_bindings": []
    }
  ],
  "routes": [
    {
      "id": "com.example.git-merger.merge",
      "parameters": {
        "repository": {
          "type": "repository_id",
          "required": true
        },
        "source_branch": {
          "type": "string",
          "required": false
        }
      },
      "screen": "screens/merge.json"
    }
  ]
}
```

The manifest declares metadata and contributions, not arbitrary shell commands.

### Provider protocol

A plugin executable implements a small versioned request/response protocol:

```rust
trait FunctionalPlugin {
    fn handshake(
        &mut self,
        request: Handshake,
    ) -> Result<Registration, PluginError>;

    fn activate(
        &mut self,
        request: ActivateRoute,
    ) -> Result<ViewModel, PluginError>;

    fn invoke(
        &mut self,
        request: InvokeAction,
    ) -> Result<PluginResponse, PluginError>;

    fn event(
        &mut self,
        request: HostEvent,
    ) -> Result<ViewModelPatch, PluginError>;
}
```

Use versioned JSON lines over stdin/stdout. Reserve stderr for diagnostic logging. Requests carry correlation IDs and bounded payloads.

For a simple action, prefer a one-shot process. Use a persistent provider only when a panel requires repeated queries or streamed progress.

### Host-rendered UI

The plugin returns constrained models:

```rust
pub enum PluginPanelView {
    List(ListModel),
    Detail(DetailModel),
    Form(FormModel),
    Status(StatusModel),
    Progress(ProgressModel),
    Empty(EmptyModel),
}
```

Jefe owns:

- rendering;
- layout;
- focus;
- key resolution;
- modal behavior;
- confirmation dialogs;
- application state transitions;
- PTY and raw terminal ownership.

Plugins cannot mount arbitrary iocraft components, capture global input, enter raw terminal mode, or attach a PTY.

### Git merger usage

A merger action could receive:

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
    -> Jefe shows host confirmation
    -> Jefe starts or calls the trusted provider
    -> typed request is written to stdin
    -> plugin performs Git/GitHub work
    -> plugin reports progress/result
    -> Jefe applies the result
    -> Jefe refreshes affected PR/repository data
```

The plugin does not receive all `AppState` and does not directly control a core PR detail panel. It receives only the context declared by its action contract.

## Installation and discovery

### Platform config roots

Use the existing Jefe config-root convention:

- **macOS:** `~/Library/Application Support/jefe`
- **Linux:** `${XDG_CONFIG_HOME:-~/.config}/jefe`
- **Explicit config:** `jefe --config <dir>` uses `<dir>` as the isolated root.

Recommended structure:

```text
<config>/
├── settings.json
├── plugins/
│   ├── installed/
│   │   └── com.example.git-merger/
│   │       └── 1.0.0/
│   │           ├── plugin.json
│   │           ├── bin/
│   │           └── screens/
│   └── staging/
└── agent-types/
    ├── llxprt.json
    └── code-puppy.json
```

Discovery scans only exact installed locations selected by settings. It should not search arbitrary working directories or execute similarly named programs found on PATH.

### Installation command

Provide:

```text
jefe plugin install ./jefe-git-merger.tar.gz
```

Installation flow:

1. extract into `<config>/plugins/staging/<random>`;
2. validate archive paths and reject traversal or escaping symlinks;
3. validate manifest/API compatibility and select the current platform binary;
4. show plugin ID, version, executable, and that it runs as the user;
5. treat the user's install/enable confirmation as the trust decision;
6. atomically rename the validated package into `installed/<id>/<version>`;
7. update settings only after installation succeeds;
8. require restart before use.

The plugin executable runs with the user's operating-system permissions and is not sandboxed. The process boundary exists for protocol stability, crash isolation, cancellation, and language independence—not to make malicious code safe.

### Enablement settings

A representative setting is:

```json
{
  "version": 2,
  "plugins": {
    "enabled": {
      "com.example.git-merger": "1.0.0"
    }
  }
}
```

Selecting an exact version makes startup deterministic and permits a failed update to leave the previously installed version intact.

### Transactional startup

All enabled plugins and workbench definitions load as one transaction:

1. resolve exact enabled plugin IDs and versions;
2. discover manifests and current-platform executables;
3. parse and validate all resources;
4. start required providers and complete bounded handshakes;
5. build a candidate registry containing shipped and plugin contributions;
6. validate unique IDs, actions, routes, activation schemas, screen definitions, same-screen relationships, key conflicts, and prohibited capabilities;
7. construct the initial screen instance;
8. publish the immutable registry only after every check succeeds;
9. on any failure, terminate providers started during the attempt and abort startup with actionable diagnostics.

There is no degraded “skip the broken plugin” mode and no partial registration. There is no hot reload; install, update, enable, disable, or definition changes require restart.

### Runtime provider failure

If a provider fails after successful startup:

- retain the immutable registry;
- render its panels as unavailable;
- fail its actions clearly;
- optionally offer an explicit restart/retry action;
- do not silently unload it or rewrite configuration.

## Plugin actions and navigation

Plugin actions enter the same contextual action registry as built-in actions. Plugin action and route IDs are namespaced by plugin ID.

A plugin action may:

- emit a typed application message allowed by its contract;
- navigate to one of its routes using manifest-validated parameters;
- request navigation through an exposed core typed route constructor;
- display host-rendered forms, confirmations, progress, or notices.

It may not establish relationships between a panel on one screen and a panel on another screen. Cross-screen behavior always goes through navigation.

Saved key overrides for a disabled plugin remain dormant and may reactivate when a compatible version of the plugin is enabled again.

## Recommended implementation order

1. Add action IDs, contexts, default bindings, conflict validation, and generated help/hints.
2. Migrate Dashboard `i` and `p` to actions while preserving behavior, then migrate the remaining screen and modal key resolvers.
3. Introduce typed screen activations and a navigation stack.
4. Implement exact Issue/PR activation with request and screen-instance identity.
5. Add PR-to-linked-Issue navigation and Back restoration.
6. Move all shipped screens through screen definitions with same-screen relationship validation.
7. Add plugin manifest validation, installation, discovery, and transactional startup.
8. Add the versioned executable provider protocol and host-rendered plugin view models.

## Final position

- `i` and `p` are shipped default bindings, not navigation semantics.
- Stable actions are the seam shared by configurable keys, generated help, plugins, tests, and navigation.
- Cross-screen behavior is a typed route activation, never a panel relationship.
- Built-in routes use typed Rust activation values; plugin routes use constrained validated schemas.
- Functional plugins are trusted executable providers with host-rendered UI.
- Agent CLI definitions remain a separate declarative mechanism.
- macOS and Linux use their standard Jefe config roots, with explicit `--config` isolation preserved.
- All enabled plugins load transactionally at startup, and changes require restart.

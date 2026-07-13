# Master Configuration and Settings UI

## Recommendation

Use **`settings.toml` as the single authoritative user configuration file**. Expand it from its current small preference file into a versioned sparse-override document.

Keep **`state.json` as application-owned operational/domain state**. Repositories, concrete agent instances, selection, scroll positions, drafts, and runtime bookkeeping should not be mixed into the hand-authored master configuration.

At startup, Jefe resolves:

1. shipped defaults;
2. enabled plugin contributions and defaults;
3. referenced user screen and agent-type definitions;
4. user overrides from `settings.toml`.

It validates the entire candidate configuration and publishes one immutable `EffectiveConfig` and workbench registry. If any enabled plugin, definition, keymap, route, layout, or plugin configuration is invalid, startup fails rather than silently running a different configuration.

Jefe should ship a definition-driven `core.settings` screen. The UI edits the same master file that users can edit by hand. Plugin configuration UI is generated from typed plugin-provided schemas and rendered with Jefe-owned controls.

## Relevant current architecture and GitHub work

### Current persistence

`src/persistence/mod.rs` currently defines:

- `settings.toml`, containing schema version, theme, and agent-terminal theme override;
- `state.json`, containing repositories, agents, selection, focus, and per-repository preferences.

Settings use TOML; operational state uses JSON. Both use temporary-file-plus-rename persistence, but current settings serialization canonicalizes the entire file and therefore does not preserve comments or formatting.

The current theme picker is already a useful Settings precedent:

- pure view projection;
- host-rendered controls;
- live preview;
- Enter saves;
- Esc restores the prior value;
- malformed existing settings are not overwritten.

### Relevant issues and PRs

- Open issue [#185 — Configurable key layouts + versioned settings.json; turn theme dialog into a Settings dialog](https://github.com/vybestack/llxprt-jefe/issues/185) requests sparse key overrides, conflict handling, centralized key lookup, and a Settings UI.
- Open issue [#137 — Switch default config/state to platform-standard directories](https://github.com/vybestack/llxprt-jefe/issues/137) tracks unresolved default-path and migration decisions while preserving `--config` isolation.
- Closed issue [#136](https://github.com/vybestack/llxprt-jefe/issues/136), implemented by merged [PR #174](https://github.com/vybestack/llxprt-jefe/pull/174), introduced the current theme picker/configuration surface.
- Closed issue [#179](https://github.com/vybestack/llxprt-jefe/issues/179), implemented by merged [PR #199](https://github.com/vybestack/llxprt-jefe/pull/199), added the terminal-theme override toggle.
- Closed issue [#62](https://github.com/vybestack/llxprt-jefe/issues/62), implemented by merged [PR #63](https://github.com/vybestack/llxprt-jefe/pull/63), added isolated `--config DIR` behavior.
- Closed issue [#65](https://github.com/vybestack/llxprt-jefe/issues/65), implemented by merged [PR #83](https://github.com/vybestack/llxprt-jefe/pull/83), added fail-fast behavior for unusable explicit configuration directories.

No current issue or PR implements general functional-plugin loading or plugin-owned Settings schemas. No PR currently implements issue 185.

## Why TOML

Keep TOML even though issue 185 calls the proposed file `settings.json`.

Reasons:

1. Jefe already exposes and documents `settings.toml`.
2. TOML is readable for sparse tables keyed by screen, context, action, plugin, and agent-type IDs.
3. TOML supports comments; JSON does not.
4. Retaining it avoids a needless filename and format migration.
5. JSON remains appropriate for application-owned state, plugin manifests, and process protocol messages.

Do not put the master configuration in `state.json`. That file is application-owned and frequently rewritten.

## Master file semantics

`settings.toml` is a **sparse override file**, not a complete materialization of every default.

- Missing file is valid and means “use shipped defaults.”
- Missing field means “inherit the shipped or plugin default.”
- An explicit value replaces the inherited value.
- An empty key-binding list explicitly unbinds an action in that context.
- Unknown Jefe-owned fields are preserved and warned about for forward compatibility.
- Unknown fields in an enabled plugin’s config are errors because the exact plugin schema is available and the key is probably a typo.
- Configuration for disabled or uninstalled plugins is preserved as dormant data and not validated until that plugin/version is enabled.
- Relative definition paths resolve from the directory containing `settings.toml`.
- Referenced screen and agent definitions participate in the same all-or-none startup validation transaction.

## Representative master configuration

```toml
schema_version = 2

[theme]
active = "github-dark"
apply_to_agents = false

[ui]
start_screen = "core.dashboard"
screen_order = [
  "core.dashboard",
  "core.repositories",
  "github.issues",
  "github.pull-requests",
  "github.actions",
  "core.settings",
]

# Omitted screen properties inherit the shipped definition.
[ui.screens."github.actions"]
enabled = true

# This replaces only the shipped Dashboard layout property.
[ui.screens."core.dashboard"]
layout = { axis = "row", children = [
  { panel = "repositories", weight = 1, min = 20 },
  { axis = "column", children = [
    { panel = "agents", weight = 1, min = 8 },
    { panel = "terminal", weight = 3, min = 12 },
  ] },
] }

# Optional user-authored screen definitions.
[ui.screen_definitions."local.review-board"]
source = "definitions/screens/review-board.toml"
expected_version = "1"

# Context -> action -> complete replacement binding list.
# Omission inherits defaults. [] explicitly unbinds.
[keymap."core.dashboard"]
"core.issues.open" = ["i"]
"core.pull-requests.open" = ["p"]
"core.settings.open" = ["f9"]

[keymap."github.pull-requests.detail"]
"core.pull-requests.open-linked-issue" = ["i"]
"com.example.git-merger.merge" = ["ctrl+m"]

[keymap."github.issues.list"]
"core.issues.clear-filters" = []

# Exact versions make startup deterministic.
[plugins.enabled]
"com.example.git-merger" = "1.2.0"

# Plugin-owned namespace, validated by that exact plugin's schema.
[plugins.config."com.example.git-merger"]
schema_version = 3
strategy = "squash"
confirm = true
max_parallel = 2
repository_pattern = "vybestack/*"
api_token = { env = "GITHUB_TOKEN" }

[agent_types]
enabled = ["core.llxprt", "core.code-puppy", "local.codex"]

[agent_types.definitions."local.codex"]
source = "definitions/agent-types/codex.toml"
expected_version = "1"

# Applied when a new repository is created. Existing repositories are not
# retroactively rewritten.
[repository_defaults]
work_root = "~/src"
agent_type = "core.llxprt"

[repository_defaults.agent."core.llxprt"]
profile = "default"
sandbox_enabled = true

[repository_defaults.agent."core.code-puppy"]
model = "default"
yolo = false
```

Routine users can ignore the recursive layout form. The Settings UI should expose common operations such as enable, order, and reset, while advanced recursive layout editing can initially remain a hand-edited capability.

Do not introduce:

- arbitrary expression languages;
- embedded scripts;
- raw shell commands;
- arbitrary graph relationships;
- plugin-owned terminal UI.

## Typed master model

Conceptually:

```rust
struct UserConfigV2 {
    schema_version: u32,
    theme: ThemeConfig,
    ui: UiOverrides,
    keymap: BTreeMap<ContextId, BTreeMap<ActionId, Vec<KeyChord>>>,
    plugins: PluginSelection,
    agent_types: AgentTypeSelection,
    repository_defaults: RepositoryDefaults,
}

struct ThemeConfig {
    active: ThemeId,
    apply_to_agents: bool,
}

struct UiOverrides {
    start_screen: Option<ScreenId>,
    screen_order: Option<Vec<ScreenId>>,
    screens: BTreeMap<ScreenId, ScreenOverride>,
    screen_definitions: BTreeMap<ScreenId, DefinitionRef>,
}

struct PluginSelection {
    enabled: BTreeMap<PluginId, ExactVersion>,
    config: BTreeMap<PluginId, DynamicConfigTable>,
}

struct AgentTypeSelection {
    enabled: Vec<AgentTypeId>,
    definitions: BTreeMap<AgentTypeId, DefinitionRef>,
}
```

Screen relationship overrides remain limited to the established same-screen `scope`, `master_detail`, and `session_target` relationships. Cross-screen behavior remains a typed action/route activation.

## What stays in `state.json`

Keep application-owned mutable data out of the master file:

- repositories and concrete agents;
- selected repository, agent, Issue, PR, and Action;
- scroll positions and focus;
- filters and drafts where persistence is desired;
- runtime sessions and liveness bookkeeping;
- navigation history if later persisted;
- transient diagnostics and migration bookkeeping.

Only defaults used when creating new repositories or agents belong in `settings.toml`.

Moving concrete repositories and agents into the master file would mix policy with mutable domain records and cause normal state saves to rewrite a user-authored file.

## Configuration layers

Maintain three representations:

### `ShippedDefaults`

Compiled-in standard screens, layouts, actions, core key bindings, agent types, themes, and repository defaults. These are not copied wholesale into the user file.

### `UserConfigDocument`

A lossless TOML syntax tree plus parsed sparse overrides and source locations.

### `EffectiveConfig`

The fully typed, immutable, validated result used by the running application.

Merge order:

1. core shipped definitions and defaults;
2. enabled plugins in deterministic plugin-ID/exact-version order;
3. referenced user definitions;
4. user sparse overrides.

User key mappings may replace core or plugin defaults except protected recovery policies.

## Preserve comments and hand edits

Because this is a master file intended for hand editing, the Settings UI must preserve:

- comments;
- table ordering;
- unknown forward-compatible fields;
- dormant plugin configuration;
- formatting outside the edited nodes where practical.

The current typed TOML serializer cannot do that. Implementation will need a lossless TOML document editor, likely `toml_edit`, after verifying and adding it to the project dependencies.

Settings should retain a hash of the loaded document. Before saving, reread it and compare the hash. If an external editor changed the file, refuse to overwrite and offer Reload/Review.

Save flow:

1. validate the full candidate configuration;
2. patch only the edited syntax-tree paths;
3. write to a unique same-directory temporary file;
4. flush and sync;
5. atomically rename;
6. sync the parent directory where supported;
7. leave the original unchanged on any failure.

Canonical rewriting is acceptable only for a newly created file or an explicit `jefe config format` command.

## Plugin configuration schemas

A plugin should declare its configurable options in its manifest. Jefe interprets the schema and renders host controls. The plugin does not supply arbitrary Settings UI.

Example:

```json
{
  "manifest_version": 1,
  "id": "com.example.git-merger",
  "version": "1.2.0",
  "host_api": "1",
  "config": {
    "schema_version": 3,
    "fields": [
      {
        "key": "strategy",
        "label": "Merge strategy",
        "description": "Strategy used when none is supplied by an action.",
        "type": "enum",
        "options": [
          { "value": "merge", "label": "Merge commit" },
          { "value": "squash", "label": "Squash" },
          { "value": "rebase", "label": "Rebase" }
        ],
        "default": "squash",
        "required": true,
        "restart": "provider"
      },
      {
        "key": "confirm",
        "label": "Confirm destructive merges",
        "type": "boolean",
        "default": true,
        "restart": "none"
      },
      {
        "key": "max_parallel",
        "label": "Maximum parallel operations",
        "type": "integer",
        "default": 2,
        "minimum": 1,
        "maximum": 8,
        "visible_when": {
          "field": "confirm",
          "equals": true
        },
        "restart": "provider"
      },
      {
        "key": "api_token",
        "label": "API token environment variable",
        "type": "secret_ref",
        "required": false,
        "restart": "provider"
      }
    ]
  }
}
```

## Plugin option types

V1 should support only controls with clear TOML representations:

- `boolean` — checkbox;
- `string` — single-line text;
- `integer` or `number` — validated numeric input;
- `enum` — chooser/cycle control with explicit values and labels;
- `path` — text input with file/directory policy and config-relative resolution;
- `string_list` — host-owned list editor;
- `secret_ref` — environment-variable reference, never a revealed secret value.

Field metadata may include:

- `required`;
- `default`;
- `description`;
- range or length constraints;
- enum options;
- restart impact;
- limited conditional visibility.

Conditional visibility in v1 should be limited to equality or inequality against one sibling scalar field. Do not allow scripts, arbitrary expressions, or plugin callbacks to control Settings rendering.

## Plugin configuration validation

Validation occurs in two stages.

### Host schema validation

The host checks:

- types;
- required fields;
- option membership;
- ranges and lengths;
- path policy;
- unique field keys;
- visibility dependencies;
- unknown fields;
- schema version.

### Provider semantic validation

The trusted plugin provider may validate combinations that cannot be represented by generic field constraints. It receives only its typed namespaced configuration, not the raw TOML document or unrelated configuration.

A rejection from an enabled plugin is a startup/candidate-validation failure with a source path such as:

```text
plugins.config."com.example.git-merger".strategy
```

Diagnostics include plugin ID and exact version but redact secrets.

## Secrets

Do not invent cross-platform keychain integration in v1, and do not encourage plaintext secret values in the master file.

Use references:

```toml
api_token = { env = "GITHUB_TOKEN" }
```

Settings displays:

- the environment-variable name;
- whether it is currently set;
- never its value.

The provider receives a resolved typed secret value during startup or invocation, and logs/diagnostics must redact it. A future `SecretRef` variant could support an OS keychain without changing plugin schema semantics.

## Plugin configuration sent to providers

Conceptually:

```rust
struct ProviderConfigRequest {
    protocol_version: u32,
    plugin_id: PluginId,
    plugin_version: ExactVersion,
    config_schema_version: u32,
    values: BTreeMap<String, ConfigValue>,
}
```

Only the plugin’s validated namespace is sent.

- Persistent providers receive config during handshake.
- One-shot actions receive config alongside typed action context.
- Providers do not parse `settings.toml` themselves.
- Config is not pushed live because there is no hot reload.

## Plugin configuration versioning

- The master `schema_version` governs Jefe-owned syntax.
- Every plugin config namespace records that plugin’s config schema version.
- Exact plugin versions remain installed side by side.
- Selecting an upgrade first validates current config against the target schema.
- If migration is required, the trusted target provider may implement a versioned `migrate-config` operation.
- Jefe validates the returned typed configuration, shows a redacted diff, and saves it only with explicit confirmation.
- Migration failure leaves the selected version and master file unchanged.
- Startup must not silently rewrite plugin configuration.

Avoid a general declarative migration language in v1.

## Settings screen

Ship `core.settings` as a standard definition-driven full screen rather than a small modal. Keys, Plugins, layouts, and diagnostics need list/detail space.

The screen uses draft semantics: Back with unsaved changes offers Save, Discard, or Cancel.

Recommended sections:

### General

- active configuration and state paths;
- current platform;
- start screen;
- repository creation defaults.

### Appearance

- current theme list;
- existing live preview behavior;
- apply-theme-to-agent-terminal toggle.

### Screens and Layout

- enable/disable screens;
- reorder screens;
- select the start screen;
- reset a screen to shipped defaults;
- validate layout overrides;
- initially direct advanced recursive layout editing to Edit File rather than building a complex visual layout designer.

### Keys

A filterable action list grouped by context, showing:

- action label and stable ID;
- owning source: core, plugin, or user;
- inherited/default bindings;
- effective user bindings;
- Add, Replace, Unbind, and Reset to Default;
- immediate conflict diagnostics;
- protected-action status.

Plugin default mappings are suggestions. Users can map plugin actions differently or unbind them.

### Plugins

For every installed package:

- plugin ID and exact versions;
- enabled version;
- current platform executable;
- notice that it runs as the user;
- contributed screens, actions, and default bindings;
- schema-generated configuration fields;
- validation status;
- Enable, Disable, Switch Version, Remove, and Delete Saved Configuration actions;
- restart requirement.

### Agent Types

Separate from Plugins:

- enabled built-in and referenced definitions;
- executable detection status;
- definition validation;
- repository/agent fields and defaults.

### Diagnostics

- master and state paths;
- `--config` isolation status;
- effective source chain;
- dormant plugin namespaces;
- validation errors with source spans;
- pending restart-required changes.

## Settings draft, validation, and save

All edits occur in `SettingsDraft`. They do not mutate the active immutable registry.

- Theme changes may preview in memory; Cancel restores the pre-open theme.
- Validate builds the complete candidate `EffectiveConfig` and registry without publishing it.
- Save validates and atomically patches the master file.
- Cosmetic theme changes may apply to the current process after a successful save.
- Plugin, agent-definition, screen/layout, and keymap changes remain pending until restart.
- Save and Exit persists the file and exits cleanly.
- Do not implement self-exec/restart machinery in v1.
- Save failure leaves active runtime configuration and the original file unchanged.

## Plugin key defaults and user remapping

Plugin actions participate in the same action registry as core actions.

Merge order for bindings:

1. core defaults;
2. plugin defaults;
3. user overrides.

Rules:

- a plugin’s default binding cannot silently displace a core binding;
- enabling a plugin previews conflicts in Settings before updating the master file;
- the user can resolve conflicts by remapping or explicitly unbinding either ordinary action;
- protected recovery actions cannot be unbound or shadowed;
- same chord in the same effective context is an error;
- same chord in disjoint contexts is valid;
- parent/child context shadowing must be explicit;
- dormant plugin key overrides remain preserved while a plugin is disabled.

The action registry remains the source for:

- dispatch;
- Settings rows;
- help;
- bottom-bar hints;
- command/action menus;
- plugin action listings.

## Transactional startup

Startup order:

1. resolve configuration root and state paths;
2. when `--config DIR` is supplied, isolate settings, state, plugins, definitions, and themes under that root;
3. parse the master document; missing is valid, malformed or unsupported is fatal;
4. load shipped screens, panels, actions, key defaults, themes, and agent types into a candidate registry;
5. resolve exact enabled plugin packages;
6. validate plugin packages, manifests, platform executables, IDs, and dependencies;
7. parse referenced screen and agent-type definitions;
8. load and validate enabled-plugin configuration schemas and values;
9. resolve secret references;
10. merge shipped defaults, plugin contributions, user definitions, and user overrides;
11. validate screens, layouts, constrained relationships, typed routes, focus, actions, keymaps, and protected actions;
12. start required providers and complete configuration handshakes;
13. construct the initial screen instance and panel-local state;
14. publish the immutable registry and effective configuration exactly once;
15. enter the TUI.

If any step fails:

- terminate providers already started for the candidate;
- publish nothing;
- exit with actionable source-aware diagnostics;
- do not silently skip a plugin or fall back to a different workbench.

A missing master file is normal. A malformed master file is not equivalent to a missing one because it selects executable plugins and defines the workbench.

## Configuration CLI

Recommended commands:

```text
jefe [--config DIR] config path
jefe [--config DIR] config validate
jefe [--config DIR] config show-effective
jefe [--config DIR] config edit
jefe [--config DIR] config format

jefe [--config DIR] plugin list
jefe [--config DIR] plugin install PATH
jefe [--config DIR] plugin enable ID --version VERSION
jefe [--config DIR] plugin disable ID
jefe [--config DIR] plugin remove ID --version VERSION
```

Behavior:

- `config path` prints settings, state, plugin, definition, and theme paths.
- `config validate` performs complete candidate composition and provider validation without writing.
- `config show-effective` reports inherited/core/plugin/user sources and redacts secrets by default.
- `config edit` launches `$VISUAL` or `$EDITOR` and validates when it exits.
- `config format` is the only explicit canonical rewrite command.

## Platform paths

For now:

### macOS

```text
~/Library/Application Support/jefe/settings.toml
~/Library/Application Support/jefe/plugins/
~/Library/Application Support/jefe/definitions/
~/Library/Application Support/jefe/themes/
```

### Linux

```text
${XDG_CONFIG_HOME:-~/.config}/jefe/settings.toml
${XDG_CONFIG_HOME:-~/.config}/jefe/plugins/
${XDG_CONFIG_HOME:-~/.config}/jefe/definitions/
${XDG_CONFIG_HOME:-~/.config}/jefe/themes/
```

Operational state remains in the platform data/state location unless `--config` is used.

### Explicit config directory

```text
jefe --config DIR

DIR/settings.toml
DIR/state.json
DIR/plugins/
DIR/definitions/
DIR/themes/
```

The explicit directory must remain isolated from default roots and `JEFE_*` environment paths.

Issue 137 may later change default path placement. Path construction should remain centralized so that migration does not alter the master grammar.

## Plugin installation and enablement

V1 supports local package paths/archives rather than an online marketplace.

Install transaction:

1. extract into `<config>/plugins/staging/<unique>`;
2. reject absolute paths, traversal, and escaping symlinks;
3. validate manifest, ID, exact version, host API, resources, and current macOS/Linux executable;
4. display plugin contributions and state clearly that the executable runs unsandboxed as the user;
5. atomically rename into `plugins/installed/<id>/<version>`;
6. leave installed packages disabled until explicit enablement;
7. enable only after config, keymap, route, schema, and provider validation succeeds;
8. atomically patch `plugins.enabled` in the master file;
9. require restart.

Upgrade installs alongside the old exact version. Switching versions is a separate validated configuration edit, allowing rollback.

Removing an enabled version is rejected. Removing a disabled version preserves its configuration as dormant data unless the user explicitly chooses Delete Saved Configuration.

## Migration from current settings

Current schema:

```toml
schema_version = 1
theme = "dracula"
override_agent_theme = true
```

Target semantics:

```toml
schema_version = 2

[theme]
active = "dracula"
apply_to_agents = true
```

Migration should be a pure typed transform plus a lossless TOML patch. The first disk rewrite should create a backup, be idempotent, and occur explicitly through Settings save or a migration command. Startup should not silently rewrite plugin or user configuration.

Do not migrate fields from `state.json` merely because they appear preference-like; classify each by ownership first.

## Design tradeoffs

### One master file versus many preference files

One master file is easier to understand, inspect, back up, validate, and edit. Auxiliary screen and agent definition files are referenced resources, not competing preference authorities.

### Sparse overrides versus materialized configuration

Sparse overrides stay readable and inherit improved shipped/plugin defaults. Effective-config diagnostics provide the complete resolved view when needed.

### Lossless TOML versus typed full rewrite

Lossless editing is more work but is necessary when both humans and the Settings UI edit the same document and plugin tables must survive unknown versions.

### Host-generated plugin Settings versus plugin UI

Schemas limit custom presentation but preserve one focus, validation, rendering, and persistence model. This is the correct v1 tradeoff.

### Restart-required structural changes

This is less immediate but keeps one immutable registry and avoids hot-reload complexity for keymaps, providers, focused panels, screen instances, and plugin schemas.

### Exact plugin versions

They make upgrades explicit but provide deterministic startup and simple rollback.

## Remaining open questions

1. Exact macOS/Linux protected recovery chords require real crossterm testing; F12 remains the current default, not a universal invariant.
2. Issue 137 still needs to decide future default path migration. It does not change the master-file format or strict `--config` isolation.
3. Remote plugin catalogs/signatures are out of scope for v1 and need a separate distribution design if later required.
4. `state.json` currently combines domain records with presentation state. Splitting it may improve ownership, but it should not expand this configuration work.

## Final position

Jefe should have one understandable, hand-editable master configuration file and one host-rendered Settings screen that edit the same authority.

Plugins may contribute:

- actions;
- default key mappings;
- screens;
- provider-backed panels;
- typed configuration schemas and defaults.

Users retain final control over mappings and values. Plugin configuration lives in a namespaced table in the master file, while Jefe renders and validates it through host controls. All structural changes validate as one candidate and take effect after restart.

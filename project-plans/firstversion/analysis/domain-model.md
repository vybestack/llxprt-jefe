# Domain Model Analysis

Plan ID: `PLAN-20260216-FIRSTVERSION-V1`

## Scope

This artifact defines the canonical domain objects, invariants, transition surfaces, and replacement map for the firstversion hybrid strategy.

## Core Entities

### Repository
- `id: RepositoryId`
- `name: String`
- `slug: String`
- `base_dir: PathBuf`
- `default_profile: String`
- `agent_ids: Vec<AgentId>`

Invariants:
- `slug` unique
- `base_dir` valid/expandable at runtime boundaries
- repository deletion resolves ownership of child agents deterministically

### Agent
- `id: AgentId`
- `display_id: String`
- `repository_id: RepositoryId`
- `name: String`
- `description: String`
- `work_dir: PathBuf`
- `profile: String`
- `mode_flags: Vec<String>`
- `pass_continue: bool`
- `status: AgentStatus`
- `runtime_binding: Option<RuntimeBinding>`
- `preview: AgentPreview`

Invariants:
- `name` non-empty
- `pass_continue` defaults true on create
- `runtime_binding` may be absent for never-launched agents

### AgentStatus (enum)
- `Running`
- `Completed`
- `Errored`
- `Waiting`
- `Paused`
- `Queued`
- `Dead`

### RuntimeBinding
- `session_name: String`
- `launch_signature: LaunchSignature`
- `attached: bool`
- `last_seen: Option<DateTimeLike>`

Invariants:
- One runtime identity per launched agent
- Relaunch uses preserved launch signature

### ThemeState
- `active_slug: String`
- `resolved_palette: ThemePalette`

Invariants:
- unresolved theme token path always falls back to Green Screen token set

### Persistence Payloads
- `settings.toml` -> user preferences (theme + display/runtime prefs)
- `state.json` -> repositories, agents, selection context, runtime linkage metadata

Invariants:
- parse + validate before apply
- schema/version required
- malformed payload => fallback + warning, no crash
- path resolution order is deterministic and environment-overridable:
  - `settings.toml`: `JEFE_SETTINGS_PATH` -> `JEFE_CONFIG_DIR/settings.toml` -> platform default
  - `state.json`: `JEFE_STATE_PATH` -> `JEFE_STATE_DIR/state.json` -> platform default

---

## App State Aggregates

### AppState
- repositories + selected repository index
- agents (global + filtered views) + selected agent index
- pane focus, screen mode, modal/form state
- terminal focus flag
- split-mode state (filter target, reorder grab state)
- search/help state
- active errors/warnings

### Event Taxonomy
- Navigation events
- Focus events
- CRUD form events
- Runtime lifecycle events (kill/relaunch)
- Terminal input/focus events
- Split mode filter/reorder events
- Persistence load/save result events
- Theme apply/result events

---

## Transition and Side-Effect Ownership

- Event reducer mutates only in-memory state deterministically.
- Runtime boundary owns tmux/PTY operations.
- Persistence boundary owns file reads/writes and validation.
- Theme boundary owns load/resolve/fallback.
- UI layer remains presentational and emits typed intents.

---

## Edge/Error Model

- Missing/malformed persistence files -> fallback defaults + UI warning.
- Runtime attach failure -> keep UI responsive, mark runtime issue.
- Kill non-running agent -> no destructive side effect; show status message.
- Relaunch without launch signature -> blocked with explicit operator error.
- Theme slug invalid -> switch to Green Screen fallback.
- Workdir delete failure after agent delete request -> report failure path without state corruption.

---

## Hybrid Replacement Map

### Reuse/Adapt from toy1 (UI-facing)
- dashboard composition model
- split mode interaction flow
- modal/form interaction patterns
- keyboard routing/focus behavior
- terminal focus border semantics and F12 pattern

### Rebuild cleanly (non-UI core)
- domain model + invariants
- typed event/reducer architecture
- runtime orchestration boundary contract
- persistence versioning/validation/atomic writes
- theme manager contract + fallback enforcement

This split satisfies REQ-TECH-010 and avoids architecture fork patterns.

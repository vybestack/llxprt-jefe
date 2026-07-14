# Issue #213: Transient Agent Support — Implementation Plan

## Overview

Add the ability to "send to a transient agent" from the Issues/PRs detail view.
A transient agent is created on-the-fly (not pre-defined), uses the repository's
default model/options, gets a temporary work directory (under a configurable
`transient_agent_dir`, defaulting to `/tmp`), clones the repo there, and
launches with the issue/PR prompt. Supports a configurable max-concurrent
queue and cleanup on exit.

## Architecture (respecting module boundaries from project-standards.md)

- **Domain layer**: New config fields on `Repository`, `is_transient` flag on `Agent`
- **State layer**: `AgentChooserState` gains `transient_available`; new queue state in `AppState`; new events
- **App-input layer**: Transient agent creation/launch orchestration; form field handling
- **Runtime layer**: Reuses existing `spawn_session_fresh` / `attach`
- **Persistence layer**: Transient agents filtered from `to_persisted_state`; new repo fields serialized with `#[serde(default)]`

## Phase 1: Domain Changes (src/domain/mod.rs)

### Repository struct — add fields:
```rust
/// Directory for transient agent work copies. Empty/None defaults to /tmp.
#[serde(default)]
pub transient_agent_dir: PathBuf,
/// Default Code Puppy YOLO for transient agents. None = no yolo.
#[serde(default)]
pub default_code_puppy_yolo: Option<bool>,
/// Max concurrent transient agents. 0 = no limit (no queueing).
#[serde(default)]
pub transient_max_concurrent: u32,
```
Update `Repository::new()` to initialize these defaults:
- `transient_agent_dir: PathBuf::new()` (empty = /tmp)
- `default_code_puppy_yolo: None`
- `transient_max_concurrent: 0`

### Agent struct — add field:
```rust
/// Whether this is a transient agent (created on-the-fly, not persisted,
/// cleaned up on exit).
#[serde(default)]
pub is_transient: bool,
```
Update `Agent::new()` to default `is_transient: false`.

### Add a helper method on Repository:
```rust
/// Resolve the effective transient agent directory (defaults to /tmp when empty).
#[must_use]
pub fn effective_transient_dir(&self) -> &Path {
    if self.transient_agent_dir.as_os_str().is_empty() {
        Path::new("/tmp")
    } else {
        &self.transient_agent_dir
    }
}
```

### Add a helper on Agent for transient creation:
```rust
/// Create a transient agent from repository defaults.
#[must_use]
pub fn new_transient(
    id: AgentId,
    repository_id: RepositoryId,
    work_dir: PathBuf,
    repo: &Repository,
) -> Self { ... }
```
This sets: `is_transient = true`, `profile = repo.default_profile`,
`code_puppy_model = repo.default_code_puppy_model`, `code_puppy_yolo = repo.default_code_puppy_yolo`,
`agent_kind = repo.default_agent_kind`, `pass_continue = false` (transient = one-shot).

## Phase 2: State Layer Changes

### AgentChooserState (src/state/types.rs)
Add a `transient_available: bool` field (default false). When true, the
transient entry appears after all regular agents at index `agents.len()`.
Navigation bounds become `agents.len() + transient_available as usize`.

### AppState (src/state/types.rs) — add runtime-only queue state:
```rust
/// Pending transient-agent sends queued because max_concurrent is reached.
/// Runtime-only — never persisted.
pub transient_queue: TransientAgentQueue,
```

### New type (in types.rs or a new module):
```rust
/// A queued transient agent send waiting for a slot.
#[derive(Debug, Clone)]
pub struct QueuedTransientSend {
    pub repository_id: RepositoryId,
    pub work_dir: PathBuf,
    pub launch_signature: LaunchSignature,
    pub payload: TransientPayload,
}

/// What to send to a transient agent (issue or PR).
#[derive(Debug, Clone)]
pub enum TransientPayload {
    Issue { payload: jefe::github::SendPayload },
    PullRequest { payload: jefe::github::PrSendPayload },
}

/// Queue of pending transient agent sends.
#[derive(Debug, Clone, Default)]
pub struct TransientAgentQueue {
    pub pending: Vec<QueuedTransientSend>,
}
```

### Events (src/state/events.rs)
Add:
```rust
/// A transient agent send was queued (max_concurrent reached).
TransientAgentQueued { queue_position: usize },
/// A transient agent was dequeued and is being launched.
TransientAgentDequeued,
```

### Reducer changes (src/state/issues_ops.rs, prs_ops.rs)
- `open_agent_chooser()`: set `transient_available = true` when an agent kind is installed for the selected repo AND the repo has a valid `github_repo` (needed for cloning).
- Navigation: bounds check includes the transient slot.
- `AgentChooserConfirm`/`PrAgentChooserConfirm`: still closes the chooser (no change to reducer — the routing happens in the app-input layer).

### Selectors (src/state/selectors.rs)
Add:
```rust
/// Count running transient agents for a repository.
pub fn running_transient_count(&self, repo_id: &RepositoryId) -> usize {
    self.agents.iter()
        .filter(|a| a.is_transient && a.repository_id == *repo_id && a.is_running())
        .count()
}
```

## Phase 3: App-Input Layer (Send Orchestration)

### Issues send (src/app_input/issues_send.rs)
In `dispatch_agent_chooser_confirm`:
1. Before applying `AgentChooserConfirm`, check if the selected entry is the
   transient slot (`selected_index == agents.len() && transient_available`).
2. If transient:
   a. Apply `AgentChooserConfirm` (closes chooser).
   b. Resolve the repository and its defaults.
   c. Check queue capacity: if `transient_max_concurrent > 0` and
      `running_transient_count >= max`, push to `transient_queue` and emit
      `TransientAgentQueued`. Return.
   d. Generate a unique temp directory under `repo.effective_transient_dir()`.
   e. Create a transient `Agent` via `Agent::new_transient`.
   f. Push the agent to `state.agents` (but NOT to `repo.agent_ids` — it's
      not a persistent agent).
   g. Build the launch signature from the transient agent + repo.
   h. Resolve clone identity from the repo.
   i. Run `prepare_issue_target` (Stop policy) on the temp dir — this clones
      the repo, checks out, and writes the prompt.
   j. Handle outcomes (Ready → launch; Dirty → shouldn't happen on a fresh
      temp dir, treat as error; OriginMismatch → shouldn't happen, error).
   k. Launch the agent via `spawn_session_fresh` + `attach`.
   l. Persist state (transient agent is in state.agents but filtered from
      persisted DTO).

### PRs send (src/app_input/prs_orchestration.rs)
Mirror the same logic for `dispatch_pr_agent_chooser_confirm`.

### Queue draining (src/app_input/mod.rs or a new module)
On `AgentStatusChanged` to `Completed`/`Errored`/`Dead` for a transient agent:
1. Check if there are queued sends for that repository.
2. If so, dequeue the oldest and launch it (same flow as step d-k above).

### to_persisted_state (src/app_input/mod.rs)
Filter out transient agents:
```rust
agents: state.agents.iter()
    .filter(|a| !a.is_transient)
    .cloned()
    .collect(),
```

## Phase 4: Repository Form (src/state/form_types.rs, form_ops.rs, form_cursor.rs, form_build.rs, modal_ops.rs)

### Form fields (form_types.rs)
Add to `RepositoryFormFields`:
```rust
pub transient_agent_dir: String,
pub default_code_puppy_yolo: bool,
pub transient_max_concurrent: String, // text field, parsed to u32
```

Add to `RepositoryFormCursor`:
```rust
pub transient_agent_dir: usize,
pub transient_max_concurrent: usize,
```

Add to `RepositoryFormFocus`:
```rust
TransientAgentDir,
DefaultCodePuppyYolo,
TransientMaxConcurrent,
```
Insert these into `next()`/`prev()` after `SetupEnvDefault` (before wrapping to `Name`).

### Form input handling (form_ops.rs)
- `handle_form_char`: write to the text fields (`transient_agent_dir`, `transient_max_concurrent`)
- `handle_form_backspace`/`handle_form_delete`: same
- `handle_form_toggle`: toggle `default_code_puppy_yolo` checkbox
- Cursor position init in `modal_ops.rs`

### Form build (form_build.rs)
- `create_repository_from_fields`: parse `transient_max_concurrent` as u32 (default 0 on parse error), set `transient_agent_dir` (expand tilde for local repos), set `default_code_puppy_yolo`
- `update_repository_from_fields`: same updates

### Modal population (modal_ops.rs)
- `open_new_repository_modal`: default `transient_agent_dir` to "/tmp", `transient_max_concurrent` to "0"
- `open_edit_repository_modal`: populate from existing repo

## Phase 5: UI Changes

### Agent chooser (src/ui/components/agent_chooser.rs)
Add the "Transient Agent" entry after the agent list when
`transient_available` is true. The entry shows at index `agents.len()`.

### Repository form rendering
Add the new fields to the repository form render (wherever the form fields
are rendered — search for where `RepositoryFormFields` is consumed by the UI).

## Phase 6: Cleanup on Exit

### App shell (src/app_shell.rs or main.rs)
On quit:
1. Read state, filter transient agents.
2. For each transient agent, remove its work directory (best-effort, log on error).
3. Kill the tmux session (best-effort).

## Phase 7: Messages System

### Events → Messages conversion (src/messages/)
Add the new events to the message conversion system:
- `issues_conversion.rs`: `TransientAgentQueued` → `IssuesMessage::TransientAgentQueued`
- `prs_conversion.rs`: same for PRs
- `event_conversion.rs`: add to the match

## Test Plan (TDD: RED → GREEN → REFACTOR)

### Domain tests (src/domain/tests.rs)
1. `Repository::new` defaults: `transient_agent_dir` empty, `default_code_puppy_yolo` None, `transient_max_concurrent` 0
2. `Repository::effective_transient_dir` returns /tmp when empty, the dir when set
3. `Agent::new_transient` creates agent with `is_transient=true`, repo defaults, `pass_continue=false`
4. `Agent::new` defaults `is_transient=false`

### State tests
1. `AgentChooserState` with `transient_available=true`: navigation bounds include the transient slot
2. `open_agent_chooser` sets `transient_available=true` when agent kind installed + github_repo set
3. `open_agent_chooser` does NOT set `transient_available` when no github_repo (can't clone)
4. `running_transient_count` counts only running transient agents for the repo
5. Queue push when at capacity
6. Queue dequeue on agent completion

### Form tests
1. Repository form with new fields: create, edit, validation
2. `transient_max_concurrent` parsing: valid number, invalid → 0
3. `transient_agent_dir` tilde expansion for local repos

### Send flow tests
1. Transient entry selected → creates temp dir, clones, launches
2. Queue at capacity → sends to queue, surfaces notice
3. Queue drain on agent completion → launches next

### Persistence tests
1. `to_persisted_state` filters out transient agents
2. Repository with new fields round-trips through serialization

## Key Constraints
- NO `unwrap`/`expect` in production paths
- NO `unsafe`
- `#[serde(default)]` on all new fields for backward compatibility
- Transient agents are runtime-only (never persisted to state.json)
- Follow existing patterns for form handling, event conversion, and send orchestration
- Lint/complexity guardrails: never loosen rules, never add suppression directives

//! Pure projections between runtime [`AppState`] and the durable schema-2
//! [`StateV2`] document (issue #381 S9b-1).
//!
//! The forward projection builds the candidate the reducer stages as a
//! `PersistState` effect; the inverse restores runtime fields from a loaded
//! document. Both are deterministic and perform no I/O: identical input
//! always yields byte-identical output, and no path is canonicalized against
//! the filesystem here (migration owns that one-way repair).
//!
//! Transient agents are runtime-only and are excluded, together with every
//! reference to them.

use std::collections::{BTreeMap, HashMap};

use serde_json::{Value, json};

use crate::domain::canonical_values::{
    canonical_remote_target, digest_parts, json_map_to_typed, normalize_remote_path, stable_id,
    type_id, typed_map_hash,
};
use crate::domain::{
    Agent, AgentDefaults, AgentId, AgentKind, AgentOrigin, AgentRecord, AgentStatus, DormantRecord,
    Id, LastKnownRuntime, LaunchSignatureV1, LocalRepositoryLocation, Preferences,
    RemoteRepositoryLocation, Repository, RepositoryId, RepositoryLocation, RepositoryRecord,
    RuntimeRecord, STATE_SCHEMA_V2, Selection, StateV2, TypedMap, UserPreferences,
};
use crate::state::{AppState, PaneFocus};

/// Version tag stamped into every projected [`LaunchSignatureV1`].
const DEFINITION_VERSION: &str = "1";

/// Runtime fields restored from a durable document.
///
/// Deliberately not an [`AppState`]: restoring never fabricates unrelated
/// runtime state (modals, screens, caches). The caller assigns these fields
/// onto the state it owns.
#[derive(Debug, Clone, Default)]
pub struct RestoredState {
    /// Durable revision the document carried.
    pub revision: u64,
    /// Restored repositories in document order.
    pub repositories: Vec<Repository>,
    /// Restored agents in document order.
    pub agents: Vec<Agent>,
    /// Selected repository index, resolved from the durable id.
    pub selected_repository_index: Option<usize>,
    /// Selected agent index, resolved from the durable id.
    pub selected_agent_index: Option<usize>,
    /// Remembered per-repository agent selection.
    pub last_selected_agent_by_repo: Vec<(RepositoryId, AgentId)>,
    /// Restored per-repository preferences.
    pub user_preferences: UserPreferences,
    /// Whether idle repositories are hidden.
    pub hide_idle_repositories: bool,
    /// Restored pane focus.
    pub pane_focus: PaneFocus,
    /// Whether the terminal pane held focus.
    pub terminal_focused: bool,
    /// Dormant schema-1 records carried through untouched.
    pub dormant_records: Vec<DormantRecord>,
}

/// Failure to project between runtime and durable state.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProjectionError {
    detail: String,
}

impl ProjectionError {
    pub(super) fn new(detail: impl Into<String>) -> Self {
        Self {
            detail: detail.into(),
        }
    }

    /// Borrow the redacted failure description.
    #[must_use]
    pub fn detail(&self) -> &str {
        &self.detail
    }
}

impl std::fmt::Display for ProjectionError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(&self.detail)
    }
}

impl std::error::Error for ProjectionError {}

/// Result of a projection in either direction.
pub type Projected<T> = Result<T, ProjectionError>;

pub(super) fn error<T>(detail: impl Into<String>) -> Projected<T> {
    Err(ProjectionError::new(detail))
}

pub(super) fn map_detail(detail: String) -> ProjectionError {
    ProjectionError::new(detail)
}

/// Project runtime state into the durable schema-2 candidate.
///
/// # Errors
///
/// Returns [`ProjectionError`] when an identifier cannot be minted, a typed
/// value cannot be encoded, or an agent references an unknown repository.
pub fn to_durable_state(state: &AppState) -> Projected<StateV2> {
    let repository_ids = mint_repository_ids(&state.repositories)?;
    let agent_ids = mint_agent_ids(&state.agents, &repository_ids)?;

    let mut repositories = Vec::with_capacity(state.repositories.len());
    for repository in &state.repositories {
        repositories.push(repository_record(repository, &repository_ids)?);
    }

    let mut agents = Vec::new();
    for agent in &state.agents {
        if agent.origin == AgentOrigin::Transient {
            continue;
        }
        let repository = state
            .repositories
            .iter()
            .find(|repository| repository.id == agent.repository_id);
        let Some(repository) = repository else {
            return error(format!(
                "agent {} references an unknown repository",
                agent.display_id
            ));
        };
        agents.push(agent_record(
            agent,
            repository,
            &repository_ids,
            &agent_ids,
        )?);
    }

    let selection = project_selection(state, &repository_ids, &agent_ids);
    let last_selected_agent_by_repo =
        project_last_selected(state, &repository_ids, &agent_ids, &agents);
    let repository_preferences = project_preferences(state, &repository_ids)?;

    Ok(StateV2 {
        state_schema: STATE_SCHEMA_V2,
        revision: state.durable_revision,
        repositories,
        agents,
        selection,
        last_selected_agent_by_repo,
        preferences: Preferences {
            hide_idle_repositories: state.hide_idle_repositories,
            pane_focus: pane_focus_text(state.pane_focus),
            terminal_focused: state.terminal_focused,
            repository_preferences,
        },
        dormant_records: state.dormant_records.clone(),
    })
}

/// Durable identifier assigned to each runtime repository.
type RepositoryIds = HashMap<RepositoryId, Id>;
/// Durable identifier assigned to each runtime agent.
type AgentIds = HashMap<AgentId, Id>;

/// Take the next collision ordinal for `key`, mirroring migration's rule that
/// equal source identities stay distinct in a stable order.
fn next_ordinal(collisions: &mut BTreeMap<String, u64>, key: &str) -> String {
    let ordinal = collisions.entry(key.to_owned()).or_default();
    let text = ordinal.to_string();
    *ordinal += 1;
    text
}

fn mint_repository_ids(repositories: &[Repository]) -> Projected<RepositoryIds> {
    let mut ids = HashMap::with_capacity(repositories.len());
    let mut collisions = BTreeMap::<String, u64>::new();
    for repository in repositories {
        let id = if let Ok(id) = Id::parse(&repository.id.0) {
            id
        } else {
            let ordinal = next_ordinal(&mut collisions, &repository.id.0);
            stable_id("repo", &[&repository.id.0, &ordinal]).map_err(map_detail)?
        };
        if ids.insert(repository.id.clone(), id).is_some() {
            return error(format!("duplicate repository id {}", repository.id.0));
        }
    }
    Ok(ids)
}

fn mint_agent_ids(agents: &[Agent], repository_ids: &RepositoryIds) -> Projected<AgentIds> {
    let mut ids = HashMap::with_capacity(agents.len());
    let mut collisions = BTreeMap::<String, u64>::new();
    for agent in agents {
        if agent.origin == AgentOrigin::Transient {
            continue;
        }
        let id = if let Ok(id) = Id::parse(&agent.id.0) {
            id
        } else {
            let Some(repository_id) = repository_ids.get(&agent.repository_id) else {
                return error(format!(
                    "agent {} references an unknown repository",
                    agent.display_id
                ));
            };
            let ordinal = next_ordinal(&mut collisions, &agent.id.0);
            stable_id("agent", &[repository_id.as_str(), &agent.id.0, &ordinal])
                .map_err(map_detail)?
        };
        if ids.insert(agent.id.clone(), id).is_some() {
            return error(format!("duplicate agent id {}", agent.id.0));
        }
    }
    Ok(ids)
}

fn repository_record(
    repository: &Repository,
    repository_ids: &RepositoryIds,
) -> Projected<RepositoryRecord> {
    let Some(id) = repository_ids.get(&repository.id).cloned() else {
        return error(format!("repository {} has no durable id", repository.id.0));
    };
    let values = json_map_to_typed(repository_values(repository)).map_err(map_detail)?;
    let type_id =
        type_id(Some(agent_kind_text(repository.default_agent_kind))).map_err(map_detail)?;
    Ok(RepositoryRecord {
        id,
        location: repository_location(repository),
        display_name: repository.name.clone(),
        agent_defaults: AgentDefaults { type_id, values },
    })
}

fn repository_location(repository: &Repository) -> RepositoryLocation {
    if repository.remote.enabled {
        RepositoryLocation::Remote(RemoteRepositoryLocation {
            remote_target: remote_identity(repository),
        })
    } else {
        RepositoryLocation::Local(LocalRepositoryLocation {
            local_path: repository.base_dir.to_string_lossy().into_owned(),
        })
    }
}

fn remote_identity(repository: &Repository) -> String {
    let remote = &repository.remote;
    let run_as = if remote.run_as_user.trim().is_empty() {
        remote.login_user.trim()
    } else {
        remote.run_as_user.trim()
    };
    canonical_remote_target(
        &remote.login_user,
        &remote.host,
        remote.port.unwrap_or(22),
        run_as,
        &repository.base_dir.to_string_lossy(),
    )
}

fn repository_values(repository: &Repository) -> Value {
    let remote = json!({
        "enabled": repository.remote.enabled,
        "login_user": repository.remote.login_user,
        "host": repository.remote.host,
        "port": repository.remote.port,
        "identity_file": path_text(&repository.remote.identity_file),
        "options": repository.remote.options,
        "run_as_user": repository.remote.run_as_user,
        "setup_env_default": repository.remote.setup_env_default,
    });
    json!({
        "slug": repository.slug,
        "default_profile": repository.default_profile,
        "default_code_puppy_model": repository.default_code_puppy_model,
        "default_code_puppy_version": repository.default_code_puppy_version,
        "github_repo": repository.github_repo,
        "github_issue_pr_repo": repository.github_issue_pr_repo,
        "remote": remote,
        "issue_base_prompt": repository.issue_base_prompt,
        "transient_agent_dir": path_text(&repository.transient_agent_dir),
        "default_code_puppy_yolo": repository.default_code_puppy_yolo,
        "default_llxprt_mode_flags": repository.default_llxprt_mode_flags,
        "transient_max_concurrent": repository.transient_max_concurrent,
        "default_llxprt_version": repository
            .default_llxprt_version
            .as_ref()
            .map(|selector| selector.as_str().to_owned()),
    })
}

fn agent_record(
    agent: &Agent,
    repository: &Repository,
    repository_ids: &RepositoryIds,
    agent_ids: &AgentIds,
) -> Projected<AgentRecord> {
    let Some(id) = agent_ids.get(&agent.id).cloned() else {
        return error(format!("agent {} has no durable id", agent.display_id));
    };
    let Some(repository_id) = repository_ids.get(&agent.repository_id).cloned() else {
        return error(format!(
            "agent {} references an unknown repository",
            agent.display_id
        ));
    };
    let type_id = type_id(Some(agent_kind_text(agent.agent_kind))).map_err(map_detail)?;
    let values = json_map_to_typed(agent_values(agent)).map_err(map_detail)?;
    let definition_hash =
        digest_parts(&[type_id.as_str(), DEFINITION_VERSION]).map_err(map_detail)?;
    let typed_value_hash = typed_map_hash(&values).map_err(map_detail)?;
    let work_target = agent_work_target(agent, repository);
    let target_fingerprint =
        digest_parts(&[&repository_identity(repository), &work_target]).map_err(map_detail)?;
    let (session_id, invocation_generation) =
        agent.runtime_binding.as_ref().map_or((None, 0), |binding| {
            (
                Some(binding.session_name.clone()),
                binding.lifecycle_generation,
            )
        });
    Ok(AgentRecord {
        id,
        repository_id,
        type_id,
        values,
        launch_signature: LaunchSignatureV1 {
            version: 1,
            definition_hash,
            typed_value_hash,
            target_fingerprint,
        },
        runtime: RuntimeRecord {
            session_id,
            invocation_generation,
            last_known: last_known_runtime(agent.status),
        },
    })
}

fn agent_values(agent: &Agent) -> Value {
    json!({
        "display_id": agent.display_id,
        "shortcut_slot": agent.shortcut_slot,
        "name": agent.name,
        "description": agent.description,
        "work_dir": path_text(&agent.work_dir),
        "profile": agent.profile,
        "code_puppy_model": agent.code_puppy_model,
        "code_puppy_version": agent.code_puppy_version,
        "code_puppy_yolo": agent.code_puppy_yolo,
        "code_puppy_quick_resume": agent.code_puppy_quick_resume,
        "mode_flags": agent.mode_flags,
        "llxprt_debug": agent.llxprt_debug,
        "pass_continue": agent.pass_continue,
        "sandbox_enabled": agent.sandbox_enabled,
        "sandbox_engine": sandbox_engine_text(agent.sandbox_engine),
        "sandbox_flags": agent.sandbox_flags,
        "llxprt_version": agent
            .llxprt_version
            .as_ref()
            .map(|selector| selector.as_str().to_owned()),
        "origin": agent_origin_text(agent.origin),
    })
}

fn agent_work_target(agent: &Agent, repository: &Repository) -> String {
    let work_dir = agent.work_dir.to_string_lossy();
    if repository.remote.enabled {
        normalize_remote_path(&work_dir)
    } else {
        work_dir.into_owned()
    }
}

fn repository_identity(repository: &Repository) -> String {
    match repository_location(repository) {
        RepositoryLocation::Local(local) => local.local_path,
        RepositoryLocation::Remote(remote) => remote.remote_target,
    }
}

fn project_selection(
    state: &AppState,
    repository_ids: &RepositoryIds,
    agent_ids: &AgentIds,
) -> Selection {
    let repository_id = state
        .selected_repository_index
        .and_then(|index| state.repositories.get(index))
        .and_then(|repository| repository_ids.get(&repository.id).cloned());
    let agent_id = state
        .selected_agent_index
        .and_then(|index| state.agents.get(index))
        .filter(|agent| agent.origin != AgentOrigin::Transient)
        .and_then(|agent| agent_ids.get(&agent.id).cloned());
    Selection {
        repository_id,
        agent_id,
        screen_id: None,
    }
}

fn project_last_selected(
    state: &AppState,
    repository_ids: &RepositoryIds,
    agent_ids: &AgentIds,
    agents: &[AgentRecord],
) -> BTreeMap<Id, Id> {
    let mut selected = BTreeMap::new();
    for (repository_id, agent_id) in &state.last_selected_agent_by_repo {
        let (Some(repository), Some(agent)) =
            (repository_ids.get(repository_id), agent_ids.get(agent_id))
        else {
            continue;
        };
        let owned = agents
            .iter()
            .any(|record| &record.id == agent && &record.repository_id == repository);
        if owned {
            selected.insert(repository.clone(), agent.clone());
        }
    }
    selected
}

fn project_preferences(
    state: &AppState,
    repository_ids: &RepositoryIds,
) -> Projected<BTreeMap<Id, TypedMap>> {
    let mut preferences = BTreeMap::new();
    for (repository_id, values) in &state.user_preferences.by_repo {
        let Some(id) = repository_ids.get(repository_id).cloned() else {
            continue;
        };
        let encoded = serde_json::to_value(values)
            .map_err(|error| ProjectionError::new(error.to_string()))?;
        preferences.insert(id, json_map_to_typed(encoded).map_err(map_detail)?);
    }
    Ok(preferences)
}

fn last_known_runtime(status: AgentStatus) -> LastKnownRuntime {
    match status {
        AgentStatus::Running => LastKnownRuntime::Running,
        AgentStatus::Dead => LastKnownRuntime::Stopped,
        _ => LastKnownRuntime::Unknown,
    }
}

const fn agent_kind_text(kind: AgentKind) -> &'static str {
    match kind {
        AgentKind::CodePuppy => "code-puppy",
        AgentKind::Llxprt => "llxprt",
    }
}

const fn agent_origin_text(origin: AgentOrigin) -> &'static str {
    match origin {
        AgentOrigin::Persistent => "persistent",
        AgentOrigin::Transient => "transient",
    }
}

const fn sandbox_engine_text(engine: crate::domain::SandboxEngine) -> &'static str {
    match engine {
        crate::domain::SandboxEngine::Podman => "podman",
        crate::domain::SandboxEngine::Docker => "docker",
        crate::domain::SandboxEngine::Seatbelt => "seatbelt",
    }
}

fn pane_focus_text(focus: PaneFocus) -> String {
    match focus {
        PaneFocus::Repositories => "repositories",
        PaneFocus::Agents => "agents",
        PaneFocus::Terminal => "terminal",
    }
    .to_owned()
}

fn path_text(path: &std::path::Path) -> String {
    path.to_string_lossy().into_owned()
}

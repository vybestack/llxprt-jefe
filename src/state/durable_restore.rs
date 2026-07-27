//! Restoration of runtime state from the durable schema-2 document.
//!
//! Inverse half of [`super::durable_projection`]: rebuilds runtime
//! repositories, agents, selection indices, and preferences from a loaded
//! [`StateV2`]. Pure and filesystem-free; runtime-only fields absent from the
//! durable schema (process ids, attachment, liveness) are left for startup
//! reconciliation to establish.

use std::path::PathBuf;

use crate::domain::canonical_values::{
    parse_remote_target, typed_field, typed_map_to_runtime_json,
};
use crate::domain::{
    Agent, AgentId, AgentKind, AgentOrigin, AgentRecord, AgentStatus, Id, LastKnownRuntime,
    LaunchSignature, RepoPreferences, Repository, RepositoryId, RepositoryLocation,
    RepositoryRecord, RuntimeBinding, StateV2, TypedMap, UserPreferences,
};
use crate::state::PaneFocus;

use super::durable_projection::{Projected, ProjectionError, RestoredState, error, map_detail};

/// Restore runtime fields from a durable schema-2 document.
///
/// # Errors
///
/// Returns [`ProjectionError`] when a record references an unknown repository
/// or a stored value cannot be decoded into its runtime type.
pub fn from_durable_state(state: &StateV2) -> Projected<RestoredState> {
    let mut repositories = Vec::with_capacity(state.repositories.len());
    for record in &state.repositories {
        repositories.push(restore_repository(record)?);
    }

    let mut agents = Vec::with_capacity(state.agents.len());
    for record in &state.agents {
        let repository = repositories
            .iter()
            .zip(&state.repositories)
            .find(|(_, source)| source.id == record.repository_id)
            .map(|(repository, _)| repository);
        let Some(repository) = repository else {
            return error(format!(
                "agent {} references an unknown repository",
                record.id.as_str()
            ));
        };
        agents.push(restore_agent(record, repository));
    }

    let selected_repository_index = state.selection.repository_id.as_ref().and_then(|id| {
        state
            .repositories
            .iter()
            .position(|record| &record.id == id)
    });
    let selected_agent_index = state
        .selection
        .agent_id
        .as_ref()
        .and_then(|id| state.agents.iter().position(|record| &record.id == id));

    let last_selected_agent_by_repo = restore_last_selected(state, &repositories, &agents);
    let user_preferences = restore_preferences(state, &repositories)?;

    Ok(RestoredState {
        revision: state.revision,
        repositories,
        agents,
        selected_repository_index,
        selected_agent_index,
        last_selected_agent_by_repo,
        user_preferences,
        hide_idle_repositories: state.preferences.hide_idle_repositories,
        pane_focus: pane_focus_from_text(&state.preferences.pane_focus),
        terminal_focused: state.preferences.terminal_focused,
        dormant_records: state.dormant_records.clone(),
    })
}

fn restore_last_selected(
    state: &StateV2,
    repositories: &[Repository],
    agents: &[Agent],
) -> Vec<(RepositoryId, AgentId)> {
    let mut restored = Vec::new();
    for (repository_id, agent_id) in &state.last_selected_agent_by_repo {
        let repository = state
            .repositories
            .iter()
            .position(|record| &record.id == repository_id);
        let agent = state
            .agents
            .iter()
            .position(|record| &record.id == agent_id);
        if let (Some(repository), Some(agent)) = (repository, agent) {
            restored.push((
                repositories[repository].id.clone(),
                agents[agent].id.clone(),
            ));
        }
    }
    restored
}

fn restore_preferences(state: &StateV2, repositories: &[Repository]) -> Projected<UserPreferences> {
    let mut preferences = UserPreferences::default();
    for (repository_id, values) in &state.preferences.repository_preferences {
        let Some(index) = state
            .repositories
            .iter()
            .position(|record| &record.id == repository_id)
        else {
            continue;
        };
        let decoded: RepoPreferences = serde_json::from_value(typed_map_to_runtime_json(values))
            .map_err(|error| ProjectionError::new(error.to_string()))?;
        preferences.update_for_repo(&repositories[index].id, decoded);
    }
    Ok(preferences)
}

/// Report whether the record stores an explicit `remote.enabled` flag.
fn remote_values_declare_enabled(values: &TypedMap) -> bool {
    match typed_field(values, "remote") {
        Some(crate::domain::TypedValue::Map(map)) => {
            let Ok(key) = crate::domain::Id::parse("enabled") else {
                return false;
            };
            map.contains_key(&key)
        }
        _ => false,
    }
}

fn restore_remote_settings(values: &TypedMap) -> crate::domain::RemoteRepositorySettings {
    let remote_values = match typed_field(values, "remote") {
        Some(crate::domain::TypedValue::Map(map)) => Some(map),
        _ => None,
    };
    crate::domain::RemoteRepositorySettings {
        enabled: remote_values.is_some_and(|map| typed_bool(map, "enabled").unwrap_or(false)),
        login_user: remote_values
            .and_then(|map| typed_string(map, "login_user"))
            .unwrap_or_default(),
        host: remote_values
            .and_then(|map| typed_string(map, "host"))
            .unwrap_or_default(),
        port: remote_values
            .and_then(|map| typed_integer(map, "port"))
            .and_then(|port| u16::try_from(port).ok()),
        identity_file: remote_values
            .and_then(|map| typed_string(map, "identity_file"))
            .map(PathBuf::from)
            .unwrap_or_default(),
        options: remote_values
            .and_then(|map| typed_string_list(map, "options"))
            .unwrap_or_default(),
        run_as_user: remote_values
            .and_then(|map| typed_string(map, "run_as_user"))
            .unwrap_or_default(),
        setup_env_default: remote_values
            .and_then(|map| typed_bool(map, "setup_env_default"))
            .unwrap_or(false),
    }
}

fn restore_repository(record: &RepositoryRecord) -> Projected<Repository> {
    let values = &record.agent_defaults.values;
    let mut remote = restore_remote_settings(values);

    let base_dir = match &record.location {
        RepositoryLocation::Local(local) => PathBuf::from(&local.local_path),
        RepositoryLocation::Remote(target) => {
            let parts = parse_remote_target(&target.remote_target).map_err(map_detail)?;
            // Connectivity fields are derived from the target only when absent;
            // the stored `enabled` flag is the user's choice and is preserved,
            // otherwise a disabled remote would be re-enabled on every load.
            if !remote_values_declare_enabled(values) {
                remote.enabled = true;
            }
            if remote.login_user.is_empty() {
                remote.login_user.clone_from(&parts.login_user);
            }
            if remote.host.is_empty() {
                remote.host.clone_from(&parts.host);
            }
            if remote.port.is_none() {
                remote.port = Some(parts.port);
            }
            PathBuf::from(parts.base_dir)
        }
    };

    Ok(Repository {
        id: RepositoryId(record.id.to_string()),
        name: record.display_name.clone(),
        slug: typed_string(values, "slug").unwrap_or_default(),
        base_dir,
        default_profile: typed_string(values, "default_profile").unwrap_or_default(),
        default_code_puppy_model: typed_string(values, "default_code_puppy_model")
            .unwrap_or_default(),
        default_code_puppy_version: repository_default_code_puppy_version(
            values,
            &record.agent_defaults.type_id,
        ),
        github_repo: typed_string(values, "github_repo").unwrap_or_default(),
        github_issue_pr_repo: typed_string(values, "github_issue_pr_repo").unwrap_or_default(),
        remote,
        issue_base_prompt: typed_string(values, "issue_base_prompt").unwrap_or_default(),
        default_agent_kind: agent_kind_from_type(&record.agent_defaults.type_id),
        transient_agent_dir: typed_string(values, "transient_agent_dir")
            .map(PathBuf::from)
            .unwrap_or_default(),
        default_code_puppy_yolo: typed_bool(values, "default_code_puppy_yolo"),
        default_llxprt_mode_flags: typed_string_list(values, "default_llxprt_mode_flags")
            .unwrap_or_default(),
        transient_max_concurrent: typed_integer(values, "transient_max_concurrent")
            .and_then(|value| u32::try_from(value).ok())
            .unwrap_or(0),
        default_llxprt_version: repository_default_llxprt_version(
            values,
            &record.agent_defaults.type_id,
        ),
        agent_ids: Vec::new(),
    })
}

/// Derive the repository's LLxprt default selector from the authoritative
/// generic `version_selector` field when the repository's declared agent kind
/// is LLxprt. A Code Puppy repository carries no LLxprt default.
fn repository_default_llxprt_version(
    values: &TypedMap,
    type_id: &crate::domain::Id,
) -> Option<crate::domain::LlxprtNpmPackageSelector> {
    if type_id.as_str() != "core.llxprt" {
        return None;
    }
    typed_string(values, "version_selector")
        .and_then(|value| crate::domain::LlxprtNpmPackageSelector::normalize(&value))
}

/// Derive the repository's Code Puppy default selector from the authoritative
/// generic `version_selector` field when the repository's declared agent kind
/// is Code Puppy.
fn repository_default_code_puppy_version(values: &TypedMap, type_id: &crate::domain::Id) -> String {
    if type_id.as_str() == "core.code-puppy" {
        typed_string(values, "version_selector").unwrap_or_default()
    } else {
        String::new()
    }
}

fn restore_agent(record: &AgentRecord, repository: &Repository) -> Agent {
    let values = &record.values;
    let id = AgentId(record.id.to_string());
    let mut agent = Agent::new(
        id.clone(),
        repository.id.clone(),
        typed_string(values, "name").unwrap_or_default(),
        typed_string(values, "work_dir")
            .map(PathBuf::from)
            .unwrap_or_default(),
    );
    agent.display_id = typed_string(values, "display_id").unwrap_or(id.0);
    agent.shortcut_slot =
        typed_integer(values, "shortcut_slot").and_then(|value| u8::try_from(value).ok());
    agent.description = typed_string(values, "description").unwrap_or_default();
    agent.profile = typed_string(values, "profile").unwrap_or_default();
    agent.code_puppy_model = typed_string(values, "code_puppy_model").unwrap_or_default();
    agent.code_puppy_yolo = typed_bool(values, "code_puppy_yolo");
    agent.code_puppy_quick_resume = typed_bool(values, "code_puppy_quick_resume").unwrap_or(false);
    agent.mode_flags = typed_string_list(values, "mode_flags").unwrap_or_default();
    agent.llxprt_debug = typed_string(values, "llxprt_debug").unwrap_or_default();
    agent.pass_continue = typed_bool(values, "pass_continue").unwrap_or(true);
    agent.sandbox_enabled = typed_bool(values, "sandbox_enabled").unwrap_or(false);
    agent.sandbox_engine = typed_string(values, "sandbox_engine")
        .and_then(|value| sandbox_engine_from_text(&value))
        .unwrap_or_default();
    if let Some(flags) = typed_string(values, "sandbox_flags") {
        agent.sandbox_flags = flags;
    }
    agent.agent_kind = agent_kind_from_type(&record.type_id);
    // The generic `version_selector` is authoritative. The product-specific
    // runtime field is derived from it based on the migrated type id, so no
    // runtime compatibility adapter reads the old selector field names.
    let version_selector = typed_string(values, "version_selector").unwrap_or_default();
    match agent.agent_kind {
        crate::domain::AgentKind::CodePuppy => {
            agent.code_puppy_version = version_selector;
        }
        crate::domain::AgentKind::Llxprt => {
            agent.llxprt_version =
                crate::domain::LlxprtNpmPackageSelector::normalize(&version_selector);
        }
    }
    agent.origin = typed_string(values, "origin")
        .and_then(|value| agent_origin_from_text(&value))
        .unwrap_or(AgentOrigin::Persistent);
    agent.status = agent_status(record.runtime.last_known);
    agent.runtime_binding = record
        .runtime
        .session_id
        .as_ref()
        .map(|session| RuntimeBinding {
            session_name: session.clone(),
            launch_signature: LaunchSignature::for_agent(&agent, repository),
            attached: false,
            last_seen: None,
            pid: None,
            process_identity: None,
            // The durable document records no process anchors (issue #332);
            // startup reconciliation re-observes them, so restore leaves these
            // empty exactly as it does for `pid` and `process_identity`.
            worker_identities: Vec::new(),
            lifecycle_generation: record.runtime.invocation_generation,
        });
    agent.persisted_launch_signature = Some(record.launch_signature.clone());
    agent
}

fn agent_status(last_known: LastKnownRuntime) -> AgentStatus {
    match last_known {
        LastKnownRuntime::Running => AgentStatus::Running,
        LastKnownRuntime::Stopped => AgentStatus::Dead,
        LastKnownRuntime::Unknown => AgentStatus::Queued,
    }
}

fn agent_kind_from_type(type_id: &Id) -> AgentKind {
    if type_id.as_str() == "core.code-puppy" {
        AgentKind::CodePuppy
    } else {
        AgentKind::Llxprt
    }
}
fn agent_origin_from_text(value: &str) -> Option<AgentOrigin> {
    match value {
        "persistent" => Some(AgentOrigin::Persistent),
        "transient" => Some(AgentOrigin::Transient),
        _ => None,
    }
}
fn sandbox_engine_from_text(value: &str) -> Option<crate::domain::SandboxEngine> {
    match value {
        "podman" => Some(crate::domain::SandboxEngine::Podman),
        "docker" => Some(crate::domain::SandboxEngine::Docker),
        "seatbelt" | "sandbox-exec" => Some(crate::domain::SandboxEngine::Seatbelt),
        _ => None,
    }
}
fn pane_focus_from_text(value: &str) -> PaneFocus {
    match value {
        "agents" => PaneFocus::Agents,
        "terminal" => PaneFocus::Terminal,
        _ => PaneFocus::Repositories,
    }
}

fn typed_string(values: &TypedMap, field: &str) -> Option<String> {
    match typed_field(values, field)? {
        crate::domain::TypedValue::String(value) => Some(value.clone()),
        _ => None,
    }
}

fn typed_bool(values: &TypedMap, field: &str) -> Option<bool> {
    match typed_field(values, field)? {
        crate::domain::TypedValue::Bool(value) => Some(*value),
        _ => None,
    }
}

fn typed_integer(values: &TypedMap, field: &str) -> Option<i64> {
    match typed_field(values, field)? {
        crate::domain::TypedValue::Integer(value) => Some(*value),
        _ => None,
    }
}

fn typed_string_list(values: &TypedMap, field: &str) -> Option<Vec<String>> {
    match typed_field(values, field)? {
        crate::domain::TypedValue::List(items) => Some(
            items
                .iter()
                .filter_map(|item| match item {
                    crate::domain::TypedValue::String(value) => Some(value.clone()),
                    _ => None,
                })
                .collect(),
        ),
        _ => None,
    }
}

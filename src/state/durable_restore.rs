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
    Agent, AgentId, AgentOrigin, AgentRecord, AgentStatus, Id, LastKnownRuntime, RepoPreferences,
    Repository, RepositoryId, RepositoryLocation, RepositoryRecord, RuntimeBinding, StateV2,
    TypedMap, UserPreferences,
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
        agents.push(restore_agent(record, repository)?);
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
    let default_type_id = active_type_id(&record.agent_defaults.type_id)?;
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

    let mut repository = Repository::new(
        RepositoryId(record.id.to_string()),
        default_type_id,
        record.agent_defaults.values.clone(),
        record.display_name.clone(),
        typed_string(values, "slug").unwrap_or_default(),
        base_dir,
    );
    repository.github_repo = typed_string(values, "github_repo").unwrap_or_default();
    repository.github_issue_pr_repo =
        typed_string(values, "github_issue_pr_repo").unwrap_or_default();
    repository.remote = remote;
    repository.issue_base_prompt = typed_string(values, "issue_base_prompt").unwrap_or_default();
    repository.transient_agent_dir = typed_string(values, "transient_agent_dir")
        .map(PathBuf::from)
        .unwrap_or_default();
    repository.transient_max_concurrent = typed_integer(values, "transient_max_concurrent")
        .and_then(|value| u32::try_from(value).ok())
        .unwrap_or(0);
    Ok(repository)
}

fn restore_agent(record: &AgentRecord, repository: &Repository) -> Projected<Agent> {
    let values = &record.values;
    let id = AgentId(record.id.to_string());
    let type_id = active_type_id(&record.type_id)?;
    let mut agent = Agent::new(
        id.clone(),
        repository.id.clone(),
        type_id,
        values.clone(),
        typed_string(values, "name").unwrap_or_default(),
        typed_string(values, "work_dir")
            .map(PathBuf::from)
            .unwrap_or_default(),
    );
    agent.display_id = typed_string(values, "display_id").unwrap_or(id.0);
    agent.shortcut_slot =
        typed_integer(values, "shortcut_slot").and_then(|value| u8::try_from(value).ok());
    agent.description = typed_string(values, "description").unwrap_or_default();
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
            launch_signature: record.launch_signature.clone(),
            attached: false,
            last_seen: None,
            // Each role is restored from its own recorded slot, so neither can
            // be inferred from the other (issue #543). Reconciliation still
            // re-observes liveness; these are anchors, not proof of life.
            pane_identity: record.runtime.pane_identity,
            worker_identity: record.runtime.worker_identity,
            // The durable document records no descendant anchors (issue #332);
            // startup reconciliation re-observes them.
            worker_identities: Vec::new(),
            lifecycle_generation: record.runtime.invocation_generation,
        });
    agent.persisted_launch_signature = Some(record.launch_signature.clone());
    Ok(agent)
}

fn active_type_id(type_id: &Id) -> Projected<crate::domain::agent_definition::AgentTypeId> {
    crate::domain::agent_definition::AgentDefinition::shipped()
        .into_iter()
        .find(|definition| definition.id.as_str() == type_id.as_str())
        .map(|definition| definition.id)
        .ok_or_else(|| ProjectionError::new(format!("unknown active agent type id {type_id}")))
}

fn agent_status(last_known: LastKnownRuntime) -> AgentStatus {
    match last_known {
        LastKnownRuntime::Running => AgentStatus::Running,
        LastKnownRuntime::Stopped => AgentStatus::Dead,
        LastKnownRuntime::Unknown => AgentStatus::Queued,
    }
}

fn agent_origin_from_text(value: &str) -> Option<AgentOrigin> {
    match value {
        "persistent" => Some(AgentOrigin::Persistent),
        "transient" => Some(AgentOrigin::Transient),
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

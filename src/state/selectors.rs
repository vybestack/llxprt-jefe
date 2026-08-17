//! Pure derived-state selectors.

use super::{Agent, AppState, Repository, RepositoryId};
use crate::domain::agent_definition::{AgentDefinition, AgentTypeId};
use crate::domain::canonical_values::typed_field;
use crate::domain::{AgentChooserGitMetadata, AgentId, TypedValue};
use std::collections::HashMap;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChooserAgentInfo {
    pub agent_id: AgentId,
    pub name: String,
    pub type_id: AgentTypeId,
    pub type_display_name: String,
    pub runtime_config_name: String,
    pub runtime_config: String,
    pub is_remote: bool,
    pub github_repo: String,
    pub work_dir: std::path::PathBuf,
}

impl AppState {
    #[must_use]
    pub fn visible_repository_indices(&self) -> Vec<usize> {
        self.repositories
            .iter()
            .enumerate()
            .filter_map(|(idx, repository)| {
                (!self.hide_idle_repositories
                    || self.has_visible_agent_in_repository(&repository.id)
                    || self
                        .sticky_visibility
                        .empty_repositories
                        .contains(&repository.id))
                .then_some(repository)
                .filter(|repository| self.dashboard_search_matches(&repository.name))
                .map(|_| idx)
            })
            .collect()
    }

    #[must_use]
    pub fn selected_repository_visible_index(&self) -> Option<usize> {
        let selected = self.selected_repository_index?;
        self.visible_repository_indices()
            .iter()
            .position(|idx| *idx == selected)
    }

    #[must_use]
    pub fn agent_indices_for_repository(&self, repository_id: &RepositoryId) -> Vec<usize> {
        self.agents
            .iter()
            .enumerate()
            .filter_map(|(idx, agent)| {
                (&agent.repository_id == repository_id
                    && self.is_agent_visible_with_idle_filter(agent))
                .then_some(idx)
            })
            .collect()
    }

    #[must_use]
    pub fn visible_agents_for_repository(&self, repository_id: &RepositoryId) -> Vec<Agent> {
        self.agent_indices_for_repository(repository_id)
            .iter()
            .filter_map(|idx| self.agents.get(*idx).cloned())
            .collect()
    }

    #[must_use]
    pub fn visible_agent_count_for_repository(&self, repository_id: &RepositoryId) -> usize {
        self.agent_indices_for_repository(repository_id).len()
    }

    #[must_use]
    pub fn visible_agent_count(&self) -> usize {
        self.agents
            .iter()
            .filter(|agent| self.is_agent_visible_with_idle_filter(agent))
            .count()
    }

    #[must_use]
    pub fn selected_repository(&self) -> Option<&Repository> {
        self.selected_repository_index
            .and_then(|index| self.repositories.get(index))
    }

    #[must_use]
    pub fn selected_agent(&self) -> Option<&Agent> {
        let repository_id = self.selected_repository_id()?;
        let selected_idx = self.selected_agent_index?;
        let agent = self.agents.get(selected_idx)?;
        (&agent.repository_id == repository_id && self.is_agent_visible_with_idle_filter(agent))
            .then_some(agent)
    }

    #[must_use]
    pub fn is_kennel_mode(&self) -> bool {
        let Some(agent) = self.selected_agent() else {
            return false;
        };
        AgentDefinition::shipped()
            .into_iter()
            .find(|definition| definition.id == agent.type_id)
            .is_some_and(|definition| {
                definition
                    .agent_fields
                    .iter()
                    .any(|field| field.id == "interactive")
            })
    }

    #[must_use]
    pub fn chooser_agents_for_repository(
        &self,
        repository_id: Option<&RepositoryId>,
    ) -> Vec<ChooserAgentInfo> {
        let definitions = AgentDefinition::shipped();
        self.agents
            .iter()
            .filter(|agent| !agent.is_running())
            .filter(|agent| repository_id.is_some_and(|id| agent.repository_id == *id))
            .filter(|agent| self.is_chooser_agent_available(agent))
            .filter_map(|agent| {
                let repository = self.repository_by_id(&agent.repository_id)?;
                let definition = definitions
                    .iter()
                    .find(|definition| definition.id == agent.type_id)?;
                let (runtime_config_name, runtime_config) = runtime_config_value(agent, definition);
                Some(ChooserAgentInfo {
                    agent_id: agent.id.clone(),
                    name: agent.name.clone(),
                    type_id: agent.type_id.clone(),
                    type_display_name: definition.display_name.clone(),
                    runtime_config_name,
                    runtime_config,
                    is_remote: repository.remote.enabled,
                    github_repo: repository.github_repo.clone(),
                    work_dir: agent.work_dir.clone(),
                })
            })
            .collect()
    }

    fn is_chooser_agent_available(&self, agent: &Agent) -> bool {
        let remote = self
            .repository_by_id(&agent.repository_id)
            .is_some_and(|repository| repository.remote.enabled);
        remote || self.is_agent_type_selectable(&agent.type_id)
    }

    /// Whether an agent type may be offered as a send-to-agent target.
    ///
    /// `available_agent_type_ids` records types the startup probe positively
    /// confirmed as `InstalledCompatible`. That list is empty until the async
    /// probe answers — seconds on Windows, where the `llxprt` npm shim spawns
    /// a Node process — and it stays empty for the rest of the session if the
    /// probe times out, because nothing re-probes. Gating the chooser on it
    /// alone made `Shift+S` silently refuse during that window (issue #633).
    ///
    /// So the gate is widened, not replaced: a positive verdict still counts,
    /// and in addition a type whose probe is still in flight or whose probe
    /// failed is treated as selectable, because neither outcome is evidence
    /// the executable is absent. A definitive `NotFound` or an
    /// `InstalledIncompatible` verdict still excludes the type.
    ///
    /// This mirrors launch admission (issues #587/#553/#575), which already
    /// refuses to let a startup verdict outlive the startup that produced it.
    #[must_use]
    fn is_agent_type_selectable(&self, type_id: &AgentTypeId) -> bool {
        if self.available_agent_type_ids.contains(type_id) {
            return true;
        }
        self.agent_type_availability
            .iter()
            .find(|observation| observation.type_id() == type_id)
            .is_some_and(|observation| {
                observation.enabled()
                    && (observation.pending_generation().is_some()
                        || matches!(
                            observation.availability(),
                            crate::domain::agent_definition::Availability::ProbeError { .. }
                        ))
            })
    }

    #[must_use]
    pub fn is_transient_available_for_repo(&self, repo_id: Option<&RepositoryId>) -> bool {
        let Some(repository) = repo_id.and_then(|id| self.repository_by_id(id)) else {
            return false;
        };
        if repository.github_repo.trim().is_empty() {
            return false;
        }
        repository.remote.enabled || self.is_agent_type_selectable(&repository.default_type_id)
    }

    #[must_use]
    pub fn running_transient_count(&self, repo_id: &RepositoryId) -> usize {
        self.agents
            .iter()
            .filter(|agent| {
                agent.is_transient() && agent.repository_id == *repo_id && agent.is_running()
            })
            .count()
    }

    #[must_use]
    pub fn last_error_title(&self) -> Option<String> {
        self.errors_state
            .last_visible_error()
            .map(|entry| entry.title.clone())
    }
}

pub fn build_chooser_entries_from_state(
    state: &AppState,
    repository_id: Option<&RepositoryId>,
    metadata: &[AgentChooserGitMetadata],
) -> Vec<crate::domain::AgentChooserEntry> {
    let metadata_map: HashMap<&AgentId, &AgentChooserGitMetadata> = {
        let mut map = HashMap::with_capacity(metadata.len());
        for entry in metadata {
            map.entry(&entry.agent_id).or_insert(entry);
        }
        map
    };
    state
        .chooser_agents_for_repository(repository_id)
        .into_iter()
        .map(|info| {
            let metadata = metadata_map.get(&info.agent_id).copied();
            crate::domain::AgentChooserEntry {
                agent_id: info.agent_id,
                name: info.name,
                type_id: info.type_id,
                type_display_name: info.type_display_name,
                runtime_config_name: info.runtime_config_name,
                runtime_config: crate::domain::ChooserRuntimeConfig::new(info.runtime_config),
                branch: metadata.and_then(|entry| entry.branch.clone()),
                dirty: metadata.map_or(crate::domain::DirtyStatus::unknown(), |entry| entry.dirty),
            }
        })
        .collect()
}

fn runtime_config_value(agent: &Agent, definition: &AgentDefinition) -> (String, String) {
    let field = definition.repository_fields.iter().find(|field| {
        field.launch_signature
            && matches!(
                field.kind,
                crate::domain::agent_definition::FieldKind::String
                    | crate::domain::agent_definition::FieldKind::Enum
            )
    });
    let Some(field) = field else {
        return ("config".to_owned(), String::new());
    };
    let value = match typed_field(&agent.values, &field.id) {
        Some(TypedValue::String(value)) => value.clone(),
        _ => String::new(),
    };
    (field.id.replace('_', " "), value)
}

//! AppState selector and query methods (extracted from mod.rs).
//!
//! @plan PLAN-20260216-FIRSTVERSION-V1.P03
//! @requirement REQ-TECH-001

use super::{Agent, AppState, Repository, RepositoryId};
use crate::domain::AgentId;

impl AppState {
    #[must_use]
    pub fn visible_repository_indices(&self) -> Vec<usize> {
        self.repositories
            .iter()
            .enumerate()
            .filter_map(|(idx, repository)| {
                (!self.hide_idle_repositories
                    || self.has_visible_agent_in_repository(&repository.id))
                .then_some(idx)
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

    /// Return the visible agents for a repository, respecting the idle filter.
    ///
    /// This uses `agent_indices_for_repository` internally so the returned
    /// list is always consistent with `selected_agent_local_index`.
    #[must_use]
    pub fn visible_agents_for_repository(&self, repository_id: &RepositoryId) -> Vec<Agent> {
        self.agent_indices_for_repository(repository_id)
            .iter()
            .filter_map(|idx| self.agents.get(*idx).cloned())
            .collect()
    }

    /// Count of visible agents for a repository, respecting the idle filter.
    #[must_use]
    pub fn visible_agent_count_for_repository(&self, repository_id: &RepositoryId) -> usize {
        self.agent_indices_for_repository(repository_id).len()
    }

    /// Total count of visible agents across all repositories.
    #[must_use]
    pub fn visible_agent_count(&self) -> usize {
        self.agents
            .iter()
            .filter(|agent| self.is_agent_visible_with_idle_filter(agent))
            .count()
    }

    /// Get the currently selected repository, if any.
    #[must_use]
    pub fn selected_repository(&self) -> Option<&Repository> {
        self.selected_repository_index
            .and_then(|i| self.repositories.get(i))
    }

    /// Get the currently selected agent, if any.
    #[must_use]
    pub fn selected_agent(&self) -> Option<&Agent> {
        let repository_id = self.selected_repository_id()?;
        let selected_idx = self.selected_agent_index?;
        let agent = self.agents.get(selected_idx)?;
        (&agent.repository_id == repository_id && self.is_agent_visible_with_idle_filter(agent))
            .then_some(agent)
    }

    /// Build the list of selectable agents for the issue/PR agent chooser.
    ///
    /// Filters to non-running agents in the specified repository, hiding
    /// agents whose local runtime kind is not installed — **unless** the
    /// agent belongs to a remote-enabled repository (remote PATH resolution
    /// is authoritative).
    ///
    /// This is the single shared selector consumed by both the issue and PR
    /// chooser open paths, ensuring repository scoping and availability
    /// filtering are identical.
    #[must_use]
    pub fn chooser_agents_for_repository(
        &self,
        repository_id: Option<&RepositoryId>,
    ) -> Vec<(AgentId, String)> {
        self.agents
            .iter()
            .filter(|a| !a.is_running())
            .filter(|a| repository_id.is_some_and(|rid| a.repository_id == *rid))
            .filter(|a| self.is_chooser_agent_available(a))
            .map(|a| (a.id.clone(), a.name.clone()))
            .collect()
    }

    /// Whether an agent should appear in the chooser based on availability.
    ///
    /// Remote-enabled agents always pass. Local agents pass only when their
    /// runtime kind is in the installed snapshot.
    fn is_chooser_agent_available(&self, agent: &Agent) -> bool {
        let repo_remote = self
            .repository_by_id(&agent.repository_id)
            .is_some_and(|r| r.remote.enabled);
        repo_remote || self.installed_agent_kinds.contains(&agent.agent_kind)
    }
}

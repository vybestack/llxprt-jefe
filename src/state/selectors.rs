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

    /// Whether the currently selected agent is a Kennel-mode (Code Puppy) agent.
    ///
    /// Centralizes the `kennel_mode` projection shared by all screen renderers
    /// (split, issues, pull_requests) so the derivation cannot drift.
    #[must_use]
    pub fn is_kennel_mode(&self) -> bool {
        self.selected_agent()
            .is_some_and(|a| a.agent_kind.is_kennel())
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

    /// Whether the transient-agent slot should be available for a repository
    /// (issue #213).
    ///
    /// The transient slot is available when:
    /// - The repository has a nonblank `github_repo` (needed for cloning), and
    /// - At least one agent kind is installed (or the repo is remote-enabled,
    ///   where PATH resolution is authoritative).
    #[must_use]
    pub fn is_transient_available_for_repo(&self, repo_id: Option<&RepositoryId>) -> bool {
        let Some(repo) = repo_id.and_then(|rid| self.repository_by_id(rid)) else {
            return false;
        };
        if repo.github_repo.trim().is_empty() {
            return false;
        }
        // Remote-enabled repos can always run agents; local repos need the
        // repository's default_agent_kind to be installed.
        repo.remote.enabled
            || self
                .installed_agent_kinds
                .contains(&repo.default_agent_kind)
    }

    /// Count running transient agents for a repository (issue #213).
    ///
    /// Used by the send orchestration to check whether `transient_max_concurrent`
    /// has been reached.
    #[must_use]
    pub fn running_transient_count(&self, repo_id: &RepositoryId) -> usize {
        self.agents
            .iter()
            .filter(|a| a.is_transient() && a.repository_id == *repo_id && a.is_running())
            .count()
    }
}

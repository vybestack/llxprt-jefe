//! AppState selector and query methods (extracted from mod.rs).
//!
//! @plan PLAN-20260216-FIRSTVERSION-V1.P03
//! @requirement REQ-TECH-001

use super::{Agent, AppState, Repository, RepositoryId};
use crate::domain::{AgentChooserGitMetadata, AgentId, AgentKind};
use std::collections::HashMap;

/// Pure projection of an agent's identity fields needed to construct an
/// [`crate::domain::AgentChooserEntry`].
///
/// Does not carry dirty status — that is resolved at the `app_input`
/// boundary via `GitRepoInfo::resolve`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChooserAgentInfo {
    pub agent_id: AgentId,
    pub name: String,
    pub kind: AgentKind,
    /// Configured profile (LLxprt) or model (Code Puppy).
    pub runtime_config: String,
    /// Whether the agent's repository is remote-enabled (dirty unknown).
    pub is_remote: bool,
    /// The repository's `github_repo` for git probing.
    pub github_repo: String,
    /// The agent's working directory for git probing.
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
    /// agent's repository is remote-enabled, in which case the remote PATH
    /// resolution is authoritative and the local installed-kind check is
    /// bypassed. Remote status does **not** exclude an agent; it only relaxes
    /// the availability filter.
    ///
    /// Returns pure identity projections ([`ChooserAgentInfo`]) carrying the
    /// fields needed to construct typed chooser entries. Dirty status is NOT
    /// resolved here (the selector is deterministic and never executes git);
    /// the `app_input` boundary resolves it via `GitRepoInfo::resolve`.
    ///
    /// This is the single shared selector consumed by both the issue and PR
    /// chooser open paths, ensuring repository scoping and availability
    /// filtering are identical.
    #[must_use]
    pub fn chooser_agents_for_repository(
        &self,
        repository_id: Option<&RepositoryId>,
    ) -> Vec<ChooserAgentInfo> {
        self.agents
            .iter()
            .filter(|a| !a.is_running())
            .filter(|a| repository_id.is_some_and(|rid| a.repository_id == *rid))
            .filter(|a| self.is_chooser_agent_available(a))
            .filter_map(|a| {
                // Fail-closed: if an agent's repository_id does not resolve
                // in state (orphaned/corrupt data), the agent is dropped
                // rather than shown in the chooser. This pure selector must
                // NOT log, panic, or fall back to a guess — callers rely on
                // it being deterministic and side-effect-free.
                let repo = self.repository_by_id(&a.repository_id)?;
                Some(ChooserAgentInfo {
                    agent_id: a.id.clone(),
                    name: a.name.clone(),
                    kind: a.agent_kind,
                    runtime_config: runtime_config_value(a),
                    is_remote: repo.remote.enabled,
                    github_repo: repo.github_repo.clone(),
                    work_dir: a.work_dir.clone(),
                })
            })
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

    /// Last error title for status-bar display (issue #292).
    ///
    /// Centralizes the `last_error().map(|e| e.title.clone())` lookup so all
    /// screen files share one source of truth.
    #[must_use]
    pub fn last_error_title(&self) -> Option<String> {
        self.errors_state.last_error().map(|e| e.title.clone())
    }
}

/// Build typed chooser entries by joining state-computed eligible agents with
/// effect-derived Git metadata.
///
/// The reducer calls the pure selector
/// [`AppState::chooser_agents_for_repository`] to get currently eligible
/// agents (authoritative identity: name, kind, runtime config, repo
/// scoping, non-running, available kind). Then it joins only the Git metadata
/// (branch + dirty) whose [`AgentId`] matches an eligible agent.
///
/// Metadata for agents that are no longer eligible (removed, running,
/// cross-repo, unavailable kind) is silently dropped — the chooser never
/// displays or selects stale/injected agents.
///
/// This function is deterministic and performs NO git subprocess calls. It is
/// the single shared builder consumed by both the issue and PR chooser open
/// paths.
pub fn build_chooser_entries_from_state(
    state: &AppState,
    repository_id: Option<&RepositoryId>,
    metadata: &[AgentChooserGitMetadata],
) -> Vec<crate::domain::AgentChooserEntry> {
    // Build an AgentId-keyed map for O(1) lookups. If duplicate metadata
    // entries exist for the same AgentId, the FIRST one wins (matching the
    // previous `.find()` semantics). This is a defensive choice: callers
    // should not produce duplicates, but first-wins is the safest stable
    // behavior.
    let metadata_map: HashMap<&AgentId, &AgentChooserGitMetadata> = {
        let mut map = HashMap::with_capacity(metadata.len());
        for m in metadata {
            map.entry(&m.agent_id).or_insert(m);
        }
        map
    };
    let infos = state.chooser_agents_for_repository(repository_id);
    infos
        .into_iter()
        .map(|info| {
            let md = metadata_map.get(&info.agent_id).copied();
            crate::domain::AgentChooserEntry {
                agent_id: info.agent_id,
                name: info.name,
                kind: info.kind,
                runtime_config: crate::domain::ChooserRuntimeConfig::new(info.runtime_config),
                branch: md.and_then(|m| m.branch.clone()),
                dirty: md.map_or(crate::domain::DirtyStatus::unknown(), |m| m.dirty),
            }
        })
        .collect()
}

/// Resolve the runtime config value for chooser display.
/// Reports the agent's **own** configured field exactly:
/// - LLxprt agents: `agent.profile` (no repo-default fallback).
/// - Code Puppy agents: `agent.code_puppy_model` (no repo-default fallback).
///
/// Empty/whitespace values are preserved so the pure label projection can
/// render `(default)` — indicating the runtime's own default is in effect.
/// This matches the launch-signature behavior for the LLxprt profile field
/// (`agent.profile` is used directly). The chooser shows what the agent is
/// configured with, not what the effective launch value would be after
/// repository-default resolution.
fn runtime_config_value(agent: &Agent) -> String {
    match agent.agent_kind {
        AgentKind::Llxprt => agent.profile.clone(),
        AgentKind::CodePuppy => agent.code_puppy_model.clone(),
    }
}

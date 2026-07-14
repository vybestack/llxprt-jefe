//! Resolved dashboard Git display snapshots for rendering and selection copy.

use crate::git_info::GitRepoInfo;
use crate::state::AppState;

/// Immutable Git display data parallel to the visible dashboard agents.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct DashboardGitInfoSnapshot {
    pub agents: Vec<GitRepoInfo>,
    pub preview: Option<GitRepoInfo>,
}

/// Resolve Git display data at the application input/render boundary.
#[must_use]
pub fn resolve_dashboard_git_info(state: &AppState) -> Option<DashboardGitInfoSnapshot> {
    let repository = state.selected_repository()?;
    let agents = state.visible_agents_for_repository(&repository.id);
    let infos = agents
        .iter()
        .map(|agent| {
            GitRepoInfo::resolve(
                &repository.github_repo,
                repository.remote.enabled,
                &agent.work_dir,
            )
        })
        .collect::<Vec<_>>();
    let preview = state
        .selected_agent_local_index()
        .and_then(|index| infos.get(index).cloned());
    Some(DashboardGitInfoSnapshot {
        agents: infos,
        preview,
    })
}

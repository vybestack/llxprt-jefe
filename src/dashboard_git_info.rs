//! Resolved dashboard Git display snapshots for rendering and selection copy.

use crate::git_info::GitRepoInfo;
use crate::state::AppState;

/// Immutable Git display data parallel to the visible dashboard agents.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct DashboardGitInfoSnapshot {
    pub agents: Vec<GitRepoInfo>,
    pub preview: Option<GitRepoInfo>,
}

/// Resolve Git display data for the selected agent alone.
///
/// The dashboard preview needs one work dir, not the whole visible list, so
/// it resolves that one rather than probing every visible agent to use a
/// single result. The arguments are the three
/// [`resolve_dashboard_git_info`] passes, so the branch and dirty state the
/// preview shows come from the same probe the pre-cutover dashboard used
/// instead of the origin-only constructor, which hardcodes `branch: None` and
/// can therefore only ever render `(unknown)` (#733).
///
/// `None` when nothing is selected or the selected agent's repository is not
/// in state; the preview then has no agent to describe either.
#[must_use]
pub fn resolve_preview_git_info(state: &AppState) -> Option<GitRepoInfo> {
    let agent = state.selected_agent()?;
    let repository = state.repository_by_id(&agent.repository_id)?;
    Some(GitRepoInfo::resolve(
        &repository.github_repo,
        repository.remote.enabled,
        &agent.work_dir,
    ))
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

//! State mutation helpers for deleting agents and repositories.
//!
//! These operate directly on `AppState` and are used by the binary's
//! modal confirmation handlers.

use tracing::warn;

use crate::domain::{AgentId, RepositoryId};
use crate::state::{AppState, PaneFocus};

/// Delete the currently selected repository from state.
pub fn delete_selected_repository(state: &mut AppState, repository_id: &RepositoryId) {
    if let Some(repo_idx) = state
        .repositories
        .iter()
        .position(|r| &r.id == repository_id)
    {
        state.repositories.remove(repo_idx);

        // Remove all agents belonging to the deleted repository.
        state
            .agents
            .retain(|agent| &agent.repository_id != repository_id);

        // Drop the deleted repo's remembered preferences so they cannot be
        // restored if the id is ever reused (issue #163).
        state.user_preferences.remove_for_repo(repository_id);

        if state.repositories.is_empty() {
            state.selected_repository_index = None;
            state.selected_agent_index = None;
            state.pane_focus = PaneFocus::Repositories;
            state.rebuild_repository_agent_ids();
            state.normalize_selection_indices();
            return;
        }

        let next_repo_idx = repo_idx.min(state.repositories.len().saturating_sub(1));
        state.selected_repository_index = Some(next_repo_idx);

        let selected_repo_id = state.repositories[next_repo_idx].id.clone();
        state.selected_agent_index = state
            .agent_indices_for_repository(&selected_repo_id)
            .first()
            .copied();

        if state.selected_agent_index.is_none() {
            state.pane_focus = PaneFocus::Repositories;
            state.terminal_focused = false;
        }

        state.rebuild_repository_agent_ids();
        state.normalize_selection_indices();
    }
}

/// Delete a selected agent from state and optionally remove its working directory.
pub fn delete_selected_agent(
    state: &mut AppState,
    agent_id: &AgentId,
    delete_work_dir: bool,
) -> Option<AgentId> {
    let agent_idx = state.agents.iter().position(|a| &a.id == agent_id)?;

    let removed_agent = state.agents.remove(agent_idx);
    let repository_remote_enabled = state
        .repositories
        .iter()
        .find(|repository| repository.id == removed_agent.repository_id)
        .is_some_and(|repository| repository.remote.enabled);
    if delete_work_dir
        && !repository_remote_enabled
        && removed_agent.work_dir.exists()
        && let Err(e) = std::fs::remove_dir_all(&removed_agent.work_dir)
    {
        warn!(
            error = %e,
            work_dir = %removed_agent.work_dir.display(),
            "could not remove work directory"
        );
    }

    let selected_repo_id = state
        .selected_repository_index
        .and_then(|idx| state.repositories.get(idx).map(|r| r.id.clone()));

    state.selected_agent_index = selected_repo_id
        .as_ref()
        .and_then(|repo_id| state.agent_indices_for_repository(repo_id).first().copied());

    if state.selected_agent_index.is_none() {
        state.pane_focus = PaneFocus::Repositories;
        state.terminal_focused = false;
    }

    state.rebuild_repository_agent_ids();
    state.normalize_selection_indices();

    Some(removed_agent.id)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::{Agent, RemoteRepositorySettings, Repository};
    use std::path::PathBuf;

    #[test]
    fn delete_selected_agent_skips_local_directory_removal_for_remote_repository() {
        let repo_id = RepositoryId("repo-1".into());
        let agent_id = AgentId("agent-1".into());

        // Use a real temp directory so the work_dir.exists() guard is exercised.
        let tmp_dir = std::env::temp_dir().join("jefe-test-remote-skip");
        if let Err(err) = std::fs::create_dir_all(&tmp_dir) {
            panic!("create temp dir: {err}");
        }

        let mut repository = Repository::new(
            repo_id.clone(),
            "Remote Repo".into(),
            "remote-repo".into(),
            PathBuf::from("/srv/agents"),
        );
        repository.remote = RemoteRepositorySettings {
            enabled: true,
            login_user: "ubuntu".into(),
            host: "192.0.2.10".into(),
            run_as_user: "acoliver".into(),
            setup_env_default: true,
        };

        let mut agent = Agent::new(
            agent_id.clone(),
            repo_id.clone(),
            "Agent One".into(),
            tmp_dir.clone(),
        );
        agent.status = crate::domain::AgentStatus::Running;

        let mut state = AppState::default();
        state.repositories.push(repository);
        state.agents.push(agent);
        state.selected_repository_index = Some(0);
        state.selected_agent_index = Some(0);
        state.rebuild_repository_agent_ids();
        state.normalize_selection_indices();

        let removed = delete_selected_agent(&mut state, &agent_id, true);

        assert_eq!(removed, Some(agent_id));
        assert!(state.agents.is_empty());
        assert!(state.repositories[0].remote.enabled);
        assert_eq!(state.selected_agent_index, None);
        // The directory must still exist because remote repos skip removal.
        assert!(
            tmp_dir.exists(),
            "remote agent work dir should not be deleted"
        );

        // Clean up.
        let _ = std::fs::remove_dir_all(&tmp_dir);
    }
}

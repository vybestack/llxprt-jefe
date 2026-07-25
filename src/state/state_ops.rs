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
        // Capture the agents about to be removed so their shell windows can
        // be cleaned up too (issue #361 PR A).
        let removed_agent_ids: Vec<AgentId> = state
            .agents
            .iter()
            .filter(|agent| &agent.repository_id == repository_id)
            .map(|agent| agent.id.clone())
            .collect();
        state
            .agents
            .retain(|agent| &agent.repository_id != repository_id);
        for agent_id in &removed_agent_ids {
            state.remove_shell_window(agent_id);
            state.clear_dead_preview(agent_id);
        }

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
    // Immediate shell-inventory cleanup on agent deletion (issue #361 PR A):
    // the agent is gone from state, so any tracked shell window must be too.
    state.remove_shell_window(&removed_agent.id);
    // Clear the runtime-only dead preview for the deleted agent (issue #374 S4).
    state.clear_dead_preview(&removed_agent.id);
    let repository_remote_enabled = state
        .repositories
        .iter()
        .find(|repository| repository.id == removed_agent.repository_id)
        .is_some_and(|repository| repository.remote.enabled);
    if delete_work_dir
        && !repository_remote_enabled
        && removed_agent.work_dir.exists()
        && !work_dir_shared_by_sibling(state, &removed_agent)
        && let Err(e) = std::fs::remove_dir_all(&removed_agent.work_dir)
    {
        warn!(
            error = %e,
            work_dir = %removed_agent.work_dir.display(),
            "could not remove work directory"
        );
    }
    if delete_work_dir
        && !repository_remote_enabled
        && removed_agent.work_dir.exists()
        && work_dir_shared_by_sibling(state, &removed_agent)
    {
        warn!(
            work_dir = %removed_agent.work_dir.display(),
            "skipping work directory removal: another agent shares this directory"
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

/// Whether another agent in the same repository shares the removed agent's
/// work directory. Used to guard against destructive `remove_dir_all` when
/// two agents collided on the same work directory (issue #403 Bug 4).
fn work_dir_shared_by_sibling(state: &AppState, removed: &crate::domain::Agent) -> bool {
    state.agents.iter().any(|agent| {
        agent.id != removed.id
            && agent.repository_id == removed.repository_id
            && crate::services::local_paths_equivalent(&agent.work_dir, &removed.work_dir)
    })
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
            ..RemoteRepositorySettings::default()
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

    fn state_with_agent_in_shell_inventory(agent_id: &AgentId) -> AppState {
        let repo_id = RepositoryId("repo-1".into());
        let repository = Repository::new(
            repo_id.clone(),
            "Repo".into(),
            "repo".into(),
            PathBuf::from("/tmp/repo"),
        );
        let mut agent = Agent::new(
            agent_id.clone(),
            repo_id.clone(),
            "Agent".into(),
            PathBuf::from("/tmp/agent"),
        );
        agent.status = crate::domain::AgentStatus::Running;
        let mut state = AppState::default();
        state.repositories.push(repository);
        state.agents.push(agent);
        state.record_shell_window(agent_id.clone());
        state.selected_repository_index = Some(0);
        state.selected_agent_index = Some(0);
        state.rebuild_repository_agent_ids();
        state.normalize_selection_indices();
        state
    }

    #[test]
    fn delete_selected_agent_removes_shell_inventory_entry() {
        let agent_id = AgentId("agent-shell".into());
        let mut state = state_with_agent_in_shell_inventory(&agent_id);
        assert!(state.has_shell_window(&agent_id));

        delete_selected_agent(&mut state, &agent_id, false);

        assert!(
            !state.has_shell_window(&agent_id),
            "deleting an agent must remove its shell inventory entry (issue #361)"
        );
    }

    #[test]
    fn delete_selected_repository_removes_shell_inventory_for_all_its_agents() {
        let repo_id = RepositoryId("repo-1".into());
        let agent_a = AgentId("agent-a".into());
        let agent_b = AgentId("agent-b".into());
        let repository = Repository::new(
            repo_id.clone(),
            "Repo".into(),
            "repo".into(),
            PathBuf::from("/tmp/repo"),
        );
        let mut state = AppState::default();
        state.repositories.push(repository);
        for id in [&agent_a, &agent_b] {
            let mut agent = Agent::new(
                id.clone(),
                repo_id.clone(),
                "Agent".into(),
                PathBuf::from("/tmp/agent"),
            );
            agent.status = crate::domain::AgentStatus::Running;
            state.agents.push(agent);
            state.record_shell_window(id.clone());
        }
        assert!(state.has_shell_window(&agent_a));
        assert!(state.has_shell_window(&agent_b));

        delete_selected_repository(&mut state, &repo_id);

        assert!(!state.has_shell_window(&agent_a));
        assert!(!state.has_shell_window(&agent_b));
    }

    #[test]
    fn delete_selected_agent_preserves_shared_work_dir() {
        // Two agents share the same work directory (issue #403 Bug 1/4).
        // Deleting one with delete_work_dir=true must NOT remove the
        // directory because the other agent still depends on it.
        let repo_id = RepositoryId("repo-shared".into());
        let agent_a = AgentId("agent-shared-a".into());
        let agent_b = AgentId("agent-shared-b".into());

        let tmp_dir = std::env::temp_dir().join("jefe-test-shared-workdir");
        if let Err(err) = std::fs::create_dir_all(&tmp_dir) {
            panic!("create temp dir: {err}");
        }

        let repository = Repository::new(
            repo_id.clone(),
            "Shared Repo".into(),
            "shared-repo".into(),
            PathBuf::from("/tmp/shared-repo"),
        );
        let mut state = AppState::default();
        state.repositories.push(repository);

        for id in [&agent_a, &agent_b] {
            let mut agent =
                Agent::new(id.clone(), repo_id.clone(), "Agent".into(), tmp_dir.clone());
            agent.status = crate::domain::AgentStatus::Running;
            state.agents.push(agent);
        }
        state.selected_repository_index = Some(0);
        state.selected_agent_index = Some(0);
        state.rebuild_repository_agent_ids();
        state.normalize_selection_indices();

        let removed = delete_selected_agent(&mut state, &agent_a, true);

        assert_eq!(removed, Some(agent_a));
        assert_eq!(state.agents.len(), 1, "agent B should remain");
        assert!(
            tmp_dir.exists(),
            "shared work directory must not be removed when another agent uses it"
        );

        let _ = std::fs::remove_dir_all(&tmp_dir);
    }

    #[test]
    fn delete_selected_agent_removes_sole_owner_work_dir() {
        // A sole-owner agent's work directory should be removed when
        // delete_work_dir is true.
        let repo_id = RepositoryId("repo-sole".into());
        let agent_id = AgentId("agent-sole".into());

        let tmp_dir = std::env::temp_dir().join("jefe-test-sole-workdir");
        if let Err(err) = std::fs::create_dir_all(&tmp_dir) {
            panic!("create temp dir: {err}");
        }

        let repository = Repository::new(
            repo_id.clone(),
            "Sole Repo".into(),
            "sole-repo".into(),
            PathBuf::from("/tmp/sole-repo"),
        );
        let mut state = AppState::default();
        state.repositories.push(repository);

        let mut agent = Agent::new(
            agent_id.clone(),
            repo_id.clone(),
            "Agent".into(),
            tmp_dir.clone(),
        );
        agent.status = crate::domain::AgentStatus::Running;
        state.agents.push(agent);
        state.selected_repository_index = Some(0);
        state.selected_agent_index = Some(0);
        state.rebuild_repository_agent_ids();
        state.normalize_selection_indices();

        let removed = delete_selected_agent(&mut state, &agent_id, true);

        assert_eq!(removed, Some(agent_id));
        assert!(
            !tmp_dir.exists(),
            "sole-owner work directory should be removed"
        );
    }
}

//! Pure repository-scoped shell resolution (issue #364).

use crate::domain::{AgentId, AgentStatus, RepositoryId};

use super::AppState;

/// Select the owning [`AgentId`] of a resumable Running shell in `repository_id`.
///
/// The selected agent wins when it owns a shell. Remaining candidates are
/// ordered by focus recency; lower `AgentId` wins ties deterministically.
#[must_use]
pub fn resolve_repository_shell(state: &AppState, repository_id: &RepositoryId) -> Option<AgentId> {
    let selected_owner = state.selected_agent().and_then(|agent| {
        (agent.repository_id == *repository_id
            && agent.status == AgentStatus::Running
            && state.has_shell_window(&agent.id))
        .then(|| agent.id.clone())
    });
    if selected_owner.is_some() {
        return selected_owner;
    }

    state
        .agents
        .iter()
        .filter(|agent| {
            agent.repository_id == *repository_id
                && agent.status == AgentStatus::Running
                && state.has_shell_window(&agent.id)
        })
        .max_by(|left, right| {
            let recency = state
                .shell_focus_ordinal(&left.id)
                .cmp(&state.shell_focus_ordinal(&right.id));
            // `max_by` needs the lower AgentId to compare greater on a tie.
            recency.then_with(|| right.id.0.cmp(&left.id.0))
        })
        .map(|agent| agent.id.clone())
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use super::*;
    use crate::domain::{Agent, Repository};

    fn fixture() -> AppState {
        let mut state = AppState::default();
        let repository_id = RepositoryId("repo".into());
        state.repositories.push(Repository::new(
            repository_id.clone(),
            "Repo".into(),
            "repo".into(),
            PathBuf::from("/tmp"),
        ));
        for id in ["b", "a"] {
            let mut agent = Agent::new(
                AgentId(id.into()),
                repository_id.clone(),
                id.into(),
                PathBuf::from(format!("/tmp/{id}")),
            );
            agent.status = AgentStatus::Running;
            state.agents.push(agent);
            state.record_shell_window(AgentId(id.into()));
        }
        state
    }

    #[test]
    fn selected_running_owner_wins() {
        let mut state = fixture();
        state.selected_repository_index = Some(0);
        state.selected_agent_index = Some(0);
        assert_eq!(
            resolve_repository_shell(&state, &RepositoryId("repo".into())),
            Some(AgentId("b".into()))
        );
    }

    #[test]
    fn recency_then_agent_id_resolves_candidates() {
        let mut state = fixture();
        state.selected_agent_index = None;
        assert_eq!(
            resolve_repository_shell(&state, &RepositoryId("repo".into())),
            Some(AgentId("a".into()))
        );
        state.record_shell_focus(&AgentId("b".into()));
        assert_eq!(
            resolve_repository_shell(&state, &RepositoryId("repo".into())),
            Some(AgentId("b".into()))
        );
    }

    #[test]
    fn non_running_owner_is_not_resumable() {
        let mut state = fixture();
        state.agents[0].status = AgentStatus::Dead;
        state.agents[1].status = AgentStatus::Dead;
        assert_eq!(
            resolve_repository_shell(&state, &RepositoryId("repo".into())),
            None
        );
    }
}

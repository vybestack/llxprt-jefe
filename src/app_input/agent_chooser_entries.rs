//! Effect-derived Git display metadata for the send-to-agent chooser
//! (issue #230).
//!
//! This is the `app_input` boundary seam that resolves `GitRepoInfo` (which
//! may spawn cached git processes) and builds [`AgentChooserGitMetadata`]
//! keyed by [`AgentId`]. Reducers/selectors remain deterministic and never
//! call into this module — they receive the metadata via the
//! `OpenAgentChooser` / `PrOpenAgentChooser` event payload and rebuild
//! authoritative identity from `AppState` via the pure selector.
//!
//! The metadata carries ONLY Git display info (branch + dirty). Identity
//! (name, kind, config) is NOT included so the chooser never trusts
//! injected/stale identity.

use std::path::Path;

use jefe::domain::{AgentChooserGitMetadata, DirtyStatus};
use jefe::git_info::GitRepoInfo;
use jefe::state::{AppState, ChooserAgentInfo};

/// Build Git display metadata for the currently selected repository's
/// eligible agents.
///
/// Delegates eligibility/filtering to the pure selector
/// [`AppState::chooser_agents_for_repository`] to determine which agents
/// should receive metadata, then resolves branch + dirty status via
/// [`GitRepoInfo::resolve`] at this boundary. Remote repos always get
/// [`DirtyStatus::unknown`] and no branch (no SSH worktree probe).
///
/// Eligible agents are probed **concurrently** using scoped threads so that
/// cold-cache latency is bounded by the slowest single agent rather than the
/// agent count. Output order is deterministic (preserves the selector's
/// ordering) because results are collected by original index.
///
/// This function MAY spawn cached git processes; it must only be called from
/// the `app_input` layer, never from reducers or selectors.
#[must_use]
pub fn build_chooser_metadata(state: &AppState) -> Vec<AgentChooserGitMetadata> {
    let repo_id = state.selected_repository_id().cloned();
    let infos = state.chooser_agents_for_repository(repo_id.as_ref());
    if infos.is_empty() {
        return Vec::new();
    }

    // Probe all eligible agents concurrently with scoped threads. Each
    // thread borrows its `ChooserAgentInfo` immutably and produces metadata.
    // Results are collected by index to preserve the deterministic selector
    // ordering.
    let results: Vec<(usize, AgentChooserGitMetadata)> = std::thread::scope(|scope| {
        // Spawn all threads first so they run concurrently, then join.
        let handles: Vec<_> = infos
            .iter()
            .enumerate()
            .map(|(idx, info)| scope.spawn(move || (idx, agent_info_to_metadata(info.clone()))))
            .collect::<Vec<_>>();
        let mut joined = Vec::with_capacity(handles.len());
        for handle in handles {
            if let Ok(result) = handle.join() {
                joined.push(result);
            }
        }
        joined
    });

    // Sort by original index to restore deterministic order.
    let mut sorted = results;
    sorted.sort_by_key(|(idx, _)| *idx);
    sorted.into_iter().map(|(_, md)| md).collect()
}

/// Convert a pure [`ChooserAgentInfo`] projection into Git display metadata
/// by resolving branch + dirty through [`GitRepoInfo::resolve`].
fn agent_info_to_metadata(info: ChooserAgentInfo) -> AgentChooserGitMetadata {
    let (branch, dirty) = resolve_git_display(&info);
    AgentChooserGitMetadata {
        agent_id: info.agent_id,
        branch,
        dirty,
    }
}

/// Resolve branch and dirty status for an agent's working tree.
///
/// Local repos use [`GitRepoInfo::resolve`] (cached git probe). Remote repos
/// are always [`DirtyStatus::unknown`] with no branch since probing would
/// require an SSH round-trip.
fn resolve_git_display(info: &ChooserAgentInfo) -> (Option<String>, DirtyStatus) {
    if info.is_remote {
        return (None, DirtyStatus::unknown());
    }
    let git_info = GitRepoInfo::resolve(&info.github_repo, false, Path::new(&info.work_dir));
    (git_info.branch, DirtyStatus(git_info.dirty))
}

#[cfg(test)]
mod tests {
    use super::*;
    use jefe::domain::{Agent, AgentId, AgentKind, Repository, RepositoryId};
    use jefe::state::AppState;
    use std::path::PathBuf;

    fn make_repo(repo_id: &str, remote: bool) -> Repository {
        let mut repo = Repository::new(
            RepositoryId(repo_id.to_string()),
            format!("Test {repo_id}"),
            repo_id.to_string(),
            PathBuf::from("/tmp/test"),
        );
        repo.remote.enabled = remote;
        repo
    }

    fn make_agent(id: &str, repo_id: &str, kind: AgentKind) -> Agent {
        let mut agent = Agent::new(
            AgentId(id.to_string()),
            RepositoryId(repo_id.to_string()),
            format!("Agent {id}"),
            PathBuf::from("/tmp/agent"),
        );
        agent.agent_kind = kind;
        agent
    }

    fn state_with_repo_and_agents(repo_id: &str, agents: &[Agent]) -> AppState {
        let mut state = AppState::default();
        state.repositories.push(make_repo(repo_id, false));
        state.selected_repository_index = Some(0);
        state.installed_agent_kinds = vec![AgentKind::Llxprt, AgentKind::CodePuppy];
        for agent in agents {
            state.agents.push(agent.clone());
        }
        state
    }

    #[test]
    fn build_metadata_empty_when_no_agents() {
        let state = state_with_repo_and_agents("r1", &[]);
        let md = build_chooser_metadata(&state);
        assert!(md.is_empty());
    }

    #[test]
    fn build_metadata_carries_agent_id() {
        let agent = make_agent("a1", "r1", AgentKind::Llxprt);
        let state = state_with_repo_and_agents("r1", &[agent]);
        let md = build_chooser_metadata(&state);
        assert_eq!(md.len(), 1);
        assert_eq!(md[0].agent_id, AgentId("a1".to_string()));
    }

    #[test]
    fn build_metadata_remote_repo_has_no_branch_or_dirty() {
        let mut state = AppState::default();
        state.repositories.push(make_repo("r1", true));
        state.selected_repository_index = Some(0);
        state.installed_agent_kinds = vec![AgentKind::Llxprt];
        let mut agent = make_agent("a1", "r1", AgentKind::Llxprt);
        agent.profile = "ops".to_string();
        state.agents.push(agent);
        let md = build_chooser_metadata(&state);
        assert_eq!(md.len(), 1);
        assert_eq!(md[0].branch, None);
        assert_eq!(md[0].dirty, DirtyStatus::unknown());
    }

    #[test]
    fn build_metadata_local_nonexistent_workdir_has_no_branch_or_dirty() {
        let mut agent = make_agent("a1", "r1", AgentKind::Llxprt);
        agent.work_dir = PathBuf::from("/nonexistent/path/that/does/not/exist");
        agent.profile = "ops".to_string();
        let state = state_with_repo_and_agents("r1", &[agent]);
        let md = build_chooser_metadata(&state);
        assert_eq!(md.len(), 1);
        assert_eq!(md[0].branch, None);
        assert_eq!(md[0].dirty, DirtyStatus::unknown());
    }

    // ── Finding 3: agent config preserved exactly (no repo default fallback) ──

    #[test]
    fn build_metadata_includes_eligible_agent_with_custom_profile() {
        // The metadata only carries git info, but the reducer rebuilds
        // identity. Here we verify the metadata is built for the right agents
        // (those that pass the selector). The config value is NOT in metadata.
        let mut agent = make_agent("a1", "r1", AgentKind::Llxprt);
        agent.profile = "agent-profile".to_string();
        let state = state_with_repo_and_agents("r1", &[agent]);
        let md = build_chooser_metadata(&state);
        assert_eq!(md.len(), 1, "agent with own profile must be eligible");
    }

    #[test]
    fn build_metadata_preserves_deterministic_order() {
        // Concurrent probing must preserve the selector's deterministic order.
        let agents = vec![
            make_agent("c3", "r1", AgentKind::Llxprt),
            make_agent("a1", "r1", AgentKind::Llxprt),
            make_agent("b2", "r1", AgentKind::CodePuppy),
        ];
        let state = state_with_repo_and_agents("r1", &agents);
        let md = build_chooser_metadata(&state);
        assert_eq!(md.len(), 3);
        // Order must match the agent insertion order (selector preserves it).
        assert_eq!(md[0].agent_id, AgentId("c3".to_string()));
        assert_eq!(md[1].agent_id, AgentId("a1".to_string()));
        assert_eq!(md[2].agent_id, AgentId("b2".to_string()));
    }
}

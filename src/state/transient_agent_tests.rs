//! Tests for transient agent state-layer behavior (issue #213).

use crate::domain::{Agent, AgentId, AgentKind, AgentStatus, Repository, RepositoryId};
use crate::state::{AgentChooserState, AppEvent, AppState};

use std::path::PathBuf;

trait PanicOption<T> {
    fn or_panic(self, context: &str) -> T;
}

impl<T> PanicOption<T> for Option<T> {
    fn or_panic(self, context: &str) -> T {
        match self {
            Some(value) => value,
            None => panic!("{context}"),
        }
    }
}

fn make_repo_with_github(github_repo: &str) -> Repository {
    let mut repo = Repository::new(
        RepositoryId("repo-1".to_owned()),
        "Test Repo".to_owned(),
        "test-repo".to_owned(),
        PathBuf::from("/tmp/repo"),
    );
    repo.github_repo = github_repo.to_owned();
    repo
}

fn make_agent(repo_id: &RepositoryId, name: &str, status: AgentStatus) -> Agent {
    let mut agent = Agent::new(
        AgentId(format!("agent-{name}").to_lowercase()),
        repo_id.clone(),
        name.to_owned(),
        PathBuf::from("/tmp/agent"),
    );
    agent.status = status;
    agent
}

#[test]
fn is_transient_available_true_when_github_repo_set_and_kinds_installed() {
    let repo = make_repo_with_github("acme/widgets");
    let mut state = AppState::default();
    state.repositories.push(repo);
    state.selected_repository_index = Some(0);
    state.installed_agent_kinds = vec![AgentKind::Llxprt];
    assert!(state.is_transient_available_for_repo(state.selected_repository_id()));
}

#[test]
fn is_transient_available_false_when_installed_kinds_dont_match_default() {
    let repo = make_repo_with_github("acme/widgets");
    let mut state = AppState::default();
    state.repositories.push(repo);
    state.selected_repository_index = Some(0);
    // repo.default_agent_kind is Llxprt (the default); installing only
    // CodePuppy must NOT make the transient slot available.
    state.installed_agent_kinds = vec![AgentKind::CodePuppy];
    assert!(!state.is_transient_available_for_repo(state.selected_repository_id()));
}

#[test]
fn is_transient_available_false_when_no_github_repo() {
    let repo = make_repo_with_github("");
    let mut state = AppState::default();
    state.repositories.push(repo);
    state.selected_repository_index = Some(0);
    state.installed_agent_kinds = vec![AgentKind::Llxprt];
    assert!(!state.is_transient_available_for_repo(state.selected_repository_id()));
}

#[test]
fn is_transient_available_false_when_no_installed_kinds_and_not_remote() {
    let mut repo = make_repo_with_github("acme/widgets");
    repo.remote.enabled = false;
    let mut state = AppState::default();
    state.repositories.push(repo);
    state.selected_repository_index = Some(0);
    state.installed_agent_kinds = vec![];
    assert!(!state.is_transient_available_for_repo(state.selected_repository_id()));
}

#[test]
fn is_transient_available_true_for_remote_repo_even_without_installed_kinds() {
    let mut repo = make_repo_with_github("acme/widgets");
    repo.remote.enabled = true;
    let mut state = AppState::default();
    state.repositories.push(repo);
    state.selected_repository_index = Some(0);
    state.installed_agent_kinds = vec![];
    assert!(state.is_transient_available_for_repo(state.selected_repository_id()));
}

#[test]
fn running_transient_count_counts_only_running_transient_agents_for_repo() {
    let repo_id = RepositoryId("repo-1".to_owned());
    let mut state = AppState::default();

    // Running transient agent for repo-1
    let mut t1 = Agent::new_transient(
        AgentId("t1".to_owned()),
        repo_id.clone(),
        PathBuf::from("/tmp/t1"),
        &make_repo_with_github("acme/widgets"),
    );
    t1.status = AgentStatus::Running;
    state.agents.push(t1);

    // Completed transient agent (should NOT count)
    let mut t2 = Agent::new_transient(
        AgentId("t2".to_owned()),
        repo_id.clone(),
        PathBuf::from("/tmp/t2"),
        &make_repo_with_github("acme/widgets"),
    );
    t2.status = AgentStatus::Completed;
    state.agents.push(t2);

    // Running non-transient agent (should NOT count)
    state
        .agents
        .push(make_agent(&repo_id, "regular", AgentStatus::Running));

    // Running transient agent for a DIFFERENT repo (should NOT count)
    let mut t3 = Agent::new_transient(
        AgentId("t3".to_owned()),
        RepositoryId("repo-2".to_owned()),
        PathBuf::from("/tmp/t3"),
        &make_repo_with_github("acme/widgets"),
    );
    t3.status = AgentStatus::Running;
    state.agents.push(t3);

    assert_eq!(state.running_transient_count(&repo_id), 1);
}

#[test]
fn agent_chooser_navigation_bounds_include_transient_slot() {
    let chooser = AgentChooserState {
        selected_index: 0,
        agents: vec![
            (AgentId("a1".to_owned()), "Agent 1".to_owned()),
            (AgentId("a2".to_owned()), "Agent 2".to_owned()),
        ],
        transient_available: true,
    };
    // The entry count is agents.len() + transient_available = 3 entries
    // (indices 0, 1, 2). Verify navigation clamps to this bound: navigate
    // down past the last index should not exceed agents.len().
    let max_index = chooser.agents.len() + usize::from(chooser.transient_available) - 1;
    assert_eq!(max_index, 2);
    // Navigate down within bounds
    let mut state = AppState::default();
    state.issues_state.agent_chooser = Some(chooser);
    state = state.apply(AppEvent::AgentChooserNavigateDown); // 0 -> 1
    state = state.apply(AppEvent::AgentChooserNavigateDown); // 1 -> 2
    state = state.apply(AppEvent::AgentChooserNavigateDown); // 2 -> clamped at 2
    let chooser = state
        .issues_state
        .agent_chooser
        .as_ref()
        .or_panic("chooser must still be open after navigation");
    assert_eq!(
        chooser.selected_index, 2,
        "navigation must clamp at the transient slot"
    );
}

#[test]
fn transient_agent_queued_event_sets_draft_notice() {
    let mut state = AppState::default();
    state = state.apply(AppEvent::TransientAgentQueued { queue_position: 2 });
    assert!(state.issues_state.draft_notice.is_some());
    assert!(
        state
            .issues_state
            .draft_notice
            .as_deref()
            .is_some_and(|n| n.contains("position 2"))
    );
}

#[test]
fn transient_agent_dequeued_event_clears_draft_notice() {
    let mut state = AppState::default();
    state.issues_state.draft_notice = Some("queued".to_string());
    state = state.apply(AppEvent::TransientAgentDequeued);
    assert!(state.issues_state.draft_notice.is_none());
}

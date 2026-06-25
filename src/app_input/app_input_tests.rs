use super::*;
use std::path::PathBuf;

use jefe::domain::{
    Agent, AgentId, AgentStatus, DEFAULT_SANDBOX_FLAGS, LaunchSignature, RemoteRepositorySettings,
    RepositoryId, RuntimeBinding, SandboxEngine,
};

trait TestResultExt<T> {
    fn value_or_panic(self, context: &str) -> T;
}

impl<T, E: std::fmt::Debug> TestResultExt<T> for Result<T, E> {
    fn value_or_panic(self, context: &str) -> T {
        match self {
            Ok(value) => value,
            Err(error) => panic!("{context}: {error:?}"),
        }
    }
}

fn sample_signature() -> LaunchSignature {
    LaunchSignature {
        work_dir: PathBuf::from("/tmp/agent"),
        profile: String::new(),
        mode_flags: vec![String::from("--yolo")],
        llxprt_debug: String::new(),
        pass_continue: true,
        sandbox_enabled: false,
        sandbox_engine: SandboxEngine::Podman,
        sandbox_flags: DEFAULT_SANDBOX_FLAGS.to_owned(),
        remote: RemoteRepositorySettings::default(),
    }
}

fn sample_agent(agent_id: &AgentId) -> Agent {
    Agent::new(
        agent_id.clone(),
        RepositoryId(String::from("repo-1")),
        String::from("Agent One"),
        PathBuf::from("/tmp/agent"),
    )
}

#[test]
fn filter_and_search_messages_are_fresh_issue_list_reloads() {
    use super::issues_list_dispatch::is_fresh_issue_list_reload;
    assert!(is_fresh_issue_list_reload(&IssuesMessage::EnterMode));
    assert!(is_fresh_issue_list_reload(&IssuesMessage::RefocusList));
    assert!(is_fresh_issue_list_reload(&IssuesMessage::ApplyFilter));
    assert!(is_fresh_issue_list_reload(&IssuesMessage::ClearFilter));
    assert!(is_fresh_issue_list_reload(&IssuesMessage::ApplySearch));
    assert!(!is_fresh_issue_list_reload(
        &IssuesMessage::NavigatePageDown
    ));
}

#[test]
fn repository_focus_toggles_checkbox_for_expected_fields() {
    assert!(repository_focus_toggles_checkbox(
        RepositoryFormFocus::RemoteEnabled
    ));
    assert!(repository_focus_toggles_checkbox(
        RepositoryFormFocus::SetupEnvDefault
    ));
    assert!(!repository_focus_toggles_checkbox(
        RepositoryFormFocus::Name
    ));
}

#[test]
fn clear_runtime_warning_clears_only_ssh_agent_warnings() {
    let mut state = AppState {
        warning_message: Some(String::from("SSH_AUTH_SOCK is missing")),
        ..AppState::default()
    };
    clear_runtime_warning(&mut state);
    assert!(state.warning_message.is_none());

    state.warning_message = Some(String::from("regular warning"));
    clear_runtime_warning(&mut state);
    assert_eq!(state.warning_message, Some(String::from("regular warning")));
}

#[test]
fn set_agent_runtime_binding_sets_session_and_signature() {
    let agent_id = AgentId(String::from("agent-1"));
    let mut state = AppState::default();
    state.agents.push(sample_agent(&agent_id));

    let signature = sample_signature();
    set_agent_runtime_binding(
        &mut state,
        &agent_id,
        String::from("jefe-agent-1"),
        signature.clone(),
    );

    let binding = state
        .agents
        .iter()
        .find(|agent| agent.id == agent_id)
        .and_then(|agent| agent.runtime_binding.as_ref());

    assert!(binding.is_some());
    if let Some(binding) = binding {
        assert_eq!(binding.session_name, String::from("jefe-agent-1"));
        assert_eq!(binding.launch_signature, signature);
        assert!(!binding.attached);
    }
}

#[test]
fn mark_and_clear_runtime_attachment_flags() {
    let agent_a = AgentId(String::from("agent-a"));
    let agent_b = AgentId(String::from("agent-b"));

    let mut first = sample_agent(&agent_a);
    first.runtime_binding = Some(RuntimeBinding {
        session_name: String::from("sess-a"),
        launch_signature: sample_signature(),
        attached: false,
        last_seen: None,
    });

    let mut second = sample_agent(&agent_b);
    second.runtime_binding = Some(RuntimeBinding {
        session_name: String::from("sess-b"),
        launch_signature: sample_signature(),
        attached: true,
        last_seen: None,
    });

    let mut state = AppState::default();
    state.agents.push(first);
    state.agents.push(second);

    mark_agent_runtime_attached(&mut state, &agent_a, true);
    assert!(
        state.agents[0]
            .runtime_binding
            .as_ref()
            .is_some_and(|binding| binding.attached)
    );

    clear_agent_runtime_attachment(&mut state);
    assert!(state.agents.iter().all(|agent| {
        agent
            .runtime_binding
            .as_ref()
            .is_none_or(|binding| !binding.attached)
    }));
}

#[test]
fn mark_runtime_session_dead_sets_dead_and_detaches() {
    let agent_id = AgentId(String::from("agent-1"));
    let mut agent = sample_agent(&agent_id);
    agent.status = AgentStatus::Running;
    agent.runtime_binding = Some(RuntimeBinding {
        session_name: String::from("sess"),
        launch_signature: sample_signature(),
        attached: true,
        last_seen: None,
    });

    let mut state = AppState::default();
    state.agents.push(agent);

    mark_runtime_session_dead_if_present(&mut state, &agent_id);

    assert_eq!(state.agents[0].status, AgentStatus::Dead);
    assert!(
        state.agents[0]
            .runtime_binding
            .as_ref()
            .is_some_and(|binding| !binding.attached)
    );
}

#[test]
fn to_persisted_state_carries_hide_idle_toggle() {
    let state = AppState {
        hide_idle_repositories: true,
        ..AppState::default()
    };

    let persisted = to_persisted_state(&state);
    assert!(persisted.hide_idle_repositories);
}

/// to_persisted_state must EXCLUDE all prs_state data — no PR key appears in
/// the serialized JSON.
///
/// Build a PullRequest populated with non-default data.
fn test_pr(number: u64) -> jefe::domain::PullRequest {
    use jefe::domain::{PrCheckStatus, PrState};
    jefe::domain::PullRequest {
        number,
        title: format!("PR #{number}"),
        state: PrState::Open,
        author_login: "testuser".to_string(),
        updated_at: "2024-01-01T00:00:00Z".to_string(),
        head_ref: "feature".to_string(),
        base_ref: "main".to_string(),
        is_draft: false,
        review_decision: None,
        checks_status: PrCheckStatus::None,
        assignee_summary: String::new(),
        labels_summary: String::new(),
        comment_count: 0,
    }
}

/// Build a PullRequestDetail populated with non-default data.
fn test_pr_detail(number: u64) -> jefe::domain::PullRequestDetail {
    use jefe::domain::{PrCheckStatus, PrState};
    jefe::domain::PullRequestDetail {
        repo_owner_name: "owner/repo".to_string(),
        number,
        title: format!("PR #{number}"),
        state: PrState::Open,
        is_draft: false,
        author_login: "octocat".to_string(),
        created_at: "2024-01-01T00:00:00Z".to_string(),
        updated_at: "2024-01-02T00:00:00Z".to_string(),
        head_ref: "feature".to_string(),
        base_ref: "main".to_string(),
        labels: vec![],
        assignees: vec![],
        milestone: None,
        body: "PR body".to_string(),
        external_url: format!("https://github.com/owner/repo/pull/{number}"),
        review_decision: None,
        checks_status: PrCheckStatus::None,
        reviews: vec![],
        checks: vec![],
        comments: vec![],
        has_more_comments: false,
        comments_cursor: None,
    }
}

/// Build an AppState populated with non-default PR data.
fn state_with_active_prs() -> jefe::state::AppState {
    use jefe::domain::{Repository, RepositoryId};
    use jefe::state::ScreenMode;
    use std::path::PathBuf;

    let prs_state = jefe::state::PullRequestsState {
        active: true,
        pull_requests: vec![test_pr(1)],
        selected_pr_index: Some(0),
        pr_detail: Some(test_pr_detail(1)),
        ..jefe::state::PullRequestsState::default()
    };
    let mut state = jefe::state::AppState {
        screen_mode: ScreenMode::DashboardPullRequests,
        prs_state,
        ..AppState::default()
    };
    state.repositories.push(Repository::new(
        RepositoryId("repo-1".to_string()),
        "Repo 1".to_string(),
        "repo-1".to_string(),
        PathBuf::from("/tmp/repo1"),
    ));
    state.selected_repository_index = Some(0);
    state
}

/// @plan PLAN-20260624-PR-MODE.P04
/// @requirement REQ-PR-NFR-002
/// @pseudocode component-001 lines 66-76
/// NOTE: this test lives in src/app_input/app_input_tests.rs (alongside the
/// to_persisted_state_carries_hide_idle_toggle precedent) because
/// to_persisted_state is module-private to app_input (declared in main.rs as
/// `mod app_input`, NOT `pub mod app_input` in lib.rs), so it is NOT reachable
/// from a test in the src/state module without changing production visibility.
#[test]
fn test_to_persisted_state_excludes_prs_state() {
    let state = state_with_active_prs();

    let persisted = to_persisted_state(&state);
    let json = serde_json::to_value(&persisted).value_or_panic("persisted should serialize");

    let json_str = serde_json::to_string(&json).value_or_panic("json should stringify");
    let lower = json_str.to_lowercase();
    assert!(
        !lower.contains("prs_state")
            && !lower.contains("pull_request")
            && !lower.contains("pr_detail"),
        "persisted state must not carry any PR data, got: {json_str}"
    );

    assert!(json.get("repositories").is_some());
    assert!(json.get("agents").is_some());
    assert!(json.get("selected_repository_index").is_some());
    assert!(json.get("selected_agent_index").is_some());
    assert!(json.get("hide_idle_repositories").is_some());
    assert!(json.get("last_selected_agent_by_repo").is_some());
}

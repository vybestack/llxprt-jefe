use super::*;
use std::path::PathBuf;

use jefe::domain::{
    Agent, AgentId, AgentStatus, DEFAULT_SANDBOX_FLAGS, LaunchSignature, RemoteRepositorySettings,
    RepositoryId, RuntimeBinding, SandboxEngine,
};

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

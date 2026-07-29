use jefe::domain::agent_definition::AgentLaunchPlan;
use jefe::domain::{AgentId, AgentStatus, RuntimeBinding};
use jefe::runtime::agent_execution_guard::{
    AuthorizationResult, ExecutionEvidence, authorize_execution,
};
use jefe::runtime::agent_preflight::{
    AuthorizedLaunchPlan, PreparationOutcome, ProcessSandboxInspector, prepare_execution,
};
use jefe::runtime::{
    NpmPackageAvailabilityError, RuntimeError, RuntimeManager, StubRuntimeManager,
};
use jefe::state::{AppEvent, AppState, PaneFocus};

use super::relaunch::{
    ServerLostRecoveryOutcome, apply_server_lost_recovery_outcomes, attach_relaunched_session,
    open_server_lost_recovery, persist_relaunch_failure,
};
use super::tests::{sample_agent, sample_launch_signature};

/// Seal a fixture plan into an [`AuthorizedLaunchPlan`] through the real
/// authorize + preflight proof chain, using evidence derived from the plan's
/// own generation-bearing fields.
fn authorized_launch_plan(plan: &AgentLaunchPlan) -> AuthorizedLaunchPlan {
    let evidence = ExecutionEvidence::new(
        plan.definition_sha256,
        plan.executable_fingerprint.clone(),
        plan.probe_generation,
        plan.target_generation,
        plan.activation_generation,
    );
    let authorized = match authorize_execution(plan, &evidence) {
        AuthorizationResult::Authorized(authorized) => authorized,
        AuthorizationResult::Rejected(error) => panic!("fixture must authorize: {error}"),
    };
    let cleared = match prepare_execution(authorized, None, &ProcessSandboxInspector::new()) {
        PreparationOutcome::Cleared(cleared) => cleared,
        PreparationOutcome::Unavailable(reason) => panic!("fixture must clear preflight: {reason}"),
    };
    AuthorizedLaunchPlan::from_cleared(cleared, plan.clone(), evidence)
        .unwrap_or_else(|error| panic!("fixture must seal: {error}"))
}

fn bound_agent_state(agent_id: &AgentId) -> AppState {
    let mut agent = sample_agent(agent_id);
    agent.status = AgentStatus::Running;
    agent.runtime_binding = Some(RuntimeBinding {
        session_name: "jefe-relaunch-test".to_owned(),
        launch_signature: sample_launch_signature(),
        attached: true,
        last_seen: None,
        process_identity: None,
        pid: None,
        lifecycle_generation: 0,
        worker_identities: Vec::new(),
    });
    AppState {
        agents: vec![agent],
        terminal_focused: true,
        pane_focus: PaneFocus::Terminal,
        ..AppState::default()
    }
}

#[test]
fn package_disappearing_after_preflight_remains_actionable_in_visible_state() {
    let agent_id = AgentId("package-race".to_owned());
    let error =
        RuntimeError::NpmPackageAvailability(NpmPackageAvailabilityError::PackageUnresolved {
            target: "local machine".to_owned(),
            selector: "nightly".to_owned(),
            diagnostic: "package was removed".to_owned(),
        });
    let mut state = bound_agent_state(&agent_id);
    persist_relaunch_failure(
        &mut state,
        &agent_id,
        AppEvent::RelaunchAgent(agent_id.clone()),
        &error,
    );
    let message = state.error_message.as_deref().unwrap_or_default();
    assert!(message.contains("nightly"));
    assert!(message.contains("registry access"));
    assert_eq!(state.agents[0].status, AgentStatus::Dead);
    assert!(state.agents[0].runtime_binding.is_none());
    assert_eq!(state.pane_focus, PaneFocus::Agents);
    assert!(!state.terminal_focused);
}

#[test]
fn attach_failure_is_preserved_as_distinct_relaunch_diagnostic() {
    let agent_id = AgentId("attach-race".to_owned());
    let attach_error =
        RuntimeError::AttachFailed("session exited before the viewer became ready".to_owned());
    let mut runtime = StubRuntimeManager::with_attach_failure(attach_error.clone());
    let plan = authorized_launch_plan(&AgentLaunchPlan::default());
    if let Err(error) = runtime.spawn_session(&agent_id, &plan, None) {
        panic!("test session should spawn: {error}");
    }

    let result = attach_relaunched_session(&mut runtime, &agent_id);
    assert!(matches!(result, Err(RuntimeError::AttachFailed(_))));

    let mut state = bound_agent_state(&agent_id);
    persist_relaunch_failure(
        &mut state,
        &agent_id,
        AppEvent::RelaunchAgent(agent_id.clone()),
        &attach_error,
    );
    assert_eq!(
        state.error_message.as_deref(),
        Some("attach failed: session exited before the viewer became ready")
    );
    assert_eq!(state.agents[0].status, AgentStatus::Dead);
    assert!(state.agents[0].runtime_binding.is_none());
}

#[test]
fn batch_recovery_keeps_failures_server_lost_and_reports_partial_success() {
    let first_id = AgentId("recover-ok".to_owned());
    let second_id = AgentId("recover-fail".to_owned());
    let mut first = bound_agent_state(&first_id).agents.remove(0);
    first.status = AgentStatus::ServerLost;
    let mut second = bound_agent_state(&second_id).agents.remove(0);
    second.status = AgentStatus::ServerLost;
    let mut state = AppState {
        agents: vec![first, second],
        warning_message: Some("Keep this warning.".to_owned()),
        ..AppState::default()
    };

    apply_server_lost_recovery_outcomes(
        &mut state,
        vec![
            ServerLostRecoveryOutcome {
                agent_id: first_id,
                result: Ok(()),
                pid: Some(100),
                process_identity: None,
            },
            ServerLostRecoveryOutcome {
                agent_id: second_id,
                result: Err(RuntimeError::SpawnFailed("psmux unavailable".to_owned())),
                pid: None,
                process_identity: None,
            },
        ],
    );

    assert_eq!(state.agents[0].status, AgentStatus::Running);
    assert!(state.agents[0].runtime_binding.is_some());
    assert_eq!(state.agents[1].status, AgentStatus::ServerLost);
    assert!(state.agents[1].runtime_binding.is_some());
    assert_eq!(
        state.error_message.as_deref(),
        Some("Recovered 1 psmux agent; 1 failed and remains Server Lost. Keep this warning.")
    );
}

#[test]
fn selected_server_lost_agent_opens_cancel_focused_batch_confirmation() {
    let first_id = AgentId("lost-one".to_owned());
    let second_id = AgentId("lost-two".to_owned());
    let mut first = bound_agent_state(&first_id).agents.remove(0);
    first.status = AgentStatus::ServerLost;
    let mut second = bound_agent_state(&second_id).agents.remove(0);
    second.status = AgentStatus::ServerLost;
    let repository = jefe::domain::Repository {
        id: first.repository_id.clone(),
        default_type_id: jefe::domain::shipped_agent_type(3),
        default_values: jefe::domain::TypedMap::new(),
        name: "Repository".to_owned(),
        slug: "repository".to_owned(),
        base_dir: std::path::PathBuf::from("/tmp"),
        github_repo: String::new(),
        github_issue_pr_repo: String::new(),
        remote: jefe::domain::RemoteRepositorySettings::default(),
        issue_base_prompt: String::new(),
        transient_agent_dir: std::path::PathBuf::new(),
        transient_max_concurrent: 0,
        agent_ids: vec![first_id.clone(), second_id.clone()],
    };
    let mut state = AppState {
        repositories: vec![repository],
        agents: vec![first, second],
        ..AppState::default()
    };

    assert!(open_server_lost_recovery(&mut state, &first_id));
    assert!(matches!(
        state.modal,
        jefe::state::ModalState::ConfirmServerLostRecovery {
            ref agent_ids,
            confirm_focus: jefe::state::ConfirmFocus::Cancel,
        } if agent_ids == &vec![AgentId("lost-one".to_owned()), AgentId("lost-two".to_owned())]
    ));
}

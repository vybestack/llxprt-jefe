use jefe::domain::{AgentId, AgentStatus, RuntimeBinding};
use jefe::runtime::{
    NpmPackageAvailabilityError, RuntimeError, RuntimeManager, StubRuntimeManager,
};
use jefe::state::{AppEvent, AppState, PaneFocus};

use super::relaunch::{
    ServerLostRecoveryOutcome, apply_server_lost_recovery_outcomes, attach_relaunched_session,
    open_server_lost_recovery, persist_relaunch_failure, spawn_relaunch_session,
};
use super::tests::{sample_agent, sample_signature};

fn bound_agent_state(agent_id: &AgentId) -> AppState {
    let mut agent = sample_agent(agent_id);
    agent.status = AgentStatus::Running;
    agent.runtime_binding = Some(RuntimeBinding {
        session_name: "jefe-relaunch-test".to_owned(),
        launch_signature: sample_signature(),
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
    let mut runtime = StubRuntimeManager::with_spawn_failure(error.clone());
    let signature = sample_signature();

    let result = spawn_relaunch_session(&mut runtime, &agent_id, &signature.work_dir, &signature);
    assert!(matches!(
        result,
        Err(RuntimeError::NpmPackageAvailability(
            NpmPackageAvailabilityError::PackageUnresolved { .. }
        ))
    ));

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
    let signature = sample_signature();
    if let Err(error) = runtime.spawn_session(&agent_id, &signature.work_dir, &signature) {
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
    let mut state = AppState {
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

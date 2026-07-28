use jefe::domain::{Agent, AgentId, AgentStatus, ProcessIdentity, RepositoryId};
use jefe::runtime::{
    MultiplexerVersion, ServerHealth, ServerIdentity, ServerLivenessEvidence,
    ServerLivenessObservation, classify_server_health, classify_server_liveness,
    parse_server_identity_output,
};
use jefe::state::transition::TransitionExt;
use jefe::state::{AppEvent, AppState};
use std::path::PathBuf;

fn identity(pid: u32, started_at: u64) -> ServerIdentity {
    ServerIdentity::new(
        ProcessIdentity::new(pid, started_at),
        MultiplexerVersion::new(3, 3, 7),
    )
}

#[test]
fn tracked_agents_with_no_server_are_server_lost() {
    let health = classify_server_health(None, None, true);

    assert_eq!(health, ServerHealth::Gone);
}

#[test]
fn no_server_without_tracked_agents_is_healthy() {
    let health = classify_server_health(None, None, false);

    assert_eq!(health, ServerHealth::Healthy);
}

#[test]
fn first_observed_server_establishes_a_healthy_baseline() {
    let current = identity(100, 1);

    let health = classify_server_health(None, Some(&current), true);

    assert_eq!(health, ServerHealth::Healthy);
}

#[test]
fn unchanged_server_identity_is_healthy() {
    let previous = identity(100, 1);

    let health = classify_server_health(Some(&previous), Some(&previous), true);

    assert_eq!(health, ServerHealth::Healthy);
}

#[test]
fn changed_server_pid_is_replaced() {
    let previous = identity(100, 1);
    let current = identity(200, 2);

    let health = classify_server_health(Some(&previous), Some(&current), true);

    assert_eq!(health, ServerHealth::Replaced);
}

#[test]
fn reused_server_pid_with_new_creation_token_is_replaced() {
    let previous = identity(100, 1);
    let current = identity(100, 2);

    let health = classify_server_health(Some(&previous), Some(&current), true);

    assert_eq!(health, ServerHealth::Replaced);
}

// --- ServerLivenessObservation classification (issue #493 Stack A) ---

/// A successful command that returns no parseable identity output cannot
/// establish server presence and fails open as Unavailable.
#[test]
fn successful_command_with_unparseable_output_is_unavailable() {
    let prior = identity(100, 1);
    let evidence = ServerLivenessEvidence::command_succeeded("garbage", "");

    let observation = classify_server_liveness(Some(&prior), &evidence);

    assert_eq!(observation, ServerLivenessObservation::Unavailable);
}

/// A successful command returning the same identity is Healthy.
#[test]
fn successful_command_same_identity_is_healthy() {
    let prior = identity(100, 1);
    let evidence = ServerLivenessEvidence::command_succeeded("100|3.3.7", "");

    let observation = classify_server_liveness(Some(&prior), &evidence);

    assert_eq!(observation, ServerLivenessObservation::Healthy(Some(prior)));
}

/// A successful command returning a different PID is Replaced.
#[test]
fn successful_command_changed_pid_is_replaced() {
    let prior = identity(100, 1);
    let evidence = ServerLivenessEvidence::command_succeeded("200|3.3.7", "");

    let observation = classify_server_liveness(Some(&prior), &evidence);

    assert_eq!(
        observation,
        ServerLivenessObservation::Replaced(identity(200, 1))
    );
}

/// A nonzero command whose stderr indicates no server is Gone.
#[test]
fn nonzero_command_with_no_server_stderr_is_gone() {
    let prior = identity(100, 1);
    let evidence = ServerLivenessEvidence::command_failed("no server running on");

    let observation = classify_server_liveness(Some(&prior), &evidence);

    assert_eq!(observation, ServerLivenessObservation::Gone);
}

/// A nonzero command whose stderr does not indicate a missing server fails
/// open as Unavailable (could be a transient multiplexer error).
#[test]
fn nonzero_command_with_unrelated_stderr_is_unavailable() {
    let prior = identity(100, 1);
    let evidence = ServerLivenessEvidence::command_failed("permission denied");

    let observation = classify_server_liveness(Some(&prior), &evidence);

    assert_eq!(observation, ServerLivenessObservation::Unavailable);
}

/// A spawn failure fails open as Unavailable.
#[test]
fn spawn_failure_is_unavailable() {
    let prior = identity(100, 1);
    let evidence = ServerLivenessEvidence::spawn_failed();

    let observation = classify_server_liveness(Some(&prior), &evidence);

    assert_eq!(observation, ServerLivenessObservation::Unavailable);
}

/// With no prior server, a successful first observation establishes Healthy.
#[test]
fn first_successful_observation_is_healthy() {
    let evidence = ServerLivenessEvidence::command_succeeded("100|3.3.7", "");

    let observation = classify_server_liveness(None, &evidence);

    assert_eq!(
        observation,
        ServerLivenessObservation::Healthy(Some(identity(100, 1)))
    );
}

/// A no-server response is Gone even before a baseline is pinned because the
/// caller only probes while local agents are tracked.
#[test]
fn no_prior_server_nonzero_command_is_gone() {
    let evidence = ServerLivenessEvidence::command_failed("no server running on");

    let observation = classify_server_liveness(None, &evidence);

    assert_eq!(observation, ServerLivenessObservation::Gone);
}

// --- parse_server_identity_output ---

#[test]
fn parse_identity_output_extracts_pid_and_version() {
    let parsed = parse_server_identity_output("100|3.3.7");

    let expected = ServerIdentity::new(
        ProcessIdentity::new(100, 1),
        MultiplexerVersion::new(3, 3, 7),
    );
    assert_eq!(parsed, Some(expected));
}

#[test]
fn parse_identity_output_rejects_missing_version() {
    assert!(parse_server_identity_output("100").is_none());
}

#[test]
fn parse_identity_output_rejects_non_numeric_pid() {
    assert!(parse_server_identity_output("abc|3.3.7").is_none());
}

#[test]
fn parse_identity_output_rejects_empty_input() {
    assert!(parse_server_identity_output("").is_none());
}

// --- AgentStatus::ServerLost domain projection (issue #493 Stack A) ---

#[test]
fn server_lost_preserves_runtime_binding_when_transitioned() {
    let mut agent = Agent::new(
        AgentId("agent-1".into()),
        RepositoryId("repo".into()),
        "Agent 1".into(),
        PathBuf::from("/tmp"),
    );
    agent.status = AgentStatus::Running;
    agent.runtime_binding = Some(jefe::domain::RuntimeBinding {
        session_name: "jefe-agent-1".into(),
        launch_signature: jefe::domain::LaunchSignature {
            work_dir: PathBuf::from("/tmp"),
            profile: "default".into(),
            code_puppy_model: String::new(),
            code_puppy_version: String::new(),
            code_puppy_yolo: Some(false),
            code_puppy_quick_resume: false,
            mode_flags: vec![],
            llxprt_debug: String::new(),
            pass_continue: true,
            sandbox_enabled: false,
            sandbox_engine: jefe::domain::SandboxEngine::Podman,
            sandbox_flags: jefe::domain::DEFAULT_SANDBOX_FLAGS.to_owned(),
            remote: jefe::domain::RemoteRepositorySettings::default(),
            agent_kind: jefe::domain::AgentKind::Llxprt,
            llxprt_version: None,
        },
        attached: false,
        last_seen: None,
        pid: Some(123),
        process_identity: Some(ProcessIdentity::new(123, 1)),
        lifecycle_generation: 0,
        worker_identities: vec![],
    });

    let agent_id = agent.id.clone();
    let mut state = AppState::default();
    state.agents.push(agent);

    let state = state
        .apply(AppEvent::AgentStatusChanged(
            agent_id,
            AgentStatus::ServerLost,
        ))
        .committed_pure();

    assert_eq!(state.agents[0].status, AgentStatus::ServerLost);
    assert!(
        state.agents[0].runtime_binding.is_some(),
        "ServerLost reducer transition must preserve runtime_binding for recovery"
    );
}

#[test]
fn server_lost_status_is_distinct_variant() {
    assert_ne!(AgentStatus::ServerLost, AgentStatus::Dead);
    assert_ne!(AgentStatus::ServerLost, AgentStatus::Running);
    assert_ne!(AgentStatus::ServerLost, AgentStatus::Errored);
}

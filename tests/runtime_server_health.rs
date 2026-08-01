use jefe::domain::{
    Agent, AgentId, AgentStatus, PaneProcessIdentity, RepositoryId, ServerProcessIdentity,
    WorkerProcessIdentity,
};
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
        ServerProcessIdentity::new(pid, started_at),
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
        ServerProcessIdentity::new(100, 1),
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
        jefe::domain::shipped_agent_type(3),
        jefe::domain::TypedMap::new(),
        "Agent 1".into(),
        PathBuf::from("/tmp"),
    );
    agent.status = AgentStatus::Running;
    agent.runtime_binding = Some(jefe::domain::RuntimeBinding {
        session_name: "jefe-agent-1".into(),
        launch_signature: jefe::domain::LaunchSignatureV1::default(),
        attached: false,
        last_seen: None,
        pane_identity: Some(PaneProcessIdentity::new(123, 1)),
        worker_identity: Some(WorkerProcessIdentity::new(123, 1)),
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

/// psmux runs one server process per session, so `#{pid}` names whichever
/// server answered the request rather than the `-L` namespace. Measured on
/// 2026-08-01: creating sessions in one namespace moved `#{pid}` through
/// 9008 -> 17784 -> 3832 while the namespace itself never restarted.
///
/// Adding a session must therefore not be reported as a replaced server. The
/// stable answer is psmux's `#{server_instance}` token, which held constant
/// across those same three probes (issue #540, upstream psmux#509).
/// Parse a probe line, naming the input in the failure so a regression says
/// which line stopped parsing rather than only that one did.
fn parse_identity(output: &str) -> ServerIdentity {
    parse_server_identity_output(output).unwrap_or_else(|| {
        panic!("a server identity probe must parse the namespace instance token, got {output:?}")
    })
}

#[test]
fn a_session_added_to_a_namespace_is_not_a_replaced_server() {
    // Same namespace instance token, different answering server process.
    let before = parse_identity("883b25f5379f199a|9008|3.3.7");
    let after = parse_identity("883b25f5379f199a|3832|3.3.7");

    let observation = classify_server_liveness(
        Some(&before),
        &ServerLivenessEvidence::command_succeeded("883b25f5379f199a|3832|3.3.7", ""),
    );

    assert_eq!(
        classify_server_health(Some(&before), Some(&after), true),
        ServerHealth::Healthy,
        "a namespace that merely gained a session has not been replaced",
    );
    assert!(
        matches!(observation, ServerLivenessObservation::Healthy(_)),
        "adding a session must not be observed as a replaced server, got {observation:?}",
    );
}

/// Two different `-L` namespaces are different servers even if a PID is
/// recycled between them, so the instance token must decide (issue #540).
#[test]
fn a_different_namespace_instance_is_a_replaced_server() {
    let ours = parse_server_identity_output("883b25f5379f199a|9008|3.3.7")
        .unwrap_or_else(|| panic!("namespace instance token must parse"));
    let theirs = parse_server_identity_output("f3cb9da032325298|9008|3.3.7")
        .unwrap_or_else(|| panic!("namespace instance token must parse"));

    assert_eq!(
        classify_server_health(Some(&ours), Some(&theirs), true),
        ServerHealth::Replaced,
        "a different namespace instance is a different server even at the same PID",
    );
}

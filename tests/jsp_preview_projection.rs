use std::path::PathBuf;

use jefe::domain::TypedMap;
use jefe::domain::agent_definition::AgentTypeId;
use jefe::domain::observation::{
    AgentObservation, FieldState, NativeActivityState, NativeActivityValue, ObservationHealth,
    Provenance,
};
use jefe::domain::{Agent, AgentId, AgentStatus, RepositoryId};
use jefe::jsp::v1::reducer::ReferenceReducer;

fn preview_agent(status: AgentStatus) -> Agent {
    let mut agent = Agent::new(
        AgentId("llxprt-agent".to_string()),
        RepositoryId("repo".to_string()),
        AgentTypeId::default(),
        TypedMap::default(),
        "LLxprt Agent".to_string(),
        PathBuf::from("/tmp/jefe-jsp-preview/work"),
    );
    agent.status = status;
    agent
}

fn live_observation() -> AgentObservation {
    let snapshot = jefe::jsp::parse_snapshot(include_bytes!(
        "../dev-docs/jsp/v1/fixtures/snapshot_full.json"
    ))
    .unwrap_or_else(|error| panic!("fixture must parse: {error}"));
    let mut reducer = ReferenceReducer::new();
    reducer.apply_snapshot(&snapshot);
    reducer.observation()
}

#[test]
fn preview_renders_real_jsp_todos_reply_and_working_status() {
    let agent = preview_agent(AgentStatus::Running);
    let observation = live_observation();
    let body =
        jefe::ui::components::preview_content_lines(Some(&agent), None, Some(&observation), 80)
            .join("\n");

    assert!(body.contains("Status: Working"));
    assert!(body.contains("Write parser"));
    assert!(body.contains("Last reply: Done."));
    assert!(!body.contains("(no tasks)"));
}

#[test]
fn status_precedence_keeps_process_and_health_separate() {
    let running = preview_agent(AgentStatus::Running);
    let mut observation = live_observation();
    observation.health = ObservationHealth::Stale;
    assert_eq!(
        jefe::preview_view::project_status(running.status, Some(&observation)),
        "Stale"
    );

    assert_eq!(
        jefe::preview_view::project_status(AgentStatus::Dead, Some(&observation)),
        "Dead"
    );

    observation.health = ObservationHealth::Live;
    observation.turn = FieldState::known(Provenance::Authoritative, None);
    observation.activity = FieldState::known(
        Provenance::Authoritative,
        NativeActivityValue {
            state: NativeActivityState::Idle,
        },
    );
    assert_eq!(
        jefe::preview_view::project_status(running.status, Some(&observation)),
        "Ready"
    );
}

#[test]
fn live_observation_supersedes_transient_queued_process_status() {
    let observation = live_observation();
    assert_eq!(
        jefe::preview_view::project_status(AgentStatus::Queued, Some(&observation)),
        "Working"
    );
    assert_eq!(
        jefe::preview_view::project_status(AgentStatus::Queued, None),
        "Starting"
    );
}

#[test]
fn unsupported_and_unknown_todos_remain_distinct() {
    let agent = preview_agent(AgentStatus::Running);
    let unsupported = AgentObservation {
        health: ObservationHealth::Unsupported,
        ..AgentObservation::default()
    };
    let unsupported_body =
        jefe::ui::components::preview_content_lines(Some(&agent), None, Some(&unsupported), 30)
            .join("\n");
    assert!(unsupported_body.contains("(unsupported)"));

    let mut unknown = unsupported;
    unknown.todos = FieldState::unknown(Provenance::Authoritative);
    let unknown_body =
        jefe::ui::components::preview_content_lines(Some(&agent), None, Some(&unknown), 30)
            .join("\n");
    assert!(unknown_body.contains("(unknown)"));
}

#[test]
fn ready_requires_known_absent_terminal_state() {
    let running = preview_agent(AgentStatus::Running);
    let mut observation = live_observation();
    observation.activity = FieldState::known(
        Provenance::Authoritative,
        NativeActivityValue {
            state: NativeActivityState::Idle,
        },
    );
    observation.wait = FieldState::known(Provenance::Authoritative, None);
    observation.turn = FieldState::known(Provenance::Authoritative, None);
    observation.terminal = FieldState::unknown(Provenance::Authoritative);
    assert_eq!(
        jefe::preview_view::project_status(running.status, Some(&observation)),
        "Unknown"
    );
    observation.terminal = FieldState::known(Provenance::Authoritative, None);
    assert_eq!(
        jefe::preview_view::project_status(running.status, Some(&observation)),
        "Ready"
    );
}

#[test]
fn turn_elapsed_advances_from_local_monotonic_anchor() {
    let agent = preview_agent(AgentStatus::Running);
    let mut observation = live_observation();
    let anchor = std::time::Instant::now();
    observation.turn_observed_at = Some(anchor);
    let view = jefe::preview_view::build_preview_view_at(
        Some(&agent),
        None,
        Some(&observation),
        80,
        anchor + std::time::Duration::from_millis(2_500),
    );
    let elapsed = view
        .lines
        .iter()
        .find(|line| line.starts_with("Turn elapsed:"))
        .unwrap_or_else(|| panic!("active turn must render elapsed time"));
    // The fixture anchors the turn at 12000 ms and the injected clock adds
    // 2500 ms, so the rendered value is deterministic. Asserting the exact
    // string keeps a regression in either the anchor or the local elapsed
    // arithmetic from passing.
    assert_eq!(elapsed, "Turn elapsed: 14s");
}

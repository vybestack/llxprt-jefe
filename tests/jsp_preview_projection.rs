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
fn queued_process_is_starting_only_until_something_is_observed() {
    let observation = live_observation();
    // A published observation proves the process is alive, so the queued
    // bookkeeping no longer describes it and the live status wins. Without an
    // observation there is nothing to supersede it and it stays Starting.
    assert_eq!(
        jefe::preview_view::project_status(AgentStatus::Queued, Some(&observation)),
        "Working"
    );
    assert_eq!(
        jefe::preview_view::project_status(AgentStatus::Queued, None),
        "Starting"
    );
}

/// The active item is whatever the producer says is in progress. It is never
/// inferred from position, so an agent working out of order, on several items,
/// or with an item blocked is still rendered truthfully. A state JSP/1 does
/// not recognize reads as "not completed" and is visibly not claimed to be
/// pending.
#[test]
fn preview_marks_the_active_todo_from_the_published_state() {
    let agent = preview_agent(AgentStatus::Running);
    let observation = observation_from_json(&serde_json::json!({
        "todos": known_field(serde_json::json!({
            "revision": 1,
            "items": [
                {"text": "later work", "state": "pending"},
                {"text": "finished work", "state": "completed"},
                {"text": "current work", "state": "in_progress"},
                {"text": "odd work", "state": "blocked"}
            ]
        }))
    }));
    let lines =
        jefe::ui::components::preview_content_lines(Some(&agent), None, Some(&observation), 80);

    for expected in [
        "  [ ] later work",
        "  [x] finished work",
        "  [>] current work",
        "  [?] odd work",
    ] {
        assert!(
            lines.iter().any(|line| line == expected),
            "preview must render {expected:?}: {lines:?}"
        );
    }
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

// ---------------------------------------------------------------------------
// Exhaustive nine-level precedence table (issue #522, J7).
//
// Every row of the acceptance matrix is exercised. The test is table-driven
// so that a regression in any single precedence level is immediately visible
// rather than hidden behind a different test's assertion.
// ---------------------------------------------------------------------------

use jefe::domain::observation::Wait;

/// Build a minimal observation with the given health, defaulting all native
/// fields to known-idle and known-absent. This gives a clean Ready baseline
/// that individual cases can override.
fn live_ready_observation() -> AgentObservation {
    AgentObservation {
        health: ObservationHealth::Live,
        activity: FieldState::known(
            Provenance::Authoritative,
            NativeActivityValue {
                state: NativeActivityState::Idle,
            },
        ),
        wait: FieldState::known(Provenance::Authoritative, None),
        turn: FieldState::known(Provenance::Authoritative, None),
        terminal: FieldState::known(Provenance::Authoritative, None),
        ..AgentObservation::default()
    }
}

/// Build an observation whose terminal field carries a source error. The
/// SourceErrorValue inner types have pub(crate) constructors, so this uses the
/// authoritative snapshot parser + reducer to produce a valid observation.
fn terminal_failure_observation() -> AgentObservation {
    observation_from_json(&serde_json::json!({
        "source_epoch": "epoch-failed",
        "native_activity": known_field(serde_json::json!({"state": "idle"})),
        "current_turn": known_field(serde_json::Value::Null),
        "last_displayed_assistant_message": known_field(serde_json::json!({"content": "bye", "committed_ms": 1})),
        "source_terminal_state": known_field(serde_json::json!({"summary": "crashed", "code": "FATAL"})),
    }))
}

/// Build an observation whose activity has degraded availability. The
/// DiagnosticCode type has a pub(crate) constructor, so this uses the
/// authoritative snapshot parser + reducer to produce a valid observation.
fn degraded_activity_observation() -> AgentObservation {
    observation_from_json(&serde_json::json!({
        "source_epoch": "epoch-degraded",
        "native_activity": serde_json::json!({
            "provenance": "authoritative",
            "availability": "degraded",
            "last_value": {"state": "idle"},
            "as_of_ms": 100,
            "diagnostic_code": "STALE"
        }),
        "current_wait": known_field(serde_json::Value::Null),
        "current_turn": known_field(serde_json::Value::Null),
        "source_terminal_state": known_field(serde_json::Value::Null),
    }))
}

fn observation_from_json(overrides: &serde_json::Value) -> AgentObservation {
    let mut snapshot_json = serde_json::json!({
        "schema": 1,
        "kind": "snapshot",
        "agent_id": "llxprt-agent",
        "lifecycle_generation": 1,
        "source_epoch": "epoch-default",
        "source_sequence": 1,
        "cursor": 0,
        "bridge_observed_ms": 1,
        "native_session": {
            "repository": "vybestack/llxprt-jefe",
            "path": "/tmp",
            "agent_kind": "llxprt",
            "pid": 999,
            "display_name": "test-agent"
        },
        "process_binding": known_field(serde_json::json!({"pid": 999, "started_at_ms": 1})),
        "native_activity": known_field(serde_json::json!({"state": "idle"})),
        "current_wait": known_field(serde_json::Value::Null),
        "current_turn": known_field(serde_json::Value::Null),
        "todos": known_field(serde_json::json!({"revision": 1, "items": []})),
        "last_displayed_assistant_message": known_field(serde_json::json!({"content": "ok", "committed_ms": 1})),
        "last_created_tool_call": known_field(serde_json::json!({"label": "Read", "phase": "succeeded"})),
        "source_terminal_state": known_field(serde_json::Value::Null),
        "source_error_state": "unsupported"
    });
    if let (Some(target), Some(src)) = (snapshot_json.as_object_mut(), overrides.as_object()) {
        for (key, value) in src {
            target.insert(key.clone(), value.clone());
        }
    }
    let bytes = serde_json::to_vec(&snapshot_json)
        .unwrap_or_else(|error| panic!("serialize snapshot: {error}"));
    let snapshot =
        jefe::jsp::parse_snapshot(&bytes).unwrap_or_else(|error| panic!("parse snapshot: {error}"));
    let mut reducer = ReferenceReducer::new();
    reducer.apply_snapshot(&snapshot);
    reducer.observation()
}

fn known_field(value: serde_json::Value) -> serde_json::Value {
    serde_json::json!({
        "provenance": "authoritative",
        "availability": "known",
        "value": value
    })
}

/// Row in the exhaustive precedence table.
struct PrecedenceCase {
    label: &'static str,
    status: AgentStatus,
    observation: Option<AgentObservation>,
    expected: &'static str,
}

fn precedence_table() -> Vec<PrecedenceCase> {
    let mut cases = Vec::new();
    cases.extend(process_level_cases());
    cases.extend(health_level_cases());
    cases.extend(live_status_cases());
    cases.extend(fallback_cases());
    cases
}

/// Levels 1–2: confirmed process exit and queued/spawning process.
fn process_level_cases() -> Vec<PrecedenceCase> {
    vec![
        // Level 1: confirmed process exit -> terminal labels. A dead process
        // is Dead regardless of any observation.
        PrecedenceCase {
            label: "L1: Dead process",
            status: AgentStatus::Dead,
            observation: Some(live_ready_observation()),
            expected: "Dead",
        },
        // Level 2: queued/spawning process -> Starting, but only while nothing
        // has been observed. Levels 3-9 describe an alive process, and a
        // published observation is proof of aliveness, so it supersedes the
        // pre-spawn bookkeeping rather than being masked by it.
        PrecedenceCase {
            label: "L2: Queued process with live work observation",
            status: AgentStatus::Queued,
            observation: Some(live_observation()),
            expected: "Working",
        },
        PrecedenceCase {
            label: "L2: Queued process with no observation",
            status: AgentStatus::Queued,
            observation: None,
            expected: "Starting",
        },
        // Waiting process status without observation still renders Waiting.
        PrecedenceCase {
            label: "Waiting process status without observation",
            status: AgentStatus::Waiting,
            observation: None,
            expected: "Waiting",
        },
        // Paused process status without observation.
        PrecedenceCase {
            label: "Paused process status without observation",
            status: AgentStatus::Paused,
            observation: None,
            expected: "Paused",
        },
        // Running without observation -> telemetry unsupported.
        PrecedenceCase {
            label: "Running without observation -> telemetry unsupported",
            status: AgentStatus::Running,
            observation: None,
            expected: "Running — telemetry unsupported",
        },
    ]
}

/// Levels 3–4: unsupported observation and degraded health values.
fn health_level_cases() -> Vec<PrecedenceCase> {
    vec![
        // Level 3: alive with unsupported observation.
        PrecedenceCase {
            label: "L3: Running with unsupported observation",
            status: AgentStatus::Running,
            observation: Some(AgentObservation {
                health: ObservationHealth::Unsupported,
                ..AgentObservation::default()
            }),
            expected: "Running — telemetry unsupported",
        },
        // Level 4: alive with degraded health values. Each health value must
        // render explicitly and must never collapse into Ready or Dead.
        PrecedenceCase {
            label: "L4: Running with connecting health",
            status: AgentStatus::Running,
            observation: Some(AgentObservation {
                health: ObservationHealth::Connecting,
                ..AgentObservation::default()
            }),
            expected: "Connecting",
        },
        PrecedenceCase {
            label: "L4: Running with stale health",
            status: AgentStatus::Running,
            observation: Some(AgentObservation {
                health: ObservationHealth::Stale,
                ..live_ready_observation()
            }),
            expected: "Stale",
        },
        PrecedenceCase {
            label: "L4: Running with disconnected health",
            status: AgentStatus::Running,
            observation: Some(AgentObservation {
                health: ObservationHealth::Disconnected,
                ..live_ready_observation()
            }),
            expected: "Disconnected",
        },
        PrecedenceCase {
            label: "L4: Running with protocol-error health",
            status: AgentStatus::Running,
            observation: Some(AgentObservation {
                health: ObservationHealth::ProtocolError,
                ..live_ready_observation()
            }),
            expected: "Protocol error",
        },
    ]
}

/// Levels 5–8: live observation status (wait, terminal, working, ready).
fn live_status_cases() -> Vec<PrecedenceCase> {
    let mut cases = Vec::new();
    cases.extend(live_wait_and_terminal_cases());
    cases.extend(live_working_and_ready_cases());
    cases
}

/// Levels 5–6: wait and terminal source state.
fn live_wait_and_terminal_cases() -> Vec<PrecedenceCase> {
    vec![
        // Level 5: live with an explicit unresolved wait.
        PrecedenceCase {
            label: "L5: Live with explicit wait (permission)",
            status: AgentStatus::Running,
            observation: Some(AgentObservation {
                wait: FieldState::known(
                    Provenance::Authoritative,
                    Some(Wait {
                        reason: jefe::domain::observation::WaitReason::Permission,
                    }),
                ),
                ..live_ready_observation()
            }),
            expected: "Waiting — permission",
        },
        // Level 6: live with terminal source state -> Failed.
        PrecedenceCase {
            label: "L6: Live with terminal source error (Failed)",
            status: AgentStatus::Running,
            observation: Some(terminal_failure_observation()),
            expected: "Failed",
        },
        // Level 6: live with session ended -> Ended.
        PrecedenceCase {
            label: "L6: Live with session ended",
            status: AgentStatus::Running,
            observation: Some(AgentObservation {
                session_ended: true,
                ..live_ready_observation()
            }),
            expected: "Ended",
        },
    ]
}

/// Levels 7–8: active work and known-ready idle.
fn live_working_and_ready_cases() -> Vec<PrecedenceCase> {
    vec![
        // Level 7: live with active turn, thinking, acting.
        PrecedenceCase {
            label: "L7: Live with active turn",
            status: AgentStatus::Running,
            observation: Some(AgentObservation {
                turn: FieldState::known(
                    Provenance::Authoritative,
                    Some(jefe::domain::observation::CurrentTurn { elapsed_ms: 5000 }),
                ),
                ..live_ready_observation()
            }),
            expected: "Working",
        },
        PrecedenceCase {
            label: "L7: Live with thinking activity",
            status: AgentStatus::Running,
            observation: Some(AgentObservation {
                activity: FieldState::known(
                    Provenance::Authoritative,
                    NativeActivityValue {
                        state: NativeActivityState::Thinking,
                    },
                ),
                ..live_ready_observation()
            }),
            expected: "Working",
        },
        PrecedenceCase {
            label: "L7: Live with acting activity",
            status: AgentStatus::Running,
            observation: Some(AgentObservation {
                activity: FieldState::known(
                    Provenance::Authoritative,
                    NativeActivityValue {
                        state: NativeActivityState::Acting,
                    },
                ),
                ..live_ready_observation()
            }),
            expected: "Working",
        },
        // Level 8: live, known idle, no wait, no turn, no terminal -> Ready.
        PrecedenceCase {
            label: "L8: Live, idle, no wait, no turn, no terminal",
            status: AgentStatus::Running,
            observation: Some(live_ready_observation()),
            expected: "Ready",
        },
    ]
}

/// Level 9 and degraded-availability combinations that must not become Ready.
fn fallback_cases() -> Vec<PrecedenceCase> {
    vec![
        // Level 9: otherwise -> Unknown. A live observation with an unknown
        // terminal field falls through to Unknown.
        PrecedenceCase {
            label: "L9: Live with unknown terminal (not Ready)",
            status: AgentStatus::Running,
            observation: Some(AgentObservation {
                terminal: FieldState::unknown(Provenance::Authoritative),
                ..live_ready_observation()
            }),
            expected: "Unknown",
        },
        PrecedenceCase {
            label: "L9: Live with all fields unsupported (not Ready)",
            status: AgentStatus::Running,
            observation: Some(AgentObservation {
                health: ObservationHealth::Live,
                ..AgentObservation::default()
            }),
            expected: "Unknown",
        },
        // Degraded-availability combinations must not become Ready.
        PrecedenceCase {
            label: "Degraded activity is not Ready",
            status: AgentStatus::Running,
            observation: Some(degraded_activity_observation()),
            expected: "Unknown",
        },
    ]
}

#[test]
fn status_precedence_table_covers_all_nine_levels() {
    for case in precedence_table() {
        let actual = jefe::preview_view::project_status(case.status, case.observation.as_ref());
        assert_eq!(actual, case.expected, "precedence case: {}", case.label);
    }
}

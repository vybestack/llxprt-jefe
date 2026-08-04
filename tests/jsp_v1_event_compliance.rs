//! JSP/1 event and heartbeat compliance suite (issue #476).
//!
//! These tests drive the public parsing entry points against the same
//! language-neutral fixture corpus that external producer and broker
//! implementations must satisfy, so the Rust reference oracle and any other
//! implementation are held to one contract.
//!
//! Acceptance rows:
//! - E1: every event in the closed inventory parses into its typed transition.
//! - E2: unknown event types and unknown payload members fail closed.
//! - E3: waiting requires an explicit event; producers cannot assert stale.
//! - E4: todo replacement carries a positive revision and bounded items.
//! - E5: event diagnostics never echo producer payload values.
//! - E6: the identity triple is validated on every document kind.
//! - E7: heartbeats report liveness and carry no sequence.

use std::fs;
use std::path::{Path, PathBuf};

use jefe::domain::observation::{
    NativeActivityState, ObservationEvent, TodoState, ToolPhase, TurnOutcome, WaitReason,
};
use jefe::jsp::v1::error::JspCode;
use jefe::jsp::v1::{parse_event, parse_heartbeat};

/// Absolute path to the shared fixture directory.
fn fixture_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("dev-docs/jsp/v1/fixtures")
}

/// Read a fixture by file name.
fn fixture(name: &str) -> Vec<u8> {
    let path = fixture_dir().join(name);
    fs::read(&path).unwrap_or_else(|error| panic!("fixture {name} must be readable: {error}"))
}

/// Parse an event fixture, requiring success.
fn parse_ok(name: &str) -> ObservationEvent {
    match parse_event(&fixture(name)) {
        Ok(record) => record.event,
        Err(error) => panic!("fixture {name} must parse: {}", error.detail()),
    }
}

/// Parse an event fixture, requiring the given failure code.
fn parse_err(name: &str, expected: JspCode) -> String {
    match parse_event(&fixture(name)) {
        Ok(_) => panic!("fixture {name} must fail with {}", expected.as_str()),
        Err(error) => {
            assert_eq!(
                error.code(),
                expected,
                "fixture {name} must fail with {}: {}",
                expected.as_str(),
                error.detail()
            );
            error.detail().to_string()
        }
    }
}

#[test]
fn e1_activity_change_parses_into_its_typed_state() {
    assert_eq!(
        parse_ok("event_activity_changed.json"),
        ObservationEvent::ActivityChanged {
            state: NativeActivityState::Acting
        }
    );
}

#[test]
fn e1_turn_end_carries_its_explicit_outcome() {
    assert_eq!(
        parse_ok("event_turn_ended.json"),
        ObservationEvent::TurnEnded {
            outcome: TurnOutcome::Cancelled
        },
        "a cancelled turn must stay distinguishable from a completed one"
    );
}

#[test]
fn e1_payload_free_events_need_no_members() {
    assert_eq!(
        parse_ok("event_session_ended.json"),
        ObservationEvent::SessionEnded
    );
}

#[test]
fn e1_tool_creation_keeps_its_label_and_phase() {
    let ObservationEvent::ToolCallCreated { tool } = parse_ok("event_tool_created.json") else {
        panic!("fixture must produce a tool-call creation");
    };
    assert_eq!(tool.label.as_str(), "read_file");
    assert_eq!(tool.phase, ToolPhase::AwaitingApproval);
}

#[test]
fn e1_displayed_message_keeps_its_commit_timestamp() {
    let ObservationEvent::AssistantMessageDisplayed { message } =
        parse_ok("event_message_displayed.json")
    else {
        panic!("fixture must produce a displayed message");
    };
    assert_eq!(message.content.as_str(), "Done.");
    assert_eq!(message.committed_ms, 1_750_000_000_450);
}

#[test]
fn e2_unknown_event_type_is_rejected_rather_than_ignored() {
    parse_err("event_unknown_type.json", JspCode::EClosedShape);
}

#[test]
fn e2_unknown_members_inside_an_event_payload_fail_closed() {
    parse_err("event_forbidden_fields.json", JspCode::EClosedShape);
}

#[test]
fn e3_explicit_reason_is_required_to_enter_waiting() {
    assert_eq!(
        parse_ok("event_wait_opened.json"),
        ObservationEvent::WaitOpened {
            reason: WaitReason::Permission
        }
    );
}

#[test]
fn e3_silence_has_no_event_that_can_create_waiting() {
    // The closed inventory has no "quiet"/"idle timeout"/"silence" transition,
    // so no producer can manufacture waiting from the absence of activity.
    for manufactured in [
        r#"{"type":"wait.silence"}"#,
        r#"{"type":"activity.silence"}"#,
        r#"{"type":"wait.opened","reason":"silence"}"#,
    ] {
        let document = format!(
            concat!(
                r#"{{"schema":1,"kind":"event","agent_id":"a","lifecycle_generation":1,"#,
                r#""source_epoch":"e","source_sequence":1,"bridge_observed_ms":1,"event":{}}}"#
            ),
            manufactured
        );
        assert!(
            parse_event(document.as_bytes()).is_err(),
            "silence must never produce a wait: {manufactured}"
        );
    }
}

#[test]
fn e3_producer_cannot_assert_stale() {
    parse_err("event_stale_phase.json", JspCode::EFieldState);
}

#[test]
fn e4_todo_replacement_carries_revision_and_items() {
    let ObservationEvent::TodosReplaced { todos } = parse_ok("event_todos_replaced.json") else {
        panic!("fixture must produce a todo replacement");
    };
    assert_eq!(todos.revision, 9);
    assert_eq!(todos.items.len(), 2);
    assert_eq!(todos.items[0].state, TodoState::Completed);
    assert_eq!(todos.items[1].state, TodoState::InProgress);
}

/// Build a `todos.replaced` event document carrying the supplied raw item
/// array.
fn todos_replaced_document(items: &str) -> String {
    format!(
        concat!(
            r#"{{"schema":1,"kind":"event","agent_id":"a","lifecycle_generation":1,"#,
            r#""source_epoch":"e","source_sequence":1,"bridge_observed_ms":1,"#,
            r#""event":{{"type":"todos.replaced","revision":1,"items":{items}}}}}"#
        ),
        items = items
    )
}

/// The event path carries the same task state as the snapshot path, so the two
/// cannot drift apart.
#[test]
fn e4_todo_replacement_carries_every_native_task_state() {
    let document = todos_replaced_document(
        r#"[{"text":"one","state":"pending"},{"text":"two","state":"in_progress"},{"text":"three","state":"completed"}]"#,
    );
    let Ok(record) = parse_event(document.as_bytes()) else {
        panic!("native task states must parse on the event path");
    };
    let ObservationEvent::TodosReplaced { todos } = record.event else {
        panic!("must produce a todo replacement");
    };
    let states: Vec<TodoState> = todos.items.iter().map(|item| item.state).collect();
    assert_eq!(
        states,
        vec![
            TodoState::Pending,
            TodoState::InProgress,
            TodoState::Completed
        ]
    );
}

/// An unknown producer state degrades on the event path too, rather than
/// failing the whole replacement or being guessed.
#[test]
fn e4_unrecognized_todo_state_degrades_on_the_event_path() {
    let document = todos_replaced_document(r#"[{"text":"one","state":"blocked"}]"#);
    let Ok(record) = parse_event(document.as_bytes()) else {
        panic!("an unknown producer state must not fail the replacement");
    };
    let ObservationEvent::TodosReplaced { todos } = record.event else {
        panic!("must produce a todo replacement");
    };
    assert_eq!(todos.items[0].state, TodoState::Unrecognized);
}

/// The retired boolean is rejected closed on the event path.
#[test]
fn e4_retired_completed_boolean_fails_closed_on_the_event_path() {
    let document = todos_replaced_document(r#"[{"text":"one","completed":false}]"#);
    let error = parse_event(document.as_bytes())
        .err()
        .unwrap_or_else(|| panic!("the retired boolean must fail"));
    assert_eq!(error.code(), JspCode::EClosedShape);
}

/// The state bound is inclusive on the event path and its diagnostic names the
/// member without echoing the value.
#[test]
fn e4_todo_state_bound_is_inclusive_on_the_event_path() {
    let at_limit = "s".repeat(64);
    let document = todos_replaced_document(&format!(r#"[{{"text":"one","state":"{at_limit}"}}]"#));
    assert!(
        parse_event(document.as_bytes()).is_ok(),
        "an at-limit state is accepted"
    );

    let over_limit = "s".repeat(65);
    let document =
        todos_replaced_document(&format!(r#"[{{"text":"one","state":"{over_limit}"}}]"#));
    let error = parse_event(document.as_bytes())
        .err()
        .unwrap_or_else(|| panic!("an over-limit state must fail"));
    assert_eq!(error.code(), JspCode::EBound);
    let detail = error.detail();
    assert!(
        detail.contains("event.todos.replaced.items[0].state"),
        "diagnostic must point at the offending member: {detail}"
    );
    assert!(
        !detail.contains(&over_limit),
        "diagnostic must not echo the payload value: {detail}"
    );
}

#[test]
fn e4_zero_revision_is_a_field_state_violation() {
    let document = concat!(
        r#"{"schema":1,"kind":"event","agent_id":"a","lifecycle_generation":1,"#,
        r#""source_epoch":"e","source_sequence":1,"bridge_observed_ms":1,"#,
        r#""event":{"type":"todos.replaced","revision":0,"items":[]}}"#
    );
    let error = parse_event(document.as_bytes())
        .err()
        .unwrap_or_else(|| panic!("a zero revision must fail"));
    assert_eq!(error.code(), JspCode::EFieldState);
}

#[test]
fn e4_an_empty_todo_list_is_valid_and_distinct_from_absent() {
    let document = concat!(
        r#"{"schema":1,"kind":"event","agent_id":"a","lifecycle_generation":1,"#,
        r#""source_epoch":"e","source_sequence":1,"bridge_observed_ms":1,"#,
        r#""event":{"type":"todos.replaced","revision":1,"items":[]}}"#
    );
    let Ok(record) = parse_event(document.as_bytes()) else {
        panic!("an explicitly empty todo list is valid");
    };
    let ObservationEvent::TodosReplaced { todos } = record.event else {
        panic!("must produce a todo replacement");
    };
    assert!(
        todos.items.is_empty(),
        "an authoritative empty list is not the same as unsupported todos"
    );
}

#[test]
fn e5_event_diagnostics_never_echo_payload_values() {
    let detail = parse_err("event_forbidden_fields.json", JspCode::EClosedShape);
    for token in ["SECRET", "kill", "run_shell"] {
        assert!(
            !detail.contains(token),
            "diagnostic leaked payload value '{token}': {detail}"
        );
    }
}

#[test]
fn e6_identity_is_validated_on_events() {
    parse_err("event_invalid_identity.json", JspCode::EIdentity);
}

#[test]
fn e7_heartbeat_reports_liveness_without_a_transition() {
    let Ok(record) = parse_heartbeat(&fixture("heartbeat_full.json")) else {
        panic!("the canonical heartbeat must parse");
    };
    assert_eq!(record.identity.agent_id.as_str(), "agent-alpha");
    assert_eq!(record.identity.lifecycle_generation, 3);
    assert_eq!(record.bridge_observed_ms, 1_750_000_001_100);
}

#[test]
fn e7_heartbeat_must_not_carry_a_sequence() {
    let error = parse_heartbeat(&fixture("heartbeat_with_sequence.json"))
        .err()
        .unwrap_or_else(|| panic!("a heartbeat carrying a sequence must fail"));
    assert_eq!(
        error.code(),
        JspCode::EClosedShape,
        "a heartbeat must not be able to advance or gap the stream"
    );
}

#[test]
fn e7_an_event_document_is_not_accepted_as_a_heartbeat() {
    let error = parse_heartbeat(&fixture("event_activity_changed.json"))
        .err()
        .unwrap_or_else(|| panic!("kinds must not be interchangeable"));
    assert_eq!(error.code(), JspCode::EUnsupportedVersion);
}

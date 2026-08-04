//! Unit tests for payload-free normalized projection tables.

use super::*;
use crate::domain::observation::{
    Availability, CurrentTurn, CurrentWaitField, DisplayedAssistantMessage, NativeActivityValue,
    Provenance, TodoItem, TodoList, ToolCallValue, ToolLabel, Wait,
};

// -----------------------------------------------------------------------
// ObservationHealth labels
// -----------------------------------------------------------------------

#[test]
fn observation_health_labels_are_stable() {
    let cases = [
        (ObservationHealth::Unsupported, "unsupported"),
        (ObservationHealth::Connecting, "connecting"),
        (ObservationHealth::Live, "live"),
        (ObservationHealth::Stale, "stale"),
        (ObservationHealth::Disconnected, "disconnected"),
        (ObservationHealth::ProtocolError, "protocol_error"),
    ];
    for (health, label) in cases {
        assert_eq!(
            health.as_str(),
            label,
            "ObservationHealth::{health:?} must serialize to {label}"
        );
        // Round-trip through serde to confirm the rename_all mapping.
        let json = serde_json::to_string(&health)
            .unwrap_or_else(|error| panic!("serialize {health:?}: {error}"));
        let expected = format!("\"{label}\"");
        assert_eq!(json, expected);
        let decoded: ObservationHealth = serde_json::from_str(&json)
            .unwrap_or_else(|error| panic!("deserialize {health:?}: {error}"));
        assert_eq!(decoded, health);
    }
}

// -----------------------------------------------------------------------
// WaitReason -> WaitProjection mapping (table-driven)
// -----------------------------------------------------------------------

#[test]
fn wait_projection_maps_all_reasons() {
    let cases = [
        (WaitReason::Permission, WaitProjection::Permission),
        (WaitReason::Question, WaitProjection::Question),
        (WaitReason::Elicitation, WaitProjection::Elicitation),
        (WaitReason::Choice, WaitProjection::Choice),
        (WaitReason::UserInput, WaitProjection::UserInput),
        (WaitReason::Other, WaitProjection::Other),
    ];
    for (reason, expected) in cases {
        assert_eq!(
            WaitProjection::from_reason(reason),
            expected,
            "WaitReason::{reason:?} must map to {expected:?}"
        );
    }
}

#[test]
fn project_wait_covers_all_field_state_branches() {
    let unsupported: CurrentWaitField = FieldState::Unsupported;
    assert_eq!(project_wait(&unsupported), WaitProjection::Unsupported);
    let known_none = FieldState::known(Provenance::Authoritative, None);
    assert_eq!(
        project_wait(&known_none),
        WaitProjection::NotWaiting,
        "Known(None) means explicitly not waiting"
    );
    let known_some = FieldState::known(
        Provenance::Inferred,
        Some(Wait {
            reason: WaitReason::Choice,
        }),
    );
    assert_eq!(
        project_wait(&known_some),
        WaitProjection::Choice,
        "Known wait projects its reason"
    );
    let unknown = FieldState::<Option<Wait>>::unknown(Provenance::Authoritative);
    assert_eq!(
        project_wait(&unknown),
        WaitProjection::Unknown,
        "Unknown availability projects to Unknown"
    );
    let degraded = FieldState::Supported {
        provenance: Provenance::Authoritative,
        availability: Availability::Degraded {
            last_value: None,
            as_of_ms: 0,
            diagnostic_code: crate::domain::observation::DiagnosticCode("X".to_string()),
        },
    };
    assert_eq!(
        project_wait(&degraded),
        WaitProjection::Unknown,
        "Degraded (invalid for wait) projects to Unknown"
    );
}

// -----------------------------------------------------------------------
// Activity -> ActivityProjection branches
// -----------------------------------------------------------------------

#[test]
fn project_activity_covers_all_states_and_availabilities() {
    assert_eq!(
        project_activity(&FieldState::Unsupported),
        ActivityProjection::Unsupported
    );
    let cases = [
        (NativeActivityState::Idle, ActivityProjection::Idle),
        (NativeActivityState::Thinking, ActivityProjection::Thinking),
        (NativeActivityState::Acting, ActivityProjection::Acting),
    ];
    for (state, expected) in cases {
        let field: NativeActivityField =
            FieldState::known(Provenance::Authoritative, NativeActivityValue { state });
        assert_eq!(
            project_activity(&field),
            expected,
            "state {state:?} -> {expected:?}"
        );
    }
    let unknown = FieldState::<NativeActivityValue>::unknown(Provenance::Authoritative);
    assert_eq!(project_activity(&unknown), ActivityProjection::Unknown);
    let degraded = FieldState::Supported {
        provenance: Provenance::Inferred,
        availability: Availability::Degraded {
            last_value: NativeActivityValue {
                state: NativeActivityState::Idle,
            },
            as_of_ms: 1,
            diagnostic_code: crate::domain::observation::DiagnosticCode("D".to_string()),
        },
    };
    assert_eq!(
        project_activity(&degraded),
        ActivityProjection::Degraded,
        "Degraded activity projects to Degraded"
    );
}

// -----------------------------------------------------------------------
// ToolPhase -> ToolPhaseProjection mapping (table-driven)
// -----------------------------------------------------------------------

#[test]
fn tool_phase_projection_maps_all_phases() {
    let cases = [
        (ToolPhase::Proposed, ToolPhaseProjection::Proposed),
        (
            ToolPhase::AwaitingApproval,
            ToolPhaseProjection::AwaitingApproval,
        ),
        (ToolPhase::Scheduled, ToolPhaseProjection::Scheduled),
        (ToolPhase::Executing, ToolPhaseProjection::Executing),
        (ToolPhase::Succeeded, ToolPhaseProjection::Succeeded),
        (ToolPhase::Failed, ToolPhaseProjection::Failed),
        (ToolPhase::Cancelled, ToolPhaseProjection::Cancelled),
    ];
    for (phase, expected) in cases {
        assert_eq!(
            ToolPhaseProjection::from_phase(phase),
            expected,
            "ToolPhase::{phase:?} must map to {expected:?}"
        );
    }
}

#[test]
fn project_tool_covers_field_state_branches() {
    let unsupported: LastToolField = FieldState::Unsupported;
    assert_eq!(
        project_tool(&unsupported),
        (None, ToolPhaseProjection::Absent),
        "Unsupported tool projects to Absent"
    );
    let known = FieldState::known(
        Provenance::Authoritative,
        ToolCallValue {
            label: ToolLabel("t".to_string()),
            phase: ToolPhase::Executing,
        },
    );
    assert_eq!(
        project_tool(&known),
        (Some("t".to_string()), ToolPhaseProjection::Executing)
    );
    let unknown = FieldState::<ToolCallValue>::unknown(Provenance::Authoritative);
    assert_eq!(
        project_tool(&unknown),
        (None, ToolPhaseProjection::Unknown),
        "Supported-but-unknown tool projects to Unknown phase"
    );
    let degraded = FieldState::Supported {
        provenance: Provenance::Authoritative,
        availability: Availability::Degraded {
            last_value: ToolCallValue {
                label: ToolLabel("stale-tool".to_string()),
                phase: ToolPhase::Failed,
            },
            as_of_ms: 9,
            diagnostic_code: crate::domain::observation::DiagnosticCode("D".to_string()),
        },
    };
    assert_eq!(
        project_tool(&degraded),
        (Some("stale-tool".to_string()), ToolPhaseProjection::Failed),
        "Degraded tool retains last label/phase"
    );
}

// -----------------------------------------------------------------------
// Todo projection branches
// -----------------------------------------------------------------------

#[test]
fn project_todos_covers_support_provenance_and_availability() {
    assert_eq!(
        project_todos(&FieldState::Unsupported),
        (TodoProjection::Unsupported, None, 0)
    );
    let authoritative_empty = FieldState::known(
        Provenance::Authoritative,
        TodoList {
            revision: 1,
            items: vec![],
        },
    );
    assert_eq!(
        project_todos(&authoritative_empty),
        (TodoProjection::AuthoritativeEmpty, Some(1), 0)
    );
    let authoritative = FieldState::known(
        Provenance::Authoritative,
        TodoList {
            revision: 5,
            items: vec![TodoItem {
                text: crate::domain::observation::BoundedText("a".to_string()),
                state: crate::domain::observation::TodoState::Pending,
            }],
        },
    );
    assert_eq!(
        project_todos(&authoritative),
        (TodoProjection::Authoritative, Some(5), 1)
    );
    let inferred = FieldState::known(
        Provenance::Inferred,
        TodoList {
            revision: 2,
            items: vec![],
        },
    );
    assert_eq!(
        project_todos(&inferred),
        (TodoProjection::Inferred, Some(2), 0)
    );
    let unknown = FieldState::<TodoList>::unknown(Provenance::Authoritative);
    assert_eq!(project_todos(&unknown), (TodoProjection::Unknown, None, 0));
    let degraded = FieldState::Supported {
        provenance: Provenance::Authoritative,
        availability: Availability::Degraded {
            last_value: TodoList {
                revision: 7,
                items: vec![],
            },
            as_of_ms: 0,
            diagnostic_code: crate::domain::observation::DiagnosticCode("D".to_string()),
        },
    };
    assert_eq!(
        project_todos(&degraded),
        (TodoProjection::Degraded, Some(7), 0)
    );
}

// -----------------------------------------------------------------------
// Message / presence projection branches
// -----------------------------------------------------------------------

#[test]
fn project_message_covers_branches() {
    let unsupported: LastMessageField = FieldState::Unsupported;
    assert_eq!(project_message(&unsupported), MessagePresence::Absent);
    let unknown = FieldState::<DisplayedAssistantMessage>::unknown(Provenance::Authoritative);
    assert_eq!(project_message(&unknown), MessagePresence::Unknown);
    let known = FieldState::known(
        Provenance::Authoritative,
        DisplayedAssistantMessage {
            content: crate::domain::observation::BoundedText("hi".to_string()),
            committed_ms: 1,
        },
    );
    assert_eq!(project_message(&known), MessagePresence::Present);
}

#[test]
fn project_presence_covers_branches() {
    let unsupported: FieldState<u32> = FieldState::Unsupported;
    assert_eq!(project_presence(&unsupported), MessagePresence::Absent);
    let unknown = FieldState::<u32>::unknown(Provenance::Authoritative);
    assert_eq!(project_presence(&unknown), MessagePresence::Unknown);
    let known = FieldState::known(Provenance::Authoritative, 5_u32);
    assert_eq!(project_presence(&known), MessagePresence::Present);
}

// -----------------------------------------------------------------------
// Turn-active and provenance projections
// -----------------------------------------------------------------------

#[test]
fn project_turn_active_only_when_known() {
    let unsupported: CurrentTurnField = FieldState::Unsupported;
    assert!(!project_turn_active(&unsupported));
    let unknown = FieldState::<Option<CurrentTurn>>::unknown(Provenance::Authoritative);
    assert!(!project_turn_active(&unknown), "Unknown turn is not active");
    let absent = FieldState::known(Provenance::Authoritative, None);
    assert!(
        !project_turn_active(&absent),
        "Known-null turn is not active"
    );
    let known = FieldState::known(
        Provenance::Authoritative,
        Some(CurrentTurn { elapsed_ms: 3 }),
    );
    assert!(project_turn_active(&known));
}

#[test]
fn project_provenance_maps_all_field_states() {
    let unsupported: FieldState<u32> = FieldState::Unsupported;
    assert_eq!(
        project_provenance(&unsupported),
        ProjectionProvenance::Unsupported
    );
    let authoritative = FieldState::known(Provenance::Authoritative, 1_u32);
    assert_eq!(
        project_provenance(&authoritative),
        ProjectionProvenance::Authoritative
    );
    let inferred = FieldState::known(Provenance::Inferred, 1_u32);
    assert_eq!(
        project_provenance(&inferred),
        ProjectionProvenance::Inferred
    );
}

#[test]
fn degraded_source_terminal_normalizes_to_unknown() {
    let degraded = FieldState::Supported {
        provenance: Provenance::Authoritative,
        availability: Availability::Degraded {
            last_value: Some(7_u32),
            as_of_ms: 1,
            diagnostic_code: crate::domain::observation::DiagnosticCode("D".to_string()),
        },
    };
    assert_eq!(
        project_source_terminal(&degraded),
        (MessagePresence::Unknown, AvailabilityProjection::Unknown)
    );
}

#[test]
fn availability_projection_maps_complete_typed_table() {
    let unsupported: FieldState<Option<u32>> = FieldState::Unsupported;
    let unknown = FieldState::<Option<u32>>::unknown(Provenance::Authoritative);
    let known = FieldState::known(Provenance::Authoritative, Some(7_u32));
    let known_absent = FieldState::known(Provenance::Authoritative, None::<u32>);
    let degraded = FieldState::Supported {
        provenance: Provenance::Inferred,
        availability: Availability::Degraded {
            last_value: Some(3_u32),
            as_of_ms: 1,
            diagnostic_code: crate::domain::observation::DiagnosticCode("D".to_string()),
        },
    };
    let cases = [
        (&unsupported, AvailabilityProjection::Unsupported),
        (&unknown, AvailabilityProjection::Unknown),
        (&known, AvailabilityProjection::Known),
        (&known_absent, AvailabilityProjection::KnownAbsent),
        (&degraded, AvailabilityProjection::Degraded),
    ];
    for (field, expected) in cases {
        assert_eq!(project_optional_availability(field), expected);
    }
}

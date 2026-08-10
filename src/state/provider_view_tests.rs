//! Pure projection tests for the provider view (issue #390 CW-10, Slice B).
//!
//! Each test proves one of the seven visual modes is projected from the pure
//! inputs. The unavailable reason is asserted byte-identical to the
//! action-registry availability. The confirmation modal content (title, body,
//! confirm label, continuation schema) is read from the exact pending token the
//! reducer registered, and the focus defaults to [`ConfirmFocus::Cancel`].

use crate::domain::action_registry::Availability;
use crate::domain::effects::{ProviderNotice, ProviderNoticeSeverity, ProviderRequestKey};
use crate::domain::plugin::action::{ActionConfirmation, ActionOutcome};
use crate::domain::plugin::field::{Field, FieldDraft, FieldKind, RestartScope};
use crate::domain::{Id, TypedMap};
use crate::runtime::provider::protocol::Outcome;
use crate::state::ConfirmFocus;
use crate::state::provider_requests::{
    ActionPolicy, InvokeInput, ProviderRequestState, UnavailableReason,
};
use crate::state::provider_view::{
    ProviderRowStatus, ProviderViewInput, ProviderViewMode, SMALL_VIEWPORT_ROW_THRESHOLD,
    project_provider_view, provider_notice_line,
};

fn owner() -> Id {
    Id::parse("host").unwrap_or_else(|_e| panic!("valid owner id"))
}

fn action() -> Id {
    Id::parse("provider.run").unwrap_or_else(|_e| panic!("valid action id"))
}

fn screen() -> Id {
    Id::parse("dashboard").unwrap_or_else(|_e| panic!("valid screen id"))
}

fn empty_map() -> TypedMap {
    TypedMap::new()
}

fn continuation_policy() -> ActionPolicy {
    ActionPolicy::new(
        ActionConfirmation::ProviderContinuation,
        vec![
            ActionOutcome::RequestHostConfirmation,
            ActionOutcome::Notice,
        ],
        false,
    )
}

fn invoke<'a>(
    state: &'a mut ProviderRequestState,
    owner: &'a Id,
    action: &'a Id,
    screen: &'a Id,
) -> crate::state::provider_requests::InvokeOutcome {
    let empty = empty_map();
    let policy = ActionPolicy::new(ActionConfirmation::None, vec![ActionOutcome::Notice], false);
    state
        .invoke(InvokeInput {
            owner,
            action_id: action,
            context_screen: screen,
            context_instance: screen,
            context_refs: &empty,
            arguments: &empty,
            policy: &policy,
        })
        .unwrap_or_else(|_e| panic!("invoke"))
}

/// Register a pending confirmation carrying exact declared UI fields and
/// return the request key that owns it.
fn register_confirmation(
    state: &mut ProviderRequestState,
    conf_id: &str,
    title: &str,
    body: &str,
    label: &str,
    schema: Vec<Field>,
) -> ProviderRequestKey {
    let o = owner();
    let a = action();
    let s = screen();
    let empty = empty_map();
    let policy = continuation_policy();
    let outcome = state
        .invoke(InvokeInput {
            owner: &o,
            action_id: &a,
            context_screen: &s,
            context_instance: &s,
            context_refs: &empty,
            arguments: &empty,
            policy: &policy,
        })
        .unwrap_or_else(|_e| panic!("invoke"));
    state
        .record_outcome(
            &outcome.key,
            Outcome::RequestHostConfirmation {
                confirmation_id: Id::parse(conf_id).unwrap_or_else(|_e| panic!("conf id")),
                title: title.to_owned(),
                body: body.to_owned(),
                confirm_label: label.to_owned(),
                destructive: false,
                continuation_schema: schema,
            },
            1000,
        )
        .unwrap_or_else(|_e| panic!("record outcome"));
    outcome.key
}

fn boolean_field(id: &str) -> Field {
    Field::parse(FieldDraft {
        id: Id::parse(id).unwrap_or_else(|_e| panic!("field id")),
        label: id.to_owned(),
        description: None,
        kind: FieldKind::Boolean,
        required: false,
        default: None,
        min: None,
        max: None,
        choices: vec![],
        unique: false,
        visible_when: None,
        restart: RestartScope::None,
    })
    .unwrap_or_else(|e| panic!("valid field: {e}"))
}

#[test]
fn normal_mode_when_idle_and_available() {
    let state = ProviderRequestState::new();
    let input = ProviderViewInput {
        requests: &state,
        availability: Some(&Availability::Available),
        focused: false,
        confirm: None,
        viewport_rows: 32,
        focused_index: None,
        action_label: Some("Run Action"),
    };
    let projection = project_provider_view(&input);
    assert_eq!(projection.mode, ProviderViewMode::Normal);
    assert!(!projection.has_active_request);
}

#[test]
fn focused_mode_when_surface_has_keyboard_focus() {
    let state = ProviderRequestState::new();
    let input = ProviderViewInput {
        requests: &state,
        availability: Some(&Availability::Available),
        focused: true,
        confirm: None,
        viewport_rows: 32,
        focused_index: None,
        action_label: Some("Run Action"),
    };
    let projection = project_provider_view(&input);
    assert_eq!(projection.mode, ProviderViewMode::Focused);
}

#[test]
fn unavailable_mode_shares_registry_reason_byte_identical() {
    let reason = "no compatible provider binary found";
    let state = ProviderRequestState::new();
    let availability = Availability::Unavailable {
        reason: reason.to_owned(),
    };
    let input = ProviderViewInput {
        requests: &state,
        availability: Some(&availability),
        focused: false,
        confirm: None,
        viewport_rows: 32,
        focused_index: None,
        action_label: Some("Run Action"),
    };
    let projection = project_provider_view(&input);
    match projection.mode {
        ProviderViewMode::Unavailable { reason: projected } => {
            assert_eq!(projected, reason);
        }
        other => panic!("expected Unavailable, got {other:?}"),
    }
    // The row also carries the shared reason.
    assert_eq!(projection.rows.len(), 1);
    match &projection.rows[0].status {
        ProviderRowStatus::Unavailable(row_reason) => assert_eq!(row_reason, reason),
        other => panic!("expected Unavailable row, got {other:?}"),
    }
}

#[test]
fn error_mode_when_request_has_terminal_error() {
    let mut state = ProviderRequestState::new();
    let o = owner();
    let a = action();
    let s = screen();
    let outcome = invoke(&mut state, &o, &a, &s);
    state
        .record_error(&outcome.key, "PLG-E502: bad arguments".to_owned())
        .unwrap_or_else(|_e| panic!("record error"));

    let input = ProviderViewInput {
        requests: &state,
        availability: Some(&Availability::Available),
        focused: false,
        confirm: None,
        viewport_rows: 32,
        focused_index: None,
        action_label: Some("Run Action"),
    };
    let projection = project_provider_view(&input);
    match &projection.mode {
        ProviderViewMode::Error { message } => {
            assert_eq!(message, "PLG-E502: bad arguments");
        }
        other => panic!("expected Error, got {other:?}"),
    }
}

#[test]
fn recovery_mode_when_generation_unavailable() {
    let mut state = ProviderRequestState::new();
    let o = owner();
    let a = action();
    let s = screen();
    let outcome = invoke(&mut state, &o, &a, &s);
    state
        .mark_unavailable(&outcome.key, UnavailableReason::Crash)
        .unwrap_or_else(|_e| panic!("mark unavailable"));

    let input = ProviderViewInput {
        requests: &state,
        availability: Some(&Availability::Available),
        focused: false,
        confirm: None,
        viewport_rows: 32,
        focused_index: None,
        action_label: Some("Run Action"),
    };
    let projection = project_provider_view(&input);
    match &projection.mode {
        ProviderViewMode::Recovery { message } => {
            assert_eq!(message, UnavailableReason::Crash.label());
        }
        other => panic!("expected Recovery, got {other:?}"),
    }
}

#[test]
fn confirmation_mode_carries_exact_declared_fields() {
    let mut state = ProviderRequestState::new();
    let schema = vec![boolean_field("confirm.flag")];
    register_confirmation(
        &mut state,
        "conf.exact",
        "Destroy Branch",
        "This cannot be undone.",
        "Delete",
        schema.clone(),
    );

    let input = ProviderViewInput {
        requests: &state,
        availability: Some(&Availability::Available),
        focused: false,
        confirm: Some(ConfirmFocus::Confirm),
        viewport_rows: 32,
        focused_index: None,
        action_label: Some("Run Action"),
    };
    let projection = project_provider_view(&input);
    match projection.mode {
        ProviderViewMode::Confirmation {
            confirm_focus,
            title,
            body,
            confirm_label,
            continuation_schema,
        } => {
            assert_eq!(confirm_focus, ConfirmFocus::Confirm);
            assert_eq!(title, "Destroy Branch");
            assert_eq!(body, "This cannot be undone.");
            assert_eq!(confirm_label, "Delete");
            assert_eq!(continuation_schema, schema);
        }
        other => panic!("expected Confirmation, got {other:?}"),
    }
}

#[test]
fn confirmation_mode_defaults_focus_to_cancel() {
    let mut state = ProviderRequestState::new();
    register_confirmation(
        &mut state,
        "conf.default",
        "Confirm Action",
        "Are you sure?",
        "Yes, proceed",
        vec![],
    );

    // No explicit focus override supplied.
    let input = ProviderViewInput {
        requests: &state,
        availability: Some(&Availability::Available),
        focused: false,
        confirm: None,
        viewport_rows: 32,
        focused_index: None,
        action_label: None,
    };
    let projection = project_provider_view(&input);
    match projection.mode {
        ProviderViewMode::Confirmation {
            confirm_focus,
            title,
            body,
            confirm_label,
            continuation_schema,
        } => {
            assert_eq!(
                confirm_focus,
                ConfirmFocus::Cancel,
                "confirmation focus must default to Cancel"
            );
            assert_eq!(title, "Confirm Action");
            assert_eq!(body, "Are you sure?");
            assert_eq!(confirm_label, "Yes, proceed");
            assert!(continuation_schema.is_empty());
        }
        other => panic!("expected Confirmation, got {other:?}"),
    }
}

#[test]
fn confirmation_mode_not_shown_without_pending_token() {
    // No pending confirmation in state and no explicit confirm hint: the
    // projection must not fabricate a Confirmation modal.
    let state = ProviderRequestState::new();
    let input = ProviderViewInput {
        requests: &state,
        availability: Some(&Availability::Available),
        focused: false,
        confirm: Some(ConfirmFocus::Confirm),
        viewport_rows: 32,
        focused_index: None,
        action_label: Some("Run Action"),
    };
    let projection = project_provider_view(&input);
    assert!(
        !matches!(projection.mode, ProviderViewMode::Confirmation { .. }),
        "no Confirmation mode without a pending token, got {:?}",
        projection.mode
    );
}

#[test]
fn small_mode_when_viewport_below_threshold() {
    let state = ProviderRequestState::new();
    let input = ProviderViewInput {
        requests: &state,
        availability: Some(&Availability::Available),
        focused: true,
        confirm: None,
        viewport_rows: SMALL_VIEWPORT_ROW_THRESHOLD - 1,
        focused_index: None,
        action_label: Some("Run Action"),
    };
    let projection = project_provider_view(&input);
    assert_eq!(projection.mode, ProviderViewMode::Small);
}

#[test]
fn progress_row_shows_completed_and_total() {
    use crate::runtime::provider::protocol::ProgressPayload;

    let mut state = ProviderRequestState::new();
    let o = owner();
    let a = action();
    let s = screen();
    let outcome = invoke(&mut state, &o, &a, &s);
    state
        .record_progress(
            &outcome.key,
            ProgressPayload {
                sequence: 1,
                message: "working".to_owned(),
                completed: Some(3),
                total: Some(10),
            },
        )
        .unwrap_or_else(|_e| panic!("progress"));

    let input = ProviderViewInput {
        requests: &state,
        availability: Some(&Availability::Available),
        focused: false,
        confirm: None,
        viewport_rows: 32,
        focused_index: Some(0),
        action_label: Some("Run Action"),
    };
    let projection = project_provider_view(&input);
    // The active-request row (index 1, after the action-label row).
    let request_row = projection
        .rows
        .iter()
        .find(|row| row.focused)
        .unwrap_or_else(|| panic!("a focused row"));
    match &request_row.status {
        ProviderRowStatus::InProgress(summary) => {
            assert_eq!(summary, "working: 3 / 10");
        }
        other => panic!("expected InProgress, got {other:?}"),
    }
}

#[test]
fn completed_row_shows_completed_status() {
    use crate::runtime::provider::protocol::Severity;

    let mut state = ProviderRequestState::new();
    let o = owner();
    let a = action();
    let s = screen();
    let outcome = invoke(&mut state, &o, &a, &s);
    state
        .record_outcome(
            &outcome.key,
            Outcome::Notice {
                severity: Severity::Info,
                message: "done".to_owned(),
            },
            1000,
        )
        .unwrap_or_else(|_e| panic!("record outcome"));

    let input = ProviderViewInput {
        requests: &state,
        availability: Some(&Availability::Available),
        focused: false,
        confirm: None,
        viewport_rows: 32,
        focused_index: None,
        action_label: Some("Run Action"),
    };
    let projection = project_provider_view(&input);
    let request_row = projection
        .rows
        .iter()
        .find(|row| row.label.contains("gen 1"))
        .unwrap_or_else(|| panic!("the request row"));
    assert_eq!(
        request_row.status,
        ProviderRowStatus::Completed("done".to_owned())
    );
    assert!(!projection.has_active_request);
}

#[test]
fn mode_precedence_small_beats_recovery() {
    let mut state = ProviderRequestState::new();
    let o = owner();
    let a = action();
    let s = screen();
    let outcome = invoke(&mut state, &o, &a, &s);
    state
        .mark_unavailable(&outcome.key, UnavailableReason::Timeout)
        .unwrap_or_else(|_e| panic!("mark unavailable"));

    let input = ProviderViewInput {
        requests: &state,
        availability: Some(&Availability::Available),
        focused: false,
        confirm: None,
        viewport_rows: 4,
        focused_index: None,
        action_label: Some("Run Action"),
    };
    let projection = project_provider_view(&input);
    assert_eq!(projection.mode, ProviderViewMode::Small);
}

#[test]
fn mode_precedence_confirmation_beats_unavailable() {
    let mut state = ProviderRequestState::new();
    register_confirmation(&mut state, "conf.prec", "Confirm", "Body", "OK", vec![]);
    let reason = "provider not installed";
    let availability = Availability::Unavailable {
        reason: reason.to_owned(),
    };
    let input = ProviderViewInput {
        requests: &state,
        availability: Some(&availability),
        focused: false,
        confirm: Some(ConfirmFocus::Cancel),
        viewport_rows: 32,
        focused_index: None,
        action_label: Some("Run Action"),
    };
    let projection = project_provider_view(&input);
    assert!(
        matches!(projection.mode, ProviderViewMode::Confirmation { .. }),
        "Confirmation must beat Unavailable"
    );
}

#[test]
fn typed_provider_notices_have_severity_specific_status_text() {
    let info = ProviderNotice {
        severity: ProviderNoticeSeverity::Info,
        message: "completed".to_owned(),
    };
    let warning = ProviderNotice {
        severity: ProviderNoticeSeverity::Warning,
        message: "check the result".to_owned(),
    };

    assert_eq!(provider_notice_line(&info), "completed");
    assert_eq!(provider_notice_line(&warning), "Warning: check the result");
}

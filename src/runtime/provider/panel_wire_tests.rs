//! Closed panel/migration wire behavioral tables (issue #391).
//!
//! These tests exercise the six new direct message kinds end to end: structural
//! parsing of every body and event kind, host-encoder round trips, the
//! request-origin role rule (panel-snapshot is provider-origin, migrated-config
//! echoes host-origin), and the cross-field invariants — positive ids,
//! mismatched kind, selected-id existence, progress counts, affordance
//! availability, action-reference resolution, unknown fields, and inclusive
//! count/byte bounds.

use super::dto::ProviderMessage;
use super::encode::{
    encode_activate_panel, encode_deactivate_panel, encode_migrate_config, encode_panel_event,
};
use super::error::ProviderError;
use super::identifiers::RequestId;
use super::panel_model::{
    ActivatePanelPayload, Affordance, BodyKind, DeactivatePanelPayload, DeactivateReason,
    DiffLineOrigin, HostLocal, ListBody, ListItem, MigrateConfigPayload, PanelBody, PanelEvent,
    PanelEventPayload, PanelSnapshot,
};
use super::protocol::{Direction, Id, RequestOrigin, TypedMap, parse_message};
use crate::domain::action_registry::ActionId;
use crate::domain::bounded_json::BoundedJsonError;
use crate::test_support::Must;

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Build one LF-terminated envelope line from its scalar parts.
fn envelope(ty: &str, request_id: &str, generation: u64, payload: &str) -> Vec<u8> {
    format!(
        "{{\"protocol\":1,\"type\":\"{ty}\",\"request_id\":\"{request_id}\",\"generation\":{generation},\"payload\":{payload}}}\n"
    )
    .into_bytes()
}

/// Parse a frame, panicking with the failure if it does not parse.
fn parsed(bytes: &[u8], stream: Direction) -> super::protocol::ParsedMessage {
    parse_message(bytes, stream).unwrap_or_else(|error| panic!("message must parse: {error}"))
}

/// Parse a frame, panicking if it parses when it must be rejected.
fn rejected(bytes: &[u8], stream: Direction) -> ProviderError {
    parse_message(bytes, stream)
        .err()
        .unwrap_or_else(|| panic!("message must be rejected"))
}

/// A snapshot payload with an empty body of the given kind.
fn snapshot_body(kind: &str, body: &str) -> String {
    let body_fields = body
        .strip_prefix('{')
        .unwrap_or_else(|| panic!("body fixture must be an object"));
    format!(
        r#"{{"model_schema":1,"panel_instance_id":1,"generation":1,"revision":1,"kind":"{kind}","title":"t","loading":false,"action_affordances":[],"body":{{"kind":"{kind}",{body_fields}}}"#
    )
}

/// Parse a panel-snapshot from a body kind + body JSON.
fn parse_snapshot(kind: &str, body: &str) -> PanelSnapshot {
    let bytes = envelope("panel-snapshot", "p-000001", 1, &snapshot_body(kind, body));
    let msg = parsed(&bytes, Direction::ProviderToHost);
    match msg.message {
        ProviderMessage::PanelSnapshot(snap) => snap,
        other => panic!("expected PanelSnapshot, got {other:?}"),
    }
}

/// A snapshot that carries one affordance and a form body referencing it.
fn affordance_form_snapshot() -> &'static str {
    r#"{"model_schema":1,"panel_instance_id":1,"generation":1,"revision":1,"kind":"form","title":"t","loading":false,"action_affordances":[{"id":"vendor.act","label":"Act","action_id":"vendor.submit","enabled":true}],"body":{"kind":"form","fields":[{"id":"vendor.f","label":"F","type":"string","required":false,"restart":"none"}],"values":{},"field_errors":[],"submit_action":"vendor.submit"}}"#
}

// ---------------------------------------------------------------------------
// A. Every direct payload parses with its exact fields
// ---------------------------------------------------------------------------

#[test]
fn activate_panel_parses_with_exact_fields() {
    let bytes = envelope(
        "activate-panel",
        "h-000001",
        1,
        r#"{"panel_instance_id":1,"screen_instance_id":2,"panel_type":"vendor.panel","activation":{"k":{"type":"string","value":"v"}},"generation":3}"#,
    );
    let msg = parsed(&bytes, Direction::HostToProvider);
    let ProviderMessage::ActivatePanel(activate) = msg.message else {
        panic!("expected ActivatePanel");
    };
    assert_eq!(activate.panel_instance_id, 1);
    assert_eq!(activate.screen_instance_id, 2);
    assert_eq!(activate.panel_type.as_str(), "vendor.panel");
    assert_eq!(activate.activation.len(), 1);
    assert_eq!(activate.generation, 3);
    assert!(activate.prior_host_local.is_none());
}

#[test]
fn activate_panel_with_prior_host_local_parses() {
    let bytes = envelope(
        "activate-panel",
        "h-000001",
        1,
        r#"{"panel_instance_id":1,"screen_instance_id":1,"panel_type":"vendor.panel","activation":{},"prior_host_local":{"focus_target":"vendor.f","scroll_offset":5,"selected_id":"vendor.s","form_draft":{"x":{"type":"bool","value":true}}},"generation":1}"#,
    );
    let msg = parsed(&bytes, Direction::HostToProvider);
    let ProviderMessage::ActivatePanel(activate) = msg.message else {
        panic!("expected ActivatePanel");
    };
    let host = activate
        .prior_host_local
        .as_ref()
        .unwrap_or_else(|| panic!("prior_host_local present"));
    assert_eq!(host.scroll_offset, 5);
    assert!(host.focus_target.is_some());
    assert!(host.form_draft.is_some());
}

#[test]
fn deactivate_panel_parses_with_exact_fields() {
    let bytes = envelope(
        "deactivate-panel",
        "h-000002",
        1,
        r#"{"panel_instance_id":1,"generation":1,"reason":"dispose"}"#,
    );
    let msg = parsed(&bytes, Direction::HostToProvider);
    let ProviderMessage::DeactivatePanel(deactivate) = msg.message else {
        panic!("expected DeactivatePanel");
    };
    assert_eq!(deactivate.reason, DeactivateReason::Dispose);
}

#[test]
fn migrate_config_parses_with_exact_fields() {
    let bytes = envelope(
        "migrate-config",
        "h-000004",
        1,
        r#"{"from_version":1,"to_version":2,"config":{"k":{"type":"string","value":"v"}},"draft_token":7}"#,
    );
    let msg = parsed(&bytes, Direction::HostToProvider);
    let ProviderMessage::MigrateConfig(migrate) = msg.message else {
        panic!("expected MigrateConfig");
    };
    assert_eq!(migrate.from_version, 1);
    assert_eq!(migrate.to_version, 2);
    assert_eq!(migrate.draft_token, 7);
    assert_eq!(migrate.config.len(), 1);
}

#[test]
fn migrate_config_secret_reference_is_exact_environment_reference() {
    let bytes = envelope(
        "migrate-config",
        "h-000004",
        1,
        r#"{"from_version":1,"to_version":2,"config":{"token":{"type":"secret_ref","value":{"env":"GITHUB_TOKEN"}}},"draft_token":7}"#,
    );
    let msg = parsed(&bytes, Direction::HostToProvider);
    let ProviderMessage::MigrateConfig(migrate) = msg.message else {
        panic!("expected MigrateConfig");
    };
    let Some(crate::domain::TypedValue::SecretRef(reference)) =
        migrate.config.get(&Id::parse("token").must("valid id"))
    else {
        panic!("expected secret reference");
    };
    assert_eq!(reference.env.env(), "GITHUB_TOKEN");
}

#[test]
fn migrate_config_rejects_legacy_secret_reference_id() {
    let bytes = envelope(
        "migrate-config",
        "h-000004",
        1,
        r#"{"from_version":1,"to_version":2,"config":{"token":{"type":"secret_ref","value":{"id":"github.token"}}},"draft_token":7}"#,
    );
    assert!(matches!(
        parse_message(&bytes, Direction::HostToProvider),
        Err(ProviderError::UnknownField { field, .. }) if field == "id"
    ));
}

#[test]
fn migrate_config_rejects_invalid_secret_environment_name() {
    let bytes = envelope(
        "migrate-config",
        "h-000004",
        1,
        r#"{"from_version":1,"to_version":2,"config":{"token":{"type":"secret_ref","value":{"env":"github-token"}}},"draft_token":7}"#,
    );
    assert!(matches!(
        parse_message(&bytes, Direction::HostToProvider),
        Err(ProviderError::InvalidValue { path, .. }) if path.to_ascii_lowercase().ends_with(".env")
    ));
}

#[test]
fn migrated_config_parses_with_exact_fields() {
    let bytes = envelope(
        "migrated-config",
        "h-000004",
        1,
        r#"{"from_version":1,"to_version":2,"config":{},"draft_token":7,"target_config":{"t":{"type":"bool","value":false}},"notes":["n1","n2"]}"#,
    );
    let msg = parsed(&bytes, Direction::ProviderToHost);
    let ProviderMessage::MigratedConfig(migrated) = msg.message else {
        panic!("expected MigratedConfig");
    };
    assert_eq!(migrated.draft_token, 7);
    assert_eq!(migrated.notes, ["n1", "n2"]);
    assert_eq!(migrated.target_config.len(), 1);
}

// ---------------------------------------------------------------------------
// B. Every body kind parses
// ---------------------------------------------------------------------------

#[test]
fn list_body_parses() {
    let snap = parse_snapshot(
        "list",
        r#"{"items":[{"id":"vendor.i","label":"Item","actions":[]}],"selected_id":"vendor.i"}"#,
    );
    let PanelBody::List(list) = snap.body else {
        panic!("expected List body");
    };
    assert_eq!(list.items.len(), 1);
    assert_eq!(list.selected_id.as_ref().map(Id::as_str), Some("vendor.i"));
}

#[test]
fn detail_body_parses() {
    let snap = parse_snapshot(
        "detail",
        r#"{"document":"hello","metadata":[{"label":"l","value":"v"}],"actions":[]}"#,
    );
    let PanelBody::Detail(detail) = snap.body else {
        panic!("expected Detail body");
    };
    assert_eq!(detail.document, "hello");
    assert_eq!(detail.metadata.len(), 1);
}

#[test]
fn form_body_parses_and_resolves_submit_action() {
    let bytes = envelope("panel-snapshot", "p-000001", 1, affordance_form_snapshot());
    let msg = parsed(&bytes, Direction::ProviderToHost);
    let ProviderMessage::PanelSnapshot(snap) = msg.message else {
        panic!("expected PanelSnapshot");
    };
    let PanelBody::Form(form) = snap.body else {
        panic!("expected Form body");
    };
    assert_eq!(form.fields.len(), 1);
}

#[test]
fn status_body_parses() {
    let snap = parse_snapshot(
        "status",
        r#"{"rows":[{"label":"l","value":"v","state":"warning"}]}"#,
    );
    let PanelBody::Status(status) = snap.body else {
        panic!("expected Status body");
    };
    assert_eq!(status.rows.len(), 1);
}

#[test]
fn progress_body_parses() {
    let snap = parse_snapshot(
        "progress",
        r#"{"message":"working","completed":2,"total":5,"cancellable":true}"#,
    );
    let PanelBody::Progress(progress) = snap.body else {
        panic!("expected Progress body");
    };
    assert_eq!(progress.completed, Some(2));
    assert_eq!(progress.total, Some(5));
    assert!(progress.cancellable);
}

#[test]
fn empty_body_parses() {
    let snap = parse_snapshot("empty", r#"{"message":"nothing"}"#);
    let PanelBody::Empty(empty) = snap.body else {
        panic!("expected Empty body");
    };
    assert_eq!(empty.message, "nothing");
}

#[test]
fn error_body_parses() {
    let snap = parse_snapshot(
        "error",
        r#"{"code":"E_X","message":"boom","retryable":true}"#,
    );
    let PanelBody::Error(error) = snap.body else {
        panic!("expected Error body");
    };
    assert_eq!(error.code, "E_X");
    assert!(error.retryable);
}

// ---------------------------------------------------------------------------
// C. Every event kind parses
// ---------------------------------------------------------------------------

fn parse_event(event_json: &str) -> PanelEvent {
    let bytes = envelope(
        "panel-event",
        "h-000003",
        1,
        &format!(r#"{{"panel_instance_id":1,"generation":1,"revision":1,"event":{event_json}}}"#),
    );
    let msg = parsed(&bytes, Direction::HostToProvider);
    match msg.message {
        ProviderMessage::PanelEvent(payload) => payload.event,
        other => panic!("expected PanelEvent, got {other:?}"),
    }
}

#[test]
fn event_selected_parses() {
    let event = parse_event(r#"{"kind":"selected","id":"vendor.i"}"#);
    assert!(matches!(event, PanelEvent::Selected { id } if id.as_str() == "vendor.i"));
}

#[test]
fn event_activated_parses() {
    let event = parse_event(r#"{"kind":"activated","id":"vendor.i"}"#);
    assert!(matches!(event, PanelEvent::Activated { id } if id.as_str() == "vendor.i"));
}

#[test]
fn event_action_parses() {
    let event = parse_event(
        r#"{"kind":"action","id":"vendor.act","arguments":{"k":{"type":"string","value":"v"}}}"#,
    );
    match event {
        PanelEvent::Action { id, arguments } => {
            assert_eq!(id.as_str(), "vendor.act");
            assert_eq!(arguments.len(), 1);
        }
        other => panic!("expected Action, got {other:?}"),
    }
}

#[test]
fn event_field_changed_parses() {
    let event = parse_event(
        r#"{"kind":"field-changed","field_id":"vendor.f","value":{"type":"bool","value":true}}"#,
    );
    match event {
        PanelEvent::FieldChanged { field_id, value } => {
            assert_eq!(field_id.as_str(), "vendor.f");
            assert!(matches!(value, crate::domain::TypedValue::Bool(true)));
        }
        other => panic!("expected FieldChanged, got {other:?}"),
    }
}

#[test]
fn event_submit_parses() {
    let event = parse_event(r#"{"kind":"submit","values":{"k":{"type":"string","value":"v"}}}"#);
    match event {
        PanelEvent::Submit { values } => assert_eq!(values.len(), 1),
        other => panic!("expected Submit, got {other:?}"),
    }
}

#[test]
fn event_page_requested_parses() {
    let event = parse_event(r#"{"kind":"page-requested","token":"abc"}"#);
    assert!(matches!(event, PanelEvent::PageRequested { ref token } if token == "abc"));
}

#[test]
fn event_retry_parses() {
    let event = parse_event(r#"{"kind":"retry"}"#);
    assert!(matches!(event, PanelEvent::Retry));
}

#[test]
fn event_cancel_parses() {
    let event = parse_event(r#"{"kind":"cancel"}"#);
    assert!(matches!(event, PanelEvent::Cancel));
}

#[test]
fn event_link_selected_parses() {
    let event = parse_event(r#"{"kind":"link-selected","link_id":"vendor.l"}"#);
    assert!(
        matches!(event, PanelEvent::LinkSelected { link_id } if link_id.as_str() == "vendor.l")
    );
}

#[test]
fn unknown_event_kind_is_rejected() {
    let bytes = envelope(
        "panel-event",
        "h-000003",
        1,
        r#"{"panel_instance_id":1,"generation":1,"revision":1,"event":{"kind":"frobnicate"}}"#,
    );
    assert!(matches!(
        rejected(&bytes, Direction::HostToProvider),
        ProviderError::UnknownValue { .. }
    ));
}

// ---------------------------------------------------------------------------
// D. Host encoder round trips
// ---------------------------------------------------------------------------

fn host_id() -> RequestId {
    RequestId::parse("h-000001").unwrap_or_else(|e| panic!("valid id: {e:?}"))
}

#[test]
fn activate_panel_round_trips_through_the_decoder() {
    let activation = TypedMap::new();
    let payload = ActivatePanelPayload {
        panel_instance_id: 1,
        screen_instance_id: 2,
        panel_type: Id::parse("vendor.panel").unwrap_or_else(|e| panic!("{e:?}")),
        activation,
        prior_host_local: Some(HostLocal {
            focus_target: None,
            scroll_offset: 3,
            selected_id: None,
            form_draft: None,
        }),
        generation: 4,
    };
    let bytes = encode_activate_panel(&host_id(), 9, &payload);
    let msg = parsed(&bytes, Direction::HostToProvider);
    let ProviderMessage::ActivatePanel(activate) = msg.message else {
        panic!("expected ActivatePanel");
    };
    assert_eq!(activate.panel_instance_id, 1);
    assert_eq!(activate.generation, 4);
    let host = activate
        .prior_host_local
        .unwrap_or_else(|| panic!("host local"));
    assert_eq!(host.scroll_offset, 3);
}

#[test]
fn deactivate_panel_round_trips_through_the_decoder() {
    let payload = DeactivatePanelPayload {
        panel_instance_id: 1,
        generation: 2,
        reason: DeactivateReason::Replace,
    };
    let bytes = encode_deactivate_panel(&host_id(), 1, &payload);
    let msg = parsed(&bytes, Direction::HostToProvider);
    let ProviderMessage::DeactivatePanel(deactivate) = msg.message else {
        panic!("expected DeactivatePanel");
    };
    assert_eq!(deactivate.reason, DeactivateReason::Replace);
}

#[test]
fn panel_event_round_trips_each_variant() {
    for event in [
        PanelEvent::Selected {
            id: Id::parse("vendor.i").unwrap_or_else(|e| panic!("{e:?}")),
        },
        PanelEvent::ExpansionChanged {
            id: Id::parse("vendor.node").unwrap_or_else(|e| panic!("{e:?}")),
            expanded: true,
        },
        PanelEvent::Retry,
        PanelEvent::Cancel,
    ] {
        let payload = PanelEventPayload {
            panel_instance_id: 1,
            generation: 1,
            revision: 1,
            event: event.clone(),
        };
        let bytes = encode_panel_event(&host_id(), 1, &payload);
        let msg = parsed(&bytes, Direction::HostToProvider);
        let ProviderMessage::PanelEvent(decoded) = msg.message else {
            panic!("expected PanelEvent");
        };
        assert_eq!(decoded.event, event);
    }
}

#[test]
fn migrate_config_round_trips_through_the_decoder() {
    let mut config = TypedMap::new();
    config.insert(
        Id::parse("token").must("valid id"),
        crate::domain::TypedValue::SecretRef(crate::domain::SecretRef {
            env: crate::domain::plugin::SecretReference::parse("GITHUB_TOKEN")
                .must("valid environment name"),
        }),
    );
    let payload = MigrateConfigPayload {
        from_version: 1,
        to_version: 2,
        config,
        draft_token: 7,
    };
    let bytes = encode_migrate_config(&host_id(), 1, &payload);
    let wire = std::str::from_utf8(&bytes).must("encoder emits UTF-8");
    assert!(wire.contains(r#""env":"GITHUB_TOKEN""#));
    assert!(!wire.contains(r#""id":"GITHUB_TOKEN""#));
    let msg = parsed(&bytes, Direction::HostToProvider);
    let ProviderMessage::MigrateConfig(migrate) = msg.message else {
        panic!("expected MigrateConfig");
    };
    assert_eq!(migrate.draft_token, 7);
    assert_eq!(migrate.config, payload.config);
}

// ---------------------------------------------------------------------------
// E. Request-origin role behavior
// ---------------------------------------------------------------------------

#[test]
fn panel_snapshot_requires_provider_origin() {
    let body = snapshot_body("empty", r#"{"message":"x"}"#);
    // Provider-origin p-* on the provider stream is accepted.
    let ok = envelope("panel-snapshot", "p-000001", 1, &body);
    assert!(parse_message(&ok, Direction::ProviderToHost).is_ok());
    // Host-origin h-* on the provider stream is rejected.
    let bad = envelope("panel-snapshot", "h-000001", 1, &body);
    assert!(matches!(
        rejected(&bad, Direction::ProviderToHost),
        ProviderError::InvalidRequestOrigin { .. }
    ));
}

#[test]
fn migrated_config_requires_host_origin() {
    let payload = r#"{"from_version":1,"to_version":2,"config":{},"draft_token":1,"target_config":{},"notes":[]}"#;
    // Host-origin h-* echoed on the provider stream is accepted.
    let ok = envelope("migrated-config", "h-000004", 1, payload);
    assert!(parse_message(&ok, Direction::ProviderToHost).is_ok());
    // Provider-origin p-* on the provider stream is rejected.
    let bad = envelope("migrated-config", "p-000004", 1, payload);
    assert!(matches!(
        rejected(&bad, Direction::ProviderToHost),
        ProviderError::InvalidRequestOrigin { .. }
    ));
}

#[test]
fn host_messages_require_host_origin() {
    let activate = envelope(
        "activate-panel",
        "p-000001",
        1,
        r#"{"panel_instance_id":1,"screen_instance_id":1,"panel_type":"vendor.p","activation":{},"generation":1}"#,
    );
    assert!(matches!(
        rejected(&activate, Direction::HostToProvider),
        ProviderError::InvalidRequestOrigin { .. }
    ));
}

#[test]
fn panel_snapshot_request_origin_is_provider() {
    assert_eq!(
        super::identifiers::MessageKind::PanelSnapshot.request_origin(),
        RequestOrigin::Provider
    );
    assert_eq!(
        super::identifiers::MessageKind::MigratedConfig.request_origin(),
        RequestOrigin::Host
    );
}

// ---------------------------------------------------------------------------
// F. Unknown fields and closed shapes
// ---------------------------------------------------------------------------

#[test]
fn unknown_field_in_activate_panel_is_rejected() {
    let bytes = envelope(
        "activate-panel",
        "h-000001",
        1,
        r#"{"panel_instance_id":1,"screen_instance_id":1,"panel_type":"vendor.p","activation":{},"generation":1,"sneaky":1}"#,
    );
    assert!(matches!(
        rejected(&bytes, Direction::HostToProvider),
        ProviderError::UnknownField { .. }
    ));
}

#[test]
fn unknown_field_in_snapshot_body_is_rejected() {
    let bytes = envelope(
        "panel-snapshot",
        "p-000001",
        1,
        &snapshot_body("empty", r#"{"message":"x","extra":1}"#),
    );
    assert!(matches!(
        rejected(&bytes, Direction::ProviderToHost),
        ProviderError::UnknownField { .. }
    ));
}

// ---------------------------------------------------------------------------
// G. Positive identities, generations, revisions
// ---------------------------------------------------------------------------

#[test]
fn zero_panel_instance_id_is_rejected() {
    let bytes = envelope(
        "deactivate-panel",
        "h-000002",
        1,
        r#"{"panel_instance_id":0,"generation":1,"reason":"suspend"}"#,
    );
    assert!(matches!(
        rejected(&bytes, Direction::HostToProvider),
        ProviderError::InvalidValue { .. }
    ));
}

#[test]
fn zero_generation_is_rejected() {
    let bytes = envelope(
        "deactivate-panel",
        "h-000002",
        1,
        r#"{"panel_instance_id":1,"generation":0,"reason":"suspend"}"#,
    );
    assert!(matches!(
        rejected(&bytes, Direction::HostToProvider),
        ProviderError::InvalidValue { .. }
    ));
}

#[test]
fn zero_revision_is_rejected() {
    let bytes = envelope(
        "panel-event",
        "h-000003",
        1,
        r#"{"panel_instance_id":1,"generation":1,"revision":0,"event":{"kind":"retry"}}"#,
    );
    assert!(matches!(
        rejected(&bytes, Direction::HostToProvider),
        ProviderError::InvalidValue { .. }
    ));
}

#[test]
fn snapshot_revision_must_be_positive() {
    let bad = envelope(
        "panel-snapshot",
        "p-000001",
        1,
        r#"{"model_schema":1,"panel_instance_id":1,"generation":1,"revision":0,"kind":"empty","title":"t","loading":false,"action_affordances":[],"body":{"kind":"empty","message":"x"}}"#,
    );
    assert!(matches!(
        rejected(&bad, Direction::ProviderToHost),
        ProviderError::InvalidValue { .. }
    ));
}

#[test]
fn wrong_model_schema_is_rejected() {
    let bad = envelope(
        "panel-snapshot",
        "p-000001",
        1,
        r#"{"model_schema":2,"panel_instance_id":1,"generation":1,"revision":1,"kind":"empty","title":"t","loading":false,"action_affordances":[],"body":{"kind":"empty","message":"x"}}"#,
    );
    assert!(matches!(
        rejected(&bad, Direction::ProviderToHost),
        ProviderError::InvalidValue { .. }
    ));
}

// ---------------------------------------------------------------------------
// H. Mismatched kind
// ---------------------------------------------------------------------------

#[test]
fn snapshot_body_kind_is_required() {
    let bytes = envelope(
        "panel-snapshot",
        "p-000001",
        1,
        r#"{"model_schema":1,"panel_instance_id":1,"generation":1,"revision":1,"kind":"empty","title":"t","loading":false,"action_affordances":[],"body":{"message":"wrong"}}"#,
    );
    assert!(matches!(
        rejected(&bytes, Direction::ProviderToHost),
        ProviderError::MissingField { .. }
    ));
}

#[test]
fn snapshot_kind_must_match_body_kind() {
    let bytes = envelope(
        "panel-snapshot",
        "p-000001",
        1,
        r#"{"model_schema":1,"panel_instance_id":1,"generation":1,"revision":1,"kind":"list","title":"t","loading":false,"action_affordances":[],"body":{"kind":"empty","message":"wrong"}}"#,
    );
    assert!(matches!(
        rejected(&bytes, Direction::ProviderToHost),
        ProviderError::InvalidValue { .. }
    ));
}

#[test]
fn event_mismatched_kind_tag_is_rejected() {
    let bytes = envelope(
        "panel-event",
        "h-000003",
        1,
        r#"{"panel_instance_id":1,"generation":1,"revision":1,"event":{"kind":"retry","id":"vendor.i"}}"#,
    );
    assert!(matches!(
        rejected(&bytes, Direction::HostToProvider),
        ProviderError::UnknownField { .. }
    ));
}

// ---------------------------------------------------------------------------
// I. Selected id must exist
// ---------------------------------------------------------------------------

#[test]
fn list_selected_id_must_reference_an_item() {
    let bytes = envelope(
        "panel-snapshot",
        "p-000001",
        1,
        &snapshot_body(
            "list",
            r#"{"items":[{"id":"vendor.a","label":"A","actions":[]}],"selected_id":"vendor.missing"}"#,
        ),
    );
    assert!(matches!(
        rejected(&bytes, Direction::ProviderToHost),
        ProviderError::InvalidValue { .. }
    ));
}

#[test]
fn duplicate_list_item_ids_are_rejected() {
    let bytes = envelope(
        "panel-snapshot",
        "p-000001",
        1,
        &snapshot_body(
            "list",
            r#"{"items":[{"id":"vendor.dup","label":"A","actions":[]},{"id":"vendor.dup","label":"B","actions":[]}]}"#,
        ),
    );
    assert!(matches!(
        rejected(&bytes, Direction::ProviderToHost),
        ProviderError::InvalidValue { .. }
    ));
}

// ---------------------------------------------------------------------------
// J. Progress counts
// ---------------------------------------------------------------------------

#[test]
fn progress_total_requires_completed() {
    let bytes = envelope(
        "panel-snapshot",
        "p-000001",
        1,
        &snapshot_body(
            "progress",
            r#"{"message":"m","total":5,"cancellable":false}"#,
        ),
    );
    assert!(matches!(
        rejected(&bytes, Direction::ProviderToHost),
        ProviderError::InvalidValue { .. }
    ));
}

#[test]
fn progress_completed_must_not_exceed_total() {
    let bytes = envelope(
        "panel-snapshot",
        "p-000001",
        1,
        &snapshot_body(
            "progress",
            r#"{"message":"m","completed":6,"total":5,"cancellable":false}"#,
        ),
    );
    assert!(matches!(
        rejected(&bytes, Direction::ProviderToHost),
        ProviderError::InvalidValue { .. }
    ));
}

#[test]
fn progress_completed_equals_total_is_accepted() {
    let snap = parse_snapshot(
        "progress",
        r#"{"message":"m","completed":5,"total":5,"cancellable":false}"#,
    );
    let PanelBody::Progress(p) = snap.body else {
        panic!("expected Progress");
    };
    assert_eq!(p.completed, Some(5));
}

// ---------------------------------------------------------------------------
// K. Affordance availability
// ---------------------------------------------------------------------------

include!("panel_wire_bounds_tests.rs");

include!("panel_tree_diff_tests.rs");

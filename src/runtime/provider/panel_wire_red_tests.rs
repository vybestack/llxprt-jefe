//! RED evidence for issue #391: the closed panel/migration wire kinds and
//! typed direct messages do not yet exist on the parser.
//!
//! These tests compile against the current (CW-10) surface and fail because the
//! six new message kinds (`activate-panel`, `deactivate-panel`, `panel-event`,
//! `panel-snapshot`, `migrate-config`, `migrated-config`) are unknown to the
//! parser, so well-formed frames are rejected instead of parsed.

use super::protocol::{Direction, parse_message};

/// Build one LF-terminated envelope line from its scalar parts.
fn envelope(ty: &str, request_id: &str, generation: u64, payload: &str) -> Vec<u8> {
    format!(
        "{{\"protocol\":1,\"type\":\"{ty}\",\"request_id\":\"{request_id}\",\"generation\":{generation},\"payload\":{payload}}}\n"
    )
    .into_bytes()
}

#[test]
fn red_activate_panel_kind_is_unrecognized() {
    let bytes = envelope(
        "activate-panel",
        "h-000001",
        1,
        r#"{"panel_instance_id":1,"screen_instance_id":1,"panel_type":"vendor.panel","activation":{},"generation":1}"#,
    );
    let result = parse_message(&bytes, Direction::HostToProvider);
    assert!(result.is_ok(), "activate-panel must parse: {result:?}");
}

#[test]
fn red_deactivate_panel_kind_is_unrecognized() {
    let bytes = envelope(
        "deactivate-panel",
        "h-000002",
        1,
        r#"{"panel_instance_id":1,"generation":1,"reason":"suspend"}"#,
    );
    let result = parse_message(&bytes, Direction::HostToProvider);
    assert!(result.is_ok(), "deactivate-panel must parse: {result:?}");
}

#[test]
fn red_panel_event_kind_is_unrecognized() {
    let bytes = envelope(
        "panel-event",
        "h-000003",
        1,
        r#"{"panel_instance_id":1,"generation":1,"revision":1,"event":{"kind":"retry"}}"#,
    );
    let result = parse_message(&bytes, Direction::HostToProvider);
    assert!(result.is_ok(), "panel-event must parse: {result:?}");
}

#[test]
fn red_panel_snapshot_kind_is_unrecognized() {
    let bytes = envelope(
        "panel-snapshot",
        "p-000001",
        1,
        r#"{"model_schema":1,"panel_instance_id":1,"generation":1,"revision":1,"kind":"empty","title":"t","loading":false,"action_affordances":[],"body":{"kind":"empty","message":"nothing"}}"#,
    );
    let result = parse_message(&bytes, Direction::ProviderToHost);
    assert!(result.is_ok(), "panel-snapshot must parse: {result:?}");
}

#[test]
fn red_migrate_config_kind_is_unrecognized() {
    let bytes = envelope(
        "migrate-config",
        "h-000004",
        1,
        r#"{"from_version":1,"to_version":2,"config":{},"draft_token":1}"#,
    );
    let result = parse_message(&bytes, Direction::HostToProvider);
    assert!(result.is_ok(), "migrate-config must parse: {result:?}");
}

#[test]
fn red_migrated_config_kind_is_unrecognized() {
    let bytes = envelope(
        "migrated-config",
        "h-000004",
        1,
        r#"{"from_version":1,"to_version":2,"config":{},"draft_token":1,"target_config":{},"notes":[]}"#,
    );
    let result = parse_message(&bytes, Direction::ProviderToHost);
    assert!(result.is_ok(), "migrated-config must parse: {result:?}");
}

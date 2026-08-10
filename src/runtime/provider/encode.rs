//! Closed host-to-provider JSONL encoding (issue #390 CW-10, Slice C1).
//!
//! Slice A delivered the decoding half of the wire boundary; this module is its
//! inverse for the five host-sent messages (`hello`, `configure`,
//! `invoke-action`, `cancel`, `shutdown`). Encoding is closed, typed, and
//! infallible: every field is emitted by a hand-written writer that reads
//! domain values through their validated accessors, and the recursive typed
//! values are rendered through their canonical string forms so the closed
//! `{"type","value"}` shape round-trips through the bounded reader exactly. No
//! dynamic JSON value is constructed and no error type is needed, because
//! every input is already a validated closed value.
//!
//! No process, application state, effect, or persistence lives here.

use std::fmt::Write as _;

use crate::domain::CanonicalSemver;
use crate::domain::{CanonicalDateTime, CanonicalDecimal, Id, SecretRef, TypedMap, TypedValue};

use super::dto::{ConfigurePayload, Continuation, InvokeActionPayload, InvokeContext};
use super::identifiers::{EnvName, RequestId};

/// The single wire protocol version this layer emits.
const PROTOCOL_VERSION: u64 = 1;

/// Encode a `hello` frame.
#[must_use]
pub fn encode_hello(
    request_id: &RequestId,
    generation: u64,
    host_api: &str,
    plugin_id: &Id,
    plugin_version: &CanonicalSemver,
) -> Vec<u8> {
    let mut payload = String::new();
    payload.push_str("{\"host_api\":");
    json_string(&mut payload, host_api);
    payload.push_str(",\"plugin_id\":");
    json_string(&mut payload, plugin_id.as_str());
    payload.push_str(",\"plugin_version\":");
    json_string(&mut payload, plugin_version.as_str());
    payload.push('}');
    envelope("hello", request_id, generation, &payload)
}

/// Encode a `configure` frame.
#[must_use]
pub fn encode_configure(
    request_id: &RequestId,
    generation: u64,
    payload: &ConfigurePayload,
) -> Vec<u8> {
    let mut body = String::new();
    let _ = write!(body, "{{\"config_version\":{}", payload.config_version);
    body.push_str(",\"config\":");
    typed_map_to_json(&mut body, &payload.config);
    body.push_str(",\"secrets\":");
    env_string_map_to_json(&mut body, &payload.secrets);
    body.push_str(",\"environment\":");
    env_string_map_to_json(&mut body, &payload.environment);
    body.push('}');
    envelope("configure", request_id, generation, &body)
}

/// Encode an `invoke-action` frame.
#[must_use]
pub fn encode_invoke_action(
    request_id: &RequestId,
    generation: u64,
    payload: &InvokeActionPayload,
) -> Vec<u8> {
    let mut body = String::new();
    body.push_str("{\"invocation_id\":");
    json_string(&mut body, payload.invocation_id.as_str());
    body.push_str(",\"action_id\":");
    json_string(&mut body, payload.action_id.as_str());
    body.push_str(",\"arguments\":");
    typed_map_to_json(&mut body, &payload.arguments);
    body.push_str(",\"context\":");
    invoke_context_to_json(&mut body, &payload.context);
    if let Some(continuation) = payload.continuation.as_ref() {
        body.push_str(",\"continuation\":");
        continuation_to_json(&mut body, continuation);
    }
    body.push('}');
    envelope("invoke-action", request_id, generation, &body)
}

/// Encode a `cancel` frame.
#[must_use]
pub fn encode_cancel(request_id: &RequestId, generation: u64, target: &RequestId) -> Vec<u8> {
    let mut payload = String::new();
    payload.push_str("{\"target_request_id\":");
    json_string(&mut payload, &target.as_str());
    payload.push('}');
    envelope("cancel", request_id, generation, &payload)
}

/// Encode a `shutdown` frame.
#[must_use]
pub fn encode_shutdown(
    request_id: &RequestId,
    generation: u64,
    reason: super::dto::ShutdownReason,
) -> Vec<u8> {
    let mut payload = String::new();
    payload.push_str("{\"reason\":");
    json_string(&mut payload, reason.as_str());
    payload.push('}');
    envelope("shutdown", request_id, generation, &payload)
}

/// Encode an `activate-panel` frame (issue #391).
#[must_use]
pub fn encode_activate_panel(
    request_id: &RequestId,
    generation: u64,
    payload: &super::panel_model::ActivatePanelPayload,
) -> Vec<u8> {
    let mut body = String::new();
    body.push('{');
    q(&mut body, "panel_instance_id");
    let _ = write!(body, ":{},", payload.panel_instance_id);
    q(&mut body, "screen_instance_id");
    let _ = write!(body, ":{},", payload.screen_instance_id);
    q(&mut body, "panel_type");
    body.push(':');
    json_string(&mut body, payload.panel_type.as_str());
    body.push(',');
    q(&mut body, "activation");
    body.push(':');
    typed_map_to_json(&mut body, &payload.activation);
    if let Some(host_local) = payload.prior_host_local.as_ref() {
        body.push(',');
        q(&mut body, "prior_host_local");
        body.push(':');
        host_local_to_json(&mut body, host_local);
    }
    body.push(',');
    q(&mut body, "generation");
    let _ = write!(body, ":{}}}", payload.generation);
    envelope("activate-panel", request_id, generation, &body)
}

/// Encode a `deactivate-panel` frame (issue #391).
#[must_use]
pub fn encode_deactivate_panel(
    request_id: &RequestId,
    generation: u64,
    payload: &super::panel_model::DeactivatePanelPayload,
) -> Vec<u8> {
    let mut body = String::new();
    body.push('{');
    q(&mut body, "panel_instance_id");
    let _ = write!(body, ":{},", payload.panel_instance_id);
    q(&mut body, "generation");
    let _ = write!(body, ":{},", payload.generation);
    q(&mut body, "reason");
    body.push(':');
    json_string(&mut body, payload.reason.as_str());
    body.push('}');
    envelope("deactivate-panel", request_id, generation, &body)
}

/// Encode a `panel-event` frame (issue #391).
#[must_use]
pub fn encode_panel_event(
    request_id: &RequestId,
    generation: u64,
    payload: &super::panel_model::PanelEventPayload,
) -> Vec<u8> {
    let mut body = String::new();
    body.push('{');
    q(&mut body, "panel_instance_id");
    let _ = write!(body, ":{},", payload.panel_instance_id);
    q(&mut body, "generation");
    let _ = write!(body, ":{},", payload.generation);
    q(&mut body, "revision");
    let _ = write!(body, ":{},", payload.revision);
    q(&mut body, "event");
    body.push(':');
    panel_event_to_json(&mut body, &payload.event);
    body.push('}');
    envelope("panel-event", request_id, generation, &body)
}

/// Encode a `migrate-config` frame (issue #391).
#[must_use]
pub fn encode_migrate_config(
    request_id: &RequestId,
    generation: u64,
    payload: &super::panel_model::MigrateConfigPayload,
) -> Vec<u8> {
    let mut body = String::new();
    body.push('{');
    q(&mut body, "from_version");
    let _ = write!(body, ":{},", payload.from_version);
    q(&mut body, "to_version");
    let _ = write!(body, ":{},", payload.to_version);
    q(&mut body, "config");
    body.push(':');
    typed_map_to_json(&mut body, &payload.config);
    body.push(',');
    q(&mut body, "draft_token");
    let _ = write!(body, ":{}}}", payload.draft_token);
    envelope("migrate-config", request_id, generation, &body)
}

/// Append one quoted JSON key.
fn q(out: &mut String, key: &str) {
    out.push('"');
    out.push_str(key);
    out.push('"');
}

/// Append a closed `HostLocal` object.
fn host_local_to_json(out: &mut String, host_local: &super::panel_model::HostLocal) {
    out.push('{');
    let focus_emitted = if let Some(focus) = host_local.focus_target.as_ref() {
        q(out, "focus_target");
        out.push(':');
        json_string(out, focus.as_str());
        true
    } else {
        false
    };
    if focus_emitted {
        out.push(',');
    }
    q(out, "scroll_offset");
    let _ = write!(out, ":{}", host_local.scroll_offset);
    if let Some(selected) = host_local.selected_id.as_ref() {
        out.push(',');
        q(out, "selected_id");
        out.push(':');
        json_string(out, selected.as_str());
    }
    if let Some(draft) = host_local.form_draft.as_ref() {
        out.push(',');
        q(out, "form_draft");
        out.push(':');
        typed_map_to_json(out, draft);
    }
    out.push('}');
}

/// Append a `"key":value` string pair after a comma separator.
fn append_string_field(out: &mut String, key: &str, value: &str) {
    out.push(',');
    q(out, key);
    out.push(':');
    json_string(out, value);
}

/// Append a closed tagged `PanelEvent` `{kind, ...}`.
fn panel_event_to_json(out: &mut String, event: &super::panel_model::PanelEvent) {
    out.push('{');
    q(out, "kind");
    out.push(':');
    match event {
        super::panel_model::PanelEvent::Selected { id } => {
            json_string(out, "selected");
            append_string_field(out, "id", id.as_str());
        }
        super::panel_model::PanelEvent::Activated { id } => {
            json_string(out, "activated");
            append_string_field(out, "id", id.as_str());
        }
        super::panel_model::PanelEvent::Action { id, arguments } => {
            json_string(out, "action");
            append_string_field(out, "id", id.as_str());
            out.push(',');
            q(out, "arguments");
            out.push(':');
            typed_map_to_json(out, arguments);
        }
        super::panel_model::PanelEvent::FieldChanged { field_id, value } => {
            json_string(out, "field-changed");
            append_string_field(out, "field_id", field_id.as_str());
            out.push(',');
            q(out, "value");
            out.push(':');
            typed_value_to_json(out, value);
        }
        super::panel_model::PanelEvent::Submit { values } => {
            json_string(out, "submit");
            out.push(',');
            q(out, "values");
            out.push(':');
            typed_map_to_json(out, values);
        }
        super::panel_model::PanelEvent::PageRequested { token } => {
            json_string(out, "page-requested");
            append_string_field(out, "token", token);
        }
        super::panel_model::PanelEvent::Retry => json_string(out, "retry"),
        super::panel_model::PanelEvent::Cancel => json_string(out, "cancel"),
        super::panel_model::PanelEvent::LinkSelected { link_id } => {
            json_string(out, "link-selected");
            append_string_field(out, "link_id", link_id.as_str());
        }
    }
    out.push('}');
}

/// Wrap one payload in the closed envelope and terminate it with a line feed.
fn envelope(kind: &str, request_id: &RequestId, generation: u64, payload: &str) -> Vec<u8> {
    let mut out = String::new();
    out.push('{');
    let _ = write!(out, "\"protocol\":{PROTOCOL_VERSION},\"type\":");
    json_string(&mut out, kind);
    out.push_str(",\"request_id\":");
    json_string(&mut out, &request_id.as_str());
    let _ = write!(out, ",\"generation\":{generation},\"payload\":{payload}");
    out.push('}');
    out.push('\n');
    out.into_bytes()
}

/// Append a JSON string literal for `value`, escaping per RFC 8259.
fn json_string(out: &mut String, value: &str) {
    out.push('"');
    for ch in value.chars() {
        match ch {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            '\u{08}' => out.push_str("\\b"),
            '\u{0c}' => out.push_str("\\f"),
            c if (c as u32) < 0x20 => {
                let _ = write!(out, "\\u{:04x}", c as u32);
            }
            c => out.push(c),
        }
    }
    out.push('"');
}

/// Append a closed typed map as a JSON object keyed by field id.
fn typed_map_to_json(out: &mut String, map: &TypedMap) {
    out.push('{');
    for (index, (key, value)) in map.iter().enumerate() {
        if index > 0 {
            out.push(',');
        }
        json_string(out, key.as_str());
        out.push(':');
        typed_value_to_json(out, value);
    }
    out.push('}');
}

/// Append one closed typed value in its canonical `{"type","value"}` shape.
fn typed_value_to_json(out: &mut String, value: &TypedValue) {
    out.push_str("{\"type\":");
    match value {
        TypedValue::String(text) => {
            out.push_str("\"string\",\"value\":");
            json_string(out, text);
        }
        TypedValue::Bool(flag) => {
            let _ = write!(out, "\"bool\",\"value\":{flag}");
        }
        TypedValue::Integer(number) => {
            let _ = write!(out, "\"integer\",\"value\":{number}");
        }
        TypedValue::Decimal(number) => {
            out.push_str("\"decimal\",\"value\":");
            json_string(out, decimal_str(number));
        }
        TypedValue::Datetime(moment) => {
            out.push_str("\"datetime\",\"value\":");
            json_string(out, datetime_str(moment));
        }
        TypedValue::List(items) => {
            out.push_str("\"list\",\"value\":[");
            for (index, item) in items.iter().enumerate() {
                if index > 0 {
                    out.push(',');
                }
                typed_value_to_json(out, item);
            }
            out.push(']');
        }
        TypedValue::Map(inner) => {
            out.push_str("\"map\",\"value\":");
            typed_map_to_json(out, inner);
        }
        TypedValue::SecretRef(reference) => {
            out.push_str("\"secret_ref\",\"value\":");
            secret_ref_to_json(out, reference);
        }
    }
    out.push('}');
}

fn secret_ref_to_json(out: &mut String, reference: &SecretRef) {
    out.push_str("{\"env\":");
    json_string(out, reference.env.env());
    out.push('}');
}

fn invoke_context_to_json(out: &mut String, context: &InvokeContext) {
    out.push_str("{\"screen_id\":");
    json_string(out, context.screen_id.as_str());
    out.push_str(",\"screen_instance\":");
    json_string(out, context.screen_instance.as_str());
    out.push_str(",\"resource_refs\":");
    typed_map_to_json(out, &context.resource_refs);
    out.push('}');
}

fn continuation_to_json(out: &mut String, continuation: &Continuation) {
    out.push_str("{\"confirmation_id\":");
    json_string(out, continuation.confirmation_id.as_str());
    let _ = write!(out, ",\"approved\":{}", continuation.approved);
    out.push_str(",\"values\":");
    typed_map_to_json(out, &continuation.values);
    out.push('}');
}

/// Append an environment-name-keyed string map as a JSON object.
fn env_string_map_to_json(out: &mut String, map: &std::collections::BTreeMap<EnvName, String>) {
    out.push('{');
    for (index, (key, value)) in map.iter().enumerate() {
        if index > 0 {
            out.push(',');
        }
        json_string(out, key.as_str());
        out.push(':');
        json_string(out, value);
    }
    out.push('}');
}

fn decimal_str(value: &CanonicalDecimal) -> &str {
    value.as_str()
}

fn datetime_str(value: &CanonicalDateTime) -> &str {
    value.as_str()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::action_registry::ActionId;
    use crate::runtime::provider::dto::{
        InvokeActionPayload, InvokeContext, ProviderMessage, ShutdownReason,
    };
    use crate::runtime::provider::identifiers::{Direction, RequestId};
    use crate::runtime::provider::payload_reader::parse_message;

    fn host_id() -> RequestId {
        RequestId::parse("h-000001").unwrap_or_else(|error| panic!("valid host id: {error:?}"))
    }

    /// Round-trip a host-emitted frame through the real closed decoder, proving
    /// the encoder produces exactly the envelope and payload the decoder accepts.
    fn round_trip_host(bytes: &[u8]) -> crate::runtime::provider::dto::ParsedMessage {
        parse_message(bytes, Direction::HostToProvider)
            .unwrap_or_else(|error| panic!("host round-trip failed: {error}"))
    }

    #[test]
    fn hello_round_trips_with_plugin_identity() {
        let plugin_id =
            Id::parse("vendor.pkg").unwrap_or_else(|error| panic!("valid id: {error:?}"));
        let version = CanonicalSemver::parse("1.2.3")
            .unwrap_or_else(|error| panic!("valid version: {error:?}"));
        let bytes = encode_hello(&host_id(), 7, "jefe/0.1", &plugin_id, &version);
        let parsed = round_trip_host(&bytes);
        assert_eq!(parsed.generation, 7);
        let ProviderMessage::Hello(hello) = &parsed.message else {
            panic!("expected hello");
        };
        assert_eq!(hello.host_api, "jefe/0.1");
        assert_eq!(hello.plugin_id, plugin_id);
        assert_eq!(hello.plugin_version, version);
    }

    #[test]
    fn shutdown_round_trips_with_the_declared_reason() {
        let bytes = encode_shutdown(&host_id(), 1, ShutdownReason::Completed);
        let parsed = round_trip_host(&bytes);
        let ProviderMessage::Shutdown(payload) = &parsed.message else {
            panic!("expected shutdown");
        };
        assert_eq!(payload.reason, ShutdownReason::Completed);
    }

    #[test]
    fn cancel_round_trips_the_target_request_id() {
        let target = RequestId::parse("p-000042")
            .unwrap_or_else(|error| panic!("valid target id: {error:?}"));
        let bytes = encode_cancel(&host_id(), 3, &target);
        let parsed = round_trip_host(&bytes);
        let ProviderMessage::Cancel(cancel) = &parsed.message else {
            panic!("expected cancel");
        };
        assert_eq!(cancel.target_request_id, target);
    }

    #[test]
    fn invoke_action_round_trips_action_id_as_text() {
        let action = ActionId::parse("vendor.pkg.run")
            .unwrap_or_else(|error| panic!("valid action id: {error:?}"));
        let mut arguments = TypedMap::new();
        arguments.insert(
            Id::parse("vendor.pkg.arg").unwrap_or_else(|error| panic!("id: {error:?}")),
            TypedValue::String("v".to_owned()),
        );
        let payload = InvokeActionPayload {
            invocation_id: Id::parse("vendor.pkg.inv")
                .unwrap_or_else(|error| panic!("id: {error:?}")),
            action_id: action.clone(),
            arguments,
            context: InvokeContext {
                screen_id: Id::parse("vendor.pkg.screen")
                    .unwrap_or_else(|error| panic!("id: {error:?}")),
                screen_instance: Id::parse("vendor.pkg.inst")
                    .unwrap_or_else(|error| panic!("id: {error:?}")),
                resource_refs: TypedMap::new(),
            },
            continuation: None,
        };
        let bytes = encode_invoke_action(&host_id(), 1, &payload);
        let parsed = round_trip_host(&bytes);
        let ProviderMessage::InvokeAction(invoked) = &parsed.message else {
            panic!("expected invoke-action");
        };
        assert_eq!(invoked.action_id, action);
    }

    #[test]
    fn every_host_frame_ends_with_a_single_line_feed() {
        let plugin_id =
            Id::parse("vendor.pkg").unwrap_or_else(|error| panic!("valid id: {error:?}"));
        let version = CanonicalSemver::parse("1.0.0")
            .unwrap_or_else(|error| panic!("valid version: {error:?}"));
        let frames = [
            encode_hello(&host_id(), 1, "api", &plugin_id, &version),
            encode_shutdown(&host_id(), 1, ShutdownReason::HostExit),
            encode_cancel(&host_id(), 1, &host_id()),
        ];
        for frame in frames {
            assert_eq!(frame.last(), Some(&b'\n'), "frame terminates with LF");
            assert!(
                !frame[..frame.len() - 1].contains(&b'\n'),
                "no interior line feed"
            );
        }
    }

    #[test]
    fn special_characters_in_strings_are_escaped() {
        let plugin_id =
            Id::parse("vendor.pkg").unwrap_or_else(|error| panic!("valid id: {error:?}"));
        let version = CanonicalSemver::parse("1.0.0")
            .unwrap_or_else(|error| panic!("valid version: {error:?}"));
        let bytes = encode_hello(&host_id(), 1, "line\nbreak\"quote", &plugin_id, &version);
        // Escaping keeps the frame decodable and removes interior control bytes.
        round_trip_host(&bytes);
        let text = String::from_utf8(bytes).unwrap_or_else(|error| panic!("utf8: {error:?}"));
        assert!(text.contains("\\n"), "newline escaped");
        assert!(text.contains("\\\""), "quote escaped");
    }
}

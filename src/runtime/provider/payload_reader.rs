//! Envelope and payload decoding for the action-provider protocol
//! (issue #390 CW-10, Slice A).
//!
//! [`parse_message`] is the wire entry point: it delegates byte-level framing
//! and JSON well-formedness to [`super::framing`], then maps the closed
//! envelope onto its typed message, validating the fixed protocol version, the
//! message direction, the request id, and the positive generation before
//! dispatching the payload onto its closed reader. Each payload object lists
//! exactly the keys it admits; any other key is an unknown-field fault, and the
//! shared bounded reader has already rejected duplicate keys at every level.
//!
//! No process, application state, effect, or persistence lives here.

use crate::domain::CanonicalSemver;
use crate::domain::action_registry::ActionId;
use crate::domain::bounded_json::BoundedJson;

use super::dto::{
    CancelPayload, Capability, ConfigurePayload, Continuation, ErrorPayload, FieldError,
    HelloAckPayload, HelloPayload, InvokeActionPayload, InvokeContext, Outcome, ParsedMessage,
    ProgressPayload, ProviderMessage, ReadyPayload, Severity, ShutdownPayload, ShutdownReason,
};
use super::error::ProviderError;
use super::framing;
use super::identifiers::{Direction, MessageKind, PROTOCOL_VERSION, RequestId};
use super::object_reader::{
    array, closed_object, find, read_bool, read_enum, read_enum_array, read_id, read_optional_u64,
    read_string, read_u16, read_u64, read_with, reject_duplicates, require, type_mismatch,
};
use super::typed_value::{read_env_string_map, read_field_declaration, read_typed_map};

/// Maximum field errors one provider error payload may carry (CW10-06 bound).
const FIELD_ERROR_LIMIT: usize = 128;

/// Maximum continuation-schema field declarations (CW10-06 bound).
const CONTINUATION_SCHEMA_LIMIT: usize = 64;

// Closed field tables. Every payload object lists exactly the keys it admits.
const ENVELOPE_KEYS: [&str; 5] = ["protocol", "type", "request_id", "generation", "payload"];
const HELLO_KEYS: [&str; 3] = ["host_api", "plugin_id", "plugin_version"];
const HELLO_ACK_KEYS: [&str; 2] = ["provider_name", "protocol"];
const CONFIGURE_KEYS: [&str; 4] = ["config_version", "config", "secrets", "environment"];
const READY_KEYS: [&str; 1] = ["capabilities"];
const INVOKE_KEYS: [&str; 5] = [
    "invocation_id",
    "action_id",
    "arguments",
    "context",
    "continuation",
];
const CONTEXT_KEYS: [&str; 3] = ["screen_id", "screen_instance", "resource_refs"];
const CONTINUATION_KEYS: [&str; 3] = ["confirmation_id", "approved", "values"];
const CANCEL_KEYS: [&str; 1] = ["target_request_id"];
const PROGRESS_KEYS: [&str; 4] = ["sequence", "message", "completed", "total"];
const ERROR_KEYS: [&str; 4] = ["code", "message", "retryable", "field_errors"];
const FIELD_ERROR_KEYS: [&str; 2] = ["path", "message"];
const SHUTDOWN_KEYS: [&str; 1] = ["reason"];
const OUTCOME_NAVIGATE_KEYS: [&str; 3] = ["kind", "route_id", "activation"];
const OUTCOME_REFRESH_KEYS: [&str; 2] = ["kind", "resource_ref"];
const OUTCOME_NOTICE_KEYS: [&str; 3] = ["kind", "severity", "message"];
const OUTCOME_RHC_KEYS: [&str; 7] = [
    "kind",
    "confirmation_id",
    "title",
    "body",
    "confirm_label",
    "destructive",
    "continuation_schema",
];

/// Parse one provider line into a typed message.
///
/// Framing, exact payload-byte accounting, and JSON well-formedness are delegated to
/// [`framing::decode_with_top_member_bytes`]; this
/// function then maps the envelope, validates the fixed protocol version, the
/// message direction against `stream`, the request id, and the positive
/// generation, and maps the closed payload.
///
/// # Errors
///
/// Returns [`ProviderError`] (`PLG-E502`) for any framing, shape, direction,
/// request-id, generation, or payload fault.
pub fn parse_message(bytes: &[u8], stream: Direction) -> Result<ParsedMessage, ProviderError> {
    let (value, payload_byte_count) = framing::decode_with_top_member_bytes(bytes, "payload")?;
    let members = closed_object(&value, "envelope", &ENVELOPE_KEYS)?;
    let protocol = read_u64(members, "envelope", "protocol")?;
    if protocol != PROTOCOL_VERSION {
        return Err(ProviderError::InvalidValue {
            path: "envelope.protocol".to_owned(),
            reason: format!("protocol {protocol} is not the supported version {PROTOCOL_VERSION}"),
        });
    }
    let type_text = read_string(members, "envelope", "type")?;
    let kind = MessageKind::from_wire(type_text).ok_or_else(|| ProviderError::UnknownValue {
        path: "envelope.type".to_owned(),
        value: type_text.to_owned(),
    })?;
    if kind.direction() != stream {
        return Err(ProviderError::InvalidDirection {
            kind: kind.as_str().to_owned(),
            stream: stream.as_str().to_owned(),
        });
    }
    let request_id_raw = read_string(members, "envelope", "request_id")?;
    let request_id =
        RequestId::parse(request_id_raw).map_err(|_| ProviderError::InvalidRequestId {
            raw: request_id_raw.to_owned(),
        })?;
    // Request origin is determined by message role, not stream direction: the
    // direct `migrated-config` response echoes the host request id and the
    // asynchronous `panel-snapshot` is provider-originated (issue #391).
    if request_id.origin() != kind.request_origin() {
        return Err(ProviderError::InvalidRequestOrigin {
            raw: request_id_raw.to_owned(),
            stream: stream.as_str().to_owned(),
        });
    }
    let generation = read_u64(members, "envelope", "generation")?;
    if generation == 0 {
        return Err(ProviderError::InvalidGeneration { value: 0 });
    }
    let payload = require(members, "envelope", "payload")?;
    let message = read_payload(kind, payload)?;
    Ok(ParsedMessage {
        request_id,
        generation,
        payload_byte_count: payload_byte_count.ok_or_else(|| ProviderError::MissingField {
            path: "envelope".to_owned(),
            field: "payload".to_owned(),
        })?,
        message,
    })
}

/// Dispatch the payload object onto its closed reader.
fn read_payload(
    kind: MessageKind,
    payload: &BoundedJson,
) -> Result<ProviderMessage, ProviderError> {
    match kind {
        MessageKind::Hello => read_hello(payload).map(ProviderMessage::Hello),
        MessageKind::HelloAck => read_hello_ack(payload).map(ProviderMessage::HelloAck),
        MessageKind::Configure => read_configure(payload).map(ProviderMessage::Configure),
        MessageKind::Ready => read_ready(payload).map(ProviderMessage::Ready),
        MessageKind::InvokeAction => read_invoke_action(payload).map(ProviderMessage::InvokeAction),
        MessageKind::Cancel => read_cancel(payload).map(ProviderMessage::Cancel),
        MessageKind::Progress => read_progress(payload).map(ProviderMessage::Progress),
        MessageKind::Outcome => read_outcome(payload).map(ProviderMessage::Outcome),
        MessageKind::Error => read_error(payload).map(ProviderMessage::Error),
        MessageKind::Shutdown => read_shutdown(payload).map(ProviderMessage::Shutdown),
        MessageKind::ShutdownAck => {
            read_shutdown_ack(payload).map(|()| ProviderMessage::ShutdownAck)
        }
        MessageKind::ActivatePanel => {
            super::panel_reader::read_activate_panel(payload).map(ProviderMessage::ActivatePanel)
        }
        MessageKind::DeactivatePanel => super::panel_reader::read_deactivate_panel(payload)
            .map(ProviderMessage::DeactivatePanel),
        MessageKind::PanelEvent => {
            super::panel_reader::read_panel_event(payload).map(ProviderMessage::PanelEvent)
        }
        MessageKind::PanelSnapshot => {
            super::panel_reader::read_panel_snapshot(payload).map(ProviderMessage::PanelSnapshot)
        }
        MessageKind::MigrateConfig => {
            super::panel_reader::read_migrate_config(payload).map(ProviderMessage::MigrateConfig)
        }
        MessageKind::MigratedConfig => {
            super::panel_reader::read_migrated_config(payload).map(ProviderMessage::MigratedConfig)
        }
    }
}

fn read_hello(payload: &BoundedJson) -> Result<HelloPayload, ProviderError> {
    let members = closed_object(payload, "hello", &HELLO_KEYS)?;
    Ok(HelloPayload {
        host_api: read_string(members, "hello", "host_api")?.to_owned(),
        plugin_id: read_id(members, "hello", "plugin_id")?,
        plugin_version: read_with(members, "hello", "plugin_version", CanonicalSemver::parse)?,
    })
}

fn read_hello_ack(payload: &BoundedJson) -> Result<HelloAckPayload, ProviderError> {
    let members = closed_object(payload, "hello-ack", &HELLO_ACK_KEYS)?;
    let protocol = read_u64(members, "hello-ack", "protocol")?;
    if protocol != PROTOCOL_VERSION {
        return Err(ProviderError::InvalidValue {
            path: "hello-ack.protocol".to_owned(),
            reason: format!("protocol {protocol} is not the supported version {PROTOCOL_VERSION}"),
        });
    }
    Ok(HelloAckPayload {
        provider_name: read_string(members, "hello-ack", "provider_name")?.to_owned(),
    })
}

fn read_configure(payload: &BoundedJson) -> Result<ConfigurePayload, ProviderError> {
    let members = closed_object(payload, "configure", &CONFIGURE_KEYS)?;
    let config_version = read_u64(members, "configure", "config_version")?;
    let config = read_typed_map(require(members, "configure", "config")?, "configure.config")?;
    let secrets = read_env_string_map(
        require(members, "configure", "secrets")?,
        "configure.secrets",
    )?;
    let environment = read_env_string_map(
        require(members, "configure", "environment")?,
        "configure.environment",
    )?;
    Ok(ConfigurePayload {
        config_version,
        config,
        secrets,
        environment,
    })
}

fn read_ready(payload: &BoundedJson) -> Result<ReadyPayload, ProviderError> {
    let members = closed_object(payload, "ready", &READY_KEYS)?;
    let capabilities = read_enum_array(
        require(members, "ready", "capabilities")?,
        "ready.capabilities",
        Capability::ALL.len(),
        Capability::from_wire,
    )?;
    reject_duplicates(&capabilities, "ready.capabilities")?;
    Ok(ReadyPayload { capabilities })
}

fn read_invoke_action(payload: &BoundedJson) -> Result<InvokeActionPayload, ProviderError> {
    let members = closed_object(payload, "invoke-action", &INVOKE_KEYS)?;
    let context = read_invoke_context(require(members, "invoke-action", "context")?)?;
    let continuation = match find(members, "continuation") {
        Some(value) => Some(read_continuation(value)?),
        None => None,
    };
    Ok(InvokeActionPayload {
        invocation_id: read_id(members, "invoke-action", "invocation_id")?,
        action_id: read_with(members, "invoke-action", "action_id", ActionId::parse)?,
        arguments: read_typed_map(
            require(members, "invoke-action", "arguments")?,
            "invoke-action.arguments",
        )?,
        context,
        continuation,
    })
}

fn read_invoke_context(value: &BoundedJson) -> Result<InvokeContext, ProviderError> {
    let members = closed_object(value, "invoke-action.context", &CONTEXT_KEYS)?;
    Ok(InvokeContext {
        screen_id: read_id(members, "invoke-action.context", "screen_id")?,
        screen_instance: read_id(members, "invoke-action.context", "screen_instance")?,
        resource_refs: read_typed_map(
            require(members, "invoke-action.context", "resource_refs")?,
            "invoke-action.context.resource_refs",
        )?,
    })
}

fn read_continuation(value: &BoundedJson) -> Result<Continuation, ProviderError> {
    let members = closed_object(value, "invoke-action.continuation", &CONTINUATION_KEYS)?;
    Ok(Continuation {
        confirmation_id: read_id(members, "invoke-action.continuation", "confirmation_id")?,
        approved: read_bool(members, "invoke-action.continuation", "approved")?,
        values: read_typed_map(
            require(members, "invoke-action.continuation", "values")?,
            "invoke-action.continuation.values",
        )?,
    })
}

fn read_cancel(payload: &BoundedJson) -> Result<CancelPayload, ProviderError> {
    let members = closed_object(payload, "cancel", &CANCEL_KEYS)?;
    let raw = read_string(members, "cancel", "target_request_id")?;
    let target_request_id = RequestId::parse(raw).map_err(|_| ProviderError::InvalidRequestId {
        raw: raw.to_owned(),
    })?;
    Ok(CancelPayload { target_request_id })
}

fn read_progress(payload: &BoundedJson) -> Result<ProgressPayload, ProviderError> {
    let members = closed_object(payload, "progress", &PROGRESS_KEYS)?;
    Ok(ProgressPayload {
        sequence: read_u16(members, "progress", "sequence")?,
        message: read_string(members, "progress", "message")?.to_owned(),
        completed: read_optional_u64(members, "progress", "completed")?,
        total: read_optional_u64(members, "progress", "total")?,
    })
}

fn read_error(payload: &BoundedJson) -> Result<ErrorPayload, ProviderError> {
    let members = closed_object(payload, "error", &ERROR_KEYS)?;
    let field_errors = array(
        require(members, "error", "field_errors")?,
        "error.field_errors",
        FIELD_ERROR_LIMIT,
    )?
    .iter()
    .map(|entry| read_field_error(entry, "error.field_errors"))
    .collect::<Result<Vec<_>, _>>()?;
    Ok(ErrorPayload {
        code: read_string(members, "error", "code")?.to_owned(),
        message: read_string(members, "error", "message")?.to_owned(),
        retryable: read_bool(members, "error", "retryable")?,
        field_errors,
    })
}

fn read_field_error(value: &BoundedJson, path: &str) -> Result<FieldError, ProviderError> {
    let members = closed_object(value, path, &FIELD_ERROR_KEYS)?;
    Ok(FieldError {
        path: read_string(members, path, "path")?.to_owned(),
        message: read_string(members, path, "message")?.to_owned(),
    })
}

fn read_shutdown(payload: &BoundedJson) -> Result<ShutdownPayload, ProviderError> {
    let members = closed_object(payload, "shutdown", &SHUTDOWN_KEYS)?;
    Ok(ShutdownPayload {
        reason: read_enum(members, "shutdown", "reason", ShutdownReason::from_wire)?,
    })
}

fn read_shutdown_ack(payload: &BoundedJson) -> Result<(), ProviderError> {
    closed_object(payload, "shutdown-ack", &[])?;
    Ok(())
}

fn read_outcome(payload: &BoundedJson) -> Result<Outcome, ProviderError> {
    let members = payload
        .as_object()
        .ok_or_else(|| type_mismatch("outcome", "object"))?;
    let kind = read_string(members, "outcome", "kind")?;
    match kind {
        "navigate" => read_outcome_navigate(payload),
        "refresh" => read_outcome_refresh(payload),
        "notice" => read_outcome_notice(payload),
        "request-host-confirmation" => read_outcome_request_host_confirmation(payload),
        other => Err(ProviderError::UnknownValue {
            path: "outcome.kind".to_owned(),
            value: other.to_owned(),
        }),
    }
}

fn read_outcome_navigate(payload: &BoundedJson) -> Result<Outcome, ProviderError> {
    let members = closed_object(payload, "outcome", &OUTCOME_NAVIGATE_KEYS)?;
    Ok(Outcome::Navigate {
        route_id: read_id(members, "outcome", "route_id")?,
        activation: read_typed_map(
            require(members, "outcome", "activation")?,
            "outcome.activation",
        )?,
    })
}

fn read_outcome_refresh(payload: &BoundedJson) -> Result<Outcome, ProviderError> {
    let members = closed_object(payload, "outcome", &OUTCOME_REFRESH_KEYS)?;
    Ok(Outcome::Refresh {
        resource_ref: read_typed_map(
            require(members, "outcome", "resource_ref")?,
            "outcome.resource_ref",
        )?,
    })
}

fn read_outcome_notice(payload: &BoundedJson) -> Result<Outcome, ProviderError> {
    let members = closed_object(payload, "outcome", &OUTCOME_NOTICE_KEYS)?;
    Ok(Outcome::Notice {
        severity: read_enum(members, "outcome", "severity", Severity::from_wire)?,
        message: read_string(members, "outcome", "message")?.to_owned(),
    })
}

fn read_outcome_request_host_confirmation(payload: &BoundedJson) -> Result<Outcome, ProviderError> {
    let members = closed_object(payload, "outcome", &OUTCOME_RHC_KEYS)?;
    let schema = array(
        require(members, "outcome", "continuation_schema")?,
        "outcome.continuation_schema",
        CONTINUATION_SCHEMA_LIMIT,
    )?
    .iter()
    .map(|entry| read_field_declaration(entry, "outcome.continuation_schema"))
    .collect::<Result<Vec<_>, _>>()?;
    Ok(Outcome::RequestHostConfirmation {
        confirmation_id: read_id(members, "outcome", "confirmation_id")?,
        title: read_string(members, "outcome", "title")?.to_owned(),
        body: read_string(members, "outcome", "body")?.to_owned(),
        confirm_label: read_string(members, "outcome", "confirm_label")?.to_owned(),
        destructive: read_bool(members, "outcome", "destructive")?,
        continuation_schema: schema,
    })
}

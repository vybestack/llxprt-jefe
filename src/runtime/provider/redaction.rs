//! Recursive secret redaction for every provider-owned observation surface
//! (issue #390 CW-10, CW10-14).
//!
//! A misbehaving provider may echo a resolved secret value into any string it
//! emits: a notice message, a typed activation/resource/panel/migration map, a
//! confirmation title/body/label/schema, an error field path or message, a
//! protocol diagnostic, or a supervisor failure string. This module is the
//! single recursive mapper that scrubs every resolved secret value out of those
//! surfaces before they leave the supervisor, so no secret reaches an operator
//! report, log, state, transcript, or [`Debug`](std::fmt::Debug) render.
//!
//! The mapper walks the closed recursive value types ([`TypedValue`] /
//! [`TypedMap`]) and every string-bearing outcome, error, and failure in place.
//! It never constructs a dynamic JSON value and carries no process handle.

use crate::domain::bounded_json::BoundedJsonError;
use crate::domain::plugin::field::{Field, FieldDraft, Scalar};
use crate::domain::{TypedMap, TypedValue};

use super::dto;
use super::environment::Redactor;
use super::error::ProviderError;
use super::panel_model::{
    Affordance, DetailBody, DetailMetadata, EmptyBody, ErrorBody, FormBody, FormFieldError,
    ListBody, ListItem, PanelBody, PanelSnapshot, ProgressBody, StatusBody, StatusRow,
};
use super::supervisor::{CleanupFailure, OneShotOutcome, SupervisorFailure};

/// Redact every resolved secret value out of a typed map (recursive).
pub(super) fn redact_typed_map(map: TypedMap, redactor: &Redactor) -> TypedMap {
    map.into_iter()
        .map(|(key, value)| (key, redact_typed_value(value, redactor)))
        .collect()
}

/// Redact every resolved secret value out of a typed value (recursive).
pub(super) fn redact_typed_value(value: TypedValue, redactor: &Redactor) -> TypedValue {
    match value {
        TypedValue::String(text) => TypedValue::String(redactor.redact(&text).into_owned()),
        TypedValue::List(items) => TypedValue::List(
            items
                .into_iter()
                .map(|item| redact_typed_value(item, redactor))
                .collect(),
        ),
        TypedValue::Map(inner) => TypedValue::Map(redact_typed_map(inner, redactor)),
        other => other,
    }
}

/// Redact every provider-authored observation surface in a panel snapshot.
///
/// Returns `None` when a field declaration cannot be rebuilt after redaction.
/// Callers must fail the provider closed rather than deliver the original
/// snapshot, because preserving a secret-bearing declaration is never safe.
pub(super) fn redact_panel_snapshot(
    snapshot: PanelSnapshot,
    redactor: &Redactor,
) -> Option<PanelSnapshot> {
    if redactor.is_empty() {
        return Some(snapshot);
    }
    let action_affordances = snapshot
        .action_affordances
        .into_iter()
        .map(|affordance| redact_affordance(affordance, redactor))
        .collect();
    Some(PanelSnapshot {
        model_schema: snapshot.model_schema,
        panel_instance_id: snapshot.panel_instance_id,
        generation: snapshot.generation,
        revision: snapshot.revision,
        kind: snapshot.kind,
        title: redact_text(snapshot.title, redactor),
        description: snapshot
            .description
            .map(|description| redact_text(description, redactor)),
        loading: snapshot.loading,
        action_affordances,
        body: redact_panel_body(snapshot.body, redactor)?,
    })
}

fn redact_affordance(affordance: Affordance, redactor: &Redactor) -> Affordance {
    Affordance {
        id: affordance.id,
        label: redact_text(affordance.label, redactor),
        action_id: affordance.action_id,
        arguments: affordance
            .arguments
            .map(|arguments| redact_typed_map(arguments, redactor)),
        enabled: affordance.enabled,
        unavailable_reason: affordance
            .unavailable_reason
            .map(|reason| redact_text(reason, redactor)),
    }
}

fn redact_panel_body(body: PanelBody, redactor: &Redactor) -> Option<PanelBody> {
    Some(match body {
        PanelBody::List(body) => PanelBody::List(ListBody {
            items: body
                .items
                .into_iter()
                .map(|item| ListItem {
                    id: item.id,
                    label: redact_text(item.label, redactor),
                    description: item.description.map(|text| redact_text(text, redactor)),
                    status: item.status.map(|text| redact_text(text, redactor)),
                    actions: item.actions,
                })
                .collect(),
            selected_id: body.selected_id,
            next_page_token: body
                .next_page_token
                .map(|token| redact_text(token, redactor)),
        }),
        PanelBody::Detail(body) => PanelBody::Detail(DetailBody {
            document: redact_text(body.document, redactor),
            metadata: body
                .metadata
                .into_iter()
                .map(|metadata| DetailMetadata {
                    label: redact_text(metadata.label, redactor),
                    value: redact_text(metadata.value, redactor),
                })
                .collect(),
            actions: body.actions,
        }),
        PanelBody::Form(body) => PanelBody::Form(redact_form_body(body, redactor)?),
        PanelBody::Status(body) => PanelBody::Status(StatusBody {
            rows: body
                .rows
                .into_iter()
                .map(|row| StatusRow {
                    label: redact_text(row.label, redactor),
                    value: redact_text(row.value, redactor),
                    state: row.state,
                })
                .collect(),
        }),
        PanelBody::Progress(body) => PanelBody::Progress(ProgressBody {
            message: redact_text(body.message, redactor),
            completed: body.completed,
            total: body.total,
            cancellable: body.cancellable,
        }),
        PanelBody::Empty(body) => PanelBody::Empty(EmptyBody {
            message: redact_text(body.message, redactor),
            action: body.action,
        }),
        PanelBody::Error(body) => PanelBody::Error(ErrorBody {
            code: redact_text(body.code, redactor),
            message: redact_text(body.message, redactor),
            retryable: body.retryable,
            retry_action: body.retry_action,
        }),
    })
}

fn redact_form_body(body: FormBody, redactor: &Redactor) -> Option<FormBody> {
    let fields = body
        .fields
        .into_iter()
        .map(|field| redact_field(field, redactor))
        .collect::<Option<Vec<_>>>()?;
    Some(FormBody {
        fields,
        values: redact_typed_map(body.values, redactor),
        field_errors: body
            .field_errors
            .into_iter()
            .map(|error| FormFieldError {
                field_id: error.field_id,
                message: redact_text(error.message, redactor),
            })
            .collect(),
        submit_action: body.submit_action,
    })
}

fn redact_text(text: String, redactor: &Redactor) -> String {
    redactor.redact(&text).into_owned()
}

/// Redact every resolved secret value out of a one-shot terminal outcome.
pub(super) fn redact_one_shot_outcome(
    outcome: OneShotOutcome,
    redactor: &Redactor,
) -> OneShotOutcome {
    if redactor.is_empty() {
        return outcome;
    }
    match outcome {
        OneShotOutcome::Completed(payload) => {
            OneShotOutcome::Completed(redact_outcome(payload, redactor))
        }
        OneShotOutcome::ProviderError(payload) => {
            OneShotOutcome::ProviderError(redact_error_payload(payload, redactor))
        }
        OneShotOutcome::Cancelled => OneShotOutcome::Cancelled,
        OneShotOutcome::Failed(failure) => {
            OneShotOutcome::Failed(redact_supervisor_failure(failure, redactor))
        }
    }
}

/// Redact every resolved secret value out of a provider outcome.
pub(super) fn redact_outcome(outcome: dto::Outcome, redactor: &Redactor) -> dto::Outcome {
    if redactor.is_empty() {
        return outcome;
    }
    match outcome {
        dto::Outcome::Navigate {
            route_id,
            activation,
        } => dto::Outcome::Navigate {
            route_id,
            activation: redact_typed_map(activation, redactor),
        },
        dto::Outcome::Refresh { resource_ref } => dto::Outcome::Refresh {
            resource_ref: redact_typed_map(resource_ref, redactor),
        },
        dto::Outcome::Notice { severity, message } => dto::Outcome::Notice {
            severity,
            message: redactor.redact(&message).into_owned(),
        },
        dto::Outcome::RequestHostConfirmation {
            confirmation_id,
            title,
            body,
            confirm_label,
            destructive,
            continuation_schema,
        } => dto::Outcome::RequestHostConfirmation {
            confirmation_id,
            title: redactor.redact(&title).into_owned(),
            body: redactor.redact(&body).into_owned(),
            confirm_label: redactor.redact(&confirm_label).into_owned(),
            destructive,
            continuation_schema: continuation_schema
                .into_iter()
                .filter_map(|field| redact_field(field, redactor))
                .collect(),
        },
    }
}

/// Redact every resolved secret value out of a provider error payload. The
/// stable code is structural and never redacted; the operator message and every
/// field-error path/message may echo a secret and are scrubbed.
pub(super) fn redact_error_payload(
    payload: dto::ErrorPayload,
    redactor: &Redactor,
) -> dto::ErrorPayload {
    if redactor.is_empty() {
        return payload;
    }
    dto::ErrorPayload {
        code: payload.code,
        message: redactor.redact(&payload.message).into_owned(),
        retryable: payload.retryable,
        field_errors: payload
            .field_errors
            .into_iter()
            .map(|field| dto::FieldError {
                path: redactor.redact(&field.path).into_owned(),
                message: redactor.redact(&field.message).into_owned(),
            })
            .collect(),
    }
}

/// Redact every resolved secret value out of a supervisor failure. Spawn/Io
/// diagnostic strings and every protocol diagnostic field are scrubbed; the
/// environment error carries only declared names, never a value.
pub(super) fn redact_supervisor_failure(
    failure: SupervisorFailure,
    redactor: &Redactor,
) -> SupervisorFailure {
    if redactor.is_empty() {
        return failure;
    }
    match failure {
        SupervisorFailure::Spawn(message) => {
            SupervisorFailure::Spawn(redactor.redact(&message).into_owned())
        }
        SupervisorFailure::Io(message) => {
            SupervisorFailure::Io(redactor.redact(&message).into_owned())
        }
        SupervisorFailure::Protocol(error) => {
            SupervisorFailure::Protocol(redact_provider_error(error, redactor))
        }
        other => other,
    }
}

/// Redact every resolved secret value out of a cleanup failure. A shutdown-ack
/// protocol fault may carry provider-supplied text; an `Io` evidence string may
/// carry an OS error; drain/reap failures carry no secret strings.
pub(super) fn redact_cleanup_failure(
    failure: CleanupFailure,
    redactor: &Redactor,
) -> CleanupFailure {
    if redactor.is_empty() {
        return failure;
    }
    match failure {
        CleanupFailure::ShutdownAck(error) => {
            CleanupFailure::ShutdownAck(redact_provider_error(error, redactor))
        }
        CleanupFailure::PostTerminal(error) => {
            CleanupFailure::PostTerminal(redact_provider_error(error, redactor))
        }
        CleanupFailure::Io(message) => CleanupFailure::Io(redactor.redact(&message).into_owned()),
        other => other,
    }
}

/// Redact every provider-supplied string field of a protocol error. Wire-name
/// fields (`expected`, phase/kind/stream) and numeric fields are structural and
/// left untouched; every path, value, reason, raw id, and JSON diagnostic text
/// is scrubbed.
fn redact_provider_error(error: ProviderError, redactor: &Redactor) -> ProviderError {
    if redactor.is_empty() {
        return error;
    }
    match error {
        ProviderError::Json(json_error) => {
            ProviderError::Json(redact_bounded_json_error(json_error, redactor))
        }
        ProviderError::UnknownField { path, field } => ProviderError::UnknownField {
            path: redactor.redact(&path).into_owned(),
            field: redactor.redact(&field).into_owned(),
        },
        ProviderError::MissingField { path, field } => ProviderError::MissingField {
            path: redactor.redact(&path).into_owned(),
            field: redactor.redact(&field).into_owned(),
        },
        ProviderError::TypeMismatch { path, expected } => ProviderError::TypeMismatch {
            path: redactor.redact(&path).into_owned(),
            expected,
        },
        ProviderError::UnknownValue { path, value } => ProviderError::UnknownValue {
            path: redactor.redact(&path).into_owned(),
            value: redactor.redact(&value).into_owned(),
        },
        ProviderError::InvalidValue { path, reason } => ProviderError::InvalidValue {
            path: redactor.redact(&path).into_owned(),
            reason: redactor.redact(&reason).into_owned(),
        },
        ProviderError::InvalidRequestId { raw } => ProviderError::InvalidRequestId {
            raw: redactor.redact(&raw).into_owned(),
        },
        ProviderError::InvalidRequestOrigin { raw, stream } => {
            ProviderError::InvalidRequestOrigin {
                raw: redactor.redact(&raw).into_owned(),
                stream,
            }
        }
        other => other,
    }
}

/// Redact the string-bearing variants of a bounded-reader diagnostic.
fn redact_bounded_json_error(error: BoundedJsonError, redactor: &Redactor) -> BoundedJsonError {
    match error {
        BoundedJsonError::DuplicateKey { key } => BoundedJsonError::DuplicateKey {
            key: redactor.redact(&key).into_owned(),
        },
        BoundedJsonError::NumberNotAdmitted { text } => BoundedJsonError::NumberNotAdmitted {
            text: redactor.redact(&text).into_owned(),
        },
        BoundedJsonError::Syntax { message, offset } => BoundedJsonError::Syntax {
            message: redactor.redact(&message).into_owned(),
            offset,
        },
        other => other,
    }
}

/// Redact the textual scalars of a declared confirmation-schema field.
///
/// Returns the rebuilt field only when it re-validates after redaction.
/// Only [`Scalar::Text`] values can echo a secret; rebuilding the declaration
/// preserves kind-matching. Redacting two enum choices to the same placeholder
/// would create a duplicate choice, which cannot re-validate: in that case the
/// field is **omitted** (`None`) rather than rebuilt, so the secret-bearing
/// declaration is never preserved verbatim.
pub(super) fn redact_field(field: Field, redactor: &Redactor) -> Option<Field> {
    let draft = FieldDraft {
        id: field.id().clone(),
        label: field.label().to_owned(),
        description: field.description().map(str::to_owned),
        kind: field.kind(),
        required: field.required(),
        default: field
            .default()
            .map(|value| redact_typed_value(value.clone(), redactor)),
        min: field
            .min()
            .map(|scalar| redact_scalar(scalar.clone(), redactor)),
        max: field
            .max()
            .map(|scalar| redact_scalar(scalar.clone(), redactor)),
        choices: field
            .choices()
            .iter()
            .map(|scalar| redact_scalar(scalar.clone(), redactor))
            .collect(),
        unique: field.unique(),
        visible_when: field.visible_when().cloned(),
        restart: field.restart(),
    };
    // Leak-proof: a field whose redacted scalars cannot re-validate (for
    // example two enum choices that both redact to the same placeholder) is
    // omitted entirely. The original secret-bearing declaration is never
    // preserved, so no secret can survive a redaction failure.
    Field::parse(draft).ok()
}

/// Redact a textual scalar in place; non-text scalars carry no secret.
fn redact_scalar(scalar: Scalar, redactor: &Redactor) -> Scalar {
    match scalar {
        Scalar::Text(text) => Scalar::Text(redactor.redact(&text).into_owned()),
        other => other,
    }
}

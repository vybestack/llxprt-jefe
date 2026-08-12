//! Unit tests for the pure supervisor surface (issue #390 CW-10, Slice C1).
//!
//! Lifecycle, queue, shutdown, and reap behavior is proven by the integration
//! tests in `tests/issue390_provider_supervisor.rs` against the real fixture
//! binary; these tests cover the pure, process-free pieces: bound defaults,
//! the diagnostic-code split, and outcome redaction.

use std::sync::mpsc;
use std::time::Duration;

use super::drains::{FinalStdoutOutcome, StdoutEvent, final_stdout_drain};
use super::dto;
use super::environment::{REDACTION_PLACEHOLDER, Redactor};
use super::error::{self, ProviderError};
use super::redaction::{redact_error_payload, redact_field, redact_outcome, redact_panel_snapshot};
#[cfg(unix)]
use super::supervisor::signal_cleanup_evidence;
use super::supervisor::{
    CleanupFailure, OneShotOutcome, SupervisorBounds, SupervisorFailure, compose_cleanup_failure,
};
use crate::domain::action_registry::ActionId;
use crate::domain::plugin::field::{Field, FieldDraft, FieldKind, RestartScope, Scalar};
use crate::domain::{Id, TypedMap, TypedValue};

use super::panel_model::{
    Affordance, DetailBody, DetailMetadata, EmptyBody, ErrorBody, FormBody, FormFieldError,
    ListBody, ListItem, PanelBody, PanelSnapshot, ProgressBody, StatusBody, StatusRow,
    StatusRowState,
};

#[test]
fn production_bounds_are_exact() {
    let bounds = SupervisorBounds::PRODUCTION;
    assert_eq!(bounds.handshake, Duration::from_secs(5));
    assert_eq!(bounds.invocation, Duration::from_secs(60));
    assert_eq!(bounds.shutdown_ack, Duration::from_secs(2));
    assert_eq!(bounds.stdin_close, Duration::from_secs(2));
    assert_eq!(bounds.final_drain, Duration::from_secs(2));
}

#[test]
fn an_empty_redactor_leaves_an_outcome_unchanged() {
    let redactor = Redactor::new(Vec::new());
    let outcome = dto::Outcome::Notice {
        severity: dto::Severity::Info,
        message: "all good".to_owned(),
    };
    let redacted = redact_outcome(outcome.clone(), &redactor);
    assert_eq!(redacted, outcome);
}

#[test]
fn a_redactor_scrubs_a_secret_in_a_notice_outcome() {
    let redactor = Redactor::new(vec!["hunter2".to_owned()]);
    let outcome = dto::Outcome::Notice {
        severity: dto::Severity::Warning,
        message: "token was hunter2 leaked".to_owned(),
    };
    let redacted = redact_outcome(outcome, &redactor);
    match redacted {
        dto::Outcome::Notice { message, .. } => {
            assert!(!message.contains("hunter2"), "secret leaked: {message}");
        }
        _ => panic!("expected notice"),
    }
}

#[test]
fn a_redactor_scrubs_secret_strings_in_an_error_payload() {
    let secret = "SUPER_SECRET_VALUE";
    let redactor = Redactor::new(vec![secret.to_owned()]);
    let payload = dto::ErrorPayload {
        code: "E1".to_owned(),
        message: format!("failed with {secret}"),
        retryable: false,
        field_errors: vec![dto::FieldError {
            path: "args.token".to_owned(),
            message: format!("bad {secret}"),
        }],
    };
    let redacted = redact_error_payload(payload, &redactor);
    assert!(!redacted.message.contains(secret));
    assert!(!redacted.field_errors[0].message.contains(secret));
}

fn panel_fixture_id(text: &str) -> Id {
    Id::parse(text).unwrap_or_else(|error| panic!("id fixture {text:?}: {error}"))
}

fn secret_panel_bodies(secret: &str, action: &ActionId) -> Vec<PanelBody> {
    vec![
        PanelBody::List(ListBody {
            items: vec![ListItem {
                id: panel_fixture_id("item"),
                label: format!("label {secret}"),
                description: Some(format!("description {secret}")),
                status: Some(format!("status {secret}")),
                actions: Vec::new(),
            }],
            selected_id: Some(panel_fixture_id("item")),
            next_page_token: Some(format!("page {secret}")),
        }),
        PanelBody::Detail(DetailBody {
            document: format!("document {secret}"),
            metadata: vec![DetailMetadata {
                label: format!("metadata label {secret}"),
                value: format!("metadata value {secret}"),
            }],
            actions: Vec::new(),
        }),
        PanelBody::Form(FormBody {
            fields: Vec::new(),
            values: TypedMap::from([(
                panel_fixture_id("value"),
                TypedValue::String(format!("form value {secret}")),
            )]),
            field_errors: vec![FormFieldError {
                field_id: panel_fixture_id("value"),
                message: format!("form error {secret}"),
            }],
            submit_action: action.clone(),
        }),
        PanelBody::Status(StatusBody {
            rows: vec![StatusRow {
                label: format!("status label {secret}"),
                value: format!("status value {secret}"),
                state: StatusRowState::Warning,
            }],
        }),
        PanelBody::Progress(ProgressBody {
            message: format!("progress {secret}"),
            completed: Some(1),
            total: Some(2),
            cancellable: true,
        }),
        PanelBody::Empty(EmptyBody {
            message: format!("empty {secret}"),
            action: None,
        }),
        PanelBody::Error(ErrorBody {
            code: format!("code {secret}"),
            message: format!("error {secret}"),
            retryable: true,
            retry_action: None,
        }),
    ]
}

#[test]
fn panel_snapshot_redaction_covers_every_provider_authored_body_surface() {
    let secret = "SUPER_SECRET_VALUE";
    let redactor = Redactor::new(vec![secret.to_owned()]);
    let action =
        ActionId::parse("vendor.action").unwrap_or_else(|error| panic!("action fixture: {error}"));
    let bodies = secret_panel_bodies(secret, &action);

    for body in bodies {
        let snapshot = PanelSnapshot {
            model_schema: 1,
            panel_instance_id: 1,
            generation: 1,
            revision: 1,
            kind: body.kind(),
            title: format!("title {secret}"),
            description: Some(format!("snapshot description {secret}")),
            loading: false,
            action_affordances: vec![Affordance {
                id: panel_fixture_id("affordance"),
                label: format!("affordance label {secret}"),
                action_id: action.clone(),
                arguments: Some(TypedMap::from([(
                    panel_fixture_id("argument"),
                    TypedValue::String(format!("argument {secret}")),
                )])),
                enabled: false,
                unavailable_reason: Some(format!("reason {secret}")),
            }],
            body,
        };
        let Some(redacted) = redact_panel_snapshot(snapshot, &redactor) else {
            panic!("valid snapshot remains representable after redaction");
        };
        let rendered = format!("{redacted:?}");
        assert!(
            !rendered.contains(secret),
            "panel secret leaked: {rendered}"
        );
        assert!(rendered.contains(REDACTION_PLACEHOLDER), "{rendered}");
    }
}

#[test]
fn protocol_failures_carry_plg_e502() {
    let failure = SupervisorFailure::Protocol(ProviderError::MissingField {
        path: "payload".to_owned(),
        field: "type".to_owned(),
    });
    assert_eq!(failure.code(), error::PROTOCOL_FAILURE_CODE);
}

#[test]
fn runtime_failures_carry_plg_e503_not_e502() {
    let cases = [
        SupervisorFailure::Spawn("no such binary".to_owned()),
        SupervisorFailure::Io("broken pipe".to_owned()),
        SupervisorFailure::HandshakeTimeout,
        SupervisorFailure::InvocationTimeout,
        SupervisorFailure::ShutdownTimeout,
        SupervisorFailure::Crashed { exit: Some(1) },
    ];
    for failure in cases {
        assert_eq!(
            failure.code(),
            error::RUNTIME_UNAVAILABLE_CODE,
            "{failure:?} must be PLG-E503, not PLG-E502"
        );
    }
}

#[test]
fn cleanup_shutdown_ack_carries_e502_and_others_carry_e503() {
    let ack = CleanupFailure::ShutdownAck(ProviderError::OutOfOrder {
        phase: "await-shutdown-ack".to_owned(),
        kind: "timeout".to_owned(),
    });
    assert_eq!(ack.code(), error::PROTOCOL_FAILURE_CODE);
    assert_eq!(
        CleanupFailure::DrainTimeout.code(),
        error::RUNTIME_UNAVAILABLE_CODE
    );
    assert_eq!(
        CleanupFailure::NotReaped.code(),
        error::RUNTIME_UNAVAILABLE_CODE
    );
    assert_eq!(
        CleanupFailure::Io("broken pipe".to_owned()).code(),
        error::RUNTIME_UNAVAILABLE_CODE,
        "a cleanup I/O failure is runtime-unavailable (PLG-E503)"
    );
}

#[test]
#[cfg(unix)]
fn signal_cleanup_evidence_ignores_a_benign_already_reaped_result() {
    // ESRCH (no such process): the signal target was already reaped, so the
    // result is benign and must not dirty an otherwise-clean cleanup.
    let already_reaped = std::io::Error::from_raw_os_error(3);
    assert_eq!(
        signal_cleanup_evidence(&[already_reaped]),
        None,
        "an already-reaped (ESRCH) signal result is benign"
    );
}

#[test]
#[cfg(unix)]
fn signal_cleanup_evidence_preserves_a_real_signal_error() {
    // A non-ESRCH error (EACCES, errno 13) is a real signal-delivery failure
    // that must be preserved rather than silently discarded when cleanup is clean.
    let real = std::io::Error::from_raw_os_error(13);
    let evidence = signal_cleanup_evidence(&[real]);
    match evidence {
        Some(message) => assert!(
            message.contains("denied"),
            "the real signal error is described: {message}"
        ),
        None => panic!("a non-ESRCH signal error must be preserved"),
    }
}

#[test]
fn one_shot_outcome_completed_debug_is_safe() {
    let outcome = OneShotOutcome::Completed(dto::Outcome::Notice {
        severity: dto::Severity::Info,
        message: "done".to_owned(),
    });
    let rendered = format!("{outcome:?}");
    assert!(rendered.contains("Completed"));
}

// ---- CW10-14: leak-proof schema redaction (C1 final correctness)

/// Two distinct enum choices that each carry a different resolved secret both
/// redact to the same `[REDACTED]` placeholder, so rebuilding the field would
/// produce a duplicate choice. `redact_field` must never fall back to the
/// original secret-bearing field: it omits the field instead. The original
/// secrets must not survive anywhere.
#[test]
fn redact_field_with_duplicate_secret_choices_omits_the_field_and_leaks_nothing() {
    let secret_a = "alpha-secret-canary";
    let secret_b = "beta-secret-canary";
    let redactor = Redactor::new(vec![secret_a.to_owned(), secret_b.to_owned()]);
    // Two distinct choices (the field is valid as declared) that both redact
    // to the single placeholder, producing a duplicate-choice revalidation
    // conflict.
    let field = Field::parse(FieldDraft {
        id: Id::parse("vendor.pkg.choice").unwrap_or_else(|error| panic!("field id: {error:?}")),
        label: "Choice".to_owned(),
        description: None,
        kind: FieldKind::Enum,
        required: false,
        default: None,
        min: None,
        max: None,
        choices: vec![
            Scalar::Text(secret_a.to_owned()),
            Scalar::Text(secret_b.to_owned()),
        ],
        unique: false,
        visible_when: None,
        restart: RestartScope::None,
    })
    .unwrap_or_else(|error| panic!("valid enum field: {error:?}"));

    let redacted = redact_field(field, &redactor);

    // The redacted field must be omitted: never the original secret-bearing
    // field.
    assert!(
        redacted.is_none(),
        "redact_field must omit a field whose redacted choices collide, not preserve the secret-bearing original"
    );
    // No original secret survives anywhere in the redacted surface.
    let rendered = format!("{redacted:?}");
    assert!(
        !rendered.contains(secret_a) && !rendered.contains(secret_b),
        "an original secret leaked through redact_field: {rendered}"
    );
}

/// A single-choice enum whose choice carries the secret redacts cleanly to the
/// placeholder and remains a valid (present) field.
#[test]
fn redact_field_with_one_secret_choice_rebuilds_cleanly() {
    let secret = "hunter2-canary";
    let redactor = Redactor::new(vec![secret.to_owned()]);
    let field = Field::parse(FieldDraft {
        id: Id::parse("vendor.pkg.solo").unwrap_or_else(|error| panic!("field id: {error:?}")),
        label: "Solo".to_owned(),
        description: None,
        kind: FieldKind::Enum,
        required: false,
        default: None,
        min: None,
        max: None,
        choices: vec![Scalar::Text(format!("only-{secret}"))],
        unique: false,
        visible_when: None,
        restart: RestartScope::None,
    })
    .unwrap_or_else(|error| panic!("valid enum field: {error:?}"));

    let redacted =
        redact_field(field, &redactor).unwrap_or_else(|| panic!("single-choice field rebuilds"));
    let rendered = format!("{redacted:?}");
    assert!(
        !rendered.contains(secret),
        "the original secret survived the rebuild: {rendered}"
    );
    assert!(
        rendered.contains(REDACTION_PLACEHOLDER),
        "the choice was redacted to the placeholder"
    );
}

// ---- CW10-11: bounded final stdout drain records EOF only when observed (C1 final correctness)

/// When the stdout drain thread exits and drops its sender, the receiver
/// observes a channel disconnection: that is the only condition the full
/// lifecycle accepts as a real stdout EOF. The final drain must report
/// [`FinalStdoutOutcome::Eof`].
#[test]
fn final_stdout_drain_observes_eof_when_the_channel_disconnects() {
    let (sender, receiver) = mpsc::channel::<StdoutEvent>();
    // The drain thread reached a clean read-of-zero and dropped the sender.
    drop(sender);
    let outcome = final_stdout_drain(&receiver, Duration::from_millis(100));
    assert_eq!(
        outcome,
        FinalStdoutOutcome::Eof,
        "a channel disconnection is the only real EOF signal"
    );
}

/// A descendant that inherits the provider's stdout keeps the pipe open even
/// after the leader is reaped, so the channel never disconnects within the
/// bound. The final drain must report [`FinalStdoutOutcome::Timeout`], never
/// a synthetic EOF: lingering inherited stdout is a cleanup failure, not a
/// clean close.
#[test]
fn final_stdout_drain_reports_timeout_when_stdout_never_closes() {
    let (_sender, receiver) = mpsc::channel::<StdoutEvent>();
    // The sender is kept alive: a lingering descendant still holds the pipe.
    let outcome = final_stdout_drain(&receiver, Duration::from_millis(100));
    assert_eq!(
        outcome,
        FinalStdoutOutcome::Timeout,
        "a pipe that never closes is a timeout, never a clean EOF"
    );
}

/// Data a misbehaving provider buffered after a valid shutdown-ack must be
/// rejected by the final drain as data-after-ack, never silently swallowed.
#[test]
fn final_stdout_drain_rejects_data_buffered_after_the_ack() {
    let (sender, receiver) = mpsc::channel::<StdoutEvent>();
    sender
        .send(StdoutEvent::Frame(
            b"stray-after-ack
"
            .to_vec(),
        ))
        .unwrap_or_else(|error| panic!("frame enqueued: {error:?}"));
    let outcome = final_stdout_drain(&receiver, Duration::from_millis(100));
    assert_eq!(
        outcome,
        FinalStdoutOutcome::DataAfterAck,
        "a frame remaining after the ack is data-after-ack"
    );
}

/// A non-frame fault (oversize/read error) remaining in the channel is a pipe
/// fault, not a clean EOF.
#[test]
fn final_stdout_drain_reports_a_fault_for_a_non_frame_event() {
    let (sender, receiver) = mpsc::channel::<StdoutEvent>();
    sender
        .send(StdoutEvent::ReadError)
        .unwrap_or_else(|error| panic!("fault enqueued: {error:?}"));
    let outcome = final_stdout_drain(&receiver, Duration::from_millis(100));
    assert_eq!(
        outcome,
        FinalStdoutOutcome::Fault,
        "a read error remaining in the channel is a pipe fault"
    );
}

// ---- CW10-11: clean cleanup requires stdout/stderr closure evidence (C1 final correctness)

/// A clean cleanup requires all four signals: leader reaped, valid ack, observed
/// stdout EOF, and a closed stderr drain. None of these alone is sufficient.
#[test]
fn compose_cleanup_failure_is_clean_only_when_reaped_ack_eof_and_stderr_all_hold() {
    let none = compose_cleanup_failure(true, None, FinalStdoutOutcome::Eof, false);
    assert!(none.is_none(), "all signals clean => no cleanup failure");
}

/// The leader reaping is not enough: if stdout did not close (a descendant
/// holds it), the cleanup fails even though the leader was reaped.
#[test]
fn compose_cleanup_failure_requires_stdout_eof_even_when_the_leader_reaped() {
    let failure = compose_cleanup_failure(true, None, FinalStdoutOutcome::Timeout, false);
    assert!(
        matches!(failure, Some(CleanupFailure::DrainTimeout)),
        "a leader reap without stdout closure is a drain timeout, got {failure:?}"
    );
}

/// The leader reaping is not enough: if stderr did not close, the cleanup fails.
#[test]
fn compose_cleanup_failure_requires_stderr_closure_even_when_stdout_eofd() {
    let failure = compose_cleanup_failure(true, None, FinalStdoutOutcome::Eof, true);
    assert!(
        matches!(failure, Some(CleanupFailure::DrainTimeout)),
        "stdout EOF without stderr closure is a drain timeout, got {failure:?}"
    );
}

/// Data buffered after a valid ack is a shutdown-ack protocol fault, ranked
/// above a drain timeout.
#[test]
fn compose_cleanup_failure_surfaces_data_after_ack_as_a_shutdown_ack_fault() {
    let failure = compose_cleanup_failure(true, None, FinalStdoutOutcome::DataAfterAck, false);
    assert!(
        matches!(failure, Some(CleanupFailure::ShutdownAck(_))),
        "data-after-ack is a shutdown-ack protocol fault, got {failure:?}"
    );
}

/// A missing/wrong ack is reported even when the leader reaped and stdout EOFd.
#[test]
fn compose_cleanup_failure_reports_a_missing_ack_above_drain_signals() {
    let failure = compose_cleanup_failure(
        true,
        Some(CleanupFailure::ShutdownAck(ProviderError::InvalidValue {
            path: "shutdown-ack".to_owned(),
            reason: "no shutdown-ack within the bound".to_owned(),
        })),
        FinalStdoutOutcome::Eof,
        false,
    );
    assert!(
        matches!(failure, Some(CleanupFailure::ShutdownAck(_))),
        "a missing ack outranks clean drain signals, got {failure:?}"
    );
}

/// Stderr exactly at the retention cap is fully retained, so reporting it as
/// truncated would be a lie the operator acts on (CW10-14).
///
/// The flag exists to say "bytes were dropped". Setting it when the buffer
/// merely reached its limit makes a complete capture look incomplete.
#[test]
fn stderr_is_only_reported_truncated_when_bytes_were_actually_dropped() {
    use super::drains::{STDERR_RETENTION_MAX, StderrRetention};

    let mut exactly_full = StderrRetention::new();
    exactly_full.push(&vec![b'x'; STDERR_RETENTION_MAX]);
    let (bytes, truncated) = exactly_full.finish();
    assert_eq!(bytes.len(), STDERR_RETENTION_MAX);
    assert!(
        !truncated,
        "a capture that dropped nothing must not report truncation"
    );

    let mut one_over = StderrRetention::new();
    one_over.push(&vec![b'x'; STDERR_RETENTION_MAX + 1]);
    let (bytes, truncated) = one_over.finish();
    assert_eq!(bytes.len(), STDERR_RETENTION_MAX);
    assert!(truncated, "one dropped byte must report truncation");

    // Chunked arrival must behave the same as one large read.
    let mut chunked = StderrRetention::new();
    for _ in 0..=STDERR_RETENTION_MAX {
        chunked.push(b"x");
    }
    let (bytes, truncated) = chunked.finish();
    assert_eq!(bytes.len(), STDERR_RETENTION_MAX);
    assert!(truncated);
}

// ---------------------------------------------------------------------------
// S15: descriptor-selected timeout_seconds carried exactly (1..=600)
// ---------------------------------------------------------------------------

#[test]
fn for_invocation_carries_exact_timeout_seconds() {
    for timeout in [1_u32, 60, 600] {
        let bounds = SupervisorBounds::for_invocation(timeout);
        assert_eq!(
            bounds.invocation,
            Duration::from_secs(u64::from(timeout)),
            "timeout {timeout} not carried exactly"
        );
        // All other bounds remain at production defaults.
        assert_eq!(bounds.handshake, Duration::from_secs(5));
        assert_eq!(bounds.shutdown_ack, Duration::from_secs(2));
        assert_eq!(bounds.stdin_close, Duration::from_secs(2));
        assert_eq!(bounds.final_drain, Duration::from_secs(2));
    }
}

// ---------------------------------------------------------------------------
// S16/S17: streaming progress and cancel hooks are exercised by the streaming
// session tests below (live delivery and cancel surface). The pure
// `OneShotOutcome::Cancelled` mapping and redaction are covered there too.
// ---------------------------------------------------------------------------

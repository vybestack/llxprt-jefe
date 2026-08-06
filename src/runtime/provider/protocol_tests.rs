//! Closed envelope/payload canonical tables, negative field tables, the
//! request-id boundary table, the lifecycle order table, and the progress
//! monotonicity table (issue #390 CW-10, rows CW10-05, CW10-06, CW10-07).

use super::error::{PROGRESS_SEQUENCE_MAX, ProgressFault, ProviderError};
use super::protocol::{
    Capability, Direction, LifecycleOrder, LifecyclePhase, MessageKind, Outcome, ProgressTracker,
    ProviderMessage, RequestOrigin, Severity, ShutdownReason, parse_message,
};

/// Build one LF-terminated envelope line from its scalar parts.
fn envelope(ty: &str, request_id: &str, generation: u64, payload: &str) -> Vec<u8> {
    format!(
        "{{\"protocol\":1,\"type\":\"{ty}\",\"request_id\":\"{request_id}\",\"generation\":{generation},\"payload\":{payload}}}\n"
    )
    .into_bytes()
}

/// Parse an envelope on the given stream, panicking with the failure if it does
/// not parse.
fn parsed(bytes: &[u8], stream: Direction) -> super::protocol::ParsedMessage {
    parse_message(bytes, stream).unwrap_or_else(|error| panic!("message must parse: {error}"))
}

/// Parse an envelope on the given stream, panicking if it parses when it must
/// be rejected.
fn rejected(bytes: &[u8], stream: Direction) -> ProviderError {
    parse_message(bytes, stream)
        .err()
        .unwrap_or_else(|| panic!("message must be rejected"))
}

// ---------------------------------------------------------------------------
// CW10-05: every closed payload parses with its exact fields.
// ---------------------------------------------------------------------------

#[test]
fn hello_parses_with_exact_fields() {
    let bytes = envelope(
        "hello",
        "h-000001",
        1,
        r#"{"host_api":"jefe","plugin_id":"vybestack.git-merger","plugin_version":"1.0.0"}"#,
    );
    let parsed = parsed(&bytes, Direction::HostToProvider);
    assert_eq!(parsed.generation, 1);
    assert_eq!(parsed.request_id.origin(), RequestOrigin::Host);
    let ProviderMessage::Hello(hello) = parsed.message else {
        panic!("expected Hello, got {:?}", parsed.message);
    };
    assert_eq!(hello.host_api, "jefe");
    assert_eq!(hello.plugin_id.as_str(), "vybestack.git-merger");
    assert_eq!(hello.plugin_version.as_str(), "1.0.0");
}

#[test]
fn hello_ack_parses_with_exact_fields_and_reaffirms_protocol() {
    let bytes = envelope(
        "hello-ack",
        "p-000001",
        1,
        r#"{"provider_name":"git-merger","protocol":1}"#,
    );
    let parsed = parsed(&bytes, Direction::ProviderToHost);
    assert_eq!(parsed.request_id.origin(), RequestOrigin::Provider);
    let ProviderMessage::HelloAck(ack) = parsed.message else {
        panic!("expected HelloAck");
    };
    assert_eq!(ack.provider_name, "git-merger");
}

#[test]
fn configure_parses_with_exact_fields() {
    let bytes = envelope(
        "configure",
        "h-000002",
        7,
        r#"{"config_version":3,"config":{"enabled":{"type":"bool","value":true}},"secrets":{"API_TOKEN":"resolved"},"environment":{"LANG":"en_US.UTF-8"}}"#,
    );
    let parsed = parsed(&bytes, Direction::HostToProvider);
    let ProviderMessage::Configure(cfg) = parsed.message else {
        panic!("expected Configure");
    };
    assert_eq!(cfg.config_version, 3);
    assert_eq!(cfg.config.len(), 1);
    assert_eq!(cfg.secrets.len(), 1);
    assert_eq!(
        cfg.environment.get("LANG").map(String::as_str),
        Some("en_US.UTF-8")
    );
}

#[test]
fn ready_parses_with_exact_fields() {
    let bytes = envelope(
        "ready",
        "p-000002",
        1,
        r#"{"capabilities":["actions","panels"]}"#,
    );
    let parsed = parsed(&bytes, Direction::ProviderToHost);
    let ProviderMessage::Ready(ready) = parsed.message else {
        panic!("expected Ready");
    };
    assert_eq!(
        ready.capabilities,
        [Capability::Actions, Capability::Panels]
    );
}

#[test]
fn invoke_action_parses_with_exact_fields() {
    let bytes = envelope(
        "invoke-action",
        "h-000003",
        1,
        r#"{"invocation_id":"merge.head","action_id":"git-merger.merge","arguments":{"branch":{"type":"string","value":"feature"}},"context":{"screen_id":"workbench","screen_instance":"default","resource_refs":{"pr":{"type":"string","value":"123"}}}}"#,
    );
    let parsed = parsed(&bytes, Direction::HostToProvider);
    let ProviderMessage::InvokeAction(invoke) = parsed.message else {
        panic!("expected InvokeAction");
    };
    assert_eq!(invoke.invocation_id.as_str(), "merge.head");
    assert_eq!(invoke.action_id.as_str(), "git-merger.merge");
    assert!(invoke.continuation.is_none());
    assert_eq!(invoke.context.screen_id.as_str(), "workbench");
    assert_eq!(invoke.context.resource_refs.len(), 1);
}

#[test]
fn invoke_action_with_continuation_parses() {
    let bytes = envelope(
        "invoke-action",
        "h-000010",
        1,
        r#"{"invocation_id":"merge.head","action_id":"git-merger.merge","arguments":{},"context":{"screen_id":"workbench","screen_instance":"default","resource_refs":{}},"continuation":{"confirmation_id":"conf.1","approved":true,"values":{"force":{"type":"bool","value":true}}}}"#,
    );
    let parsed = parsed(&bytes, Direction::HostToProvider);
    let ProviderMessage::InvokeAction(invoke) = parsed.message else {
        panic!("expected InvokeAction");
    };
    let continuation = invoke
        .continuation
        .clone()
        .unwrap_or_else(|| panic!("continuation present"));
    assert!(continuation.approved);
    assert_eq!(continuation.confirmation_id.as_str(), "conf.1");
    assert_eq!(continuation.values.len(), 1);
}

#[test]
fn cancel_parses_with_exact_fields() {
    let bytes = envelope(
        "cancel",
        "h-000004",
        1,
        r#"{"target_request_id":"h-000003"}"#,
    );
    let parsed = parsed(&bytes, Direction::HostToProvider);
    let ProviderMessage::Cancel(cancel) = parsed.message else {
        panic!("expected Cancel");
    };
    assert_eq!(cancel.target_request_id.as_str(), "h-000003");
}

#[test]
fn progress_parses_with_exact_fields() {
    let bytes = envelope(
        "progress",
        "p-000003",
        1,
        r#"{"sequence":1,"message":"checking","completed":1,"total":4}"#,
    );
    let parsed = parsed(&bytes, Direction::ProviderToHost);
    let ProviderMessage::Progress(progress) = parsed.message else {
        panic!("expected Progress");
    };
    assert_eq!(progress.sequence, 1);
    assert_eq!(progress.message, "checking");
    assert_eq!(progress.completed, Some(1));
    assert_eq!(progress.total, Some(4));
}

#[test]
fn error_payload_parses_with_exact_fields() {
    let bytes = envelope(
        "error",
        "p-000005",
        1,
        r#"{"code":"E_MERGE","message":"conflict","retryable":true,"field_errors":[{"path":"branch","message":"required"}]}"#,
    );
    let parsed = parsed(&bytes, Direction::ProviderToHost);
    let ProviderMessage::Error(err) = parsed.message else {
        panic!("expected Error");
    };
    assert_eq!(err.code, "E_MERGE");
    assert!(err.retryable);
    assert_eq!(err.field_errors.len(), 1);
    assert_eq!(err.field_errors[0].path, "branch");
}

#[test]
fn shutdown_parses_with_exact_fields() {
    let bytes = envelope("shutdown", "h-000005", 1, r#"{"reason":"completed"}"#);
    let parsed = parsed(&bytes, Direction::HostToProvider);
    let ProviderMessage::Shutdown(shutdown) = parsed.message else {
        panic!("expected Shutdown");
    };
    assert_eq!(shutdown.reason, ShutdownReason::Completed);
}

#[test]
fn shutdown_ack_parses_with_exact_fields() {
    let bytes = envelope("shutdown-ack", "p-000006", 1, "{}");
    let parsed = parsed(&bytes, Direction::ProviderToHost);
    assert!(matches!(parsed.message, ProviderMessage::ShutdownAck));
}

#[test]
fn request_host_confirmation_continuation_schema_parses_fields() {
    let bytes = envelope(
        "outcome",
        "p-000020",
        1,
        r#"{"kind":"request-host-confirmation","confirmation_id":"conf.1","title":"Confirm","body":"Proceed?","confirm_label":"OK","destructive":true,"continuation_schema":[{"id":"force","kind":"boolean","required":false,"restart":"none"}]}"#,
    );
    let parsed = parsed(&bytes, Direction::ProviderToHost);
    let ProviderMessage::Outcome(Outcome::RequestHostConfirmation {
        continuation_schema,
        ..
    }) = parsed.message
    else {
        panic!("expected request-host-confirmation outcome");
    };
    assert_eq!(
        continuation_schema.len(),
        1,
        "the declared field must parse"
    );
}

#[test]
fn an_unknown_outcome_kind_is_rejected() {
    let bytes = envelope("outcome", "p-000007", 1, r#"{"kind":"run-command"}"#);
    assert!(matches!(
        rejected(&bytes, Direction::ProviderToHost),
        ProviderError::UnknownValue { .. }
    ));
}

#[test]
fn each_of_the_seven_outcome_kinds_parses() {
    let kinds = [
        "navigate",
        "refresh",
        "notice",
        "replace-panel",
        "request-host-confirmation",
        "close-panel",
        "migrated-config",
    ];
    for (index, kind) in kinds.iter().enumerate() {
        let payload = outcome_payload(kind);
        let bytes = envelope("outcome", &format!("p-{index:06}"), 1, &payload);
        let parsed = parsed(&bytes, Direction::ProviderToHost);
        let ProviderMessage::Outcome(outcome) = parsed.message else {
            panic!("expected Outcome for {kind}");
        };
        assert_eq!(
            outcome,
            expected_outcome(kind),
            "outcome {kind} must round-trip its exact fields"
        );
    }
}

// ---------------------------------------------------------------------------
// CW10-06: closed-shape, direction, request-id, generation, and order faults.
// ---------------------------------------------------------------------------

#[test]
fn an_unknown_envelope_field_is_rejected() {
    let bytes = b"{\"protocol\":1,\"type\":\"hello\",\"request_id\":\"h-000001\",\"generation\":1,\"payload\":{\"host_api\":\"jefe\",\"plugin_id\":\"vybestack.git-merger\",\"plugin_version\":\"1.0.0\"},\"extra\":1}\n";
    assert!(matches!(
        rejected(bytes, Direction::HostToProvider),
        ProviderError::UnknownField { .. }
    ));
}

#[test]
fn a_missing_envelope_field_is_rejected() {
    let bytes = b"{\"protocol\":1,\"type\":\"hello\",\"request_id\":\"h-000001\",\"payload\":{\"host_api\":\"jefe\",\"plugin_id\":\"vybestack.git-merger\",\"plugin_version\":\"1.0.0\"}}\n";
    assert!(matches!(
        rejected(bytes, Direction::HostToProvider),
        ProviderError::MissingField { .. }
    ));
}

#[test]
fn a_wrong_field_type_is_rejected() {
    let bytes = b"{\"protocol\":1,\"type\":\"hello\",\"request_id\":\"h-000001\",\"generation\":\"oops\",\"payload\":{\"host_api\":\"jefe\",\"plugin_id\":\"vybestack.git-merger\",\"plugin_version\":\"1.0.0\"}}\n";
    assert!(matches!(
        rejected(bytes, Direction::HostToProvider),
        ProviderError::TypeMismatch { .. }
    ));
}

#[test]
fn an_unknown_payload_field_is_rejected_at_every_nesting() {
    // Unknown field directly in the payload object.
    let bytes = envelope(
        "hello",
        "h-000001",
        1,
        r#"{"host_api":"jefe","plugin_id":"vybestack.git-merger","plugin_version":"1.0.0","extra":1}"#,
    );
    assert!(matches!(
        rejected(&bytes, Direction::HostToProvider),
        ProviderError::UnknownField { .. }
    ));
    // Unknown field nested inside the invoke-action context object.
    let nested = envelope(
        "invoke-action",
        "h-000003",
        1,
        r#"{"invocation_id":"merge.head","action_id":"git-merger.merge","arguments":{},"context":{"screen_id":"workbench","screen_instance":"default","resource_refs":{},"sneaky":1}}"#,
    );
    assert!(matches!(
        rejected(&nested, Direction::HostToProvider),
        ProviderError::UnknownField { .. }
    ));
}

#[test]
fn an_unknown_message_type_is_rejected() {
    let bytes = envelope("frobnicate", "h-000001", 1, "{}");
    assert!(matches!(
        rejected(&bytes, Direction::HostToProvider),
        ProviderError::UnknownValue { .. }
    ));
}

#[test]
fn an_unsupported_protocol_version_is_rejected() {
    let bytes = b"{\"protocol\":2,\"type\":\"hello\",\"request_id\":\"h-000001\",\"generation\":1,\"payload\":{\"host_api\":\"jefe\",\"plugin_id\":\"vybestack.git-merger\",\"plugin_version\":\"1.0.0\"}}\n";
    assert!(
        rejected(bytes, Direction::HostToProvider)
            .code()
            .chars()
            .next()
            .is_some()
    );
}

#[test]
fn a_payload_on_the_wrong_stream_is_rejected() {
    // hello is host->provider; arriving on the provider->host stream is fatal.
    let bytes = envelope(
        "hello",
        "h-000001",
        1,
        r#"{"host_api":"jefe","plugin_id":"vybestack.git-merger","plugin_version":"1.0.0"}"#,
    );
    assert!(matches!(
        rejected(&bytes, Direction::ProviderToHost),
        ProviderError::InvalidDirection { .. }
    ));
    // hello-ack is provider->host; arriving on the host->provider stream is fatal.
    let ack = envelope(
        "hello-ack",
        "p-000001",
        1,
        r#"{"provider_name":"x","protocol":1}"#,
    );
    assert!(matches!(
        rejected(&ack, Direction::HostToProvider),
        ProviderError::InvalidDirection { .. }
    ));
}

#[test]
fn request_ids_accept_and_reject_their_digit_boundaries() {
    let base = |rid: &str| {
        envelope(
            "hello",
            rid,
            1,
            r#"{"host_api":"jefe","plugin_id":"vybestack.git-merger","plugin_version":"1.0.0"}"#,
        )
    };
    let _ = parsed(&base("h-000001"), Direction::HostToProvider);
    let _ = parsed(
        &base(&format!("h-{}", "1".repeat(20))),
        Direction::HostToProvider,
    );
    for invalid in [
        "h-1",
        "h-12345",
        &format!("h-{}", "1".repeat(21)),
        "x-123456",
        "123456",
        "h-abcd",
    ] {
        let error = rejected(&base(invalid), Direction::HostToProvider);
        assert!(
            matches!(error, ProviderError::InvalidRequestId { .. }),
            "{invalid:?} must be an invalid request id, got {error:?}"
        );
    }
}

#[test]
fn a_non_positive_generation_is_rejected() {
    let bytes = envelope(
        "hello",
        "h-000001",
        0,
        r#"{"host_api":"jefe","plugin_id":"vybestack.git-merger","plugin_version":"1.0.0"}"#,
    );
    assert!(matches!(
        rejected(&bytes, Direction::HostToProvider),
        ProviderError::InvalidGeneration { value: 0 }
    ));
}

#[test]
fn the_handshake_order_is_enforced() {
    let mut order = LifecycleOrder::new();
    observe_ok(&mut order, MessageKind::Hello, 1, "hello first");
    observe_ok(&mut order, MessageKind::HelloAck, 1, "then hello-ack");
    observe_ok(&mut order, MessageKind::Configure, 1, "then configure");
    observe_ok(&mut order, MessageKind::Ready, 1, "then ready");
    assert_eq!(order.phase(), LifecyclePhase::Ready);
}

#[test]
fn an_out_of_order_handshake_step_is_rejected() {
    let mut order = LifecycleOrder::new();
    let error = observe_err(&mut order, MessageKind::HelloAck, 1);
    assert!(matches!(error, ProviderError::OutOfOrder { .. }));
}

#[test]
fn a_generation_change_within_one_process_is_rejected() {
    let mut order = LifecycleOrder::new();
    observe_ok(
        &mut order,
        MessageKind::Hello,
        7,
        "first sets the generation",
    );
    let error = observe_err(&mut order, MessageKind::HelloAck, 8);
    assert!(matches!(error, ProviderError::InvalidGeneration { .. }));
}

#[test]
fn data_after_terminal_is_rejected() {
    let mut order = LifecycleOrder::new();
    for (kind, generation) in [
        (MessageKind::Hello, 1),
        (MessageKind::HelloAck, 1),
        (MessageKind::Configure, 1),
        (MessageKind::Ready, 1),
        (MessageKind::Shutdown, 1),
        (MessageKind::ShutdownAck, 1),
    ] {
        observe_ok(&mut order, kind, generation, "in-order step");
    }
    assert_eq!(order.phase(), LifecyclePhase::Terminated);
    let error = observe_err(&mut order, MessageKind::Progress, 1);
    assert!(matches!(error, ProviderError::OutOfOrder { .. }));
}

// ---------------------------------------------------------------------------
// CW10-07 (protocol half): progress sequence/count/total monotonicity.
// ---------------------------------------------------------------------------

#[test]
fn progress_sequence_increments_exactly_one_from_one() {
    let mut tracker = ProgressTracker::new();
    for sequence in 1..=10 {
        observe_progress_ok(
            &mut tracker,
            sequence,
            None,
            None,
            "monotonic sequence accepted",
        );
    }
    let error = observe_progress_err(&mut tracker, 12, None, None);
    assert!(matches!(
        error,
        ProviderError::Progress(ProgressFault::SequenceGap {
            expected: 11,
            observed: 12
        })
    ));
}

#[test]
fn progress_sequence_must_start_at_one() {
    for bad in [2_u16, 3_u16] {
        let mut tracker = ProgressTracker::new();
        let error = observe_progress_err(&mut tracker, bad, None, None);
        assert!(
            matches!(error, ProviderError::Progress(ProgressFault::BadStart { observed }) if observed == bad),
            "{bad} must not start a sequence"
        );
    }
}

#[test]
fn progress_sequence_must_not_repeat_or_decrease() {
    let mut tracker = ProgressTracker::new();
    observe_progress_ok(&mut tracker, 1, None, None, "first");
    let error = observe_progress_err(&mut tracker, 1, None, None);
    assert!(matches!(
        error,
        ProviderError::Progress(ProgressFault::SequenceNotIncreasing {
            previous: 1,
            observed: 1
        })
    ));
    let error = observe_progress_err(&mut tracker, 0, None, None);
    assert!(matches!(
        error,
        ProviderError::Progress(ProgressFault::SequenceNotIncreasing {
            previous: 1,
            observed: 0
        })
    ));
}

#[test]
fn progress_sequence_one_to_the_max_then_one_more() {
    let mut tracker = ProgressTracker::new();
    for sequence in 1..=PROGRESS_SEQUENCE_MAX {
        observe_progress_ok(
            &mut tracker,
            sequence,
            Some(u64::from(sequence)),
            Some(u64::from(PROGRESS_SEQUENCE_MAX)),
            "sequence within the max is accepted",
        );
    }
    let error = observe_progress_err(&mut tracker, PROGRESS_SEQUENCE_MAX + 1, None, None);
    assert!(matches!(
        error,
        ProviderError::Progress(ProgressFault::SequenceOverMax { observed, max })
        if observed == PROGRESS_SEQUENCE_MAX + 1 && max == PROGRESS_SEQUENCE_MAX
    ));
}

#[test]
fn progress_total_requires_completed() {
    let mut tracker = ProgressTracker::new();
    observe_progress_ok(&mut tracker, 1, None, None, "no totals");
    let error = observe_progress_err(&mut tracker, 2, None, Some(4));
    assert!(matches!(
        error,
        ProviderError::Progress(ProgressFault::TotalWithoutCompleted)
    ));
}

#[test]
fn progress_completed_must_not_exceed_total() {
    let mut tracker = ProgressTracker::new();
    let error = observe_progress_err(&mut tracker, 1, Some(5), Some(4));
    assert!(matches!(
        error,
        ProviderError::Progress(ProgressFault::CompletedExceedsTotal {
            completed: 5,
            total: 4
        })
    ));
}

#[test]
fn progress_completed_and_total_never_decrease() {
    let mut tracker = ProgressTracker::new();
    observe_progress_ok(&mut tracker, 1, Some(2), Some(4), "initial counts");
    let error = observe_progress_err(&mut tracker, 2, Some(1), Some(4));
    assert!(matches!(
        error,
        ProviderError::Progress(ProgressFault::CompletedDecreased {
            previous: 2,
            observed: 1
        })
    ));
    let error = observe_progress_err(&mut tracker, 2, Some(2), Some(3));
    assert!(matches!(
        error,
        ProviderError::Progress(ProgressFault::TotalDecreased {
            previous: 4,
            observed: 3
        })
    ));
}

// ---------------------------------------------------------------------------
// Small shared helpers.
// ---------------------------------------------------------------------------

/// Observe one lifecycle step, panicking with the failure if it is rejected.
fn observe_ok(order: &mut LifecycleOrder, kind: MessageKind, generation: u64, context: &str) {
    order
        .observe(kind, generation)
        .unwrap_or_else(|error| panic!("{context}: {error}"));
}

/// Observe one lifecycle step, panicking if it is accepted when it must fail.
fn observe_err(order: &mut LifecycleOrder, kind: MessageKind, generation: u64) -> ProviderError {
    order
        .observe(kind, generation)
        .err()
        .unwrap_or_else(|| panic!("step must be rejected"))
}

/// Observe one progress event, panicking with the failure if it is rejected.
fn observe_progress_ok(
    tracker: &mut ProgressTracker,
    sequence: u16,
    completed: Option<u64>,
    total: Option<u64>,
    context: &str,
) {
    tracker
        .observe(sequence, completed, total)
        .unwrap_or_else(|error| panic!("{context}: {error}"));
}

/// Observe one progress event, panicking if it is accepted when it must fail.
fn observe_progress_err(
    tracker: &mut ProgressTracker,
    sequence: u16,
    completed: Option<u64>,
    total: Option<u64>,
) -> ProviderError {
    tracker
        .observe(sequence, completed, total)
        .err()
        .unwrap_or_else(|| panic!("progress event must be rejected"))
}

fn route_id() -> super::protocol::Id {
    id("workbench")
}

fn empty_map() -> super::protocol::TypedMap {
    super::protocol::TypedMap::new()
}

fn empty_snapshot() -> super::protocol::PanelSnapshot {
    super::protocol::PanelSnapshot(empty_map())
}

fn empty_migration() -> super::protocol::MigratedConfig {
    super::protocol::MigratedConfig(empty_map())
}

fn id(value: &str) -> super::protocol::Id {
    super::protocol::Id::parse(value)
        .unwrap_or_else(|error| panic!("fixture id must parse: {error}"))
}

fn outcome_payload(kind: &str) -> String {
    match kind {
        "navigate" => r#"{"kind":"navigate","route_id":"workbench","activation":{}}"#.to_owned(),
        "refresh" => r#"{"kind":"refresh","resource_ref":{}}"#.to_owned(),
        "notice" => r#"{"kind":"notice","severity":"info","message":"hi"}"#.to_owned(),
        "replace-panel" => {
            r#"{"kind":"replace-panel","panel_instance_id":"panel.1","snapshot":{}}"#.to_owned()
        }
        "request-host-confirmation" => r#"{"kind":"request-host-confirmation","confirmation_id":"conf.1","title":"Confirm","body":"Proceed?","confirm_label":"OK","destructive":true,"continuation_schema":[]}"#.to_owned(),
        "close-panel" => r#"{"kind":"close-panel","panel_instance_id":"panel.1"}"#.to_owned(),
        "migrated-config" => r#"{"kind":"migrated-config","migration":{}}"#.to_owned(),
        other => panic!("no outcome fixture for {other:?}"),
    }
}

/// The typed outcome each canonical fixture payload must round-trip to.
fn expected_outcome(kind: &str) -> Outcome {
    match kind {
        "navigate" => Outcome::Navigate {
            route_id: route_id(),
            activation: empty_map(),
        },
        "refresh" => Outcome::Refresh {
            resource_ref: empty_map(),
        },
        "notice" => Outcome::Notice {
            severity: Severity::Info,
            message: "hi".to_owned(),
        },
        "replace-panel" => Outcome::ReplacePanel {
            panel_instance_id: id("panel.1"),
            snapshot: empty_snapshot(),
        },
        "request-host-confirmation" => Outcome::RequestHostConfirmation {
            confirmation_id: id("conf.1"),
            title: "Confirm".to_owned(),
            body: "Proceed?".to_owned(),
            confirm_label: "OK".to_owned(),
            destructive: true,
            continuation_schema: Vec::new(),
        },
        "close-panel" => Outcome::ClosePanel {
            panel_instance_id: id("panel.1"),
        },
        "migrated-config" => Outcome::MigratedConfig {
            migration: empty_migration(),
        },
        other => panic!("no outcome fixture for {other:?}"),
    }
}

//! Adversarial integration tests for JSP/1 compliance Slice B: runner-owned
//! challenge execution.
//!
//! These tests reproduce fabricated adapter_version/no process, arbitrary
//! marker, fake capacity/gap, unknown handles, partial binding, rejected
//! mutation, state-changing SSE tail, snapshot-only, unknown activity lease,
//! and draft exclusion scenarios.

use std::path::PathBuf;

use jefe::jsp::v1::compliance::challenge::{
    AdapterKind, ChallengeFailure, ChallengeNonce, ChallengeVerification, RunnerChallenge,
    verify_clock, verify_draft_exclusion, verify_gap, verify_nonce, verify_redaction, verify_sink,
};
use jefe::jsp::v1::compliance::profile::{
    validate_producer_trace, validate_producer_trace_with_challenge,
    validate_producer_trace_with_nonce,
};
use jefe::jsp::v1::compliance::server_profile::{
    validate_server_transcript, validate_server_transcript_with_challenge,
    validate_server_transcript_with_nonce,
};
use jefe::jsp::v1::compliance::{invoke_adapter, run_reference_adapter};
use serde_json::Value;

fn workspace_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

fn traces_dir() -> PathBuf {
    workspace_root().join("dev-docs/jsp/v1/compliance/traces")
}

fn read_json(path: &std::path::Path) -> Value {
    let raw =
        std::fs::read_to_string(path).unwrap_or_else(|e| panic!("read {}: {e}", path.display()));
    serde_json::from_str(&raw).unwrap_or_else(|e| panic!("parse {}: {e}", path.display()))
}

fn json_bytes(value: &Value) -> Vec<u8> {
    serde_json::to_vec(value).unwrap_or_else(|error| panic!("serialize test mutation: {error}"))
}

fn reference_response(kind: AdapterKind, nonce: u64) -> (RunnerChallenge, Value) {
    let challenge = RunnerChallenge::reference(kind, nonce);
    let challenge_bytes = serde_json::to_vec(&challenge)
        .unwrap_or_else(|error| panic!("serialize runner challenge: {error}"));
    let output = run_reference_adapter(&challenge_bytes)
        .unwrap_or_else(|error| panic!("run reference adapter: {error}"));
    let response = serde_json::from_slice(&output.stdout)
        .unwrap_or_else(|error| panic!("parse reference response: {error}"));
    (challenge, response)
}

// ---------------------------------------------------------------------------
// Challenge nonce binding
// ---------------------------------------------------------------------------

#[test]
fn producer_nonce_mismatch_fails() {
    let trace = read_json(&traces_dir().join("producer-trace.json"));
    let expected_nonce = ChallengeNonce(99999);
    let report = validate_producer_trace_with_nonce(&json_bytes(&trace), expected_nonce);
    assert!(
        !report.passed,
        "nonce mismatch must fail: {:?}",
        report.findings
    );
    assert!(
        report
            .findings
            .iter()
            .any(|f| f.invariant == "challenge_nonce_binding"),
        "must report nonce binding failure"
    );
}

#[test]
fn producer_nonce_match_passes() {
    let trace = read_json(&traces_dir().join("producer-trace.json"));
    let expected_nonce = ChallengeNonce(0);
    let report = validate_producer_trace_with_nonce(&json_bytes(&trace), expected_nonce);
    assert!(
        report.passed,
        "nonce match must pass: {:?}",
        report.findings
    );
}

#[test]
fn server_nonce_mismatch_fails() {
    let transcript = read_json(&traces_dir().join("server-transcript.json"));
    let expected_nonce = ChallengeNonce(99999);
    let report = validate_server_transcript_with_nonce(&json_bytes(&transcript), expected_nonce);
    assert!(
        !report.passed,
        "nonce mismatch must fail: {:?}",
        report.findings
    );
    assert!(
        report
            .findings
            .iter()
            .any(|f| f.invariant == "challenge_nonce_binding"),
        "must report nonce binding failure"
    );
}

#[test]
fn server_nonce_match_passes() {
    let transcript = read_json(&traces_dir().join("server-transcript.json"));
    let expected_nonce = ChallengeNonce(0);
    let report = validate_server_transcript_with_nonce(&json_bytes(&transcript), expected_nonce);
    assert!(
        report.passed,
        "nonce match must pass: {:?}",
        report.findings
    );
}

// ---------------------------------------------------------------------------
// Challenge verification unit tests
// ---------------------------------------------------------------------------

#[test]
fn challenge_nonce_mismatch_detected() {
    let result = verify_nonce(1, ChallengeNonce(2));
    assert_eq!(
        result,
        ChallengeVerification::Failed(ChallengeFailure::NonceMismatch)
    );
}

#[test]
fn challenge_nonce_match_verified() {
    let result = verify_nonce(42, ChallengeNonce(42));
    assert_eq!(result, ChallengeVerification::Verified);
}

#[test]
fn challenge_fabricated_queue_arithmetic_detected() {
    use jefe::jsp::v1::compliance::challenge::SinkChallenge;
    let challenge = SinkChallenge {
        capacity: 5,
        deadline_ms: 1000,
        operations: 6,
    };
    // Fabricated: accepted (3) != capacity (5)
    let result = verify_sink(&challenge, 6, 3);
    assert_eq!(
        result,
        ChallengeVerification::Failed(ChallengeFailure::QueueArithmetic)
    );
}

#[test]
fn challenge_valid_queue_arithmetic_passes() {
    use jefe::jsp::v1::compliance::challenge::SinkChallenge;
    let challenge = SinkChallenge {
        capacity: 5,
        deadline_ms: 1000,
        operations: 6,
    };
    let result = verify_sink(&challenge, 6, 5);
    assert_eq!(result, ChallengeVerification::Verified);
}

#[test]
fn challenge_uncaptured_gap_detected() {
    let result = verify_gap(&jefe::jsp::v1::compliance::challenge::GapChallenge {
        emitted_through: 5,
        dropped_start: 6,
        dropped_end: 7,
        next_emitted: 99,
    });
    assert_eq!(
        result,
        ChallengeVerification::Failed(ChallengeFailure::GapNotCaptured)
    );
}

#[test]
fn challenge_arbitrary_marker_present_detected() {
    let result = verify_redaction(
        b"content with MARKER inside",
        &jefe::jsp::v1::compliance::challenge::RedactionChallenge {
            document_index: 0,
            marker: "MARKER".to_string(),
        },
    );
    assert_eq!(
        result,
        ChallengeVerification::Failed(ChallengeFailure::MarkerPresent)
    );
}

#[test]
fn challenge_draft_exclusion_detected() {
    let result = verify_draft_exclusion(b"has DRAFT content", "DRAFT");
    assert_eq!(
        result,
        ChallengeVerification::Failed(ChallengeFailure::DraftLeaked)
    );
}

// ---------------------------------------------------------------------------
// Fabricated adapter_version / no process binding
// ---------------------------------------------------------------------------

#[test]
fn producer_fabricated_adapter_version_fails_executable_qualification() {
    let (challenge, mut trace) = reference_response(AdapterKind::Producer, 8_101);
    trace["adapter_version"] = Value::String("fabricated-version-xyz".to_string());
    let report = validate_producer_trace_with_challenge(&json_bytes(&trace), &challenge);
    assert!(!report.passed, "fabricated executable identity must fail");
    assert!(
        report
            .findings
            .iter()
            .any(|finding| finding.invariant == "adapter_identity")
    );
}

#[test]
fn producer_missing_process_binding_fails() {
    let mut trace = read_json(&traces_dir().join("producer-trace.json"));
    // Remove process binding from the snapshot
    if let Some(facts) = trace.get_mut("facts").and_then(Value::as_array_mut)
        && let Some(doc) = facts.iter_mut().find_map(|f| {
            if f.get("fact").and_then(Value::as_str) == Some("document") {
                f.get_mut("document")
            } else {
                None
            }
        })
        && doc["kind"] == "snapshot"
    {
        doc["process_binding"] = Value::String("unsupported".to_string());
    }
    let report = validate_producer_trace(&json_bytes(&trace));
    assert!(!report.passed, "missing process binding must fail");
    assert!(
        report
            .findings
            .iter()
            .any(|f| f.invariant == "launch_process_binding"),
        "must report launch_process_binding failure: {:?}",
        report.findings
    );
}

// ---------------------------------------------------------------------------
// Fake capacity/gap adversarial
// ---------------------------------------------------------------------------

#[test]
fn producer_fabricated_capacity_challenge_fails() {
    let mut trace = read_json(&traces_dir().join("producer-trace.json"));
    // Mutate the nonblocking challenge to have fabricated arithmetic
    if let Some(facts) = trace.get_mut("facts").and_then(Value::as_array_mut) {
        for fact in facts.iter_mut() {
            if fact.get("fact").and_then(Value::as_str) == Some("nonblocking_challenge") {
                // Set accepted != capacity to create fabricated arithmetic
                fact["accepted"] = Value::Number(serde_json::Number::from(999));
            }
        }
    }
    let report = validate_producer_trace(&json_bytes(&trace));
    assert!(!report.passed);
    assert!(
        report
            .findings
            .iter()
            .any(|f| f.invariant == "nonblocking_publication"),
        "fabricated capacity must fail: {:?}",
        report.findings
    );
}

#[test]
fn producer_fabricated_gap_challenge_fails() {
    let mut trace = read_json(&traces_dir().join("producer-trace.json"));
    // Mutate the gap challenge to have inconsistent next_emitted
    if let Some(facts) = trace.get_mut("facts").and_then(Value::as_array_mut) {
        for fact in facts.iter_mut() {
            if fact.get("fact").and_then(Value::as_str) == Some("gap_challenge") {
                fact["next_emitted"] = Value::Number(serde_json::Number::from(99999));
            }
        }
    }
    let report = validate_producer_trace(&json_bytes(&trace));
    assert!(!report.passed);
    assert!(
        report
            .findings
            .iter()
            .any(|f| f.invariant == "nonblocking_gap_signaling"),
        "fabricated gap must fail: {:?}",
        report.findings
    );
}

// ---------------------------------------------------------------------------
// Unknown handles / partial binding
// ---------------------------------------------------------------------------

#[test]
fn server_unknown_credential_handle_fails() {
    let mut transcript = read_json(&traces_dir().join("server-transcript.json"));
    // Replace a credential handle with an unknown one
    if let Some(interactions) = transcript
        .get_mut("interactions")
        .and_then(Value::as_array_mut)
        && let Some(first) = interactions.first_mut()
    {
        first["request"]["credential_handle"] =
            Value::String("unknown-credential-handle".to_string());
    }
    let report = validate_server_transcript(&json_bytes(&transcript));
    // The unknown credential should be treated as forbidden (403), not
    // authenticated. Since the response still says 201 registered, this
    // should fail.
    assert!(
        !report.passed,
        "unknown credential must fail: {:?}",
        report.findings
    );
}

#[test]
fn server_partial_binding_fails() {
    let mut transcript = read_json(&traces_dir().join("server-transcript.json"));
    // Change only the principal_handle to create a partial binding
    if let Some(interactions) = transcript
        .get_mut("interactions")
        .and_then(Value::as_array_mut)
        && let Some(first) = interactions.first_mut()
    {
        first["request"]["principal_handle"] = Value::String("wrong-principal".to_string());
    }
    let report = validate_server_transcript(&json_bytes(&transcript));
    assert!(
        !report.passed,
        "partial binding must fail: {:?}",
        report.findings
    );
}

// ---------------------------------------------------------------------------
// Rejected mutation must have immediate trusted observation
// ---------------------------------------------------------------------------

#[test]
fn server_rejected_mutation_does_not_mutate_canonical_state() {
    let transcript = read_json(&traces_dir().join("server-transcript.json"));
    let report = validate_server_transcript(&json_bytes(&transcript));
    assert!(
        report.passed,
        "rejections must not mutate: {:?}",
        report.findings
    );
}

#[test]
fn producer_arbitrary_absent_marker_fails_runner_binding() {
    let (challenge, mut trace) = reference_response(AdapterKind::Producer, 8_102);
    let facts = trace["facts"]
        .as_array_mut()
        .unwrap_or_else(|| panic!("facts array"));
    let fact = facts
        .iter_mut()
        .find(|fact| fact["fact"] == "redaction_challenge")
        .unwrap_or_else(|| panic!("redaction fact"));
    fact["forbidden_marker"] = Value::String("arbitrary-absent-marker".to_string());
    let report = validate_producer_trace_with_challenge(&json_bytes(&trace), &challenge);
    assert!(!report.passed);
    assert!(
        report
            .findings
            .iter()
            .any(|finding| finding.invariant == "source_redaction")
    );
}

#[test]
fn producer_gap_requires_captured_next_publication() {
    let (challenge, mut trace) = reference_response(AdapterKind::Producer, 8_103);
    let facts = trace["facts"]
        .as_array_mut()
        .unwrap_or_else(|| panic!("facts array"));
    let fact = facts
        .iter_mut()
        .find(|fact| fact["fact"] == "gap_challenge")
        .unwrap_or_else(|| panic!("gap fact"));
    fact.as_object_mut()
        .unwrap_or_else(|| panic!("gap object"))
        .remove("next_publication");
    let report = validate_producer_trace_with_challenge(&json_bytes(&trace), &challenge);
    assert!(!report.passed);
}

#[test]
fn server_unknown_auth_and_partial_binding_mutations_fail_strict_profile() {
    let (challenge, mut transcript) = reference_response(AdapterKind::Server, 8_104);
    let interactions = transcript["interactions"]
        .as_array_mut()
        .unwrap_or_else(|| panic!("interactions array"));
    let unknown = interactions
        .iter_mut()
        .find(|interaction| interaction["response"]["kind"] == "unknown_authentication")
        .unwrap_or_else(|| panic!("unknown authentication proof"));
    unknown["response"]["status"] = Value::from(403);
    unknown["response"]["kind"] = Value::String("forbidden_role".to_string());
    let report = validate_server_transcript_with_challenge(&json_bytes(&transcript), &challenge);
    assert!(!report.passed, "unknown auth must remain distinct");

    let (challenge, mut transcript) = reference_response(AdapterKind::Server, 8_105);
    transcript["interactions"][0]["request"]["source_epoch"] =
        Value::String("partially-bound-epoch".to_string());
    let report = validate_server_transcript_with_challenge(&json_bytes(&transcript), &challenge);
    assert!(!report.passed, "partial identity binding must fail");
}

#[test]
fn server_requires_immediate_digest_real_tail_and_snapshot_only() {
    let (challenge, mut transcript) = reference_response(AdapterKind::Server, 8_106);
    let interactions = transcript["interactions"]
        .as_array_mut()
        .unwrap_or_else(|| panic!("interactions array"));
    let digest_index = interactions
        .iter()
        .position(|interaction| {
            interaction["request"]["route"] == "/jsp/1/internal/observation_digest"
        })
        .unwrap_or_else(|| panic!("digest interaction"));
    interactions.remove(digest_index);
    let report = validate_server_transcript_with_challenge(&json_bytes(&transcript), &challenge);
    assert!(!report.passed, "missing immediate digest must fail");

    let (challenge, mut transcript) = reference_response(AdapterKind::Server, 8_107);
    let interactions = transcript["interactions"]
        .as_array_mut()
        .unwrap_or_else(|| panic!("interactions array"));
    let tail = interactions
        .iter_mut()
        .find(|interaction| interaction["name"] == "runner_owned_atomic_tail")
        .unwrap_or_else(|| panic!("atomic tail"));
    tail["stream"]
        .as_array_mut()
        .unwrap_or_else(|| panic!("tail stream"))
        .truncate(1);
    let report = validate_server_transcript_with_challenge(&json_bytes(&transcript), &challenge);
    assert!(!report.passed, "missing state-changing tail must fail");

    let (challenge, mut transcript) = reference_response(AdapterKind::Server, 8_108);
    let interactions = transcript["interactions"]
        .as_array_mut()
        .unwrap_or_else(|| panic!("interactions array"));
    interactions.retain(|interaction| {
        !matches!(
            interaction["name"].as_str(),
            Some("observe_after_bound_rejection" | "observe_stream_snapshot_first")
        )
    });
    let report = validate_server_transcript_with_challenge(&json_bytes(&transcript), &challenge);
    assert!(!report.passed, "missing snapshot-only proof must fail");
}

// ---------------------------------------------------------------------------
// Activity value never maps unknown/degraded/unsupported to idle
// ---------------------------------------------------------------------------

#[test]
fn server_activity_value_never_maps_unknown_to_idle() {
    // The HealthWire's native_activity field must carry full provenance.
    // If a transcript reports activity as unknown/degraded/unsupported,
    // it must NOT be collapsed to idle.
    let mut transcript = read_json(&traces_dir().join("server-transcript.json"));
    // Find the lease interaction and set native_activity to unsupported
    if let Some(interactions) = transcript
        .get_mut("interactions")
        .and_then(Value::as_array_mut)
    {
        for interaction in interactions.iter_mut() {
            if let Some(response) = interaction.get_mut("response")
                && response.get("kind").and_then(Value::as_str) == Some("observation_health_stale")
                && let Some(body) = response.get_mut("body")
            {
                body["native_activity"] = Value::String("unsupported".to_string());
            }
        }
    }
    let report = validate_server_transcript(&json_bytes(&transcript));
    assert!(
        !report.passed,
        "unsupported activity must not pass as idle: {:?}",
        report.findings
    );
}

// ---------------------------------------------------------------------------
// Draft exclusion (S9 draft-negative runner challenge)
// ---------------------------------------------------------------------------

/// The runner-owned draft marker. It matches the distinctive
/// `JSP-DRAFT-{nonce}` shape produced by `RunnerChallenge::reference` rather
/// than a generic English word, so the check cannot be satisfied or defeated
/// by incidental protocol vocabulary such as the `draft_challenge` fact name.
const DRAFT_MARKER: &str = "JSP-DRAFT-477";

#[test]
fn producer_draft_marker_absent_from_output_passes() {
    // The producer trace must never contain uncommitted draft content.
    let trace = read_json(&traces_dir().join("producer-trace.json"));
    let bytes = json_bytes(&trace);
    let result = verify_draft_exclusion(&bytes, DRAFT_MARKER);
    assert_eq!(result, ChallengeVerification::Verified);
}

#[test]
fn producer_draft_marker_present_fails() {
    let mut trace = read_json(&traces_dir().join("producer-trace.json"));
    // Inject draft content into the trace.
    trace["description"] = Value::String(format!("contains {DRAFT_MARKER} content here"));
    let bytes = json_bytes(&trace);
    let result = verify_draft_exclusion(&bytes, DRAFT_MARKER);
    assert_eq!(
        result,
        ChallengeVerification::Failed(ChallengeFailure::DraftLeaked)
    );
}

// ---------------------------------------------------------------------------
// Reference adapter execution
// ---------------------------------------------------------------------------

#[test]
fn reference_adapter_producer_challenge_produces_valid_trace() {
    let challenge = jefe::jsp::v1::compliance::challenge::RunnerChallenge::reference(
        jefe::jsp::v1::compliance::challenge::AdapterKind::Producer,
        12345,
    );
    let challenge = serde_json::to_vec(&challenge)
        .unwrap_or_else(|error| panic!("serialize challenge: {error}"));
    let output = run_reference_adapter(&challenge)
        .unwrap_or_else(|error| panic!("reference adapter must produce output: {error:?}"));
    assert!(!output.stdout.is_empty());
    // The output must be valid JSON with the nonce bound
    let trace: Value = serde_json::from_slice(&output.stdout)
        .unwrap_or_else(|error| panic!("valid JSON output: {error}"));
    assert_eq!(trace["challenge_nonce"], 12345);
    assert_eq!(trace["schema"], 1);
}

#[test]
fn reference_adapter_server_challenge_produces_valid_transcript() {
    let challenge = jefe::jsp::v1::compliance::challenge::RunnerChallenge::reference(
        jefe::jsp::v1::compliance::challenge::AdapterKind::Server,
        54321,
    );
    let challenge = serde_json::to_vec(&challenge)
        .unwrap_or_else(|error| panic!("serialize challenge: {error}"));
    let output = run_reference_adapter(&challenge)
        .unwrap_or_else(|error| panic!("reference adapter must produce output: {error:?}"));
    assert!(!output.stdout.is_empty());
    let transcript: Value = serde_json::from_slice(&output.stdout)
        .unwrap_or_else(|error| panic!("valid JSON output: {error}"));
    assert_eq!(transcript["challenge_nonce"], 54321);
    assert_eq!(transcript["schema"], 1);
}

#[test]
fn reference_adapter_missing_fields_returns_error() {
    let challenge = serde_json::json!({
        "nonce": 1,
        "kind": "producer",
        "agent_id": "",
        "lifecycle_generation": 1,
        "source_epoch": "e",
    });
    let result = run_reference_adapter(challenge.to_string().as_bytes());
    assert!(result.is_err(), "missing agent_id must fail");
}

// ---------------------------------------------------------------------------
// External adapter invocation
// ---------------------------------------------------------------------------

// The echo/nonzero/oversized adapter integration tests rely on `cat`/`false`,
// which are POSIX-only. They are gated to `cfg(unix)` so the suite compiles
// and runs cross-platform; the nonexistent-program test below is portable and
// remains ungated to retain adapter-spawn coverage everywhere.

#[cfg(unix)]
#[test]
fn invoke_adapter_cat_echoes_input() {
    let result = invoke_adapter(&["cat".to_string()], br#"{"test": true}"#);
    assert!(result.is_ok());
    let output = result.unwrap_or_else(|e| panic!("adapter failed: {e:?}"));
    assert_eq!(output.stdout, br#"{"test": true}"#);
}

#[test]
fn invoke_adapter_nonexistent_program_fails() {
    let result = invoke_adapter(&["jefe-nonexistent-477".to_string()], b"{}");
    assert!(result.is_err());
}

#[cfg(unix)]
#[test]
fn invoke_adapter_false_exits_nonzero() {
    let result = invoke_adapter(&["false".to_string()], b"{}");
    assert!(result.is_err());
}

#[cfg(unix)]
#[test]
fn invoke_adapter_oversized_input_fails() {
    let huge = vec![b' '; (2 * 1024 * 1024) + 1];
    let result = invoke_adapter(&["cat".to_string()], &huge);
    assert!(result.is_err());
}

// ---------------------------------------------------------------------------
// Clock challenge verification
// ---------------------------------------------------------------------------

#[test]
fn clock_challenge_monotonic_passes() {
    let result = verify_clock(&jefe::jsp::v1::compliance::challenge::ClockChallenge {
        timestamps: vec![100, 200, 300, 400],
    });
    assert_eq!(result, ChallengeVerification::Verified);
}

#[test]
fn clock_challenge_non_monotonic_fails() {
    let result = verify_clock(&jefe::jsp::v1::compliance::challenge::ClockChallenge {
        timestamps: vec![100, 200, 150, 400],
    });
    assert_eq!(
        result,
        ChallengeVerification::Failed(ChallengeFailure::ClockMismatch)
    );
}

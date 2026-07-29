//! Runner-owned producer challenge binding.

use super::dto::{
    DocumentWire, ProducerFactKind, ProducerFactWire, ProducerTraceWire, TypedDocument,
};
use super::profile::{
    MAX_PROFILE_INPUT_BYTES, ProducerReport, ProfileError, contains_bytes, failed_bound,
    failed_shape, validate_producer_trace, validate_typed_trace,
};
use super::reducer::ReferenceReducer;

/// Validate producer qualification against the complete runner-owned challenge.
#[must_use]
pub(super) fn validate_producer_trace_with_challenge(
    input: &[u8],
    challenge: &super::challenge::RunnerChallenge,
) -> ProducerReport {
    if input.len() > MAX_PROFILE_INPUT_BYTES {
        return failed_bound();
    }
    let mut report = validate_producer_trace(input);
    let trace: ProducerTraceWire = match serde_json::from_slice(input) {
        Ok(trace) => trace,
        Err(_) => return failed_shape(),
    };
    if challenge.kind != super::challenge::AdapterKind::Producer || !challenge.structurally_valid()
    {
        challenge_finding(&mut report, "challenge_shape", "JSP-C-CHALLENGE-SHAPE");
        return report;
    }
    if trace.challenge_nonce != challenge.nonce {
        challenge_finding(
            &mut report,
            "challenge_nonce_binding",
            "JSP-C-NONCE-MISMATCH",
        );
    }
    if trace.adapter_version != challenge.adapter_version
        || trace.adapter_version != super::challenge::PRODUCER_ADAPTER_VERSION
    {
        challenge_finding(&mut report, "adapter_identity", "JSP-C-ADAPTER-VERSION");
    }
    bind_producer_facts(trace.facts, challenge, &mut report);
    report.passed = report.findings.is_empty();
    report
}

#[derive(Default)]
struct ProducerBinding {
    clocks: Vec<u64>,
    documents: Vec<Vec<u8>>,
    proofs: u8,
    reducer: ReferenceReducer,
}

impl ProducerBinding {
    const LAUNCH: u8 = 1 << 0;
    const REDACTION: u8 = 1 << 1;
    const DRAFT: u8 = 1 << 2;
    const SINK: u8 = 1 << 3;
    const GAP: u8 = 1 << 4;

    fn prove(&mut self, bit: u8, valid: bool) {
        if valid {
            self.proofs |= bit;
        }
    }

    fn proved(&self, bit: u8) -> bool {
        self.proofs & bit != 0
    }

    fn apply(&mut self, document: TypedDocument) {
        match document {
            TypedDocument::Snapshot(snapshot) => self.reducer.apply_snapshot(&snapshot),
            TypedDocument::Event(event) => {
                let _ = self.reducer.apply_event(&event);
            }
            TypedDocument::Heartbeat(heartbeat) => {
                let _ = self.reducer.apply_heartbeat(&heartbeat);
            }
        }
    }
}

fn bind_producer_facts(
    facts: Vec<ProducerFactWire>,
    challenge: &super::challenge::RunnerChallenge,
    report: &mut ProducerReport,
) {
    let mut binding = ProducerBinding::default();
    for fact in facts {
        bind_producer_fact(fact, challenge, &mut binding);
    }
    let projection_bytes = serde_json::to_vec(&binding.reducer.projection()).unwrap_or_default();
    let redacted = binding
        .documents
        .get(challenge.redaction.document_index)
        .is_some_and(|document| !contains_bytes(document, challenge.redaction.marker.as_bytes()));
    let draft_absent = binding
        .documents
        .iter()
        .all(|document| !contains_bytes(document, challenge.draft.marker.as_bytes()))
        && !contains_bytes(&projection_bytes, challenge.draft.marker.as_bytes());
    for (valid, invariant, code) in [
        (
            binding.proved(ProducerBinding::LAUNCH),
            "launch_process_binding",
            "JSP-C-BINDING-MISMATCH",
        ),
        (
            binding.clocks == challenge.clock_sequence,
            "timestamp_provenance",
            "JSP-C-CLOCK-MISMATCH",
        ),
        (
            binding.proved(ProducerBinding::REDACTION) && redacted,
            "source_redaction",
            "JSP-C-MARKER-BINDING",
        ),
        (
            binding.proved(ProducerBinding::DRAFT) && draft_absent,
            "draft_exclusion",
            "JSP-C-DRAFT-LEAKED",
        ),
        (
            binding.proved(ProducerBinding::SINK),
            "nonblocking_publication",
            "JSP-C-QUEUE-ARITHMETIC",
        ),
        (
            binding.proved(ProducerBinding::GAP),
            "nonblocking_gap_signaling",
            "JSP-C-GAP-NOT-CAPTURED",
        ),
    ] {
        if !valid {
            challenge_finding(report, invariant, code);
        }
    }
}
fn bind_producer_fact(
    fact: ProducerFactWire,
    challenge: &super::challenge::RunnerChallenge,
    binding: &mut ProducerBinding,
) {
    match fact.fact {
        ProducerFactKind::ClockSet => binding.clocks.extend(fact.now_ms),
        ProducerFactKind::Document => bind_document_fact(fact.document, challenge, binding),
        ProducerFactKind::RedactionChallenge => binding.prove(
            ProducerBinding::REDACTION,
            fact.source_handle.as_deref() == Some(challenge.redaction.source_handle.as_str())
                && fact.document_index == Some(challenge.redaction.document_index)
                && fact.forbidden_marker.as_deref() == Some(challenge.redaction.marker.as_str()),
        ),
        ProducerFactKind::DraftChallenge => binding.prove(
            ProducerBinding::DRAFT,
            fact.source_handle.as_deref() == Some(challenge.draft.source_handle.as_str()),
        ),
        ProducerFactKind::NonblockingChallenge => binding.prove(
            ProducerBinding::SINK,
            fact.operation_handle.as_deref() == Some(challenge.sink.operation_handle.as_str())
                && fact.operation_handles.as_ref() == Some(&challenge.sink.operations)
                && fact.queue_capacity == Some(challenge.sink.capacity)
                && fact.attempted == u64::try_from(challenge.sink.operations.len()).ok()
                && fact.accepted == Some(challenge.sink.capacity)
                && fact.deadline_ms == Some(challenge.sink.deadline_ms)
                && fact
                    .elapsed_ms
                    .is_some_and(|elapsed| elapsed < challenge.sink.deadline_ms),
        ),
        ProducerFactKind::GapChallenge => {
            binding.prove(ProducerBinding::GAP, bind_gap_fact(&fact, challenge));
        }
        ProducerFactKind::BoundChallenge => {}
    }
}

fn bind_document_fact(
    document: Option<DocumentWire>,
    challenge: &super::challenge::RunnerChallenge,
    binding: &mut ProducerBinding,
) {
    let Some(document) = document else {
        return;
    };
    let bytes = document.as_bytes().to_vec();
    if let Ok(typed) = document.into_typed() {
        binding.prove(
            ProducerBinding::LAUNCH,
            bind_launch_document(&typed, challenge),
        );
        binding.apply(typed);
    }
    binding.documents.push(bytes);
}

fn bind_launch_document(
    document: &TypedDocument,
    challenge: &super::challenge::RunnerChallenge,
) -> bool {
    let TypedDocument::Snapshot(snapshot) = document else {
        return false;
    };
    let expected = &challenge.launch.identity;
    let identity_matches = snapshot.identity.agent_id.as_str() == expected.agent_id.as_str()
        && snapshot.identity.lifecycle_generation == expected.lifecycle_generation
        && snapshot.identity.source_epoch.as_str() == expected.source_epoch.as_str();
    let crate::domain::observation::FieldState::Supported { availability, .. } =
        &snapshot.process_binding
    else {
        return false;
    };
    matches!(availability, crate::domain::observation::Availability::Known(binding)
        if identity_matches && binding.pid == challenge.launch.pid
            && binding.started_at_ms == challenge.launch.started_at_ms)
}

fn bind_gap_fact(fact: &ProducerFactWire, challenge: &super::challenge::RunnerChallenge) -> bool {
    let Some(publication) = fact.next_publication.as_ref() else {
        return false;
    };
    let Ok(event) = crate::jsp::v1::parse_event(publication.as_bytes()) else {
        return false;
    };
    let expected = &challenge.launch.identity;
    fact.operation_handle.as_deref() == Some(challenge.gap.operation_handle.as_str())
        && fact.emitted_through == Some(challenge.gap.emitted_through)
        && fact.dropped_start == Some(challenge.gap.dropped_start)
        && fact.dropped_end == Some(challenge.gap.dropped_end)
        && fact.next_emitted == Some(challenge.gap.next_emitted)
        && event.source_sequence == challenge.gap.next_emitted
        && event.identity.agent_id.as_str() == expected.agent_id.as_str()
        && event.identity.lifecycle_generation == expected.lifecycle_generation
        && event.identity.source_epoch.as_str() == expected.source_epoch.as_str()
}

fn challenge_finding(report: &mut ProducerReport, invariant: &str, detail: &str) {
    report.findings.push(ProfileError {
        profile: "producer".to_string(),
        invariant: invariant.to_string(),
        detail: detail.to_string(),
    });
    report.passed = false;
}
/// Validate a producer trace with a runner-owned challenge nonce.
///
/// The trace's `challenge_nonce` must match the runner-supplied nonce.
/// This replaces the replayable self-attested model: the observed result
/// must bind to the runner's nonce, so a replayed trace from a different
/// nonce cannot pass.
///
/// # Errors
/// Returns a [`ProducerReport`] with findings if validation fails.
#[must_use]
pub(super) fn validate_producer_trace_with_nonce(
    input: &[u8],
    expected_nonce: super::challenge::ChallengeNonce,
) -> ProducerReport {
    if input.len() > MAX_PROFILE_INPUT_BYTES {
        return failed_bound();
    }
    let trace: ProducerTraceWire = match serde_json::from_slice(input) {
        Ok(trace) => trace,
        Err(_) => return failed_shape(),
    };
    let mut report = validate_typed_trace(trace);
    if !report.passed {
        return report;
    }
    // Re-parse to check nonce (validate_typed_trace consumes the trace).
    let reparse: ProducerTraceWire = match serde_json::from_slice(input) {
        Ok(trace) => trace,
        Err(_) => return failed_shape(),
    };
    match super::challenge::verify_nonce(reparse.challenge_nonce, expected_nonce) {
        super::challenge::ChallengeVerification::Verified => {}
        super::challenge::ChallengeVerification::Failed(failure) => {
            report.passed = false;
            report.findings.push(ProfileError {
                profile: "producer".to_string(),
                invariant: "challenge_nonce_binding".to_string(),
                detail: failure.code().to_string(),
            });
        }
    }
    report
}

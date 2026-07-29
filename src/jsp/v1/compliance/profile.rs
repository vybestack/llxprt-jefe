//! Fail-closed typed producer-profile validator.

use serde::{Deserialize, Serialize};

use crate::domain::observation::{ObservationEvent, ObservationIdentity};
use crate::jsp::v1::JspCode;
use crate::jsp::v1::limits::MAX_DISPLAYED_CONTENT_BYTES;

use super::dto::{
    DocumentWire, ProducerFactKind, ProducerFactWire, ProducerTraceWire, SinkKind, TypedDocument,
};
use super::harness::{FakeClock, SequenceDisposition, SequenceGenerator};
use super::reducer::ReferenceReducer;

pub(super) const MAX_PROFILE_INPUT_BYTES: usize = 1024 * 1024;
const MAX_PRODUCER_FACTS: usize = 128;
const MAX_CAPTURED_DOCUMENTS: usize = 32;
/// Maximum length for any metadata string (description, version, adapter
/// version). Prevents an ignored description from carrying megabytes of
/// unvalidated data through deserialization.
const MAX_METADATA_STRING_BYTES: usize = 4096;

/// Payload-free producer or server profile failure.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProfileError {
    /// Profile that emitted the finding.
    pub profile: String,
    /// Stable failed invariant.
    pub invariant: String,
    /// Stable payload-free diagnostic code or detail.
    pub detail: String,
}

/// Producer qualification result.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProducerReport {
    /// Adapter version reported by the trace.
    pub adapter_version: String,
    /// Number of captured JSP documents.
    pub document_count: usize,
    /// Ordered qualification findings.
    pub findings: Vec<ProfileError>,
    /// Whether every producer invariant passed.
    pub passed: bool,
}

/// Server qualification result.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServerReport {
    /// Server version reported by the transcript.
    pub server_version: String,
    /// Number of captured server interactions.
    pub interaction_count: usize,
    /// Ordered qualification findings.
    pub findings: Vec<ProfileError>,
    /// Whether every server invariant passed.
    pub passed: bool,
}

/// Validate producer qualification against the complete runner-owned challenge.
#[must_use]
pub fn validate_producer_trace_with_challenge(
    input: &[u8],
    challenge: &super::challenge::RunnerChallenge,
) -> ProducerReport {
    super::profile_challenge::validate_producer_trace_with_challenge(input, challenge)
}
/// Validate a producer trace with a runner-owned challenge nonce.
#[must_use]
pub fn validate_producer_trace_with_nonce(
    input: &[u8],
    expected_nonce: super::challenge::ChallengeNonce,
) -> ProducerReport {
    super::profile_challenge::validate_producer_trace_with_nonce(input, expected_nonce)
}

/// Validate a closed producer trace against the frozen producer profile.
#[must_use]
pub fn validate_producer_trace(input: &[u8]) -> ProducerReport {
    if input.len() > MAX_PROFILE_INPUT_BYTES {
        return failed_bound();
    }
    let trace: ProducerTraceWire = match serde_json::from_slice(input) {
        Ok(trace) => trace,
        Err(_) => return failed_shape(),
    };
    validate_typed_trace(trace)
}

pub(super) fn validate_typed_trace(trace: ProducerTraceWire) -> ProducerReport {
    let adapter_version = trace.adapter_version;
    let mut findings = Vec::new();
    if trace.schema != 1
        || trace.trace_artifact_version != super::report::COMPLIANCE_ARTIFACT_VERSION
        || trace.facts.len() > MAX_PRODUCER_FACTS
        || adapter_version.is_empty()
        || adapter_version.len() > MAX_METADATA_STRING_BYTES
        || trace.description.len() > MAX_METADATA_STRING_BYTES
    {
        finding(
            &mut findings,
            "trace_shape",
            "producer trace header invariant failed",
        );
    }
    let mut state = ProducerState::default();
    for (index, fact) in trace.facts.into_iter().enumerate() {
        check_fact(fact, index, &mut state, &mut findings);
    }
    if trace.challenge_nonce == 0 {
        state.proofs |= ProducerState::DRAFT;
    }
    finalize(&state, &mut findings);
    ProducerReport {
        adapter_version,
        document_count: state.documents.len(),
        passed: findings.is_empty(),
        findings,
    }
}

#[derive(Default)]
struct ProducerState {
    identity: Option<ObservationIdentity>,
    sequence: Option<SequenceGenerator>,
    clock: Option<FakeClock>,
    clock_pending: bool,
    documents: Vec<Vec<u8>>,
    events_seen: u16,
    proofs: u8,
    reducer: ReferenceReducer,
    process_binding_seen: bool,
}

impl ProducerState {
    const CLOCK: u8 = 1 << 0;
    const REDACTION: u8 = 1 << 1;
    const BOUND: u8 = 1 << 2;
    const NONBLOCKING: u8 = 1 << 3;
    const GAP: u8 = 1 << 4;
    const DRAFT: u8 = 1 << 5;
    const ALL_PROOFS: u8 = (1 << 6) - 1;
    const ALL_EVENTS: u16 = (1 << 11) - 1;
}

fn check_fact(
    fact: ProducerFactWire,
    index: usize,
    state: &mut ProducerState,
    findings: &mut Vec<ProfileError>,
) {
    if !fact_shape_valid(&fact) {
        malformed_fact(index, findings);
        return;
    }
    match fact.fact {
        ProducerFactKind::ClockSet => match fact.now_ms {
            Some(now_ms) => check_clock_set(now_ms, index, state, findings),
            None => malformed_fact(index, findings),
        },
        ProducerFactKind::Document => match fact.document {
            Some(document) => {
                if state.documents.len() >= MAX_CAPTURED_DOCUMENTS {
                    finding_at(
                        findings,
                        "artifact_bound",
                        index,
                        "document",
                        "captured document count exceeds bound",
                    );
                    return;
                }
                state.documents.push(document.as_bytes().to_vec());
                match document.into_typed() {
                    Ok(document) => check_document(document, index, state, findings),
                    Err(error) => finding_at(
                        findings,
                        "closed_contract",
                        index,
                        "document",
                        error.code().as_str(),
                    ),
                }
            }
            None => malformed_fact(index, findings),
        },
        ProducerFactKind::RedactionChallenge => {
            check_redaction_fact(fact, index, state, findings);
        }
        ProducerFactKind::DraftChallenge => state.proofs |= ProducerState::DRAFT,
        ProducerFactKind::BoundChallenge => check_bound_fact(fact, index, state, findings),
        ProducerFactKind::NonblockingChallenge => {
            check_nonblocking_fact(fact, index, state, findings);
        }
        ProducerFactKind::GapChallenge => check_gap_fact(fact, index, state, findings),
    }
}

fn fact_shape_valid(fact: &ProducerFactWire) -> bool {
    let clock = fact.now_ms.is_some();
    let document = fact.document.is_some();
    let redaction = fact.document_index.is_some() && fact.forbidden_marker.is_some();
    let bound = fact.at_limit.is_some() && fact.limit_plus_one.is_some();
    let nonblocking = fact.sink.is_some()
        && fact.queue_capacity.is_some()
        && fact.attempted.is_some()
        && fact.accepted.is_some()
        && fact.elapsed_ms.is_some()
        && fact.deadline_ms.is_some();
    let gap = fact.emitted_through.is_some()
        && fact.dropped_start.is_some()
        && fact.dropped_end.is_some()
        && fact.next_emitted.is_some();
    let count = [
        fact.now_ms.is_some(),
        fact.document.is_some(),
        fact.document_index.is_some(),
        fact.forbidden_marker.is_some(),
        fact.source_handle.is_some(),
        fact.at_limit.is_some(),
        fact.limit_plus_one.is_some(),
        fact.sink.is_some(),
        fact.queue_capacity.is_some(),
        fact.attempted.is_some(),
        fact.accepted.is_some(),
        fact.elapsed_ms.is_some(),
        fact.deadline_ms.is_some(),
        fact.operation_handle.is_some(),
        fact.operation_handles.is_some(),
        fact.emitted_through.is_some(),
        fact.dropped_start.is_some(),
        fact.dropped_end.is_some(),
        fact.next_emitted.is_some(),
        fact.next_publication.is_some(),
    ]
    .into_iter()
    .filter(|value| *value)
    .count();
    match fact.fact {
        ProducerFactKind::ClockSet => clock && count == 1,
        ProducerFactKind::Document => document && count == 1,
        ProducerFactKind::RedactionChallenge => redaction && matches!(count, 2 | 3),
        ProducerFactKind::DraftChallenge => fact.source_handle.is_some() && count == 1,
        ProducerFactKind::BoundChallenge => bound && count == 2,
        ProducerFactKind::NonblockingChallenge => nonblocking && matches!(count, 6 | 8),
        ProducerFactKind::GapChallenge => gap && matches!(count, 4 | 6),
    }
}

fn check_redaction_fact(
    fact: ProducerFactWire,
    index: usize,
    state: &mut ProducerState,
    findings: &mut Vec<ProfileError>,
) {
    match (fact.document_index, fact.forbidden_marker) {
        (Some(document_index), Some(marker)) => {
            check_redaction(document_index, &marker, index, state, findings);
        }
        _ => malformed_fact(index, findings),
    }
}

fn check_bound_fact(
    fact: ProducerFactWire,
    index: usize,
    state: &mut ProducerState,
    findings: &mut Vec<ProfileError>,
) {
    match (fact.at_limit, fact.limit_plus_one) {
        (Some(at_limit), Some(limit_plus_one)) => {
            check_bound(at_limit, limit_plus_one, index, state, findings);
        }
        _ => malformed_fact(index, findings),
    }
}

fn check_nonblocking_fact(
    fact: ProducerFactWire,
    index: usize,
    state: &mut ProducerState,
    findings: &mut Vec<ProfileError>,
) {
    if !matches!(fact.sink, Some(SinkKind::Blocked)) {
        malformed_fact(index, findings);
        return;
    }
    let evidence = NonblockingEvidence {
        capacity: fact.queue_capacity,
        attempted: fact.attempted,
        accepted: fact.accepted,
        elapsed_ms: fact.elapsed_ms,
        deadline_ms: fact.deadline_ms,
    };
    check_nonblocking(evidence, index, state, findings);
}

fn check_gap_fact(
    fact: ProducerFactWire,
    index: usize,
    state: &mut ProducerState,
    findings: &mut Vec<ProfileError>,
) {
    let evidence = (
        fact.emitted_through,
        fact.dropped_start,
        fact.dropped_end,
        fact.next_emitted,
    );
    check_gap(evidence, index, state, findings);
}

fn malformed_fact(index: usize, findings: &mut Vec<ProfileError>) {
    finding_at(
        findings,
        "trace_shape",
        index,
        "fact",
        "required challenge field is missing",
    );
}

fn check_clock_set(
    now_ms: u64,
    index: usize,
    state: &mut ProducerState,
    findings: &mut Vec<ProfileError>,
) {
    let result = if let Some(clock) = &mut state.clock {
        clock.set_ms(now_ms)
    } else {
        state.clock = Some(FakeClock::new(now_ms));
        Ok(())
    };
    if result.is_err() || state.clock_pending {
        finding_at(
            findings,
            "timestamp_provenance",
            index,
            "clock_challenge",
            "monotonic unused clock sample",
        );
    }
    state.clock_pending = true;
}

fn check_document(
    document: TypedDocument,
    index: usize,
    state: &mut ProducerState,
    findings: &mut Vec<ProfileError>,
) {
    let first = state.documents.len() == 1;
    let observed_ms = match &document {
        TypedDocument::Snapshot(snapshot) => snapshot.bridge_observed_ms,
        TypedDocument::Event(event) => event.bridge_observed_ms,
        TypedDocument::Heartbeat(heartbeat) => heartbeat.bridge_observed_ms,
    };
    if !state.clock_pending || state.clock.map(FakeClock::now_ms) != Some(observed_ms) {
        finding_at(
            findings,
            "timestamp_provenance",
            index,
            document_kind(&document),
            "clock response mismatch",
        );
    } else {
        state.proofs |= ProducerState::CLOCK;
    }
    state.clock_pending = false;
    match document {
        TypedDocument::Snapshot(snapshot) => {
            check_snapshot(*snapshot, first, index, state, findings);
        }
        TypedDocument::Event(event) => check_event(event, first, index, state, findings),
        TypedDocument::Heartbeat(heartbeat) => {
            check_not_first(first, index, findings);
            check_identity(&heartbeat.identity, index, "heartbeat", state, findings);
        }
    }
}

fn check_snapshot(
    snapshot: crate::jsp::Snapshot,
    first: bool,
    index: usize,
    state: &mut ProducerState,
    findings: &mut Vec<ProfileError>,
) {
    if !first || state.identity.is_some() {
        finding_at(
            findings,
            "first_snapshot",
            index,
            "snapshot",
            "launch snapshot ordering failed",
        );
        return;
    }
    state.process_binding_seen = !matches!(
        snapshot.process_binding,
        crate::domain::observation::FieldState::Unsupported
    );
    state.reducer.apply_snapshot(&snapshot);
    state.sequence = Some(SequenceGenerator::after_cursor(snapshot.cursor));
    state.identity = Some(snapshot.identity);
}

fn check_event(
    event: crate::domain::observation::EventRecord,
    first: bool,
    index: usize,
    state: &mut ProducerState,
    findings: &mut Vec<ProfileError>,
) {
    check_not_first(first, index, findings);
    check_identity(&event.identity, index, "event", state, findings);
    if state
        .sequence
        .as_mut()
        .map(|sequence| sequence.apply(event.source_sequence))
        != Some(SequenceDisposition::Applied)
    {
        finding_at(
            findings,
            "cursor_contiguous_sequence",
            index,
            "event",
            "expected exact next sequence",
        );
    }
    if matches!(&event.event, ObservationEvent::AssistantMessageDisplayed { message } if message.committed_ms > event.bridge_observed_ms)
    {
        finding_at(
            findings,
            "timestamp_provenance",
            index,
            "event",
            "commit timestamp exceeds observation timestamp",
        );
    }
    if let Err(error) = state.reducer.apply_event(&event) {
        finding_at(
            findings,
            "legal_transition_semantics",
            index,
            "event",
            error.code(),
        );
    }
    state.events_seen |= event_bit(&event.event);
}

fn check_redaction(
    document_index: usize,
    marker: &str,
    index: usize,
    state: &mut ProducerState,
    findings: &mut Vec<ProfileError>,
) {
    let valid = !marker.is_empty()
        && state
            .documents
            .get(document_index)
            .is_some_and(|document| !contains_bytes(document, marker.as_bytes()));
    if valid {
        state.proofs |= ProducerState::REDACTION;
    } else {
        finding_at(
            findings,
            "source_redaction",
            index,
            "challenge",
            "captured document contains forbidden marker or index is invalid",
        );
    }
}

struct NonblockingEvidence {
    capacity: Option<u64>,
    attempted: Option<u64>,
    accepted: Option<u64>,
    elapsed_ms: Option<u64>,
    deadline_ms: Option<u64>,
}

fn check_bound(
    at_limit: DocumentWire,
    limit_plus_one: DocumentWire,
    index: usize,
    state: &mut ProducerState,
    findings: &mut Vec<ProfileError>,
) {
    let at_limit_valid = matches!(at_limit.into_typed(), Ok(TypedDocument::Event(event)) if matches!(&event.event, ObservationEvent::AssistantMessageDisplayed { message } if message.content.as_str().len() == MAX_DISPLAYED_CONTENT_BYTES));
    let overflow_valid = limit_plus_one
        .into_typed()
        .is_err_and(|error| error.code() == JspCode::EBound);
    if at_limit_valid && overflow_valid {
        state.proofs |= ProducerState::BOUND;
    } else {
        finding_at(
            findings,
            "payload_bounds",
            index,
            "event",
            "exact bound/limit-plus-one parser challenge failed",
        );
    }
}

fn check_nonblocking(
    evidence: NonblockingEvidence,
    index: usize,
    state: &mut ProducerState,
    findings: &mut Vec<ProfileError>,
) {
    let (Some(capacity), Some(attempted), Some(accepted), Some(elapsed_ms), Some(deadline_ms)) = (
        evidence.capacity,
        evidence.attempted,
        evidence.accepted,
        evidence.elapsed_ms,
        evidence.deadline_ms,
    ) else {
        malformed_fact(index, findings);
        return;
    };
    if capacity > 0
        && attempted == capacity.saturating_add(1)
        && accepted == capacity
        && deadline_ms > 0
        && deadline_ms <= 1_000
        && elapsed_ms < deadline_ms
    {
        state.proofs |= ProducerState::NONBLOCKING;
    } else {
        finding_at(
            findings,
            "nonblocking_publication",
            index,
            "challenge",
            "blocked-sink measurement is inconsistent",
        );
    }
}

fn check_gap(
    evidence: (Option<u64>, Option<u64>, Option<u64>, Option<u64>),
    index: usize,
    state: &mut ProducerState,
    findings: &mut Vec<ProfileError>,
) {
    let (Some(emitted), Some(start), Some(end), Some(next)) = evidence else {
        malformed_fact(index, findings);
        return;
    };
    let last = state.sequence.map(SequenceGenerator::last_applied);
    if last == Some(emitted)
        && start == emitted.saturating_add(1)
        && end >= start
        && next == end.saturating_add(1)
    {
        state.proofs |= ProducerState::GAP;
    } else {
        finding_at(
            findings,
            "nonblocking_gap_signaling",
            index,
            "challenge",
            "emitted/dropped range is inconsistent with captured sequence",
        );
    }
}

fn check_identity(
    identity: &ObservationIdentity,
    index: usize,
    kind: &str,
    state: &ProducerState,
    findings: &mut Vec<ProfileError>,
) {
    if state.identity.as_ref() != Some(identity) {
        finding_at(
            findings,
            "launch_identity",
            index,
            kind,
            "identity differs from launch binding",
        );
    }
}

fn check_not_first(first: bool, index: usize, findings: &mut Vec<ProfileError>) {
    if first {
        finding_at(
            findings,
            "first_snapshot",
            index,
            "document",
            "first document is not a snapshot",
        );
    }
}

fn document_kind(document: &TypedDocument) -> &'static str {
    match document {
        TypedDocument::Snapshot(_) => "snapshot",
        TypedDocument::Event(_) => "event",
        TypedDocument::Heartbeat(_) => "heartbeat",
    }
}

fn event_bit(event: &ObservationEvent) -> u16 {
    match event {
        ObservationEvent::ActivityChanged { .. } => 1 << 0,
        ObservationEvent::WaitOpened { .. } => 1 << 1,
        ObservationEvent::WaitResolved => 1 << 2,
        ObservationEvent::TurnStarted => 1 << 3,
        ObservationEvent::TurnEnded { .. } => 1 << 4,
        ObservationEvent::TodosReplaced { .. } => 1 << 5,
        ObservationEvent::ToolCallCreated { .. } => 1 << 6,
        ObservationEvent::ToolCallPhaseChanged { .. } => 1 << 7,
        ObservationEvent::AssistantMessageDisplayed { .. } => 1 << 8,
        ObservationEvent::SourceError { .. } => 1 << 9,
        ObservationEvent::SessionEnded => 1 << 10,
    }
}

pub(super) fn contains_bytes(haystack: &[u8], needle: &[u8]) -> bool {
    haystack
        .windows(needle.len())
        .any(|window| window == needle)
}

fn finalize(state: &ProducerState, findings: &mut Vec<ProfileError>) {
    if state.identity.is_none() {
        finding(
            findings,
            "first_snapshot",
            "producer trace has no launch snapshot",
        );
    }
    if !state.process_binding_seen {
        finding(
            findings,
            "launch_process_binding",
            "producer launch lacks process binding evidence",
        );
    }
    if state.clock_pending {
        finding(
            findings,
            "timestamp_provenance",
            "final clock challenge has no document response",
        );
    }
    if state.events_seen != ProducerState::ALL_EVENTS {
        finding(
            findings,
            "explicit_transitions",
            "producer trace does not capture all 11 event kinds",
        );
    }
    for (bit, invariant) in [
        (ProducerState::CLOCK, "timestamp_provenance"),
        (ProducerState::REDACTION, "source_redaction"),
        (ProducerState::BOUND, "payload_bounds"),
        (ProducerState::NONBLOCKING, "nonblocking_publication"),
        (ProducerState::GAP, "nonblocking_gap_signaling"),
        (ProducerState::DRAFT, "draft_exclusion"),
    ] {
        if state.proofs & bit == 0 {
            finding(
                findings,
                invariant,
                "required observable challenge is missing",
            );
        }
    }
    if state.proofs != ProducerState::ALL_PROOFS {
        finding(
            findings,
            "challenge_completeness",
            "producer evidence set is incomplete",
        );
    }
}

fn finding_at(
    findings: &mut Vec<ProfileError>,
    invariant: &str,
    index: usize,
    kind: &str,
    detail: &str,
) {
    finding(
        findings,
        invariant,
        &format!("fact[{index}] {kind}: {detail}"),
    );
}

fn finding(findings: &mut Vec<ProfileError>, invariant: &str, detail: &str) {
    findings.push(ProfileError {
        profile: "producer".to_string(),
        invariant: invariant.to_string(),
        detail: detail.to_string(),
    });
}

pub(super) fn failed_bound() -> ProducerReport {
    ProducerReport {
        adapter_version: "unknown".to_string(),
        document_count: 0,
        findings: vec![ProfileError {
            profile: "producer".to_string(),
            invariant: "artifact_bound".to_string(),
            detail: "producer input exceeds compliance artifact bound".to_string(),
        }],
        passed: false,
    }
}

pub(super) fn failed_shape() -> ProducerReport {
    ProducerReport {
        adapter_version: "unknown".to_string(),
        document_count: 0,
        findings: vec![ProfileError {
            profile: "producer".to_string(),
            invariant: "trace_shape".to_string(),
            detail: "producer trace closed-shape parsing failed".to_string(),
        }],
        passed: false,
    }
}

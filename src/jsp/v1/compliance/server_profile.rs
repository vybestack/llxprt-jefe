//! Fail-closed typed normalized HTTP/SSE server transcript validator.

use super::dto::{
    ActivityValueWire, RejectionReasonWire, ResponseBodyWire, ResponseKindWire, RoleWire,
    RouteWire, ServerResponseWire, ServerTranscriptWire,
};
use super::harness::FakeClock;
use super::profile::{ProfileError, ServerReport};
use super::projection::{ActivityProjection, NormalizedProjection, ObservationHealth};
use super::reducer::ReferenceReducer;
use super::server_profile_request::check_interaction;
use crate::domain::observation::ObservationIdentity;

/// Maximum length for any metadata string (description, version).
/// Prevents an ignored description from carrying megabytes of unvalidated
/// data through deserialization.
const MAX_METADATA_STRING_BYTES: usize = 4096;

/// Maximum size of a serialized server transcript before it is rejected as
/// exceeding the compliance artifact bound. Shared by all public entry points
/// so the bound is enforced uniformly before deserialization.
const MAX_SERVER_TRANSCRIPT_BYTES: usize = 2 * 1024 * 1024;

/// Validate server qualification against the complete runner-owned challenge.
#[must_use]
pub fn validate_server_transcript_with_challenge(
    input: &[u8],
    challenge: &super::challenge::RunnerChallenge,
) -> ServerReport {
    if input.len() > MAX_SERVER_TRANSCRIPT_BYTES {
        return failed_bound();
    }
    let transcript: ServerTranscriptWire = match serde_json::from_slice(input) {
        Ok(transcript) => transcript,
        Err(_) => return failed_shape(),
    };
    if challenge.kind != super::challenge::AdapterKind::Server || !challenge.structurally_valid() {
        return challenge_failure("challenge_shape", "JSP-C-CHALLENGE-SHAPE");
    }
    let nonce_matches = transcript.challenge_nonce == challenge.nonce;
    let version_matches = transcript.server_version == challenge.adapter_version
        && transcript.server_version == super::challenge::SERVER_ADAPTER_VERSION;
    let credentials = challenge_credentials(challenge);
    let mut report = validate_typed_with_credentials(transcript, credentials, true);
    if !nonce_matches {
        server_challenge_finding(
            &mut report,
            "challenge_nonce_binding",
            "JSP-C-NONCE-MISMATCH",
        );
    }
    if !version_matches {
        server_challenge_finding(&mut report, "adapter_identity", "JSP-C-ADAPTER-VERSION");
    }
    report.passed = report.findings.is_empty();
    report
}

fn challenge_credentials(challenge: &super::challenge::RunnerChallenge) -> Vec<TrustedCredential> {
    challenge
        .trusted_credentials
        .iter()
        .filter_map(|credential| {
            let binding = crate::jsp::v1::validate::build_event_identity(
                &credential.identity.agent_id,
                credential.identity.lifecycle_generation,
                &credential.identity.source_epoch,
            )
            .ok()?;
            let role = match credential.role {
                super::challenge::ChallengeRole::Publisher => RoleWire::Publisher,
                super::challenge::ChallengeRole::Observer => RoleWire::Observer,
                super::challenge::ChallengeRole::Server => RoleWire::Server,
            };
            Some(TrustedCredential {
                credential_handle: credential.credential_handle.clone(),
                principal_handle: credential.principal_handle.clone(),
                role,
                binding: Some(binding),
            })
        })
        .collect()
}

fn challenge_failure(invariant: &str, detail: &str) -> ServerReport {
    let mut report = failed_shape();
    report.findings.clear();
    server_challenge_finding(&mut report, invariant, detail);
    report
}

fn server_challenge_finding(report: &mut ServerReport, invariant: &str, detail: &str) {
    report.findings.push(ProfileError {
        profile: "server".to_string(),
        invariant: invariant.to_string(),
        detail: detail.to_string(),
    });
    report.passed = false;
}

/// Validate a server transcript with a runner-owned challenge nonce.
///
/// The transcript's `challenge_nonce` must match the runner-supplied nonce.
/// This replaces the replayable self-attested model: the observed result
/// must bind to the runner's nonce, so a replayed transcript from a different
/// nonce cannot pass.
///
/// # Errors
/// Returns a [`ServerReport`] with findings if validation fails.
#[must_use]
pub fn validate_server_transcript_with_nonce(
    input: &[u8],
    expected_nonce: super::challenge::ChallengeNonce,
) -> ServerReport {
    if input.len() > MAX_SERVER_TRANSCRIPT_BYTES {
        return failed_bound();
    }
    let transcript: ServerTranscriptWire = match serde_json::from_slice(input) {
        Ok(transcript) => transcript,
        Err(_) => return failed_shape(),
    };
    let nonce = transcript.challenge_nonce;
    let mut report = validate_typed(transcript);
    if !report.passed {
        return report;
    }
    match super::challenge::verify_nonce(nonce, expected_nonce) {
        super::challenge::ChallengeVerification::Verified => {}
        super::challenge::ChallengeVerification::Failed(failure) => {
            report.passed = false;
            report.findings.push(ProfileError {
                profile: "server".to_string(),
                invariant: "challenge_nonce_binding".to_string(),
                detail: failure.code().to_string(),
            });
        }
    }
    report
}

#[must_use]
pub fn validate_server_transcript(input: &[u8]) -> ServerReport {
    if input.len() > MAX_SERVER_TRANSCRIPT_BYTES {
        return failed_bound();
    }
    let transcript: ServerTranscriptWire = match serde_json::from_slice(input) {
        Ok(transcript) => transcript,
        Err(_) => return failed_shape(),
    };
    validate_typed(transcript)
}

fn validate_typed(transcript: ServerTranscriptWire) -> ServerReport {
    validate_typed_with_credentials(transcript, trusted_credentials(), false)
}

fn validate_typed_with_credentials(
    transcript: ServerTranscriptWire,
    credentials: Vec<TrustedCredential>,
    strict_challenge: bool,
) -> ServerReport {
    let mut findings = Vec::new();
    let mut state = ServerState::new(credentials, strict_challenge);
    if transcript.schema != 1
        || transcript.transcript_artifact_version != super::report::COMPLIANCE_ARTIFACT_VERSION
        || transcript.description.len() > MAX_METADATA_STRING_BYTES
        || transcript.server_version.len() > MAX_METADATA_STRING_BYTES
    {
        finding(
            &mut findings,
            "transcript_shape",
            "server transcript header invariant failed",
        );
    }
    let count = transcript.interactions.len();
    for (index, interaction) in transcript.interactions.into_iter().enumerate() {
        check_interaction(interaction, index, &mut state, &mut findings);
    }
    state.finalize(&mut findings);
    ServerReport {
        server_version: transcript.server_version,
        interaction_count: count,
        passed: findings.is_empty(),
        findings,
    }
}

pub(super) struct TrustedCredential {
    pub(super) credential_handle: String,
    pub(super) principal_handle: String,
    pub(super) role: RoleWire,
    pub(super) binding: Option<ObservationIdentity>,
}

fn trusted_credentials() -> Vec<TrustedCredential> {
    let publisher = crate::jsp::v1::validate::build_event_identity("srv-1", 2, "srv-epoch-1").ok();
    let other = crate::jsp::v1::validate::build_event_identity("srv-other", 2, "other-epoch").ok();
    vec![
        TrustedCredential {
            credential_handle: "publisher-credential".to_string(),
            principal_handle: "publisher-principal".to_string(),
            role: RoleWire::Publisher,
            binding: publisher.clone(),
        },
        TrustedCredential {
            credential_handle: "publisher-other".to_string(),
            principal_handle: "publisher-other-principal".to_string(),
            role: RoleWire::Publisher,
            binding: other,
        },
        TrustedCredential {
            credential_handle: "observer-credential".to_string(),
            principal_handle: "observer-principal".to_string(),
            role: RoleWire::Observer,
            binding: publisher.clone(),
        },
        TrustedCredential {
            credential_handle: "server-credential".to_string(),
            principal_handle: "server-principal".to_string(),
            role: RoleWire::Server,
            binding: publisher,
        },
    ]
}

pub(super) struct ServerState {
    pub(super) credentials: Vec<TrustedCredential>,
    pub(super) registered: Option<ObservationIdentity>,
    pub(super) reducer: ReferenceReducer,
    pub(super) last_heartbeat_ms: Option<u64>,
    pub(super) clock: Option<FakeClock>,
    pub(super) pending_observations: Vec<NormalizedProjection>,
    pub(super) strict_challenge: bool,
    pub(super) accepted_states: Vec<NormalizedProjection>,
    pub(super) last_observed_projection: Option<NormalizedProjection>,
    pub(super) proved: u32,
}

impl ServerState {
    pub(super) const REGISTER: u32 = 1 << 0;
    pub(super) const SNAPSHOT: u32 = 1 << 1;
    pub(super) const EVENT: u32 = 1 << 2;
    pub(super) const DUPLICATE: u32 = 1 << 3;
    pub(super) const OUT_OF_ORDER: u32 = 1 << 4;
    pub(super) const GAP_FRESH: u32 = 1 << 5;
    pub(super) const UNRELATED: u32 = 1 << 6;
    pub(super) const STALE_GENERATION: u32 = 1 << 7;
    pub(super) const STALE_EPOCH: u32 = 1 << 8;
    pub(super) const OBSERVER_PUBLISH: u32 = 1 << 9;
    pub(super) const PUBLISHER_OBSERVE: u32 = 1 << 10;
    pub(super) const OBSERVER_CONTROL: u32 = 1 << 11;
    pub(super) const STREAM: u32 = 1 << 12;
    pub(super) const HEARTBEAT: u32 = 1 << 13;
    pub(super) const LEASE: u32 = 1 << 14;
    pub(super) const BOUND: u32 = 1 << 15;
    pub(super) const UNKNOWN_AUTH: u32 = 1 << 16;
    pub(super) const STREAM_TAIL: u32 = 1 << 17;
    pub(super) const STREAM_SNAPSHOT_ONLY: u32 = 1 << 18;
    pub(super) const ALL: u32 = (1 << 19) - 1;
    /// Required proof mask for the non-strict (self-attested) profile: every
    /// semantic up to and including `BOUND`. The strict-challenge-only
    /// proofs (`UNKNOWN_AUTH` and the stream-tail/snapshot-only distinctions)
    /// are not required here.
    pub(super) const NON_STRICT_REQUIRED: u32 = Self::UNKNOWN_AUTH - 1;

    fn new(credentials: Vec<TrustedCredential>, strict_challenge: bool) -> Self {
        Self {
            credentials,
            registered: None,
            reducer: ReferenceReducer::new(),
            last_heartbeat_ms: None,
            clock: None,
            pending_observations: Vec::new(),
            strict_challenge,
            accepted_states: Vec::new(),
            last_observed_projection: None,
            proved: 0,
        }
    }

    fn finalize(&self, findings: &mut Vec<ProfileError>) {
        let required = if self.strict_challenge {
            Self::ALL
        } else {
            Self::NON_STRICT_REQUIRED
        };
        if self.proved & required != required {
            finding(
                findings,
                "required_semantics",
                "server transcript evidence set is incomplete",
            );
        }
        if !self.pending_observations.is_empty() {
            finding(
                findings,
                "rejection_no_mutation",
                "rejection lacks a subsequent canonical observation",
            );
        }
    }
}

pub(super) fn check_forbidden(
    response: Option<ServerResponseWire>,
    index: usize,
    route: RouteWire,
    state: &mut ServerState,
    findings: &mut Vec<ProfileError>,
) {
    let before = state.reducer.projection();
    if response_matches(response.as_ref(), 403, ResponseKindWire::ForbiddenRole) {
        state.proved |= match route {
            RouteWire::Publish => ServerState::OBSERVER_PUBLISH,
            RouteWire::Observe => ServerState::PUBLISHER_OBSERVE,
            RouteWire::Control => ServerState::OBSERVER_CONTROL,
            _ => 0,
        };
        pending_unchanged(state, before);
    } else {
        finding_at(
            findings,
            "role_separation",
            index,
            "request",
            "credential role response mismatch",
        );
    }
}

pub(super) fn reject_unchanged(
    state: &mut ServerState,
    findings: &mut Vec<ProfileError>,
    index: usize,
    invariant: &str,
    detail: &str,
) {
    pending_unchanged(state, state.reducer.projection());
    finding_at(findings, invariant, index, "document", detail);
}

pub(super) fn pending_unchanged(state: &mut ServerState, before: NormalizedProjection) {
    if state.pending_observations.is_empty() {
        state.pending_observations.push(before);
    }
}

pub(super) fn native_state_equal_except_health(
    left: &NormalizedProjection,
    right: &NormalizedProjection,
) -> bool {
    let mut left = left.clone();
    let mut right = right.clone();
    left.observation_health = ObservationHealth::Live;
    right.observation_health = ObservationHealth::Live;
    left == right
}

pub(super) fn activity_value(activity: ActivityProjection) -> ActivityValueWire {
    match activity {
        ActivityProjection::Idle => ActivityValueWire::Idle,
        ActivityProjection::Thinking => ActivityValueWire::Thinking,
        ActivityProjection::Acting => ActivityValueWire::Acting,
        ActivityProjection::Unsupported => ActivityValueWire::Unsupported,
        ActivityProjection::Unknown => ActivityValueWire::Unknown,
        ActivityProjection::Degraded => ActivityValueWire::Degraded,
    }
}

pub(super) fn response_matches(
    response: Option<&ServerResponseWire>,
    status: u16,
    kind: ResponseKindWire,
) -> bool {
    response.is_some_and(|response| {
        response.status == status
            && response.kind == kind
            && (response.body.is_none() || status >= 400)
    })
}

pub(super) fn rejection_reason(
    response: Option<&ServerResponseWire>,
) -> Option<RejectionReasonWire> {
    match response?.body.as_ref()? {
        ResponseBodyWire::Rejection(rejection) => Some(rejection.reason),
        ResponseBodyWire::Binding(_) | ResponseBodyWire::Health(_) => None,
    }
}

pub(super) fn finding_at(
    findings: &mut Vec<ProfileError>,
    invariant: &str,
    index: usize,
    kind: &str,
    detail: &str,
) {
    finding(
        findings,
        invariant,
        &format!("interaction[{index}] {kind}: {detail}"),
    );
}

fn finding(findings: &mut Vec<ProfileError>, invariant: &str, detail: &str) {
    findings.push(ProfileError {
        profile: "server".to_string(),
        invariant: invariant.to_string(),
        detail: detail.to_string(),
    });
}

fn failed_shape() -> ServerReport {
    ServerReport {
        server_version: "unknown".to_string(),
        interaction_count: 0,
        findings: vec![ProfileError {
            profile: "server".to_string(),
            invariant: "transcript_shape".to_string(),
            detail: "server transcript closed-shape parsing failed".to_string(),
        }],
        passed: false,
    }
}

fn failed_bound() -> ServerReport {
    ServerReport {
        server_version: "unknown".to_string(),
        interaction_count: 0,
        findings: vec![ProfileError {
            profile: "server".to_string(),
            invariant: "artifact_bound".to_string(),
            detail: "server input exceeds compliance artifact bound".to_string(),
        }],
        passed: false,
    }
}

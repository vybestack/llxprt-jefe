//! Runner-owned, language-neutral executable qualification challenges.
//!
//! A producer or server response is qualification evidence only when it is the
//! response to one complete challenge supplied on the invoked adapter's stdin.
//! Captured fixtures remain useful corpus artifacts, but are not executable
//! qualification evidence.

use serde::{Deserialize, Serialize};

use crate::domain::observation::{FieldState, ObservationIdentity, ProcessBinding};

/// Version reported by the checked reference producer adapter.
pub const PRODUCER_ADAPTER_VERSION: &str = "jefe-jsp-producer-reference/1";
/// Version reported by the checked reference server adapter.
pub const SERVER_ADAPTER_VERSION: &str = "jefe-jsp-server-reference/1";

/// A runner-supplied challenge nonce.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ChallengeNonce(pub u64);

impl ChallengeNonce {
    #[must_use]
    pub const fn value(self) -> u64 {
        self.0
    }
}

/// Complete subprocess challenge. Every nested object is closed so adapters in
/// any language receive the same trust inventory and operation schedule.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RunnerChallenge {
    pub schema: u64,
    pub kind: AdapterKind,
    pub nonce: u64,
    pub adapter_version: String,
    pub launch: LaunchChallengeWire,
    pub redaction: SourceChallenge,
    pub draft: SourceChallenge,
    pub clock_sequence: Vec<u64>,
    pub sink: SinkChallengeWire,
    pub gap: GapChallengeWire,
    pub trusted_credentials: Vec<CredentialChallenge>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AdapterKind {
    Producer,
    Server,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct IdentityChallenge {
    pub agent_id: String,
    pub lifecycle_generation: u64,
    pub source_epoch: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LaunchChallengeWire {
    pub identity: IdentityChallenge,
    pub pid: u32,
    pub started_at_ms: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SourceChallenge {
    pub source_handle: String,
    pub source: String,
    pub marker: String,
    pub document_index: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SinkChallengeWire {
    pub operation_handle: String,
    pub capacity: u64,
    pub deadline_ms: u64,
    pub operations: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GapChallengeWire {
    pub operation_handle: String,
    pub emitted_through: u64,
    pub dropped_start: u64,
    pub dropped_end: u64,
    pub next_emitted: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CredentialChallenge {
    pub credential_handle: String,
    pub principal_handle: String,
    pub role: ChallengeRole,
    pub identity: IdentityChallenge,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ChallengeRole {
    Publisher,
    Observer,
    Server,
}

impl RunnerChallenge {
    /// Build the deterministic checked-self-test challenge for a nonce.
    #[must_use]
    pub fn reference(kind: AdapterKind, nonce: u64) -> Self {
        let suffix = nonce.to_string();
        let identity = IdentityChallenge {
            agent_id: format!("agent-reference-{suffix}"),
            lifecycle_generation: 7,
            source_epoch: format!("epoch-reference-{suffix}"),
        };
        let trusted_credentials = reference_credentials(&identity, &suffix);
        Self {
            schema: 1,
            kind,
            nonce,
            adapter_version: match kind {
                AdapterKind::Producer => PRODUCER_ADAPTER_VERSION,
                AdapterKind::Server => SERVER_ADAPTER_VERSION,
            }
            .to_string(),
            launch: LaunchChallengeWire {
                identity: identity.clone(),
                pid: 4_242,
                started_at_ms: 10_000 + nonce % 1_000,
            },
            redaction: SourceChallenge {
                source_handle: format!("source-redaction-{suffix}"),
                source: format!("private JSP-REDACT-{suffix} source"),
                marker: format!("JSP-REDACT-{suffix}"),
                document_index: 0,
            },
            draft: SourceChallenge {
                source_handle: format!("source-draft-{suffix}"),
                source: format!("uncommitted JSP-DRAFT-{suffix} source"),
                marker: format!("JSP-DRAFT-{suffix}"),
                document_index: 11,
            },
            clock_sequence: (0..13).map(|offset| 1_000 + offset * 100).collect(),
            sink: SinkChallengeWire {
                operation_handle: format!("sink-{suffix}"),
                capacity: 3,
                deadline_ms: 250,
                operations: (0..4)
                    .map(|index| format!("sink-{suffix}-{index}"))
                    .collect(),
            },
            gap: GapChallengeWire {
                operation_handle: format!("gap-{suffix}"),
                emitted_through: 11,
                dropped_start: 12,
                dropped_end: 13,
                next_emitted: 14,
            },
            trusted_credentials,
        }
    }

    #[must_use]
    pub fn structurally_valid(&self) -> bool {
        self.schema == 1
            && !self.adapter_version.is_empty()
            && !self.launch.identity.agent_id.is_empty()
            && self.launch.identity.lifecycle_generation > 0
            && !self.launch.identity.source_epoch.is_empty()
            && self.launch.pid > 0
            && source_valid(&self.redaction)
            && source_valid(&self.draft)
            && self.clock_sequence.len() == 13
            && self.clock_sequence.windows(2).all(|pair| pair[1] > pair[0])
            && self.sink.capacity > 0
            && self.sink.deadline_ms > 0
            && u64::try_from(self.sink.operations.len()).ok()
                == Some(self.sink.capacity.saturating_add(1))
            && unique_nonempty(&self.sink.operations)
            && self.gap.dropped_start == self.gap.emitted_through.saturating_add(1)
            && self.gap.dropped_end >= self.gap.dropped_start
            && self.gap.next_emitted == self.gap.dropped_end.saturating_add(1)
            && credentials_valid(&self.trusted_credentials)
    }
}

fn source_valid(source: &SourceChallenge) -> bool {
    !source.source_handle.is_empty()
        && !source.marker.is_empty()
        && source
            .source
            .as_bytes()
            .windows(source.marker.len())
            .any(|bytes| bytes == source.marker.as_bytes())
}

fn unique_nonempty(values: &[String]) -> bool {
    values.iter().enumerate().all(|(index, value)| {
        !value.is_empty() && values[..index].iter().all(|prior| prior != value)
    })
}

fn credentials_valid(credentials: &[CredentialChallenge]) -> bool {
    credentials.len() >= 6
        && credentials.iter().enumerate().all(|(index, credential)| {
            !credential.credential_handle.is_empty()
                && !credential.principal_handle.is_empty()
                && !credential.identity.agent_id.is_empty()
                && credential.identity.lifecycle_generation > 0
                && !credential.identity.source_epoch.is_empty()
                && credentials[..index].iter().all(|prior| {
                    prior.credential_handle != credential.credential_handle
                        && prior.principal_handle != credential.principal_handle
                })
        })
        && credentials
            .iter()
            .any(|value| value.role == ChallengeRole::Publisher)
        && credentials
            .iter()
            .any(|value| value.role == ChallengeRole::Observer)
        && credentials
            .iter()
            .any(|value| value.role == ChallengeRole::Server)
}

fn reference_credentials(identity: &IdentityChallenge, suffix: &str) -> Vec<CredentialChallenge> {
    let stale_generation = IdentityChallenge {
        lifecycle_generation: identity.lifecycle_generation.saturating_sub(1),
        ..identity.clone()
    };
    let stale_epoch = IdentityChallenge {
        source_epoch: format!("epoch-stale-{suffix}"),
        ..identity.clone()
    };
    let unrelated = IdentityChallenge {
        agent_id: format!("agent-unrelated-{suffix}"),
        source_epoch: format!("epoch-unrelated-{suffix}"),
        ..identity.clone()
    };
    let credential = |label: &str, role, binding: &IdentityChallenge| CredentialChallenge {
        credential_handle: format!("credential-{label}-{suffix}"),
        principal_handle: format!("principal-{label}-{suffix}"),
        role,
        identity: binding.clone(),
    };
    vec![
        credential("publisher", ChallengeRole::Publisher, identity),
        credential(
            "stale-generation",
            ChallengeRole::Publisher,
            &stale_generation,
        ),
        credential("stale-epoch", ChallengeRole::Publisher, &stale_epoch),
        credential("unrelated", ChallengeRole::Publisher, &unrelated),
        credential("observer", ChallengeRole::Observer, identity),
        credential("server", ChallengeRole::Server, identity),
    ]
}

/// Backward-compatible focused launch challenge used by unit-level oracles.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LaunchChallenge {
    pub identity: ObservationIdentity,
    pub process_binding: ProcessBinding,
    pub epoch: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RedactionChallenge {
    pub document_index: usize,
    pub marker: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ClockChallenge {
    pub timestamps: Vec<u64>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SinkChallenge {
    pub capacity: u64,
    pub deadline_ms: u64,
    pub operations: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GapChallenge {
    pub emitted_through: u64,
    pub dropped_start: u64,
    pub dropped_end: u64,
    pub next_emitted: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ChallengeVerification {
    Verified,
    Failed(ChallengeFailure),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChallengeFailure {
    NonceMismatch,
    NoProcessBinding,
    BindingMismatch,
    EpochMismatch,
    MarkerPresent,
    ClockMismatch,
    QueueArithmetic,
    GapNotCaptured,
    DraftLeaked,
}

impl ChallengeFailure {
    #[must_use]
    pub const fn code(self) -> &'static str {
        match self {
            Self::NonceMismatch => "JSP-C-NONCE-MISMATCH",
            Self::NoProcessBinding => "JSP-C-NO-PROCESS-BINDING",
            Self::BindingMismatch => "JSP-C-BINDING-MISMATCH",
            Self::EpochMismatch => "JSP-C-EPOCH-MISMATCH",
            Self::MarkerPresent => "JSP-C-MARKER-PRESENT",
            Self::ClockMismatch => "JSP-C-CLOCK-MISMATCH",
            Self::QueueArithmetic => "JSP-C-QUEUE-ARITHMETIC",
            Self::GapNotCaptured => "JSP-C-GAP-NOT-CAPTURED",
            Self::DraftLeaked => "JSP-C-DRAFT-LEAKED",
        }
    }
}

#[must_use]
pub fn verify_nonce(observed: u64, expected: ChallengeNonce) -> ChallengeVerification {
    if observed == expected.value() {
        ChallengeVerification::Verified
    } else {
        ChallengeVerification::Failed(ChallengeFailure::NonceMismatch)
    }
}

#[must_use]
pub fn verify_launch(
    observed_binding: &FieldState<ProcessBinding>,
    observed_identity: &ObservationIdentity,
    challenge: &LaunchChallenge,
) -> ChallengeVerification {
    let FieldState::Supported { availability, .. } = observed_binding else {
        return ChallengeVerification::Failed(ChallengeFailure::NoProcessBinding);
    };
    let crate::domain::observation::Availability::Known(binding) = availability else {
        return ChallengeVerification::Failed(ChallengeFailure::NoProcessBinding);
    };
    if binding != &challenge.process_binding || observed_identity != &challenge.identity {
        return ChallengeVerification::Failed(ChallengeFailure::BindingMismatch);
    }
    if observed_identity.lifecycle_generation != challenge.epoch {
        return ChallengeVerification::Failed(ChallengeFailure::EpochMismatch);
    }
    ChallengeVerification::Verified
}

#[must_use]
pub fn verify_redaction(
    document_bytes: &[u8],
    challenge: &RedactionChallenge,
) -> ChallengeVerification {
    if contains_bytes(document_bytes, challenge.marker.as_bytes()) {
        ChallengeVerification::Failed(ChallengeFailure::MarkerPresent)
    } else {
        ChallengeVerification::Verified
    }
}

#[must_use]
pub fn verify_clock(challenge: &ClockChallenge) -> ChallengeVerification {
    if challenge
        .timestamps
        .windows(2)
        .all(|pair| pair[1] > pair[0])
    {
        ChallengeVerification::Verified
    } else {
        ChallengeVerification::Failed(ChallengeFailure::ClockMismatch)
    }
}

#[must_use]
pub fn verify_sink(
    challenge: &SinkChallenge,
    attempted: u64,
    accepted: u64,
) -> ChallengeVerification {
    if challenge.capacity > 0
        && challenge.deadline_ms > 0
        && challenge.operations == challenge.capacity.saturating_add(1)
        && attempted == challenge.operations
        && accepted == challenge.capacity
    {
        ChallengeVerification::Verified
    } else {
        ChallengeVerification::Failed(ChallengeFailure::QueueArithmetic)
    }
}

#[must_use]
pub fn verify_gap(challenge: &GapChallenge) -> ChallengeVerification {
    if challenge.dropped_start == challenge.emitted_through.saturating_add(1)
        && challenge.dropped_end >= challenge.dropped_start
        && challenge.next_emitted == challenge.dropped_end.saturating_add(1)
    {
        ChallengeVerification::Verified
    } else {
        ChallengeVerification::Failed(ChallengeFailure::GapNotCaptured)
    }
}

#[must_use]
pub fn verify_draft_exclusion(wire_bytes: &[u8], draft_marker: &str) -> ChallengeVerification {
    if contains_bytes(wire_bytes, draft_marker.as_bytes()) {
        ChallengeVerification::Failed(ChallengeFailure::DraftLeaked)
    } else {
        ChallengeVerification::Verified
    }
}

fn contains_bytes(haystack: &[u8], needle: &[u8]) -> bool {
    !needle.is_empty()
        && haystack
            .windows(needle.len())
            .any(|window| window == needle)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reference_challenge_is_closed_and_complete() {
        for kind in [AdapterKind::Producer, AdapterKind::Server] {
            let challenge = RunnerChallenge::reference(kind, 47);
            assert!(challenge.structurally_valid());
            let bytes =
                serde_json::to_vec(&challenge).unwrap_or_else(|error| panic!("serialize: {error}"));
            let decoded: RunnerChallenge =
                serde_json::from_slice(&bytes).unwrap_or_else(|error| panic!("decode: {error}"));
            assert_eq!(decoded, challenge);
        }
    }

    #[test]
    fn focused_challenge_checks_reject_fabrication() {
        assert_eq!(
            verify_nonce(1, ChallengeNonce(2)),
            ChallengeVerification::Failed(ChallengeFailure::NonceMismatch)
        );
        assert_eq!(
            verify_clock(&ClockChallenge {
                timestamps: vec![2, 1]
            }),
            ChallengeVerification::Failed(ChallengeFailure::ClockMismatch)
        );
        assert_eq!(
            verify_sink(
                &SinkChallenge {
                    capacity: 3,
                    deadline_ms: 10,
                    operations: 4
                },
                4,
                2
            ),
            ChallengeVerification::Failed(ChallengeFailure::QueueArithmetic)
        );
        assert_eq!(
            verify_gap(&GapChallenge {
                emitted_through: 1,
                dropped_start: 2,
                dropped_end: 3,
                next_emitted: 9
            }),
            ChallengeVerification::Failed(ChallengeFailure::GapNotCaptured)
        );
        assert_eq!(
            verify_draft_exclusion(b"secret-draft", "draft"),
            ChallengeVerification::Failed(ChallengeFailure::DraftLeaked)
        );
    }
}

//! Pure execution authorization guard (issue #382 CW02-12 / S8).
//!
//! This is the **pure** pre-execution boundary that rejects any stale
//! generation before a single filesystem, prompt, tmux, SSH, or spawn effect
//! may occur. It consumes an immutable [`AgentLaunchPlan`] produced by the
//! local planner and the current [`ExecutionEvidence`] captured by the caller,
//! and decides — with no side effects — whether execution may proceed.
//!
//! # Contract
//!
//! - Exact match across every dimension returns [`AuthorizedExecution`], a
//!   thin wrapper that borrows the plan. No allocation, no I/O, no closures,
//!   no callbacks.
//! - Any single mismatch returns [`AuthorizationRejection`] carrying the
//!   closed `AGT-E203` code and the exact mismatched [`StaleDimension`].
//! - On reject, no effect hook or closure supplied by the caller is invoked;
//!   the guard performs only comparisons.
//!
//! # Architectural boundaries
//!
//! This module owns no `AppState`, no `RuntimeManager`, no process spawn, no
//! filesystem probe, and no persistence. It is a deterministic comparison
//! function over typed values. Product knowledge lives only in the definition
//! data that produced the plan; this module contains no product tokens and
//! performs no product matching.
//!
//! # Slice scope (S8)
//!
//! Only the pure authorization comparison. Spawning, `agent_launcher`,
//! scenarios, effects, app state, remote, preflight, send, manifests, and
//! scripts are intentionally outside this slice.

use crate::agent_candidate_fingerprint::CandidateFingerprint;
use crate::domain::agent_definition::sha256::DefinitionSha256;
use crate::domain::agent_definition::types::AgentLaunchPlan;
use crate::domain::agent_definition::types::ProbeErrorCode;

// ---------------------------------------------------------------------------
// Current evidence
// ---------------------------------------------------------------------------

/// Current evidence captured by the caller immediately before execution.
///
/// Each field is the *current* observation that must match the corresponding
/// dimension stamped onto the [`AgentLaunchPlan`] at plan time. The caller is
/// responsible for capturing these observations; the guard performs only the
/// comparison. Every field is owned so the guard never borrows mutable caller
/// state.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExecutionEvidence {
    /// Current content digest of the agent definition the plan was built from.
    definition_sha256: DefinitionSha256,
    /// Current physical fingerprint of the resolved executable.
    executable_fingerprint: CandidateFingerprint,
    /// Current probe generation stamp.
    probe_generation: u64,
    /// Current target generation stamp.
    target_generation: u64,
    /// Current activation generation stamp.
    activation_generation: u64,
}

impl ExecutionEvidence {
    /// Construct current execution evidence from the five generation-bearing
    /// dimensions the guard compares.
    ///
    /// The caller captures each observation immediately before requesting
    /// authorization; the guard performs no capture itself.
    #[must_use]
    pub fn new(
        definition_sha256: DefinitionSha256,
        executable_fingerprint: CandidateFingerprint,
        probe_generation: u64,
        target_generation: u64,
        activation_generation: u64,
    ) -> Self {
        Self {
            definition_sha256,
            executable_fingerprint,
            probe_generation,
            target_generation,
            activation_generation,
        }
    }

    /// Current definition SHA-256.
    #[must_use]
    pub const fn definition_sha256(&self) -> &DefinitionSha256 {
        &self.definition_sha256
    }

    /// Current executable fingerprint.
    #[must_use]
    pub const fn executable_fingerprint(&self) -> &CandidateFingerprint {
        &self.executable_fingerprint
    }

    /// Current probe generation stamp.
    #[must_use]
    pub const fn probe_generation(&self) -> u64 {
        self.probe_generation
    }

    /// Current target generation stamp.
    #[must_use]
    pub const fn target_generation(&self) -> u64 {
        self.target_generation
    }

    /// Current activation generation stamp.
    #[must_use]
    pub const fn activation_generation(&self) -> u64 {
        self.activation_generation
    }
}

// ---------------------------------------------------------------------------
// Stale dimension / rejection / authorization result
// ---------------------------------------------------------------------------

/// The single dimension that became stale between plan time and execution.
///
/// Each variant maps one-to-one to a generation-bearing field the guard
/// compares. The variant is reported on reject so the caller can surface the
/// exact dimension in its `AGT-E203` diagnostic and recovery hint.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StaleDimension {
    /// The definition SHA-256 changed.
    DefinitionSha256,
    /// The physical executable fingerprint changed.
    ExecutableFingerprint,
    /// The probe generation changed.
    ProbeGeneration,
    /// The target generation changed.
    TargetGeneration,
    /// The activation generation changed.
    ActivationGeneration,
}

impl StaleDimension {
    /// Human-readable dimension label for diagnostics.
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::DefinitionSha256 => "definition_sha256",
            Self::ExecutableFingerprint => "executable_fingerprint",
            Self::ProbeGeneration => "probe_generation",
            Self::TargetGeneration => "target_generation",
            Self::ActivationGeneration => "activation_generation",
        }
    }
}

/// Typed rejection produced when any generation-bearing dimension is stale.
///
/// Carries the closed `AGT-E203` code and the exact mismatched dimension so
/// the caller can surface the precise recovery action (reprobe). The guard
/// never constructs this for any other failure mode: authorization is purely
/// a comparison, so the only possible failure is a stale dimension.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AuthorizationRejection {
    /// The closed stale-generation code.
    code: ProbeErrorCode,
    /// The dimension that mismatched.
    dimension: StaleDimension,
}

impl AuthorizationRejection {
    /// Construct the fail-closed executable-fingerprint mismatch diagnostic.
    #[must_use]
    pub(crate) const fn executable_fingerprint() -> Self {
        Self {
            code: ProbeErrorCode::Agte203,
            dimension: StaleDimension::ExecutableFingerprint,
        }
    }

    /// The closed stale-generation code (`AGT-E203`).
    #[must_use]
    pub const fn code(&self) -> ProbeErrorCode {
        self.code
    }

    /// The dimension that mismatched.
    #[must_use]
    pub const fn dimension(&self) -> StaleDimension {
        self.dimension
    }
}

impl std::fmt::Display for AuthorizationRejection {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{}: stale {} before execution; reprobe required",
            self.code.as_str(),
            self.dimension.label()
        )
    }
}

impl std::error::Error for AuthorizationRejection {}

/// The outcome of [`authorize_execution`].
#[derive(Debug)]
pub enum AuthorizationResult<'a> {
    /// Authorization succeeded; the caller may proceed with the borrowed plan.
    Authorized(AuthorizedExecution<'a>),
    /// Authorization failed; zero effects may occur.
    Rejected(AuthorizationRejection),
}

/// Thin authorized wrapper borrowing the immutable plan.
///
/// Constructed only by [`authorize_execution`] on exact match. Holding this
/// value is the proof that every generation-bearing dimension was current at
/// the moment of authorization. It borrows immutably so the caller cannot
/// mutate the plan after authorization.
#[derive(Debug, Clone, Copy)]
pub struct AuthorizedExecution<'a> {
    plan: &'a AgentLaunchPlan,
}

impl<'a> AuthorizedExecution<'a> {
    /// Borrow the authorized immutable plan.
    #[must_use]
    pub const fn plan(&self) -> &'a AgentLaunchPlan {
        self.plan
    }
}

// ---------------------------------------------------------------------------
// Pure entry point
// ---------------------------------------------------------------------------

/// Authorize execution of `plan` against current `evidence`.
///
/// Pure and side-effect-free: the function performs only equality comparisons
/// across the five generation-bearing dimensions and returns either an
/// [`AuthorizedExecution`] borrowing `plan` or an [`AuthorizationRejection`].
/// No closure, callback, or effect hook supplied by the caller is invoked,
/// including on reject.
///
/// # Dimensions compared
///
/// 1. definition SHA-256,
/// 2. executable fingerprint,
/// 3. probe generation,
/// 4. target generation,
/// 5. activation generation.
///
/// The dimensions are compared in the order above; the first mismatch is
/// reported. An exact match across all five authorizes execution.
///
/// # Errors
///
/// Returns [`AuthorizationResult::Rejected`] carrying `AGT-E203` and the
/// mismatched [`StaleDimension`]. There is no other failure mode.
#[must_use]
pub fn authorize_execution<'a>(
    plan: &'a AgentLaunchPlan,
    evidence: &ExecutionEvidence,
) -> AuthorizationResult<'a> {
    if plan.definition_sha256 != *evidence.definition_sha256() {
        return AuthorizationResult::Rejected(AuthorizationRejection {
            code: ProbeErrorCode::Agte203,
            dimension: StaleDimension::DefinitionSha256,
        });
    }
    if plan.executable_fingerprint != *evidence.executable_fingerprint() {
        return AuthorizationResult::Rejected(AuthorizationRejection {
            code: ProbeErrorCode::Agte203,
            dimension: StaleDimension::ExecutableFingerprint,
        });
    }
    if plan.probe_generation != evidence.probe_generation() {
        return AuthorizationResult::Rejected(AuthorizationRejection {
            code: ProbeErrorCode::Agte203,
            dimension: StaleDimension::ProbeGeneration,
        });
    }
    if plan.target_generation != evidence.target_generation() {
        return AuthorizationResult::Rejected(AuthorizationRejection {
            code: ProbeErrorCode::Agte203,
            dimension: StaleDimension::TargetGeneration,
        });
    }
    if plan.activation_generation != evidence.activation_generation() {
        return AuthorizationResult::Rejected(AuthorizationRejection {
            code: ProbeErrorCode::Agte203,
            dimension: StaleDimension::ActivationGeneration,
        });
    }
    AuthorizationResult::Authorized(AuthorizedExecution { plan })
}

#[cfg(test)]
#[path = "agent_execution_guard_tests.rs"]
mod tests;

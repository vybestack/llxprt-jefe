//! Ordered execution preparation boundary (issue #382 CW02-09 / S10).
//!
//! This is the **ordered** gate between S8 execution authorization and any
//! clone/reset/prompt/SSH/tmux/spawn preparation effect. It consumes an
//! [`AuthorizedExecution`] — which can only be constructed by
//! [`authorize_execution`](super::agent_execution_guard::authorize_execution)
//! after every generation-bearing dimension is proven current — and a fixed
//! structural sandbox inspector, and decides whether preparation may proceed.
//!
//! # Contract
//!
//! - Authorization (S8) must succeed first; this boundary takes an
//!   [`AuthorizedExecution`], making the ordering structural in types.
//! - Preflight success returns [`PreflightCleared`], the only typed value
//!   through which clone/reset/prompt/SSH/tmux/spawn may proceed.
//! - Missing/changed engine fingerprint, unavailable image, or missing
//!   required environment names returns [`UnavailableReason`] and a
//!   zero-effect outcome: no later preparation effect runs.
//! - Sandbox inspection uses fixed structural argv only; never pull/build/network.
//! - Diagnostics name required environment names only; never their values.
//!
//! # Architectural boundaries
//!
//! This module owns no `AppState`, no `RuntimeManager`, no cancellation, and
//! no persistence. The [`SandboxInspector`] trait abstracts the fixed-argv
//! inspection so production uses [`ProcessSandboxInspector`] and tests use a
//! recording inspector. The boundary is definition-generic: product knowledge
//! lives only in the `Preflight` contract stamped onto the plan.
//!
//! # Slice scope (S10)
//!
//! Only the ordered preparation boundary. No process manager, no cancellation
//! subsystem, no fresh-send orchestration, no migration, no UI changes.

use std::fmt;
use std::process::Command;

use crate::agent_candidate_fingerprint::capture_candidate_fingerprint;
use crate::domain::agent_definition::types::{AgentLaunchPlan, Target};
use crate::runtime::agent_execution_guard::{
    AuthorizationRejection, AuthorizationResult, AuthorizedExecution, ExecutionEvidence,
    authorize_execution,
};

// ---------------------------------------------------------------------------
// Inspection outcome
// ---------------------------------------------------------------------------

/// The result of a single fixed-argv sandbox inspection.
///
/// Carries an availability flag and, for engine inspections, a stable
/// fingerprint string used to detect engine identity changes. The fingerprint
/// is never an environment value.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InspectOutcome {
    available: bool,
    fingerprint: String,
}

impl InspectOutcome {
    /// A successful inspection carrying the observed fingerprint.
    #[must_use]
    pub fn available(fingerprint: impl Into<String>) -> Self {
        Self {
            available: true,
            fingerprint: fingerprint.into(),
        }
    }

    /// A failed inspection (engine/image missing or non-inspectable).
    #[must_use]
    pub fn unavailable() -> Self {
        Self {
            available: false,
            fingerprint: String::new(),
        }
    }

    /// Whether the inspected resource is available.
    #[must_use]
    pub const fn is_available(&self) -> bool {
        self.available
    }

    /// The observed fingerprint (engine identity string).
    #[must_use]
    pub fn fingerprint(&self) -> &str {
        &self.fingerprint
    }
}

// ---------------------------------------------------------------------------
// Sandbox inspector trait
// ---------------------------------------------------------------------------

/// Fixed structural argv sandbox inspection boundary.
///
/// Implementations inspect the sandbox engine and image availability using
/// fixed structural argv only. They must **never** pull, build, or perform any
/// network operation. Required environment values may be queried only to decide
/// presence; they must never be returned or logged.
///
/// This trait is intentionally minimal and side-effect-bounded so the
/// preparation boundary remains deterministic and testable through a
/// recording inspector rather than mock call counts.
pub trait SandboxInspector {
    /// Inspect the sandbox engine using fixed argv (e.g. `--version`).
    ///
    /// Returns the engine fingerprint on success or [`InspectOutcome::unavailable`].
    fn inspect_engine(&self, engine: &str) -> InspectOutcome;

    /// Inspect whether the image is available locally using fixed argv
    /// (e.g. `image inspect`). **Never** pulls or builds.
    fn inspect_image(&self, engine: &str, image: &str) -> InspectOutcome;

    /// Whether a required environment name is present.
    ///
    /// Checks name presence only; the value is never read or returned.
    fn env_present(&self, name: &str) -> bool;
}

// ---------------------------------------------------------------------------
// Typed unavailability reason
// ---------------------------------------------------------------------------

/// Typed reason the preparation boundary rejected preparation.
///
/// Every variant is a struct-like enum carrying only names and identifiers.
/// **No variant ever carries an environment value.** The
/// [`MissingRequiredEnv`](Self::MissingRequiredEnv) variant names the absent
/// environment names only.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum UnavailableReason {
    /// Preflight is required but the contract has no engine or image configured.
    ContractUnconfigured,
    /// The sandbox engine binary is missing or could not be inspected.
    EngineMissing {
        /// The configured engine name (e.g. "podman").
        engine: String,
    },
    /// The sandbox engine fingerprint changed since it was last observed.
    EngineFingerprintChanged {
        /// The configured engine name.
        engine: String,
        /// The fingerprint expected (captured at probe time).
        expected: String,
        /// The fingerprint currently observed.
        actual: String,
    },
    /// The sandbox image is not available locally (never pulled/built).
    ImageMissing {
        /// The configured engine name.
        engine: String,
        /// The configured image reference.
        image: String,
    },
    /// One or more required environment names are absent (names only).
    MissingRequiredEnv {
        /// The absent environment variable names, never their values.
        names: Vec<String>,
    },
}

impl UnavailableReason {
    /// Whether this reason indicates the engine is unavailable in any form.
    #[must_use]
    pub const fn is_engine_unavailable(&self) -> bool {
        matches!(
            self,
            Self::EngineMissing { .. } | Self::EngineFingerprintChanged { .. }
        )
    }
}

impl fmt::Display for UnavailableReason {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::ContractUnconfigured => {
                f.write_str("sandbox preflight is required but engine or image is not configured")
            }
            Self::EngineMissing { engine } => {
                write!(
                    f,
                    "sandbox engine `{engine}` is not available or not inspectable"
                )
            }
            Self::EngineFingerprintChanged { engine, .. } => {
                write!(
                    f,
                    "sandbox engine `{engine}` fingerprint changed since last verified"
                )
            }
            Self::ImageMissing { engine, image } => {
                write!(
                    f,
                    "sandbox image `{image}` is not available locally in `{engine}`"
                )
            }
            Self::MissingRequiredEnv { names } => write!(
                f,
                "required environment names not set: {}",
                names.join(", ")
            ),
        }
    }
}

impl std::error::Error for UnavailableReason {}

// ---------------------------------------------------------------------------
// Preparation outcome / cleared wrapper
// ---------------------------------------------------------------------------

/// The outcome of the ordered preparation boundary.
///
/// On success, [`PreparationOutcome::Cleared`] returns a [`PreflightCleared`]
/// borrowing the authorized plan — the only typed value through which
/// clone/reset/prompt/SSH/tmux/spawn may proceed. On failure,
/// [`PreparationOutcome::Unavailable`] returns the typed reason and
/// guarantees zero later preparation effects.
#[derive(Debug)]
pub enum PreparationOutcome<'a> {
    /// Preflight succeeded; preparation may proceed through the cleared wrapper.
    Cleared(PreflightCleared<'a>),
    /// Preflight failed; zero preparation effects may occur.
    Unavailable(UnavailableReason),
}

/// The only typed value through which preparation effects may proceed.
///
/// Constructed exclusively by [`prepare_execution`] after:
/// 1. S8 [`authorize_execution`] succeeded (proven by the borrowed
///    [`AuthorizedExecution`]), and
/// 2. preflight inspection passed.
///
/// Holding this value is the proof that both gates passed. It borrows the
/// authorized plan immutably so the caller cannot mutate it after preflight.
#[derive(Debug, Clone)]
pub struct PreflightCleared<'a> {
    authorized: AuthorizedExecution<'a>,
    engine_fingerprint: Option<String>,
}

impl<'a> PreflightCleared<'a> {
    /// Borrow the authorized execution.
    #[must_use]
    pub const fn authorized(&self) -> AuthorizedExecution<'a> {
        self.authorized
    }

    /// Borrow the authorized immutable plan.
    #[must_use]
    pub const fn plan(&self) -> &'a AgentLaunchPlan {
        self.authorized.plan()
    }

    /// The engine fingerprint observed during preflight, if preflight ran.
    ///
    /// `None` when preflight was not required. The caller can stamp this for
    /// future change detection.
    #[must_use]
    pub fn engine_fingerprint(&self) -> Option<&str> {
        self.engine_fingerprint.as_deref()
    }
}

/// Owned proof that a launch plan passed authorization and preflight.
///
/// One immutable launch plan passed full authorization and sandbox preflight.
/// Runtime creation accepts this proof rather than a raw plan and rechecks it
/// immediately before its first effect.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AuthorizedLaunchPlan {
    plan: AgentLaunchPlan,
    evidence: ExecutionEvidence,
    engine_fingerprint: Option<String>,
}

/// Failure while sealing or immediately revalidating an authorized launch.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LaunchProofError {
    /// Current execution evidence no longer matches the immutable plan.
    Authorization(AuthorizationRejection),
    /// Sandbox preflight did not clear.
    Preflight(UnavailableReason),
    /// Post-preflight prompt assembly changed a plan dimension other than argv.
    FinalPlanChanged,
}

impl fmt::Display for LaunchProofError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Authorization(error) => error.fmt(formatter),
            Self::Preflight(error) => error.fmt(formatter),
            Self::FinalPlanChanged => {
                formatter.write_str("post-preflight launch assembly changed protected plan fields")
            }
        }
    }
}

impl std::error::Error for LaunchProofError {}

impl AuthorizedLaunchPlan {
    /// Seal a cleared plan. `final_plan` may differ only in argv assembled by
    /// the fresh-send boundary.
    ///
    /// This is the public sealing entry point for the launch-proof boundary:
    /// callers must supply a [`PreflightCleared`] (only obtainable from
    /// [`prepare_execution`] after [`authorize_execution`] succeeded) plus
    /// matching [`ExecutionEvidence`]. The boundary re-authorizes the plan
    /// against the evidence, so a forged or stale combination is rejected with
    /// [`LaunchProofError::Authorization`]. No private field is ever set
    /// directly; this is the only way to construct an [`AuthorizedLaunchPlan`].
    pub fn from_cleared(
        cleared: PreflightCleared<'_>,
        final_plan: AgentLaunchPlan,
        evidence: ExecutionEvidence,
    ) -> Result<Self, LaunchProofError> {
        if !same_protected_plan(cleared.plan(), &final_plan) {
            return Err(LaunchProofError::FinalPlanChanged);
        }
        match authorize_execution(&final_plan, &evidence) {
            AuthorizationResult::Authorized(_) => Ok(Self {
                plan: final_plan,
                evidence,
                engine_fingerprint: cleared.engine_fingerprint.clone(),
            }),
            AuthorizationResult::Rejected(error) => Err(LaunchProofError::Authorization(error)),
        }
    }

    /// Borrow the immutable plan for non-effectful projection.
    #[must_use]
    pub const fn plan(&self) -> &AgentLaunchPlan {
        &self.plan
    }

    /// Reauthorize and repeat preflight against the fingerprint captured by the
    /// first clearance. Runtime managers call this immediately before effects.
    pub fn prepare_current(
        &self,
        inspector: &dyn SandboxInspector,
    ) -> Result<PreflightCleared<'_>, LaunchProofError> {
        let current_evidence = self.current_execution_evidence()?;
        let authorized = match authorize_execution(&self.plan, &current_evidence) {
            AuthorizationResult::Authorized(authorized) => authorized,
            AuthorizationResult::Rejected(error) => {
                return Err(LaunchProofError::Authorization(error));
            }
        };
        match prepare_execution(authorized, self.engine_fingerprint.as_deref(), inspector) {
            PreparationOutcome::Cleared(cleared) => Ok(cleared),
            PreparationOutcome::Unavailable(reason) => Err(LaunchProofError::Preflight(reason)),
        }
    }

    fn current_execution_evidence(&self) -> Result<ExecutionEvidence, LaunchProofError> {
        let executable_fingerprint =
            match self.plan.target {
                Target::Local { .. } => capture_candidate_fingerprint(&self.plan.executable)
                    .map_err(|error| {
                        tracing::warn!(
                            %error,
                            executable = %self.plan.executable.display(),
                            "failed to recapture local executable fingerprint before launch"
                        );
                        LaunchProofError::Authorization(
                            AuthorizationRejection::executable_fingerprint(),
                        )
                    })?,
                Target::Remote(_) => self.evidence.executable_fingerprint().clone(),
            };
        Ok(ExecutionEvidence::new(
            *self.evidence.definition_sha256(),
            executable_fingerprint,
            self.evidence.probe_generation(),
            self.evidence.target_generation(),
            self.evidence.activation_generation(),
        ))
    }
}

fn same_protected_plan(before: &AgentLaunchPlan, after: &AgentLaunchPlan) -> bool {
    before.type_id == after.type_id
        && before.operation == after.operation
        && before.definition_sha256 == after.definition_sha256
        && before.executable == after.executable
        && before.executable_fingerprint == after.executable_fingerprint
        && before.executable_wrapper == after.executable_wrapper
        && before.env == after.env
        && before.cwd == after.cwd
        && before.target == after.target
        && before.probe_generation == after.probe_generation
        && before.target_generation == after.target_generation
        && before.activation_generation == after.activation_generation
        && before.preflight == after.preflight
        && before.signature == after.signature
}

// ---------------------------------------------------------------------------
// Ordered boundary entry point
// ---------------------------------------------------------------------------

/// Run the ordered execution preparation boundary.
///
/// # Ordering
///
/// This function **requires** an [`AuthorizedExecution`], which can only be
/// obtained from S8 [`authorize_execution`]. The preflight checks then run in
/// a fixed order, each short-circuiting on failure with zero later effects:
///
/// 1. If preflight is not required → cleared without inspection.
/// 2. If preflight is required but unconfigured (no engine/image) →
///    [`UnavailableReason::ContractUnconfigured`].
/// 3. Inspect engine with fixed argv → missing returns
///    [`UnavailableReason::EngineMissing`].
/// 4. Compare engine fingerprint if expected → changed returns
///    [`UnavailableReason::EngineFingerprintChanged`].
/// 5. Inspect image with fixed argv → missing returns
///    [`UnavailableReason::ImageMissing`].
/// 6. Check required environment names → absent returns
///    [`UnavailableReason::MissingRequiredEnv`] with names only.
/// 7. All passed → [`PreflightCleared`].
///
/// No pull, build, or network operation is performed. Environment values are
/// queried only for presence and are never returned or logged.
///
/// # Errors
///
/// Returns [`PreparationOutcome::Unavailable`] for any preflight failure.
///
/// [`authorize_execution`]: super::agent_execution_guard::authorize_execution
#[must_use]
pub fn prepare_execution<'a>(
    authorized: AuthorizedExecution<'a>,
    expected_engine_fingerprint: Option<&str>,
    inspector: &dyn SandboxInspector,
) -> PreparationOutcome<'a> {
    let preflight = &authorized.plan().preflight;

    // 1. Not required → cleared without any inspection.
    if !preflight.is_required() {
        return PreparationOutcome::Cleared(PreflightCleared {
            authorized,
            engine_fingerprint: None,
        });
    }

    // 2. Required but unconfigured (no engine or image).
    let (Some(engine), Some(image)) = (&preflight.engine, &preflight.image) else {
        return PreparationOutcome::Unavailable(UnavailableReason::ContractUnconfigured);
    };

    // 3. Inspect engine with fixed structural argv.
    let engine_outcome = inspector.inspect_engine(engine);
    if !engine_outcome.is_available() {
        return PreparationOutcome::Unavailable(UnavailableReason::EngineMissing {
            engine: engine.clone(),
        });
    }

    // 4. Detect engine fingerprint change.
    let observed = engine_outcome.fingerprint();
    if let Some(expected) = expected_engine_fingerprint
        && observed != expected
    {
        return PreparationOutcome::Unavailable(UnavailableReason::EngineFingerprintChanged {
            engine: engine.clone(),
            expected: expected.to_owned(),
            actual: observed.to_owned(),
        });
    }

    // 5. Inspect image with fixed structural argv (never pull/build).
    let image_outcome = inspector.inspect_image(engine, image);
    if !image_outcome.is_available() {
        return PreparationOutcome::Unavailable(UnavailableReason::ImageMissing {
            engine: engine.clone(),
            image: image.clone(),
        });
    }

    // 6. Check required environment names (name presence only).
    let missing: Vec<String> = preflight
        .required_env
        .iter()
        .filter(|name| !inspector.env_present(name))
        .cloned()
        .collect();
    if !missing.is_empty() {
        return PreparationOutcome::Unavailable(UnavailableReason::MissingRequiredEnv {
            names: missing,
        });
    }

    // 7. All checks passed — cleared for preparation.
    PreparationOutcome::Cleared(PreflightCleared {
        authorized,
        engine_fingerprint: Some(observed.to_owned()),
    })
}

// ---------------------------------------------------------------------------
// Production inspector (fixed structural argv)
// ---------------------------------------------------------------------------

/// Production sandbox inspector using fixed structural argv.
///
/// Engine inspection runs `<engine> --version` and treats the stdout output
/// as the engine fingerprint. Image inspection runs
/// `<engine> image inspect <image>` — a read-only local query that **never**
/// pulls, builds, or touches the network. Environment presence is checked via
/// [`std::env::var_os`], and the returned value is discarded without logging.
#[derive(Debug, Clone, Copy, Default)]
pub struct ProcessSandboxInspector;

impl ProcessSandboxInspector {
    /// Construct the production inspector.
    #[must_use]
    pub const fn new() -> Self {
        Self
    }
}

impl SandboxInspector for ProcessSandboxInspector {
    fn inspect_engine(&self, engine: &str) -> InspectOutcome {
        let Ok(output) = Command::new(engine).arg("--version").output() else {
            return InspectOutcome::unavailable();
        };
        if !output.status.success() {
            return InspectOutcome::unavailable();
        }
        let fingerprint = String::from_utf8_lossy(&output.stdout).trim().to_owned();
        InspectOutcome::available(fingerprint)
    }

    fn inspect_image(&self, engine: &str, image: &str) -> InspectOutcome {
        let Ok(output) = Command::new(engine)
            .args(["image", "inspect", image])
            .output()
        else {
            return InspectOutcome::unavailable();
        };
        if output.status.success() {
            InspectOutcome::available(String::new())
        } else {
            InspectOutcome::unavailable()
        }
    }

    fn env_present(&self, name: &str) -> bool {
        std::env::var_os(name).is_some()
    }
}

#[cfg(test)]
#[path = "agent_preflight_tests.rs"]
mod tests;

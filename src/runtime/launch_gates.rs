//! The declared registry of every gate in the agent launch pipeline (issue #544).
//!
//! Contract: `dev-docs/standards/windows-launch-pipeline.md`.
//!
//! The Windows launch path runs fifteen sequential gates where macOS runs about
//! four. Every one of them was written as an unconditional refusal, and six of
//! them collapsed into a bare `spawn failed: ...` that named no stage, so a user
//! could not tell which gate had stopped them or what to do about it. That is
//! the defect class behind #519 -> #525 -> #529, and the reason #530 stayed
//! latent for seventeen days.
//!
//! This module is the executable form of the contract. Every accessor on
//! [`LaunchGate`] is an exhaustive match, so a new gate cannot compile until its
//! id, precondition, failure behaviour and remediation are declared, and
//! [`tests`] asserts that the same gate is documented in the standards file.

use std::fmt;

/// What a gate is required to do when its precondition is not met.
///
/// `Degrade` is only correct where the lost property is not a safety property.
/// Containment is a cleanup guarantee, so losing it is recoverable. Ownership,
/// authorization and working-directory correctness are not.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GateFailureBehaviour {
    /// The launch stops, and the diagnostic must name this gate, the observed
    /// cause and a remediation.
    Refuse,
    /// The launch continues in the named documented lesser mode, and the user is
    /// warned by that name.
    Degrade {
        /// The stable name of the degraded mode, as documented in the standard.
        mode: &'static str,
    },
}

impl GateFailureBehaviour {
    /// The stable label used in the standards document and in tests.
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::Refuse => "refuse",
            Self::Degrade { .. } => "degrade",
        }
    }

    /// Whether reaching this behaviour still permits the launch to continue.
    #[must_use]
    pub const fn continues_launch(self) -> bool {
        matches!(self, Self::Degrade { .. })
    }
}

/// One gate in the agent launch pipeline, in execution order.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum LaunchGate {
    /// Assembling the launch request and the evidence captured for it.
    LaunchComposition,
    /// Resolving a binary name against the immutable PATH/PATHEXT snapshot.
    ExecutableDiscovery,
    /// Classifying the resolved binary as a direct executable or a wrapper.
    ExecutableStrategy,
    /// The identity subprocess that authoritatively names the agent.
    IdentityProbe,
    /// Preparing a jefe-managed npm install for a nonblank version selector.
    ManagedPackageInstall,
    /// Checking the probed capabilities against the requested operation.
    CapabilitySupport,
    /// Confirming every piece of captured evidence is still current.
    ExecutionAuthorization,
    /// Inspecting the configured sandbox engine and image.
    SandboxPreflight,
    /// Assembling exactly one typed prompt within the pane-command budget.
    PromptAssembly,
    /// Staging the content-addressed session host image (Windows).
    SessionHostStaging,
    /// Handing the argv/env payload to the worker (Windows).
    LaunchPlanTransport,
    /// Building the pane command the multiplexer will run.
    PaneCommand,
    /// Placing the worker in a Job Object so its tree is reaped (Windows).
    WorkerContainment,
    /// Identifying the process that owns the session host (Windows).
    OwnerAnchor,
    /// Spawning the worker from the consumed launch plan.
    WorkerSpawn,
}

impl LaunchGate {
    /// Every declared gate, in execution order.
    pub const ALL: [Self; 15] = [
        Self::LaunchComposition,
        Self::ExecutableDiscovery,
        Self::ExecutableStrategy,
        Self::IdentityProbe,
        Self::ManagedPackageInstall,
        Self::CapabilitySupport,
        Self::ExecutionAuthorization,
        Self::SandboxPreflight,
        Self::PromptAssembly,
        Self::SessionHostStaging,
        Self::LaunchPlanTransport,
        Self::PaneCommand,
        Self::WorkerContainment,
        Self::OwnerAnchor,
        Self::WorkerSpawn,
    ];

    /// The stable identifier that appears in every diagnostic and in the
    /// standards document. Never reword one of these without updating both.
    #[must_use]
    pub const fn id(self) -> &'static str {
        match self {
            Self::LaunchComposition => "launch-composition",
            Self::ExecutableDiscovery => "executable-discovery",
            Self::ExecutableStrategy => "executable-strategy",
            Self::IdentityProbe => "identity-probe",
            Self::ManagedPackageInstall => "managed-package-install",
            Self::CapabilitySupport => "capability-support",
            Self::ExecutionAuthorization => "execution-authorization",
            Self::SandboxPreflight => "sandbox-preflight",
            Self::PromptAssembly => "prompt-assembly",
            Self::SessionHostStaging => "session-host-staging",
            Self::LaunchPlanTransport => "launch-plan-transport",
            Self::PaneCommand => "pane-command",
            Self::WorkerContainment => "worker-containment",
            Self::OwnerAnchor => "owner-anchor",
            Self::WorkerSpawn => "worker-spawn",
        }
    }

    /// Execution position, so a diagnostic can say how far the launch got.
    #[must_use]
    pub const fn ordinal(self) -> usize {
        match self {
            Self::LaunchComposition => 0,
            Self::ExecutableDiscovery => 1,
            Self::ExecutableStrategy => 2,
            Self::IdentityProbe => 3,
            Self::ManagedPackageInstall => 4,
            Self::CapabilitySupport => 5,
            Self::ExecutionAuthorization => 6,
            Self::SandboxPreflight => 7,
            Self::PromptAssembly => 8,
            Self::SessionHostStaging => 9,
            Self::LaunchPlanTransport => 10,
            Self::PaneCommand => 11,
            Self::WorkerContainment => 12,
            Self::OwnerAnchor => 13,
            Self::WorkerSpawn => 14,
        }
    }

    /// What must hold for the gate to pass.
    #[must_use]
    pub const fn precondition(self) -> &'static str {
        match self {
            Self::LaunchComposition => {
                "a complete launch request with the evidence captured for it"
            }
            Self::ExecutableDiscovery => {
                "an immutable PATH/PATHEXT snapshot and a binary name to resolve"
            }
            Self::ExecutableStrategy => {
                "a resolved binary classified as a direct executable or a wrapper script"
            }
            Self::IdentityProbe => "a resolved candidate and a monotonic probe generation",
            Self::ManagedPackageInstall => {
                "a nonblank version selector, a writable cache root, and a working npm"
            }
            Self::CapabilitySupport => {
                "a probed agent whose capabilities cover the requested operation"
            }
            Self::ExecutionAuthorization => {
                "definition, executable, target, probe and activation evidence all still current"
            }
            Self::SandboxPreflight => {
                "the configured sandbox engine and image are present and inspectable"
            }
            Self::PromptAssembly => {
                "exactly one typed prompt that fits the measured pane-command budget"
            }
            Self::SessionHostStaging => "Windows, a valid session name, and a readable host image",
            Self::LaunchPlanTransport => "a private directory jefe owns and can write",
            Self::PaneCommand => {
                "a validated multiplexer and a pane command within the measured budget"
            }
            Self::WorkerContainment => {
                "Windows, and a Job Object the session host can create and own"
            }
            Self::OwnerAnchor => "Windows, and an identifiable owning process for the session host",
            Self::WorkerSpawn => "a consumable launch plan and an existing working directory",
        }
    }

    /// What the gate is required to do when its precondition fails.
    #[must_use]
    pub const fn failure_behaviour(self) -> GateFailureBehaviour {
        match self {
            Self::WorkerContainment => GateFailureBehaviour::Degrade {
                mode: UNCONTAINED_WORKER_MODE,
            },
            Self::LaunchComposition
            | Self::ExecutableDiscovery
            | Self::ExecutableStrategy
            | Self::IdentityProbe
            | Self::ManagedPackageInstall
            | Self::CapabilitySupport
            | Self::ExecutionAuthorization
            | Self::SandboxPreflight
            | Self::PromptAssembly
            | Self::SessionHostStaging
            | Self::LaunchPlanTransport
            | Self::PaneCommand
            | Self::OwnerAnchor
            | Self::WorkerSpawn => GateFailureBehaviour::Refuse,
        }
    }

    /// The action the user can take. This is shown verbatim, so it must be an
    /// instruction rather than a restatement of the failure.
    #[must_use]
    pub const fn remediation(self) -> &'static str {
        match self {
            Self::LaunchComposition => {
                "reopen the agent and check its type, target, version selector and field values"
            }
            Self::ExecutableDiscovery => {
                "install the agent, or correct the configured command so it resolves on PATH"
            }
            Self::ExecutableStrategy => {
                "reinstall the agent through its official installer so its launcher layout is complete"
            }
            Self::IdentityProbe => {
                "run the agent's own version command by hand and fix what it reports"
            }
            Self::ManagedPackageInstall => {
                "check network access to the npm registry and that Node.js and npm are installed"
            }
            Self::CapabilitySupport => {
                "choose an agent version that supports this operation, or change the operation"
            }
            Self::ExecutionAuthorization => {
                "reprobe the agent and launch again; its executable or configuration changed underneath"
            }
            Self::SandboxPreflight => {
                "start or install the sandbox engine, or turn the sandbox off for this agent"
            }
            Self::PromptAssembly => {
                "shorten the prompt, or send the issue reference instead of its full body"
            }
            Self::SessionHostStaging => {
                "free space in the jefe state directory and check that antivirus is not quarantining the staged host"
            }
            Self::LaunchPlanTransport => {
                "check that the jefe state directory exists and is writable"
            }
            Self::PaneCommand => {
                "install the required psmux build, or shorten the launch so the pane command fits the budget"
            }
            Self::WorkerContainment => {
                "no action is required; the agent runs uncontained and must be closed from its own pane"
            }
            Self::OwnerAnchor => {
                "launch jefe from a normal terminal; the current host does not expose an owner chain"
            }
            Self::WorkerSpawn => {
                "check that the agent's working directory still exists and that the executable is runnable"
            }
        }
    }

    /// Wrap an observed cause as a failure attributed to this gate.
    #[must_use]
    pub fn refused(self, cause: impl Into<String>) -> LaunchGateFailure {
        LaunchGateFailure {
            gate: self,
            cause: cause.into(),
        }
    }
}

/// The stable name of the degraded mode gate `worker-containment` falls back to.
pub const UNCONTAINED_WORKER_MODE: &str = "uncontained-worker";

/// A launch refusal attributed to the gate that produced it.
///
/// The `Display` form leads with the gate id and ends with the remediation, so
/// the same string is useful in a log, in the errors ring buffer, and pasted
/// straight into an agent session.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LaunchGateFailure {
    gate: LaunchGate,
    cause: String,
}

impl LaunchGateFailure {
    /// The gate that refused.
    #[must_use]
    pub const fn gate(&self) -> LaunchGate {
        self.gate
    }

    /// The observed cause, without the gate framing.
    #[must_use]
    pub fn cause(&self) -> &str {
        &self.cause
    }

    /// The remediation offered for this gate.
    #[must_use]
    pub const fn remediation(&self) -> &'static str {
        self.gate.remediation()
    }
}

impl fmt::Display for LaunchGateFailure {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "[{}] {} (gate {} of {}); remediation: {}",
            self.gate.id(),
            self.cause,
            self.gate.ordinal() + 1,
            LaunchGate::ALL.len(),
            self.gate.remediation()
        )
    }
}

impl std::error::Error for LaunchGateFailure {}

/// A warning raised when a gate degraded instead of refusing.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LaunchGateDegradation {
    gate: LaunchGate,
    mode: &'static str,
    cause: String,
}

impl LaunchGateDegradation {
    /// Record that `gate` degraded because of `cause`.
    ///
    /// Returns `None` when the gate is not declared as degradable, so a caller
    /// cannot invent a fallback the contract does not permit.
    #[must_use]
    pub fn new(gate: LaunchGate, cause: impl Into<String>) -> Option<Self> {
        match gate.failure_behaviour() {
            GateFailureBehaviour::Degrade { mode } => Some(Self {
                gate,
                mode,
                cause: cause.into(),
            }),
            GateFailureBehaviour::Refuse => None,
        }
    }

    /// The gate that degraded.
    #[must_use]
    pub const fn gate(&self) -> LaunchGate {
        self.gate
    }

    /// The documented name of the mode now in effect.
    #[must_use]
    pub const fn mode(&self) -> &'static str {
        self.mode
    }

    /// The observed cause of the degradation.
    #[must_use]
    pub fn cause(&self) -> &str {
        &self.cause
    }
}

impl fmt::Display for LaunchGateDegradation {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "[{}] degraded to {}: {}",
            self.gate.id(),
            self.mode,
            self.cause
        )
    }
}

#[cfg(test)]
mod tests {
    use super::{GateFailureBehaviour, LaunchGate, LaunchGateDegradation, UNCONTAINED_WORKER_MODE};

    /// The standards document is the human half of the contract; this module is
    /// the executable half. Reading it here is what makes the check mechanical.
    const STANDARD: &str = include_str!("../../dev-docs/standards/windows-launch-pipeline.md");

    #[test]
    fn all_lists_every_gate_once_in_execution_order() {
        for (index, gate) in LaunchGate::ALL.iter().enumerate() {
            assert_eq!(
                gate.ordinal(),
                index,
                "{} is listed at position {index} but declares ordinal {}",
                gate.id(),
                gate.ordinal()
            );
        }
    }

    #[test]
    fn every_gate_declares_a_unique_non_empty_id() {
        let mut seen: Vec<&str> = Vec::new();
        for gate in LaunchGate::ALL {
            let id = gate.id();
            assert!(!id.is_empty(), "{gate:?} declares an empty id");
            assert!(
                !seen.contains(&id),
                "{id} is declared by more than one gate"
            );
            seen.push(id);
        }
    }

    #[test]
    fn every_gate_declares_a_precondition_and_a_remediation() {
        for gate in LaunchGate::ALL {
            assert!(
                !gate.precondition().is_empty(),
                "{} declares no precondition",
                gate.id()
            );
            assert!(
                !gate.remediation().is_empty(),
                "{} declares no remediation",
                gate.id()
            );
        }
    }

    #[test]
    fn every_gate_is_documented_with_its_failure_behaviour() {
        for gate in LaunchGate::ALL {
            assert!(
                STANDARD.contains(gate.id()),
                "{} is declared in code but absent from windows-launch-pipeline.md",
                gate.id()
            );
            assert!(
                STANDARD.contains(gate.failure_behaviour().label()),
                "failure behaviour {} of {} is not documented",
                gate.failure_behaviour().label(),
                gate.id()
            );
        }
    }

    #[test]
    fn containment_is_the_only_gate_that_degrades() {
        let degrading: Vec<&str> = LaunchGate::ALL
            .into_iter()
            .filter(|gate| gate.failure_behaviour().continues_launch())
            .map(LaunchGate::id)
            .collect();
        assert_eq!(degrading, vec!["worker-containment"]);
    }

    #[test]
    fn degradation_names_the_documented_mode() {
        let Some(degraded) = LaunchGateDegradation::new(
            LaunchGate::WorkerContainment,
            "job object creation denied by the enclosing job",
        ) else {
            panic!("worker-containment must be declared degradable");
        };
        assert_eq!(degraded.mode(), UNCONTAINED_WORKER_MODE);
        assert!(STANDARD.contains(UNCONTAINED_WORKER_MODE));
        assert!(degraded.to_string().contains("worker-containment"));
        assert!(degraded.to_string().contains(UNCONTAINED_WORKER_MODE));
    }

    #[test]
    fn a_refusing_gate_cannot_be_recorded_as_degraded() {
        assert!(LaunchGateDegradation::new(LaunchGate::OwnerAnchor, "no owner").is_none());
        assert!(LaunchGateDegradation::new(LaunchGate::WorkerSpawn, "denied").is_none());
    }

    #[test]
    fn a_failure_names_its_gate_cause_and_remediation() {
        let failure = LaunchGate::LaunchPlanTransport.refused("state directory is read-only");
        let rendered = failure.to_string();
        assert!(rendered.contains("launch-plan-transport"), "{rendered}");
        assert!(
            rendered.contains("state directory is read-only"),
            "{rendered}"
        );
        assert!(rendered.contains("remediation:"), "{rendered}");
        assert!(
            rendered.contains(LaunchGate::LaunchPlanTransport.remediation()),
            "{rendered}"
        );
        assert_eq!(failure.gate(), LaunchGate::LaunchPlanTransport);
    }

    #[test]
    fn no_failure_renders_a_gateless_diagnostic() {
        for gate in LaunchGate::ALL {
            if gate.failure_behaviour() == (GateFailureBehaviour::Refuse) {
                let rendered = gate.refused("observed cause").to_string();
                assert!(
                    rendered.starts_with(&format!("[{}]", gate.id())),
                    "{rendered} does not lead with its gate"
                );
            }
        }
    }
}

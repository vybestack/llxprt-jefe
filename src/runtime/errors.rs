//! Runtime error types.
//!
//! @plan PLAN-20260216-FIRSTVERSION-V1.P06
//! @requirement REQ-TECH-004

use crate::domain::AgentId;

use super::agent_executable::AgentExecutableError;
use super::launch_gates::{LaunchGate, LaunchGateFailure};
use super::multiplexer::MultiplexerError;
use super::package_probe::NpmPackageAvailabilityError;
use super::session_host::SessionHostError;

/// Errors from runtime operations.
#[derive(Debug, Clone)]
pub enum RuntimeError {
    /// Session not found by name.
    SessionNotFound(String),
    /// Failed to attach to session.
    AttachFailed(String),
    /// Runtime use was attempted before the first frame supplied geometry.
    InitialGeometryUnavailable,
    /// The first committed frame resolved to a zero-sized PTY viewport.
    InvalidInitialGeometry { rows: u16, cols: u16 },
    /// Initial geometry may be committed only once.
    InitialGeometryAlreadyConfigured,
    /// Failed to spawn session.
    SpawnFailed(String),
    /// A named gate in the launch pipeline refused (issue #544).
    ///
    /// The pipeline runs fifteen gates. Carrying the gate identity means the
    /// user is told which one stopped them and what to do about it, instead of
    /// reading an unattributed `spawn failed` and having to guess.
    LaunchGateRefused(LaunchGateFailure),
    /// The immutable Windows pane host could not be staged.
    SessionHostStaging(SessionHostError),
    /// Local agent executable resolution or launch-strategy failure.
    AgentExecutable(AgentExecutableError),
    /// npm or the requested LLxprt package is unavailable on the effective target.
    NpmPackageAvailability(NpmPackageAvailabilityError),
    /// Local multiplexer dependency or policy failure.
    Multiplexer(MultiplexerError),
    /// Failed to execute remote SSH session lifecycle command.
    RemoteExecutionFailed(String),
    /// A runtime capability probe could not execute successfully.
    CapabilityProbeFailed(String),
    /// A runtime capability required by the launch is unavailable.
    CapabilityCheckFailed(String),
    /// Failed to kill session.
    KillFailed(String),
    /// Agent is already running.
    AlreadyRunning(AgentId),
    /// Agent is not running.
    NotRunning(AgentId),
    /// No viewer currently attached.
    NoAttachedViewer,
    /// Write to PTY failed.
    WriteFailed(String),
    /// Resize failed.
    ResizeFailed(String),
    /// Relaunch refused because a validated orphan worker descendant is still
    /// alive and could not be reaped (issue #332). Spawning now would create a
    /// duplicate `--continue` worker, so the caller must surface this to the
    /// user rather than spawn.
    OrphanBlocked(AgentId),
}

impl RuntimeError {
    /// Attribute an otherwise anonymous refusal to the gate that produced it.
    ///
    /// The launch pipeline collects failures from a wide set of leaf helpers,
    /// most of which reported [`Self::SpawnFailed`] with no indication of where
    /// in the pipeline they came from. Rather than teach sixty leaves their own
    /// location, the orchestrator tags each stage as it calls it: the leaf keeps
    /// saying what went wrong, and the boundary says where.
    ///
    /// Errors that already identify their origin are returned untouched, so the
    /// more specific answer is never overwritten by the coarser one and a
    /// message can never accumulate two remediations.
    #[must_use]
    pub fn attributed_to(self, gate: LaunchGate) -> Self {
        match self {
            Self::SpawnFailed(message) => Self::LaunchGateRefused(gate.refused(message)),
            already_attributed => already_attributed,
        }
    }
}

impl std::fmt::Display for RuntimeError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::SessionNotFound(name) => write!(f, "session not found: {name}"),
            Self::AttachFailed(msg) => write!(f, "attach failed: {msg}"),
            Self::InitialGeometryUnavailable => {
                write!(f, "runtime geometry is unavailable before the first frame")
            }
            Self::InvalidInitialGeometry { rows, cols } => {
                write!(f, "invalid initial runtime geometry: {cols}x{rows}")
            }
            Self::InitialGeometryAlreadyConfigured => {
                write!(f, "initial runtime geometry is already configured")
            }
            Self::SpawnFailed(msg) => write!(f, "spawn failed: {msg}"),
            Self::LaunchGateRefused(failure) => write!(f, "{failure}"),
            Self::SessionHostStaging(error) => write!(f, "session host staging failed: {error}"),
            Self::AgentExecutable(error) => write!(f, "agent launch unavailable: {error}"),
            Self::NpmPackageAvailability(error) => write!(f, "agent launch unavailable: {error}"),
            Self::Multiplexer(error) => write!(f, "multiplexer dependency failed: {error}"),
            Self::RemoteExecutionFailed(msg) => write!(f, "remote execution failed: {msg}"),
            Self::CapabilityProbeFailed(msg) => write!(f, "capability probe failed: {msg}"),
            Self::CapabilityCheckFailed(msg) => write!(f, "capability check failed: {msg}"),
            Self::KillFailed(msg) => write!(f, "kill failed: {msg}"),
            Self::AlreadyRunning(id) => write!(f, "agent already running: {}", id.0),
            Self::NotRunning(id) => write!(f, "agent not running: {}", id.0),
            Self::NoAttachedViewer => write!(f, "no attached viewer"),
            Self::WriteFailed(msg) => write!(f, "write failed: {msg}"),
            Self::ResizeFailed(msg) => write!(f, "resize failed: {msg}"),
            Self::OrphanBlocked(id) => write!(
                f,
                "relaunch blocked: orphan worker for agent {} still alive; retry after cleanup",
                id.0
            ),
        }
    }
}

impl std::error::Error for RuntimeError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::SessionHostStaging(error) => Some(error),
            _ => None,
        }
    }
}

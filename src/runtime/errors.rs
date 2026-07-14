//! Runtime error types.
//!
//! @plan PLAN-20260216-FIRSTVERSION-V1.P06
//! @requirement REQ-TECH-004

use crate::domain::AgentId;
use crate::domain::VersionSelectorError;

use super::agent_executable::AgentExecutableError;
use super::multiplexer::MultiplexerError;

/// Destructive phase reached by a prepared runtime replacement failure.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReplacementFailurePhase {
    /// The old session could not be killed and may still be running.
    Kill,
    /// The old session was killed, but its replacement could not be spawned.
    Spawn,
}

/// Errors from runtime operations.
#[derive(Debug, Clone)]
pub enum RuntimeError {
    /// Session not found by name.
    SessionNotFound(String),
    /// Failed to attach to session.
    AttachFailed(String),
    /// Failed to spawn session.
    SpawnFailed(String),
    /// Local agent executable resolution or launch-strategy failure.
    AgentExecutable(AgentExecutableError),
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
    /// A prepared replacement failed after preparation, with the destructive
    /// phase retained explicitly for app-state reconciliation.
    ReplacementFailed {
        phase: ReplacementFailurePhase,
        source: Box<Self>,
    },
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
    /// A persisted LLxprt version selector is structurally invalid (e.g.
    /// contains an embedded NUL byte). Rejected at the runtime boundary
    /// before any destructive session kill (issue #269).
    InvalidVersionSelector(VersionSelectorError),
}

impl std::fmt::Display for RuntimeError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::SessionNotFound(name) => write!(f, "session not found: {name}"),
            Self::AttachFailed(msg) => write!(f, "attach failed: {msg}"),
            Self::SpawnFailed(msg) => write!(f, "spawn failed: {msg}"),
            Self::AgentExecutable(error) => write!(f, "agent launch unavailable: {error}"),
            Self::Multiplexer(error) => write!(f, "multiplexer dependency failed: {error}"),
            Self::RemoteExecutionFailed(msg) => write!(f, "remote execution failed: {msg}"),
            Self::CapabilityProbeFailed(msg) => write!(f, "capability probe failed: {msg}"),
            Self::CapabilityCheckFailed(msg) => write!(f, "capability check failed: {msg}"),
            Self::KillFailed(msg) => write!(f, "kill failed: {msg}"),
            Self::ReplacementFailed { phase, source } => {
                write!(f, "replacement {phase:?} phase failed: {source}")
            }
            Self::AlreadyRunning(id) => write!(f, "agent already running: {}", id.0),
            Self::NotRunning(id) => write!(f, "agent not running: {}", id.0),
            Self::NoAttachedViewer => write!(f, "no attached viewer"),
            Self::WriteFailed(msg) => write!(f, "write failed: {msg}"),
            Self::ResizeFailed(msg) => write!(f, "resize failed: {msg}"),
            Self::InvalidVersionSelector(error) => {
                write!(f, "invalid LLxprt version selector: {error}")
            }
        }
    }
}

impl std::error::Error for RuntimeError {}

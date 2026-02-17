//! Runtime error types.
//!
//! @plan PLAN-20260216-FIRSTVERSION-V1.P06
//! @requirement REQ-TECH-004

use crate::domain::AgentId;

/// Errors from runtime operations.
#[derive(Debug, Clone)]
pub enum RuntimeError {
    /// Session not found by name.
    SessionNotFound(String),
    /// Failed to attach to session.
    AttachFailed(String),
    /// Failed to spawn session.
    SpawnFailed(String),
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
}

impl std::fmt::Display for RuntimeError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::SessionNotFound(name) => write!(f, "session not found: {name}"),
            Self::AttachFailed(msg) => write!(f, "attach failed: {msg}"),
            Self::SpawnFailed(msg) => write!(f, "spawn failed: {msg}"),
            Self::KillFailed(msg) => write!(f, "kill failed: {msg}"),
            Self::AlreadyRunning(id) => write!(f, "agent already running: {}", id.0),
            Self::NotRunning(id) => write!(f, "agent not running: {}", id.0),
            Self::NoAttachedViewer => write!(f, "no attached viewer"),
            Self::WriteFailed(msg) => write!(f, "write failed: {msg}"),
            Self::ResizeFailed(msg) => write!(f, "resize failed: {msg}"),
        }
    }
}

impl std::error::Error for RuntimeError {}

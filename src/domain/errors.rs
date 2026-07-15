//! Domain types for the in-app error log.
//!
//! Errors captured from runtime failures, GitHub operations, and validation
//! paths are recorded here so the user can review them in the dedicated errors
//! panel (issue #292). These are plain data records — all state transitions
//! happen in the reducer layer.

use serde::{Deserialize, Serialize};

/// Maximum number of errors retained in the ring buffer.
pub const ERROR_STORE_CAPACITY: usize = 50;

/// The source screen or subsystem that produced an error, for display.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum ErrorSource {
    Issues,
    PullRequests,
    Actions,
    Persistence,
    Agent,
    Startup,
    #[default]
    Other,
}

impl ErrorSource {
    #[must_use]
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Issues => "issues",
            Self::PullRequests => "prs",
            Self::Actions => "actions",
            Self::Persistence => "persistence",
            Self::Agent => "agent",
            Self::Startup => "startup",
            Self::Other => "other",
        }
    }
}

/// A single captured error entry in the error log.
///
/// `title` is a short summary for the list pane; `detail` is the full error
/// message for the detail pane. Both are stored as plain strings because they
/// originate from heterogeneous sources (GitHub API errors, persistence
/// failures, validation messages).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ErrorEntry {
    /// Monotonically increasing sequence number for stable ordering.
    pub seq: u64,
    /// Short title (first line / summary) shown in the list pane.
    pub title: String,
    /// Full error message shown in the detail pane.
    pub detail: String,
    /// Where the error originated.
    pub source: ErrorSource,
    /// Unix epoch seconds (UTC) of when the error was captured.
    pub timestamp: String,
}

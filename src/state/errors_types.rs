//! Errors-mode aggregate state types (issue #292).
//!
//! The error log is a bounded ring buffer of the last N errors captured from
//! runtime failures, GitHub operations, and validation paths. It is purely
//! local (no remote fetching), so all data is eager-loaded.

use crate::domain::{ERROR_STORE_CAPACITY, ErrorEntry, ErrorSource};

/// Focus domain within Errors Mode — mirrors Issues/PRs/Actions mode.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum ErrorsFocus {
    RepoList,
    #[default]
    ErrorList,
    ErrorDetail,
}

/// Aggregate state for Errors Mode.
#[derive(Debug, Clone)]
pub struct ErrorsState {
    pub active: bool,
    /// Ring buffer of captured errors, newest first.
    pub errors: Vec<ErrorEntry>,
    /// Next sequence number to assign.
    pub next_seq: u64,
    /// Currently selected error index in the list (for detail display).
    pub selected_index: Option<usize>,
    /// Scroll offset for the detail pane viewport.
    pub detail_scroll_offset: usize,
    /// Last rendered detail viewport height in rows.
    pub detail_viewport_rows: usize,
    pub focus: ErrorsFocus,
    /// Saved agent-mode focus for restoration on exit (mirrors issues/prs/actions).
    pub prior_agent_focus: Option<super::PriorAgentFocus>,
    // ── Error-capture dedup tracking (runtime-only) ────────────────────────
    //
    // `finalize_message` runs after every reducer step and inspects the various
    // error slots. These fields remember the last text seen in each slot so we
    // only push a new entry when the text actually changes (not on every
    // subsequent message that leaves the same error visible).
    last_captured_global: Option<String>,
    last_captured_issues: Option<String>,
    last_captured_prs: Option<String>,
    last_captured_actions: Option<String>,
}

impl ErrorsState {
    /// Reset the global-error dedup tracker (called when `error_message` is None).
    pub(super) fn reset_global_tracker(&mut self) {
        self.last_captured_global = None;
    }

    /// Reset the issues-error dedup tracker.
    pub(super) fn reset_issues_tracker(&mut self) {
        self.last_captured_issues = None;
    }

    /// Reset the PRs-error dedup tracker.
    pub(super) fn reset_prs_tracker(&mut self) {
        self.last_captured_prs = None;
    }

    /// Reset the Actions-error dedup tracker.
    pub(super) fn reset_actions_tracker(&mut self) {
        self.last_captured_actions = None;
    }

    /// Read-only snapshot of the global dedup tracker for fast-change checks.
    pub(super) fn last_captured_global_snapshot(&self) -> Option<&str> {
        self.last_captured_global.as_deref()
    }

    /// Read-only snapshot of the issues dedup tracker.
    pub(super) fn last_captured_issues_snapshot(&self) -> Option<&str> {
        self.last_captured_issues.as_deref()
    }

    /// Read-only snapshot of the PRs dedup tracker.
    pub(super) fn last_captured_prs_snapshot(&self) -> Option<&str> {
        self.last_captured_prs.as_deref()
    }

    /// Read-only snapshot of the actions dedup tracker.
    pub(super) fn last_captured_actions_snapshot(&self) -> Option<&str> {
        self.last_captured_actions.as_deref()
    }
}

impl Default for ErrorsState {
    fn default() -> Self {
        Self {
            active: false,
            errors: Vec::new(),
            next_seq: 1,
            selected_index: None,
            detail_scroll_offset: 0,
            detail_viewport_rows: 0,
            focus: ErrorsFocus::ErrorList,
            prior_agent_focus: None,
            last_captured_global: None,
            last_captured_issues: None,
            last_captured_prs: None,
            last_captured_actions: None,
        }
    }
}

impl ErrorsState {
    /// Push a new error entry, evicting the oldest when at capacity.
    /// Returns the entry that was stored (with its assigned seq).
    pub fn push(&mut self, title: String, detail: String, source: ErrorSource, timestamp: String) {
        let entry = ErrorEntry {
            seq: self.next_seq,
            title,
            detail,
            source,
            timestamp,
        };
        self.next_seq = self.next_seq.saturating_add(1);
        self.errors.insert(0, entry);
        if self.errors.len() > ERROR_STORE_CAPACITY {
            self.errors.truncate(ERROR_STORE_CAPACITY);
        }
        // Reset selection to the newest error.
        self.selected_index = Some(0);
        self.detail_scroll_offset = 0;
    }

    /// The most recent error, if any.
    #[must_use]
    pub fn last_error(&self) -> Option<&ErrorEntry> {
        self.errors.first()
    }

    /// Whether the error log is empty.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.errors.is_empty()
    }

    /// The currently selected error, if any.
    #[must_use]
    pub fn selected_error(&self) -> Option<&ErrorEntry> {
        self.selected_index.and_then(|idx| self.errors.get(idx))
    }

    /// Number of stored errors.
    #[must_use]
    pub fn count(&self) -> usize {
        self.errors.len()
    }

    /// Capture a global `error_message` change. Returns `true` if a new entry
    /// was pushed (caller may clear `error_message` to avoid duplicate UI
    /// rendering).
    pub fn capture_global(&mut self, msg: &str, source: ErrorSource, timestamp: &str) -> bool {
        if self.last_captured_global.as_deref() == Some(msg) {
            return false;
        }
        self.last_captured_global = Some(msg.to_string());
        self.push(
            msg.to_string(),
            msg.to_string(),
            source,
            timestamp.to_string(),
        );
        true
    }

    /// Capture a per-mode issues error change.
    pub fn capture_issues(&mut self, msg: &str, timestamp: &str) -> bool {
        if self.last_captured_issues.as_deref() == Some(msg) {
            return false;
        }
        self.last_captured_issues = Some(msg.to_string());
        self.push(
            msg.to_string(),
            msg.to_string(),
            ErrorSource::Issues,
            timestamp.to_string(),
        );
        true
    }

    /// Capture a per-mode PRs error change.
    pub fn capture_prs(&mut self, msg: &str, timestamp: &str) -> bool {
        if self.last_captured_prs.as_deref() == Some(msg) {
            return false;
        }
        self.last_captured_prs = Some(msg.to_string());
        self.push(
            msg.to_string(),
            msg.to_string(),
            ErrorSource::PullRequests,
            timestamp.to_string(),
        );
        true
    }

    /// Capture a per-mode Actions error change.
    pub fn capture_actions(&mut self, msg: &str, timestamp: &str) -> bool {
        if self.last_captured_actions.as_deref() == Some(msg) {
            return false;
        }
        self.last_captured_actions = Some(msg.to_string());
        self.push(
            msg.to_string(),
            msg.to_string(),
            ErrorSource::Actions,
            timestamp.to_string(),
        );
        true
    }
}

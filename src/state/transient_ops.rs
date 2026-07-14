//! Transient agent state-transition helpers (issue #213).
//!
//! Extracted from `mod.rs` to keep that file under the source-file-size
//! hard limit. These methods are the deterministic reducer transitions for
//! transient-agent queue events.

use crate::state::AppState;

impl AppState {
    /// Set the transient-agent-queued draft notice on both issues and PRs
    /// draft-notice fields (issue #213). Position 0 means "launching next";
    /// any higher number is a 1-based queue position.
    pub(crate) fn apply_transient_queued(&mut self, queue_position: usize) {
        let notice = if queue_position == 0 {
            "Transient agent queued — launching next…".to_string()
        } else {
            format!("Transient agent queued (position {queue_position})")
        };
        self.issues_state.draft_notice = Some(notice.clone());
        self.prs_state.draft_notice = Some(notice);
    }

    /// Clear the transient-agent draft notice on both issues and PRs
    /// (issue #213). Called when a transient agent is dequeued (launched).
    pub(crate) fn clear_transient_notice(&mut self) {
        self.issues_state.draft_notice = None;
        self.prs_state.draft_notice = None;
    }
}

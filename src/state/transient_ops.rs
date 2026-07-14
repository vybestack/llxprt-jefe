//! Transient agent state-transition helpers (issue #213).
//!
//! Extracted from `mod.rs` to keep that file under the source-file-size
//! hard limit. These methods are the deterministic reducer transitions for
//! transient-agent queue events.

use crate::state::AppState;
use crate::state::types::QueuedTransientSend;

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
    pub fn clear_transient_notice(&mut self) {
        self.issues_state.draft_notice = None;
        self.prs_state.draft_notice = None;
    }

    /// Push a queued transient send onto the queue and return its 1-based
    /// position (issue #213).
    pub fn push_transient_queue_item(&mut self, item: QueuedTransientSend) -> usize {
        self.transient_queue.pending.push(item);
        self.transient_queue.pending.len()
    }

    /// Pop the oldest queued transient send for a given repository (issue #213).
    /// Returns the item if one was found and removed.
    pub fn pop_transient_queue_for_repo(
        &mut self,
        repo_id: &crate::domain::RepositoryId,
    ) -> Option<QueuedTransientSend> {
        let pos = self
            .transient_queue
            .pending
            .iter()
            .position(|q| &q.repository_id == repo_id)?;
        Some(self.transient_queue.pending.remove(pos))
    }
}

//! Reducers for the multi-agent workbench (issue #626).
//!
//! These live beside the rest of the workbench state rather than in
//! `state/mod.rs` so the central reducer file stays inside the source-size
//! gate. They are pure: they mutate only `AppState` and perform no I/O.

use super::AppState;
use super::workbench_filter::WorkbenchStatusFilter;
use crate::messages::UiNavigationMessage;
use crate::workbench_view::StatusBucket;

/// The filter rail lists the buckets in this order, top to bottom.
const FILTER_ORDER: [StatusBucket; 4] = [
    StatusBucket::NeedsYou,
    StatusBucket::Working,
    StatusBucket::Ready,
    StatusBucket::Stale,
];

impl AppState {
    /// The bucket the filter cursor currently sits on.
    #[must_use]
    pub fn workbench_filter_cursor_bucket(&self) -> StatusBucket {
        FILTER_ORDER[self.workbench_filter_cursor.min(FILTER_ORDER.len() - 1)]
    }

    /// Handle multi-agent workbench navigation messages.
    ///
    /// Paging deliberately has no upper bound here. The number of pages depends
    /// on terminal size, which is a render-time fact and is not part of
    /// `AppState`, so the projection clamps the requested page against the real
    /// page count when it builds the view.
    pub(super) fn apply_workbench_navigation(&mut self, message: UiNavigationMessage) {
        match message {
            UiNavigationMessage::ToggleWorkbenchStatusBucket(bucket) => {
                self.apply_workbench_status_toggle(bucket);
            }
            UiNavigationMessage::WorkbenchNextPage => {
                self.workbench_page = self.workbench_page.saturating_add(1);
            }
            UiNavigationMessage::WorkbenchPrevPage => {
                self.workbench_page = self.workbench_page.saturating_sub(1);
            }
            UiNavigationMessage::WorkbenchFilterCursorPrev => {
                self.workbench_filter_cursor = self.workbench_filter_cursor.saturating_sub(1);
            }
            UiNavigationMessage::WorkbenchFilterCursorNext => {
                self.workbench_filter_cursor =
                    (self.workbench_filter_cursor + 1).min(FILTER_ORDER.len() - 1);
            }
            _ => unreachable!("non-workbench message routed to apply_workbench_navigation"),
        }
    }

    /// Toggle one status bucket in the workbench filter mask and reset the page
    /// to 0, so a shrinking list cannot strand the view on an empty page.
    fn apply_workbench_status_toggle(&mut self, bucket: StatusBucket) {
        let current = self.workbench_status_filter.mask();
        self.workbench_status_filter =
            WorkbenchStatusFilter(current.with(bucket, !current.allows(bucket)));
        self.workbench_page = 0;
    }
}

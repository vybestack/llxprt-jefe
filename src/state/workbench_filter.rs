//! Workbench status-filter state (issue #626).
//!
//! Lives outside `types.rs` because that file sits at the source-size gate,
//! and because the default-flipping wrapper is a self-contained concern.

use crate::workbench_view::StatusFilterMask;

/// Newtype wrapper around [`StatusFilterMask`] whose [`Default`] is all-on.
///
/// The projection's `StatusFilterMask` derives `Default = all_off`, which is
/// the empty-state trigger. `AppState` cannot use that derive directly for its
/// `workbench_status_filter` field or a freshly started app would show the
/// "everything is filtered out" message instead of its agents. This wrapper
/// flips the default to all-on and delegates every operation to the inner mask.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WorkbenchStatusFilter(pub StatusFilterMask);

impl Default for WorkbenchStatusFilter {
    fn default() -> Self {
        Self(StatusFilterMask::all_on())
    }
}

impl WorkbenchStatusFilter {
    /// The inner mask.
    #[must_use]
    pub const fn mask(&self) -> StatusFilterMask {
        self.0
    }
}

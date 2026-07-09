//! Layout descriptor passed to [`crate::selection::pane_at`].
//!
//! Carries the raw terminal size plus the conditional band flags (error banner,
//! filter controls) that affect the vertical row split in Issues / PR mode.
//! Keeping these in a single struct keeps [`crate::selection::pane_at`]'s
//! signature stable as more conditional bands are added.

use crate::state::ScreenMode;

/// Inputs needed to compute pane geometry for the current screen.
///
/// All fields are plain values (no iocraft types) so the descriptor can be
/// constructed cheaply from [`crate::state::AppState`] snapshots and passed to
/// the pure [`crate::selection::pane_at`] function.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ScreenLayout {
    /// Raw terminal width in columns (as reported by crossterm).
    pub term_cols: u16,
    /// Raw terminal height in rows (as reported by crossterm).
    pub term_rows: u16,
    /// Active screen mode (drives the layout template).
    pub screen_mode: ScreenMode,
    /// Whether an error banner is visible in the workspace (Issues/PR mode).
    pub error_visible: bool,
    /// Whether the filter-controls band is open (Issues/PR mode).
    pub filter_controls_open: bool,
}

impl ScreenLayout {
    /// Construct a layout descriptor from its raw fields.
    #[must_use]
    pub const fn new(
        term_cols: u16,
        term_rows: u16,
        screen_mode: ScreenMode,
        error_visible: bool,
        filter_controls_open: bool,
    ) -> Self {
        Self {
            term_cols,
            term_rows,
            screen_mode,
            error_visible,
            filter_controls_open,
        }
    }

    /// Whether this layout is for PR mode (affects pane identity, not geometry).
    #[must_use]
    pub fn is_pr_mode(self) -> bool {
        matches!(self.screen_mode, ScreenMode::DashboardPullRequests)
    }
}

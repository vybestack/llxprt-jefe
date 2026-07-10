//! Layout descriptor passed to [`crate::selection::pane_at`].
//!
//! Carries the raw terminal size plus the conditional band flags (error banner,
//! filter controls) that affect the vertical row split in Issues / PR mode.
//! Keeping these in a single struct keeps [`crate::selection::pane_at`]'s
//! signature stable as more conditional bands are added.

use crate::selection::SelectablePane;
use crate::state::ScreenMode;

/// Which overlay (modal / form / chooser) is currently active, if any.
///
/// When an overlay is active, [`crate::selection::pane_at`] resolves
/// coordinates to the overlay pane instead of the underlying screen panes so
/// mouse text-selection works inside the overlay (issue #178).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum OverlayPane {
    /// No overlay active — normal pane layout.
    #[default]
    None,
    /// Help modal (full-screen overlay).
    HelpModal,
    /// Agent-definition form (full-screen overlay).
    AgentForm,
    /// Repository-definition form (full-screen overlay).
    RepositoryForm,
    /// Confirmation dialog (full-screen overlay).
    ConfirmModal,
    /// Agent chooser (positioned overlay inside the workspace).
    AgentChooser,
    /// Merge chooser (positioned overlay inside the workspace).
    MergeChooser,
}

impl OverlayPane {
    /// Map this overlay to its [`SelectablePane`] identity.
    #[must_use]
    pub const fn to_pane(self) -> Option<SelectablePane> {
        match self {
            Self::None => None,
            Self::HelpModal => Some(SelectablePane::HelpModal),
            Self::AgentForm => Some(SelectablePane::AgentForm),
            Self::RepositoryForm => Some(SelectablePane::RepositoryForm),
            Self::ConfirmModal => Some(SelectablePane::ConfirmModal),
            Self::AgentChooser => Some(SelectablePane::AgentChooser),
            Self::MergeChooser => Some(SelectablePane::MergeChooser),
        }
    }

    /// Whether this overlay is a full-screen modal/form (covers all panes).
    ///
    /// Full-screen overlays intercept *every* coordinate; positioned overlays
    /// (choosers) only intercept coordinates inside their bounds.
    #[must_use]
    pub const fn is_full_screen(self) -> bool {
        matches!(
            self,
            Self::HelpModal | Self::AgentForm | Self::RepositoryForm | Self::ConfirmModal
        )
    }
}

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
    /// Active overlay (modal/form/chooser), if any (issue #178).
    pub overlay: OverlayPane,
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
            overlay: OverlayPane::None,
        }
    }

    /// Whether this layout is for PR mode (affects pane identity, not geometry).
    #[must_use]
    pub fn is_pr_mode(self) -> bool {
        matches!(self.screen_mode, ScreenMode::DashboardPullRequests)
    }

    /// Return a copy of this layout with the given overlay active (issue #178).
    #[must_use]
    pub const fn with_overlay(self, overlay: OverlayPane) -> Self {
        Self { overlay, ..self }
    }
}

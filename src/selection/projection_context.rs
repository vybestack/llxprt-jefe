//! Explicit runtime inputs for pure pane-content projection.

use crate::dashboard_git_info::DashboardGitInfoSnapshot;
use crate::runtime::TerminalSnapshot;

/// Runtime snapshots passed into side-effect-free pane projection.
pub struct PaneContentContext<'a> {
    pub terminal_snapshot: Option<&'a TerminalSnapshot>,
    pub history_lines: &'a [String],
    pub term_cols: u16,
    pub term_rows: u16,
    pub dashboard_git_info: Option<&'a DashboardGitInfoSnapshot>,
}

//! Hit-testing against the resolved geometry snapshot (issue #384).
//!
//! This is the bridge between the descriptor's panel identities and the
//! selection layer's pane vocabulary. When a snapshot is available the pane
//! under a cell is whatever the snapshot says occupies it, rather than the
//! result of re-deriving the screen's arithmetic — which is how the two used to
//! drift the moment a band opened or the terminal was resized.
//!
//! Bands (the search row, notice banners, filter controls) map to no pane
//! because they were never selectable; a click on one yields `None`, exactly as
//! before.

use crate::selection::geometry::PaneGeometry;
use crate::selection::text::SelectablePane;
use crate::workbench::{PanelId, ResolvedLayout, ResolvedPanel, ScreenId};

/// The selectable pane a descriptor panel corresponds to.
///
/// Returns `None` for panels that are not selectable, which is every
/// conditional band.
#[must_use]
pub fn panel_to_selectable(panel: PanelId) -> Option<SelectablePane> {
    match panel.as_str() {
        "repositories" => Some(SelectablePane::Sidebar),
        "agents" => Some(SelectablePane::AgentList),
        "terminal" | "shell-preview" => Some(SelectablePane::TerminalView),
        "preview" => Some(SelectablePane::Preview),
        "issue-list" => Some(SelectablePane::IssueList),
        "issue-detail" => Some(SelectablePane::IssueDetail),
        "pr-list" => Some(SelectablePane::PrList),
        "pr-detail" => Some(SelectablePane::PrDetail),
        "action-list" => Some(SelectablePane::ActionsList),
        "action-detail" => Some(SelectablePane::ActionsDetail),
        "error-list" => Some(SelectablePane::ErrorList),
        "error-detail" => Some(SelectablePane::ErrorDetail),
        _ => None,
    }
}

/// The pane rectangle of one resolved panel.
///
/// The snapshot already carries both the chrome rectangle and the content
/// rectangle inside it, so nothing here re-derives a chrome offset.
#[must_use]
pub fn pane_geometry(panel: &ResolvedPanel) -> PaneGeometry {
    PaneGeometry::new(
        panel.chrome.col,
        panel.chrome.row,
        panel.chrome.width,
        panel.chrome.height,
        panel.content.col,
        panel.content.row,
    )
}

/// The selectable pane under a cell, according to the snapshot.
///
/// `terminal_input_enabled` excludes the terminal pane: while the terminal has
/// focus its mouse events are forwarded to the child process, so a drag there
/// must not start an application selection.
#[must_use]
pub fn pane_at_resolved(
    col: u16,
    row: u16,
    layout: &ResolvedLayout,
    terminal_input_enabled: bool,
) -> Option<(SelectablePane, PaneGeometry)> {
    let panel = layout.panel_at(col, row)?;
    let pane = panel_to_selectable(panel.id)?;
    if terminal_input_enabled && pane == SelectablePane::TerminalView {
        return None;
    }
    Some((pane, pane_geometry(panel)))
}

/// Columns a detail pane reserves *inside* its content rectangle for the
/// scrollbar and the trailing safety margin.
///
/// This is renderer chrome, not pane geometry: it sits within the rectangle the
/// resolver hands out, so the descriptor does not model it and the wrap width
/// subtracts it here.
pub const DETAIL_INNER_RESERVED_COLS: u16 = 2;

/// The width detail text wraps at, taken from the snapshot.
///
/// Returns `None` when the pane is not showing, in which case there is nothing
/// to wrap.
#[must_use]
pub fn detail_wrap_width(
    layout: &ResolvedLayout,
    pane: SelectablePane,
    screen: ScreenId,
) -> Option<usize> {
    let panel = selectable_to_panel(pane, screen)?;
    let resolved = layout.panel(&panel)?;
    if !resolved.visible {
        return None;
    }
    Some(usize::from(
        resolved
            .content
            .width
            .saturating_sub(DETAIL_INNER_RESERVED_COLS),
    ))
}

/// The panel identity a selectable pane corresponds to on a screen.
///
/// The inverse of [`panel_to_selectable`], for consumers that hold a pane and
/// need its rectangle from the snapshot.
///
/// The screen is a parameter because one pane can name different panels on
/// different screens: the embedded terminal is `terminal` on the dashboard and
/// `shell-preview` in the Terminal Manager. Resolving without the screen would
/// silently miss the Terminal Manager's pane.
#[must_use]
pub fn selectable_to_panel(pane: SelectablePane, screen: ScreenId) -> Option<PanelId> {
    let id = match pane {
        SelectablePane::Sidebar => "repositories",
        SelectablePane::AgentList => "agents",
        SelectablePane::TerminalView => match screen {
            ScreenId::Terminals => "shell-preview",
            _ => "terminal",
        },
        SelectablePane::Preview => "preview",
        SelectablePane::IssueList => "issue-list",
        SelectablePane::IssueDetail => "issue-detail",
        SelectablePane::PrList => "pr-list",
        SelectablePane::PrDetail => "pr-detail",
        SelectablePane::ActionsList => "action-list",
        SelectablePane::ActionsDetail => "action-detail",
        SelectablePane::ErrorList => "error-list",
        SelectablePane::ErrorDetail => "error-detail",
        _ => return None,
    };
    Some(PanelId::from_static(id))
}

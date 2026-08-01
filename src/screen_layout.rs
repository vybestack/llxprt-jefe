//! The single place a screen's geometry snapshot is produced (issue #384).
//!
//! Geometry is resolved exactly once per size or state change, here, and every
//! consumer reads the resulting [`ResolvedLayout`]. Nothing else may derive a
//! panel rectangle: a consumer that measures the terminal itself will disagree
//! with the renderer the moment a band opens, a pane collapses, or the terminal
//! is resized mid-frame.
//!
//! This module is the boundary between the application's state and the I/O-free
//! resolver. It answers two questions the resolver cannot: how much of the
//! terminal the screen may use once global chrome is removed, and which panels
//! the application is currently hiding.

use crate::layout::{OUTER_BARS_HEIGHT, effective_render_size};
use crate::state::AppState;
use crate::workbench::{
    PanelId, PanelState, Rect, ResolvedLayout, ScreenId, ScreenInstanceId, resolve_layout,
    screen_descriptor,
};

/// Resolve the active screen's geometry for a terminal size.
///
/// Returns `None` only if the compiled descriptor table is malformed, which
/// startup already rejects, or if the arithmetic would leave the checked range.
#[must_use]
pub fn resolve_screen(state: &AppState, term_cols: u16, term_rows: u16) -> Option<ResolvedLayout> {
    let descriptor = screen_descriptor(state.screen).ok()?;
    let outer = screen_rect(term_cols, term_rows);
    let panel_state = hidden_panels(state);
    resolve_layout(descriptor, ScreenInstanceId::next(), outer, &panel_state).ok()
}

/// The rectangle a screen may use, after the status bar and keybind bar.
///
/// Global chrome is removed exactly once, here, so no descriptor has to model
/// it and no panel can accidentally overlap a bar.
#[must_use]
pub fn screen_rect(term_cols: u16, term_rows: u16) -> Rect {
    let (render_cols, render_rows) = effective_render_size(term_cols, term_rows);
    Rect::new(
        0,
        1,
        render_cols,
        render_rows.saturating_sub(OUTER_BARS_HEIGHT),
    )
}

/// Which panels the application is currently hiding.
///
/// These are the conditions the descriptor deliberately does not model: a band
/// that is only shown while a filter is open, a preview that is replaced by an
/// overlay, a detail pane with nothing selected.
fn hidden_panels(state: &AppState) -> PanelState {
    let mut panel_state = PanelState::all_visible();
    for panel in hidden_panel_ids(state) {
        panel_state = panel_state.hiding(&panel);
    }
    panel_state
}

/// Panel identities the application is hiding on the active screen.
fn hidden_panel_ids(state: &AppState) -> Vec<PanelId> {
    let mut hidden = Vec::new();
    match state.screen {
        ScreenId::Dashboard => {
            if !state.dashboard_search_active() && !state.dashboard_search.input_focused {
                hidden.push(PanelId::from_static("search"));
            }
            if state.shell_overlay_active() {
                // The embedded shell takes the whole workspace, so the agent
                // list and preview are not on screen at all.
                hidden.push(PanelId::from_static("agents"));
                hidden.push(PanelId::from_static("preview"));
            }
        }
        // The split view, the errors screen, and the Terminal Manager render
        // no conditional band, so nothing is ever hidden on them.
        ScreenId::Repositories | ScreenId::Errors | ScreenId::Terminals => {}
        ScreenId::Issues => {
            push_band_state(
                &mut hidden,
                "issue-list",
                state.error_message.is_some(),
                state.issues_state.filter_ui.controls_open,
            );
        }
        ScreenId::PullRequests => {
            push_band_state(
                &mut hidden,
                "pr-list",
                state.error_message.is_some(),
                state.prs_state.filter_ui.controls_open,
            );
        }
        ScreenId::Actions => {
            push_band_state(
                &mut hidden,
                "action-list",
                state.error_message.is_some(),
                state.actions_state.ui.filter_ui_open,
            );
        }
    }
    hidden
}

/// Hide the notice banner and filter band unless the screen is showing them.
fn push_band_state(
    hidden: &mut Vec<PanelId>,
    list: &'static str,
    banner_visible: bool,
    filter_open: bool,
) {
    if !banner_visible {
        hidden.push(banner_panel(list));
    }
    if !filter_open {
        hidden.push(filter_panel(list));
    }
}

/// The notice-band panel belonging to a workspace list.
#[must_use]
pub fn banner_panel(list: &'static str) -> PanelId {
    match list {
        "issue-list" => PanelId::from_static("issue-list-banner"),
        "pr-list" => PanelId::from_static("pr-list-banner"),
        _ => PanelId::from_static("action-list-banner"),
    }
}

/// The filter-band panel belonging to a workspace list.
#[must_use]
pub fn filter_panel(list: &'static str) -> PanelId {
    match list {
        "issue-list" => PanelId::from_static("issue-list-filter"),
        "pr-list" => PanelId::from_static("pr-list-filter"),
        _ => PanelId::from_static("action-list-filter"),
    }
}

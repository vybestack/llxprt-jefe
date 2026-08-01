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
/// Returns `None` only for conditions startup already rules out: a malformed
/// compiled descriptor table, or allocation arithmetic leaving the checked
/// range. Both are logged rather than swallowed, because a frame that silently
/// falls back to no geometry is far harder to diagnose than one that says why.
#[must_use]
pub fn resolve_screen(state: &AppState, term_cols: u16, term_rows: u16) -> Option<ResolvedLayout> {
    let descriptor = match screen_descriptor(state.screen) {
        Ok(descriptor) => descriptor,
        Err(error) => {
            tracing::error!(screen = %state.screen, %error, "no compiled descriptor for the active screen");
            return None;
        }
    };
    let outer = screen_rect(term_cols, term_rows);
    let panel_state = hidden_panels(state);
    match resolve_layout(descriptor, ScreenInstanceId::next(), outer, &panel_state) {
        Ok(layout) => Some(layout),
        Err(error) => {
            tracing::error!(screen = %state.screen, %error, ?outer, "layout resolution failed");
            None
        }
    }
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
///
/// The identities are literals, so nothing but a test can notice a descriptor
/// renaming a panel out from under them; `screen_layout_tests` asserts every
/// identity produced here is declared by the screen it names.
pub(crate) fn hidden_panel_ids(state: &AppState) -> Vec<PanelId> {
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
                WorkspaceBands {
                    banner: PanelId::from_static("issue-list-banner"),
                    filter: PanelId::from_static("issue-list-filter"),
                },
                state.error_message.is_some(),
                state.issues_state.filter_ui.controls_open,
            );
        }
        ScreenId::PullRequests => {
            push_band_state(
                &mut hidden,
                WorkspaceBands {
                    banner: PanelId::from_static("pr-list-banner"),
                    filter: PanelId::from_static("pr-list-filter"),
                },
                state.error_message.is_some(),
                state.prs_state.filter_ui.controls_open,
            );
        }
        ScreenId::Actions => {
            push_band_state(
                &mut hidden,
                WorkspaceBands {
                    banner: PanelId::from_static("action-list-banner"),
                    filter: PanelId::from_static("action-list-filter"),
                },
                state.error_message.is_some(),
                state.actions_state.ui.filter_ui_open,
            );
        }
    }
    hidden
}

/// The two conditional bands a workspace screen declares.
///
/// The identities are passed in rather than derived from the list name: a
/// derivation would need a fallback arm, and a fallback that guesses the wrong
/// band is a silent rendering bug instead of a compile error.
struct WorkspaceBands {
    /// The notice-band panel.
    banner: PanelId,
    /// The filter-controls panel.
    filter: PanelId,
}

/// Hide the notice banner and filter band unless the screen is showing them.
fn push_band_state(
    hidden: &mut Vec<PanelId>,
    bands: WorkspaceBands,
    banner_visible: bool,
    filter_open: bool,
) {
    if !banner_visible {
        hidden.push(bands.banner);
    }
    if !filter_open {
        hidden.push(bands.filter);
    }
}

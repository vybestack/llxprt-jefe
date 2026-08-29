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
use crate::messages::settings::SettingsSection;
use crate::state::AppState;
use crate::workbench::{
    PTY_PANEL_TYPE, PanelId, PanelState, Rect, ResolvedLayout, ScreenId, pty_content_rect,
    resolve_layout,
};

/// Resolve the active screen's geometry for a terminal size.
///
/// Returns `None` only for conditions startup already rules out: a malformed
/// compiled descriptor table, or allocation arithmetic leaving the checked
/// range. Both are logged rather than swallowed, because a frame that silently
/// falls back to no geometry is far harder to diagnose than one that says why.
#[must_use]
pub fn resolve_screen(state: &AppState, term_cols: u16, term_rows: u16) -> Option<ResolvedLayout> {
    let screen = state.screen();
    let registry = state.published_workbench().screen_registry();
    let Some(descriptor) = registry.get_identity(screen) else {
        tracing::error!(screen = %screen, "no descriptor for the active screen");
        return None;
    };
    let outer = screen_rect(term_cols, term_rows);
    let panel_state = hidden_panels(state);
    match resolve_layout(descriptor, state.nav.current().id, outer, &panel_state) {
        Ok(layout) => Some(layout),
        Err(error) => {
            tracing::error!(screen = %state.screen(), %error, ?outer, "layout resolution failed");
            None
        }
    }
}
/// Derive the initial runtime PTY size from the first resolved screen frame.
///
/// A visible PTY panel is authoritative when the committed screen declares one.
/// Screens without a visible PTY still commit their resolved outer frame so
/// restored sessions can start without consulting ambient terminal dimensions.
#[must_use]
pub fn initial_runtime_geometry(state: &AppState) -> Option<(u16, u16)> {
    let layout = state.resolved_layout.as_ref()?;
    let descriptor = state
        .published_workbench()
        .screen_registry()
        .get_identity(state.screen())?;
    let panel_geometry = descriptor
        .panels
        .iter()
        .filter(|panel| panel.panel_type.as_str() == PTY_PANEL_TYPE)
        .find_map(|panel| pty_content_rect(descriptor, layout, &panel.id))
        .filter(|rect| rect.width > 0 && rect.height > 0)
        .map(|rect| (rect.height, rect.width));

    panel_geometry.or_else(|| {
        (layout.outer.width > 0 && layout.outer.height > 0)
            .then_some((layout.outer.height, layout.outer.width))
    })
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

fn hidden_host_panel_ids(state: &AppState) -> Option<Vec<PanelId>> {
    let descriptor = state
        .published_workbench()
        .screen_registry()
        .get_identity(state.screen())?;
    let host_controls = descriptor
        .panels
        .iter()
        .filter_map(|panel| {
            panel
                .host_capability
                .map(|capability| (panel, capability.control_kind()))
        })
        .collect::<Vec<_>>();
    if host_controls.is_empty() {
        return None;
    }
    let mut hidden = Vec::new();
    for (panel, kind) in host_controls {
        let form_is_inactive = kind == crate::host_controls::ControlKind::Form
            && !state.dashboard_filter_active()
            && state.active_overlay_kind() != Some(crate::workbench::OverlayKind::Search);
        if form_is_inactive || state.shell_overlay_active() && !panel.required {
            hidden.push(panel.id);
        }
    }
    Some(hidden)
}

/// Panel identities the application is hiding on the active screen.
///
/// The identities are literals, so nothing but a test can notice a descriptor
/// renaming a panel out from under them; `screen_layout_tests` asserts every
/// identity produced here is declared by the screen it names.
pub(crate) fn hidden_panel_ids(state: &AppState) -> Vec<PanelId> {
    let mut hidden = hidden_host_panel_ids(state).unwrap_or_default();
    let Some(screen) = state.compiled_screen() else {
        return hidden;
    };
    match screen {
        // The split view, the errors screen, and the Terminal Manager render
        // no conditional band, so nothing is ever hidden on them.
        ScreenId::Repositories | ScreenId::Errors | ScreenId::Terminals => {}
        ScreenId::Settings => push_unfocused_settings_sections(&mut hidden, state),
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

/// Hide every Settings section panel except the one in view.
///
/// The descriptor declares all three because all three are real panels with
/// their own content and focus; which one is showing is an application decision
/// the descriptor deliberately does not model.
fn push_unfocused_settings_sections(hidden: &mut Vec<PanelId>, state: &AppState) {
    for section in SettingsSection::ALL {
        if section != state.settings_state.section {
            hidden.push(PanelId::from_static(settings_section_panel(section)));
        }
    }
}

/// The panel one Settings section renders into.
pub(crate) const fn settings_section_panel(section: SettingsSection) -> &'static str {
    match section {
        SettingsSection::General => crate::workbench::SETTINGS_GENERAL_PANEL,
        SettingsSection::Appearance => crate::workbench::SETTINGS_APPEARANCE_PANEL,
        SettingsSection::AgentTypes => crate::workbench::SETTINGS_AGENT_TYPES_PANEL,
        SettingsSection::Screens => crate::workbench::SETTINGS_SCREENS_PANEL,
        SettingsSection::Keys => crate::workbench::SETTINGS_KEYS_PANEL,
        SettingsSection::Plugins => crate::workbench::SETTINGS_PLUGINS_PANEL,
        SettingsSection::Diagnostics => crate::workbench::SETTINGS_DIAGNOSTICS_PANEL,
    }
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

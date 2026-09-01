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
    PTY_PANEL_TYPE, PanelId, PanelState, Rect, ResolvedLayout, RuntimeViewport, ScreenDescriptor,
    ScreenId, pty_content_rect, resolve_layout,
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
/// The returned viewport carries the committed frame's generation so the
/// create effect can be proven current (issue #706 CWR3-02).
#[must_use]
pub fn initial_runtime_geometry(state: &AppState) -> Option<RuntimeViewport> {
    let layout = state.resolved_layout.as_ref()?;
    let descriptor = state
        .published_workbench()
        .screen_registry()
        .get_identity(state.screen())?;
    committed_viewport_from(descriptor, layout).or_else(|| {
        (layout.outer.width > 0 && layout.outer.height > 0).then_some(RuntimeViewport {
            rows: layout.outer.height,
            cols: layout.outer.width,
            generation: layout.generation,
        })
    })
}

/// The visible PTY panel's exact content rectangle in the committed frame.
///
/// Mouse hit-testing and scroll geometry need the on-screen rectangle the
/// renderer actually drew, so they read the committed frame rather than
/// re-deriving dimensions from the terminal size. `None` means this frame
/// shows no visible nonzero PTY panel — there is no rectangle to hit-test
/// against and no fabricated fallback.
#[must_use]
pub fn committed_pty_content_rect(state: &AppState) -> Option<Rect> {
    let layout = state.resolved_layout.as_ref()?;
    let descriptor = state
        .published_workbench()
        .screen_registry()
        .get_identity(state.screen())?;
    pty_content_rect_from(descriptor, layout)
}

/// The runtime viewport the committed frame offers on layout commit.
///
/// On layout commit the runtime may be offered at most one ordered resize
/// carrying this exact rectangle and generation; the manager drops offers
/// whose generation it has already superseded, so a stale completion changes
/// nothing (issue #706 CWR3-04). `None` means the frame shows no visible
/// nonzero PTY panel — a hidden or zero-size panel defers and no resize is
/// offered.
#[must_use]
pub fn committed_runtime_viewport(state: &AppState) -> Option<RuntimeViewport> {
    let layout = state.resolved_layout.as_ref()?;
    let descriptor = state
        .published_workbench()
        .screen_registry()
        .get_identity(state.screen())?;
    committed_viewport_from(descriptor, layout)
}

/// The visible PTY panel's viewport in one committed frame, with its identity.
fn committed_viewport_from(
    descriptor: &ScreenDescriptor,
    layout: &ResolvedLayout,
) -> Option<RuntimeViewport> {
    pty_content_rect_from(descriptor, layout).map(|rect| RuntimeViewport {
        rows: rect.height,
        cols: rect.width,
        generation: layout.generation,
    })
}

/// The visible PTY panel's exact content rectangle in one resolved frame.
fn pty_content_rect_from(descriptor: &ScreenDescriptor, layout: &ResolvedLayout) -> Option<Rect> {
    descriptor
        .panels
        .iter()
        .filter(|panel| panel.panel_type.as_str() == PTY_PANEL_TYPE)
        .find_map(|panel| pty_content_rect(descriptor, layout, &panel.id))
        .filter(|rect| rect.width > 0 && rect.height > 0)
}

/// The PTY viewport a resize must send for the active screen.
///
/// Resolved through the same single authority the renderer commits, so the
/// dimensions a child receives are the rectangle it occupies on screen; the
/// answer carries the generation its own resolve minted, which is the frame
/// that would commit for these inputs. `None` means this frame shows no
/// visible nonzero PTY panel and no resize may be sent — there is no
/// fabricated fallback.
#[must_use]
pub fn pty_resize_viewport(
    state: &AppState,
    term_cols: u16,
    term_rows: u16,
) -> Option<RuntimeViewport> {
    let layout = resolve_screen(state, term_cols, term_rows)?;
    let descriptor = state
        .published_workbench()
        .screen_registry()
        .get_identity(state.screen())?;
    committed_viewport_from(descriptor, &layout)
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

/// The effective render size a committed frame was resolved from.
///
/// [`screen_rect`] removes the outer bars when it derives a frame's outer
/// rectangle, so a committed layout carries the render size only in that
/// subtracted form. This inverts the subtraction once, beside the authority
/// that made it: consumers that must reproduce a display-basis viewport from
/// a committed frame (the workbench grid's page count, whose layout helpers
/// subtract terminal chrome themselves) read the render size through here
/// instead of re-deriving it or mistaking a panel rectangle for it (issue
/// #706).
#[must_use]
pub fn committed_render_size(layout: &ResolvedLayout) -> (u16, u16) {
    (
        layout.outer.width,
        layout.outer.height.saturating_add(OUTER_BARS_HEIGHT),
    )
}

/// The display-basis viewport for a committed frame, or the panel content
/// rectangle when no frame is committed.
///
/// The workbench grid's page-count helpers subtract terminal chrome
/// themselves, so callers must feed the **full render size**, not a panel's
/// content rect. [`committed_render_size`] inverts the outer-bar subtraction
/// from the committed frame. When no frame is committed there is no display
/// geometry to reproduce, so the panel content rect is the best available
/// approximation — but the grid's page-clamp is inert in that state anyway
/// (a committed frame is required for the display to page) (issue #706).
#[must_use]
pub fn committed_render_size_or_content(
    layout: Option<&ResolvedLayout>,
    content: &Rect,
) -> (u16, u16) {
    layout.map_or((content.width, content.height), |layout| {
        committed_render_size(layout)
    })
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
        // The split view and the errors screen render no conditional band,
        // so nothing is ever hidden on them. The Terminal Manager is a
        // descriptor screen and returns before this match.
        ScreenId::Repositories | ScreenId::Errors => {}
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

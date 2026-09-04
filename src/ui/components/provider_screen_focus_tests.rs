//! Focus-cue contracts for the shared provider-screen renderer (issue #731).
//!
//! Border colour is the one focus cue a schema-1 scenario cannot observe:
//! `Frame` carries `lines: Vec<String>` and the PTY reader discards every SGR
//! attribute (`src/harness/v1/report.rs:14-20`, `src/harness/v1/pty.rs:22`).
//! These assertions stand in for the frames.

use super::{panel_border_color, panel_border_style};
use crate::provider_panel_view::{PanelProjection, PanelStatus, project_current_screen};
use crate::state::{AppState, PaneFocus};
use crate::theme::{ResolvedColors, ThemeColors};
use iocraft::prelude::BorderStyle;

fn colors() -> ResolvedColors {
    ResolvedColors::from_theme(Some(&ThemeColors::default()))
}

/// The dashboard at the size the issue's scenario runs, with the agent list on
/// screen and pane focus moved onto it.
fn dashboard_with_agents_focused() -> Vec<PanelProjection> {
    let mut state = AppState::test_fixture();
    state.repositories = vec![crate::test_support::host_panel_repository("one")];
    state.agents = vec![crate::test_support::host_panel_agent(
        "Alpha One",
        "repo-one",
        crate::domain::AgentStatus::Running,
    )];
    state.selected_repository_index = Some(0);
    state.pane_focus = PaneFocus::Agents;
    state.resolved_layout = crate::screen_layout::resolve_screen(&state, 120, 40);
    let layout = state
        .resolved_layout
        .clone()
        .unwrap_or_else(|| panic!("the dashboard must resolve at 120x40"));
    let descriptor = state
        .published_workbench()
        .screen_registry()
        .get_identity(crate::workbench::DASHBOARD_IDENTITY)
        .unwrap_or_else(|| panic!("the dashboard descriptor must be published"))
        .clone();
    project_current_screen(&state, &descriptor, &layout)
        .unwrap_or_else(|error| panic!("the dashboard must project: {error}"))
        .panels
}

fn projected(panels: &[PanelProjection], id: &str) -> PanelProjection {
    panels
        .iter()
        .find(|panel| panel.visible && panel.id.as_str() == id)
        .unwrap_or_else(|| panic!("a visible {id:?} pane must be projected"))
        .clone()
}

/// The colour half of A1: the schema-1 frames prove the marker and the border
/// set moved, and this proves the accent moved with them.
#[test]
fn the_focused_accent_moves_with_pane_focus_on_the_dashboard() {
    let rc = colors();
    let panels = dashboard_with_agents_focused();

    let agents = projected(&panels, "agents");
    let repositories = projected(&panels, "repositories");

    assert_eq!(
        panel_border_color(agents.status, agents.focused, &rc),
        rc.border_focused,
        "the focused agents pane must resolve the focused accent"
    );
    assert_eq!(
        panel_border_style(agents.focused),
        BorderStyle::Double,
        "the focused agents pane must resolve the double border set"
    );
    assert_eq!(
        panel_border_color(repositories.status, repositories.focused, &rc),
        rc.border,
        "the unfocused repositories pane must fall back to the ordinary border"
    );
}

#[test]
fn focused_panel_border_resolves_the_focused_accent() {
    let rc = colors();

    assert_eq!(
        panel_border_color(PanelStatus::Active, true, &rc),
        rc.border_focused
    );
    assert_eq!(
        panel_border_color(PanelStatus::Active, false, &rc),
        rc.border
    );
}

#[test]
fn failed_panel_keeps_the_error_accent_whether_focused_or_not() {
    let rc = colors();

    assert_eq!(panel_border_color(PanelStatus::Failed, true, &rc), rc.error);
    assert_eq!(
        panel_border_color(PanelStatus::Failed, false, &rc),
        rc.error
    );
}

#[test]
fn focus_changes_both_the_border_set_and_the_border_color() {
    let rc = colors();

    assert_eq!(panel_border_style(true), BorderStyle::Double);
    assert_eq!(panel_border_style(false), BorderStyle::Round);
    assert_ne!(
        panel_border_color(PanelStatus::Active, true, &rc),
        panel_border_color(PanelStatus::Active, false, &rc)
    );
}

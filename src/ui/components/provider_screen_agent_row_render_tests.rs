//! Painted Agent-list row contracts for the shared provider-screen renderer.

use iocraft::prelude::*;

use crate::domain::AgentStatus;
use crate::selection::{SelectablePane, SelectionPoint, TextSelection};
use crate::state::AppState;
use crate::test_support::{host_panel_agent, host_panel_repository};
use crate::theme::ThemeColors;

const COLUMNS: u16 = 120;
const ROWS: u16 = 40;

fn agent_list_state() -> AppState {
    let mut state = AppState::new(crate::test_support::published_workbench());
    state.repositories = vec![host_panel_repository("alpha")];
    state.agents = vec![host_panel_agent(
        "alpha-agent",
        "repo-alpha",
        AgentStatus::Dead,
    )];
    state.selected_repository_index = Some(0);
    state.selected_agent_index = Some(0);
    state.selection = Some(TextSelection {
        anchor: SelectionPoint::new(SelectablePane::AgentList, 0, 0),
        focus: SelectionPoint::new(SelectablePane::AgentList, 0, usize::MAX),
    });
    state.resolved_layout = crate::screen_layout::resolve_screen(&state, COLUMNS, ROWS);
    state
}

fn selection_colors() -> ThemeColors {
    ThemeColors {
        selection_bg: "#112233".to_owned(),
        selection_fg: "#445566".to_owned(),
        ..ThemeColors::default()
    }
}

#[test]
fn dragged_agent_row_paints_selection_foreground_and_background() {
    let state = agent_list_state();
    let colors = selection_colors();
    let mut element = element! {
        Box(width: u32::from(COLUMNS), height: u32::from(ROWS)) {
            super::ProviderScreen(
                state: Some(state),
                colors: colors,
                theme_name: "selection-test".to_owned(),
            )
        }
    };

    let canvas = element.render(Some(usize::from(COLUMNS)));
    let painted = canvas.to_string();
    assert!(
        painted.lines().any(|row| row.contains("alpha-agent")),
        "the shared provider screen painted no agent row:\n{painted}"
    );

    let mut ansi = Vec::new();
    canvas
        .write_ansi(&mut ansi)
        .unwrap_or_else(|error| panic!("write ANSI: {error}"));
    let ansi = String::from_utf8_lossy(&ansi);

    assert!(
        ansi.contains("\u{1b}[48;2;17;34;51m"),
        "drag-highlighted agent row must use selection_bg; ANSI: {ansi}"
    );
    assert!(
        ansi.contains("\u{1b}[38;2;68;85;102m"),
        "drag-highlighted agent row must use selection_fg; ANSI: {ansi}"
    );
}

#[test]
fn drag_highlight_uses_the_projected_window_origin() {
    let mut state = AppState::new(crate::test_support::published_workbench());
    state.agent_scroll_offset = 0;
    state.selection = Some(TextSelection {
        anchor: SelectionPoint::new(SelectablePane::AgentList, 3, 0),
        focus: SelectionPoint::new(SelectablePane::AgentList, 3, usize::MAX),
    });
    let panel = crate::provider_panel_view::PanelProjection {
        id: crate::workbench::PanelId::from_static("agents"),
        title: "Agents".to_owned(),
        visible: true,
        focused: true,
        chrome: crate::workbench::Rect::new(0, 0, 80, 5),
        content: crate::workbench::Rect::new(1, 2, 78, 1),
        status: crate::provider_panel_view::PanelStatus::Active,
        lines: vec!["third-visible-agent".to_owned()],
        spans: vec![Vec::new()],
        visible_window_origin: 3,
        max_scroll_offset: 3,
        hit_targets: vec![None],
        rect_hit_targets: Vec::new(),
        render: crate::provider_panel_view::PanelRender::Control,
    };
    let colors = selection_colors();
    let props = super::ProviderScreenProps {
        state: Some(state.clone()),
        colors: colors.clone(),
        ..Default::default()
    };
    let resolved = crate::theme::ResolvedColors::from_theme(Some(&colors));
    let mut children = Vec::new();
    super::render_panel(&panel, &state, &props, &resolved, &mut children);
    let mut element = element! {
        Box(width: 80u32, height: 5u32) {
            #(children)
        }
    };
    let canvas = element.render(Some(80));
    let mut ansi = Vec::new();
    canvas
        .write_ansi(&mut ansi)
        .unwrap_or_else(|error| panic!("write ANSI: {error}"));
    let ansi = String::from_utf8_lossy(&ansi);

    assert!(
        ansi.contains("\u{1b}[48;2;17;34;51m"),
        "row three must be highlighted from the projection origin, not state offset: {ansi}"
    );
}

fn render_agent_projection_ansi(spans: Vec<crate::host_controls::HostControlSpan>) -> String {
    let line = spans
        .iter()
        .map(|span| span.text.as_str())
        .collect::<String>();
    let panel = crate::provider_panel_view::PanelProjection {
        id: crate::workbench::PanelId::from_static("agents"),
        title: "Agents".to_owned(),
        visible: true,
        focused: true,
        chrome: crate::workbench::Rect::new(0, 0, 80, 5),
        content: crate::workbench::Rect::new(1, 2, 78, 1),
        status: crate::provider_panel_view::PanelStatus::Active,
        lines: vec![line],
        spans: vec![spans],
        visible_window_origin: 0,
        max_scroll_offset: 0,
        hit_targets: vec![None],
        rect_hit_targets: Vec::new(),
        render: crate::provider_panel_view::PanelRender::Control,
    };
    let state = AppState::new(crate::test_support::published_workbench());
    let colors = ThemeColors {
        accent_secondary: "#112233".to_owned(),
        ..ThemeColors::default()
    };
    let props = super::ProviderScreenProps {
        state: Some(state.clone()),
        colors: colors.clone(),
        ..Default::default()
    };
    let resolved = crate::theme::ResolvedColors::from_theme(Some(&colors));
    let mut children = Vec::new();
    super::render_panel(&panel, &state, &props, &resolved, &mut children);
    let mut element = element! {
        Box(width: 80u32, height: 5u32) {
            #(children)
        }
    };
    let canvas = element.render(Some(80));
    let mut ansi = Vec::new();
    canvas
        .write_ansi(&mut ansi)
        .unwrap_or_else(|error| panic!("write ANSI: {error}"));
    String::from_utf8_lossy(&ansi).into_owned()
}

#[test]
fn keyboard_selected_agent_row_keeps_its_fixed_glyph_role() {
    use crate::host_controls::{HostControlSpan, HostControlSpanRole};

    let ansi = render_agent_projection_ansi(vec![
        HostControlSpan {
            text: ">> ".to_owned(),
            role: HostControlSpanRole::Themed,
        },
        HostControlSpan {
            text: "x".to_owned(),
            role: HostControlSpanRole::Red,
        },
        HostControlSpan {
            text: " selected-agent".to_owned(),
            role: HostControlSpanRole::Themed,
        },
    ]);

    assert!(ansi.contains("selected-agent"), "agent row missing: {ansi}");
    assert!(
        ansi.contains("\u{1b}[38;5;9m"),
        "keyboard selection must not replace the fixed red glyph role: {ansi}"
    );
}

#[test]
fn agent_git_suffix_paints_with_the_shared_dim_role() {
    use crate::host_controls::{HostControlSpan, HostControlSpanRole};

    let ansi = render_agent_projection_ansi(vec![
        HostControlSpan {
            text: ">> ".to_owned(),
            role: HostControlSpanRole::Themed,
        },
        HostControlSpan {
            text: "*".to_owned(),
            role: HostControlSpanRole::Bright,
        },
        HostControlSpan {
            text: " selected-agent".to_owned(),
            role: HostControlSpanRole::Themed,
        },
        HostControlSpan {
            text: "  owner/repo @ main".to_owned(),
            role: HostControlSpanRole::Dim,
        },
    ]);

    assert!(
        ansi.contains("owner/repo @ main"),
        "git suffix missing: {ansi}"
    );
    assert!(
        ansi.contains("\u{1b}[38;2;17;34;51m"),
        "git suffix must use the shared dim role: {ansi}"
    );
}

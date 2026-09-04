//! Which panel the shared runtime marks focused (issue #731).
//!
//! `PanelProjection.focused` is what the renderer turns into the `▶` marker,
//! the double border set and `border_focused`. These tests drive the shipped
//! path — declared descriptor, resolver geometry, `project_current_screen` —
//! from the focus authority the keyboard actually writes, rather than handing
//! `project_provider_screen` a `PanelId` chosen by the test.

use crate::domain::AgentStatus;
use crate::provider_panel_view::{PanelProjection, ProviderScreenView, project_current_screen};
use crate::screen_layout::resolve_screen;
use crate::state::{AppState, PaneFocus};
use crate::workbench::{PanelId, ScreenDescriptor};

/// The size the issue's scenario runs at.
const COLS: u16 = 120;
/// The size the issue's scenario runs at.
const ROWS: u16 = 40;

fn current_descriptor(state: &AppState) -> ScreenDescriptor {
    state
        .published_workbench()
        .screen_registry()
        .get_identity(state.screen())
        .unwrap_or_else(|| panic!("{} must be published", state.screen().as_str()))
        .clone()
}

fn project(state: &mut AppState) -> ProviderScreenView {
    state.resolved_layout = resolve_screen(state, COLS, ROWS);
    let layout = state
        .resolved_layout
        .clone()
        .unwrap_or_else(|| panic!("{} must resolve at {COLS}x{ROWS}", state.screen().as_str()));
    let descriptor = current_descriptor(state);
    project_current_screen(state, &descriptor, &layout)
        .unwrap_or_else(|error| panic!("{} must project: {error}", state.screen().as_str()))
}

fn visible_ids(view: &ProviderScreenView) -> Vec<&str> {
    view.panels
        .iter()
        .filter(|panel| panel.visible)
        .map(|panel| panel.id.as_str())
        .collect()
}

fn pane<'a>(view: &'a ProviderScreenView, id: &str) -> &'a PanelProjection {
    view.panels
        .iter()
        .find(|panel| panel.visible && panel.id.as_str() == id)
        .unwrap_or_else(|| {
            panic!(
                "a visible {id:?} pane must be projected, visible panes were {:?}",
                visible_ids(view)
            )
        })
}

fn focused_ids(view: &ProviderScreenView) -> Vec<&str> {
    view.panels
        .iter()
        .filter(|panel| panel.visible && panel.focused)
        .map(|panel| panel.id.as_str())
        .collect()
}

/// A dashboard with one repository and one running agent, so the ordinary
/// agent-list form of the workspace is the visible one.
fn dashboard() -> AppState {
    let mut state = AppState::test_fixture();
    state.repositories = vec![crate::test_support::host_panel_repository("one")];
    state.agents = vec![crate::test_support::host_panel_agent(
        "Alpha One",
        "repo-one",
        AgentStatus::Running,
    )];
    state.selected_repository_index = Some(0);
    state
}

/// The Repositories split screen, reached the way `s` reaches it.
fn split() -> AppState {
    let mut state = dashboard();
    let _ = state.enter_split_definition();
    assert_eq!(
        state.screen(),
        crate::workbench::REPOSITORIES_IDENTITY,
        "the split fixture must be on the repositories screen"
    );
    state
}

// ---------------------------------------------------------------------------
// Dashboard
// ---------------------------------------------------------------------------

#[test]
fn dashboard_focus_starts_on_the_repositories_pane() {
    let mut state = dashboard();

    let view = project(&mut state);

    assert_eq!(focused_ids(&view), vec!["repositories"]);
}

#[test]
fn dashboard_focus_follows_pane_focus_to_the_agents_pane() {
    let mut state = dashboard();
    state.pane_focus = PaneFocus::Agents;

    let view = project(&mut state);

    assert!(
        pane(&view, "agents").focused,
        "the agents pane must hold the focus cue once pane focus moved to it"
    );
    assert!(
        !pane(&view, "repositories").focused,
        "the repositories pane must give the focus cue up"
    );
}

#[test]
fn dashboard_focus_follows_pane_focus_back_to_the_repositories_pane() {
    let mut state = dashboard();
    state.pane_focus = PaneFocus::Agents;
    let _ = project(&mut state);
    state.pane_focus = PaneFocus::Repositories;

    let view = project(&mut state);

    assert_eq!(focused_ids(&view), vec!["repositories"]);
}

/// The zero-agent availability pane is the agent list's stand-in (#734/#736),
/// so `PaneFocus::Agents` must land on whichever of the two is on screen.
#[test]
fn dashboard_focus_lands_on_the_agent_types_pane_when_it_replaces_the_agent_list() {
    let mut state = AppState::test_fixture();
    state.agent_type_availability = vec![
        crate::agent_status_view::AgentAvailabilityObservation::not_found(
            &crate::domain::agent_definition::AgentDefinition::shipped()
                .into_iter()
                .next()
                .unwrap_or_else(|| panic!("a shipped agent definition must exist")),
            true,
            1,
        ),
    ];
    state.pane_focus = PaneFocus::Agents;

    let view = project(&mut state);

    assert_eq!(
        focused_ids(&view),
        vec![crate::workbench::AGENT_TYPES_PANEL],
        "the availability pane takes the agent list's place in the traversal"
    );
}

/// The per-instance `panel_focus` is not a second authority on the dashboard:
/// a stale value must not be able to move the focus cue.
#[test]
fn dashboard_ignores_a_stored_panel_focus_that_disagrees_with_pane_focus() {
    let mut state = dashboard();
    state.nav.current_mut().panel_focus = PanelId::from_static("terminal");
    state.pane_focus = PaneFocus::Repositories;

    let view = project(&mut state);

    assert_eq!(focused_ids(&view), vec!["repositories"]);
}

// ---------------------------------------------------------------------------
// The embedded terminal answers to terminal focus, not to the pane index
// ---------------------------------------------------------------------------

#[test]
fn terminal_pane_is_unfocused_while_the_pane_is_selected_but_not_focused() {
    let mut state = dashboard();
    state.pane_focus = PaneFocus::Terminal;
    state.terminal_focused = false;

    let view = project(&mut state);

    assert!(
        !pane(&view, "terminal").focused,
        "an unfocused terminal must keep advertising its focus hint"
    );
    assert_eq!(
        focused_ids(&view),
        Vec::<&str>::new(),
        "no list pane may hold the cue once the terminal pane is selected"
    );
}

#[test]
fn terminal_pane_is_focused_once_terminal_focus_is_taken() {
    let mut state = dashboard();
    state.pane_focus = PaneFocus::Terminal;
    state.terminal_focused = true;

    let view = project(&mut state);

    assert_eq!(focused_ids(&view), vec!["terminal"]);
}

// ---------------------------------------------------------------------------
// Repositories split screen
// ---------------------------------------------------------------------------

#[test]
fn split_focus_starts_on_the_repositories_pane() {
    let mut state = split();

    let view = project(&mut state);

    assert_eq!(focused_ids(&view), vec!["repositories"]);
}

#[test]
fn split_focus_follows_pane_focus_through_its_declared_traversal() {
    let mut state = split();

    state.pane_focus = PaneFocus::Agents;
    let view = project(&mut state);
    assert_eq!(
        focused_ids(&view),
        vec!["status"],
        "the split's declared traversal is [repositories, status, cards]"
    );

    state.pane_focus = PaneFocus::Terminal;
    let view = project(&mut state);
    assert_eq!(focused_ids(&view), vec!["cards"]);

    state.pane_focus = PaneFocus::Repositories;
    let view = project(&mut state);
    assert_eq!(focused_ids(&view), vec!["repositories"]);
}

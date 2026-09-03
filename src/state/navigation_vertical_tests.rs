//! Dashboard vertical navigation follows the focused pane (issue #722).
//!
//! Routing Up/Down into the startup agent-type selection whenever `agents` was
//! empty left the visible repositories and agents panes ignoring arrows
//! entirely. These tests pin the contract: Up/Down act on `pane_focus`, the
//! same routing `handle_navigate_page` already uses.
//!
//! The availability list is the zero-agent form of the agents pane (#734), so
//! `PaneFocus::Agents` addresses it exactly while it is the pane on screen —
//! never while the repositories pane holds the focus, which is the case #722
//! reported.

use super::*;
use crate::agent_status_view::AgentAvailabilityObservation;
use crate::domain::agent_definition::AgentDefinition;
use crate::domain::{Agent, AgentId, Repository, RepositoryId, TypedMap};
use crate::messages::{AppMessage, UiNavigationMessage};
use crate::state::transition::TransitionExt;

fn shipped_definition() -> AgentDefinition {
    AgentDefinition::shipped()
        .into_iter()
        .next()
        .unwrap_or_else(|| panic!("shipped definitions must not be empty"))
}

fn repository(id: &str, name: &str) -> Repository {
    Repository::new(
        RepositoryId(id.to_string()),
        shipped_definition().id,
        TypedMap::new(),
        name.to_string(),
        id.to_string(),
        std::path::PathBuf::from("/tmp").join(id),
    )
}

fn agent(id: &str, repository_id: &str, name: &str) -> Agent {
    Agent::new(
        AgentId(id.to_string()),
        RepositoryId(repository_id.to_string()),
        shipped_definition().id,
        TypedMap::new(),
        name.to_string(),
        std::path::PathBuf::from("/tmp").join(id),
    )
}

/// The #722 repro shape: three repositories, no agents yet, and a startup
/// availability list that no dashboard pane renders.
fn dashboard_repro() -> AppState {
    let mut state = AppState::test_fixture();
    state.repositories = vec![
        repository("repo-a", "Alpha Repo"),
        repository("repo-b", "Beta Repo"),
        repository("repo-c", "Gamma Repo"),
    ];
    state.selected_repository_index = Some(0);
    state.pane_focus = PaneFocus::Repositories;
    state.agent_type_availability = vec![
        AgentAvailabilityObservation::not_found(&shipped_definition(), true, 1),
        AgentAvailabilityObservation::not_found(&shipped_definition(), true, 2),
    ];
    state
}

#[test]
fn navigate_down_moves_the_repository_selection_when_no_agents_exist() {
    let state = dashboard_repro();
    let state = state
        .apply_message(AppMessage::UiNavigation(UiNavigationMessage::NavigateDown))
        .committed_pure();

    assert_eq!(
        state.selected_repository_index,
        Some(1),
        "Down on the focused repositories pane must move the repository cursor"
    );
    assert_eq!(
        state.selected_agent_type_index, 0,
        "Down must not move the agent-type selection no pane renders"
    );
}

#[test]
fn navigate_up_moves_the_repository_selection_when_no_agents_exist() {
    let state = dashboard_repro();
    let state = state
        .apply_message(AppMessage::UiNavigation(UiNavigationMessage::NavigateRight))
        .committed_pure();
    let state = state
        .apply_message(AppMessage::UiNavigation(UiNavigationMessage::NavigateLeft))
        .committed_pure();
    let state = state
        .apply_message(AppMessage::UiNavigation(UiNavigationMessage::NavigateDown))
        .committed_pure();
    let state = state
        .apply_message(AppMessage::UiNavigation(UiNavigationMessage::NavigateDown))
        .committed_pure();
    let state = state
        .apply_message(AppMessage::UiNavigation(UiNavigationMessage::NavigateUp))
        .committed_pure();

    assert_eq!(
        state.selected_repository_index,
        Some(1),
        "Up on the focused repositories pane must move the repository cursor"
    );
    assert_eq!(
        state.selected_agent_type_index, 0,
        "Up must not move the agent-type selection no pane renders"
    );
}

#[test]
fn navigate_down_moves_the_agent_selection_in_the_agents_pane() {
    let mut state = dashboard_repro();
    state.agents = vec![
        agent("agent-1", "repo-a", "First Agent"),
        agent("agent-2", "repo-a", "Second Agent"),
    ];
    state.pane_focus = PaneFocus::Agents;
    state.selected_agent_index = Some(0);

    let state = state
        .apply_message(AppMessage::UiNavigation(UiNavigationMessage::NavigateDown))
        .committed_pure();

    assert_eq!(
        state.selected_agent_index,
        Some(1),
        "Down on the focused agents pane must move the agent cursor"
    );
}

#[test]
fn navigate_up_moves_the_agent_selection_in_the_agents_pane() {
    let mut state = dashboard_repro();
    state.agents = vec![
        agent("agent-1", "repo-a", "First Agent"),
        agent("agent-2", "repo-a", "Second Agent"),
    ];
    state.pane_focus = PaneFocus::Agents;
    state.selected_agent_index = Some(1);

    let state = state
        .apply_message(AppMessage::UiNavigation(UiNavigationMessage::NavigateUp))
        .committed_pure();

    assert_eq!(
        state.selected_agent_index,
        Some(0),
        "Up on the focused agents pane must move the agent cursor"
    );
}

/// The availability repro: the pane the zero-agent dashboard renders in the
/// agent list's place holds the focus (`a`, or the persisted
/// `pane_focus: agents` the `issue382/agent-probe-negative` fixture boots
/// with), so its own cursor is the one vertical keys move (#734).
fn availability_pane_focused() -> AppState {
    let mut state = dashboard_repro();
    state.pane_focus = PaneFocus::Agents;
    state.agent_type_availability = vec![
        AgentAvailabilityObservation::not_found(&shipped_definition(), true, 1),
        AgentAvailabilityObservation::not_found(&shipped_definition(), true, 2),
        AgentAvailabilityObservation::not_found(&shipped_definition(), true, 3),
    ];
    state
}

#[test]
fn navigate_down_moves_the_availability_cursor_when_its_pane_is_focused() {
    let state = availability_pane_focused()
        .apply_message(AppMessage::UiNavigation(UiNavigationMessage::NavigateDown))
        .committed_pure();

    assert_eq!(
        state.selected_agent_type_index, 1,
        "Down on the focused availability pane must move its cursor"
    );
    assert_eq!(
        state.selected_repository_index,
        Some(0),
        "the repository cursor belongs to the pane that is not focused"
    );
}

#[test]
fn navigate_up_moves_the_availability_cursor_back_and_stops_at_the_first_row() {
    let mut state = availability_pane_focused();
    state.selected_agent_type_index = 1;

    let state = state
        .apply_message(AppMessage::UiNavigation(UiNavigationMessage::NavigateUp))
        .committed_pure();
    assert_eq!(state.selected_agent_type_index, 0);

    let state = state
        .apply_message(AppMessage::UiNavigation(UiNavigationMessage::NavigateUp))
        .committed_pure();
    assert_eq!(
        state.selected_agent_type_index, 0,
        "the cursor stops on the first row rather than wrapping"
    );
}

#[test]
fn navigate_down_stops_the_availability_cursor_on_the_last_row() {
    let state = availability_pane_focused();
    let last = state.agent_type_availability.len() - 1;

    let state = (0..5).fold(state, |state, _| {
        state
            .apply_message(AppMessage::UiNavigation(UiNavigationMessage::NavigateDown))
            .committed_pure()
    });

    assert_eq!(
        state.selected_agent_type_index, last,
        "the cursor clamps to the last published observation"
    );
}

#[test]
fn the_availability_cursor_stays_put_once_the_pane_is_replaced_by_the_agent_list() {
    let mut state = availability_pane_focused();
    state.agents = vec![agent("agent-1", "repo-a", "First Agent")];
    state.selected_agent_index = Some(0);

    let state = state
        .apply_message(AppMessage::UiNavigation(UiNavigationMessage::NavigateDown))
        .committed_pure();

    assert_eq!(
        state.selected_agent_type_index, 0,
        "with agents on screen the agents pane owns the vertical keys"
    );
}

//! Dashboard vertical navigation follows the focused pane (issue #722).
//!
//! Since the #715 dashboard cutover no screen renders the startup agent-type
//! availability list, so routing Up/Down into that invisible selection
//! whenever `agents` was empty left the visible repositories and agents panes
//! ignoring arrows entirely. These tests pin the restored contract: Up/Down
//! act on `pane_focus`, the same routing `handle_navigate_page` already uses.

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

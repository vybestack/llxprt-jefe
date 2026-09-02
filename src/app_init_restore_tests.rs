//! Behavioural tests for durable selection restore at startup.
//!
//! Issue #716: selection writes go through the navigation instance, so the
//! restore path is order-sensitive around re-rooting navigation. These tests
//! pin that contract; the module is included via `#[path]` like the other
//! app_init test modules.

use super::restore_persisted_state;
use super::tests::code_puppy_agent_and_repository;
use jefe::domain::{Agent, AgentId, Repository, RepositoryId};

/// Issue #715 regression: selection writes go through the navigation instance,
/// so restoring them before re-rooting navigation silently discarded them and
/// startup normalize moved the operator onto the first visible repository.
#[test]
fn restore_keeps_the_selection_the_document_carried() {
    let mut state = crate::test_app_state();
    let (agent, repository) = code_puppy_agent_and_repository();
    let second_repository = Repository::new(
        RepositoryId("repo-second".to_owned()),
        jefe::domain::shipped_agent_type(1),
        jefe::domain::TypedMap::new(),
        "Second Repo".to_owned(),
        "second-repo".to_owned(),
        std::path::PathBuf::from("/tmp/second-repo"),
    );
    let second_agent = Agent::new(
        AgentId("agent-second".to_owned()),
        second_repository.id.clone(),
        jefe::domain::shipped_agent_type(1),
        jefe::domain::TypedMap::new(),
        "Second Agent".to_owned(),
        std::path::PathBuf::from("/tmp/second-agent"),
    );
    let persisted = jefe::state::durable_projection::RestoredState {
        revision: 7,
        repositories: vec![repository, second_repository],
        agents: vec![agent, second_agent],
        selected_repository_index: Some(1),
        selected_agent_index: Some(1),
        last_selected_agent_by_repo: Vec::new(),
        user_preferences: jefe::domain::UserPreferences::default(),
        hide_idle_repositories: false,
        screen: jefe::workbench::ScreenIdentity::default(),
        pane_focus: jefe::state::PaneFocus::default(),
        terminal_focused: false,
        dormant_records: Vec::new(),
    };

    restore_persisted_state(&mut state, persisted);

    assert_eq!(
        state.selected_repository_index,
        Some(1),
        "the restored root must keep the repository the document selected"
    );
    assert_eq!(
        state.repositories[1].id,
        RepositoryId("repo-second".to_owned()),
        "the restored selection must still resolve to the second repository"
    );
    assert_eq!(
        state.selected_agent_index,
        Some(1),
        "the restored root must keep the agent the document selected"
    );
    assert_eq!(
        state.agents[1].id,
        AgentId("agent-second".to_owned()),
        "the restored selection must still resolve to the second agent"
    );
}

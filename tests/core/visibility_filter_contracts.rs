//! Agent visibility filter and display–selection consistency tests (issue #41).

use crate::support::TestOptionExt;

use std::path::PathBuf;

use jefe::domain::{Agent, AgentId, AgentStatus, Repository, RepositoryId};
use jefe::state::{AppEvent, AppState, ModalState, PaneFocus};

fn repository(id: &str) -> Repository {
    Repository::new(
        RepositoryId(id.into()),
        id.to_uppercase(),
        id.into(),
        PathBuf::from(format!("/{id}")),
    )
}

fn agent(id: &str, name: &str, status: AgentStatus) -> Agent {
    let mut agent = Agent::new(
        AgentId(id.into()),
        RepositoryId("r1".into()),
        name.into(),
        PathBuf::from(format!("/r1/{id}")),
    );
    agent.status = status;
    agent
}

#[test]
fn visible_agents_matches_agent_indices_when_idle_hidden() {
    let state = AppState {
        repositories: vec![Repository::new(
            RepositoryId("r1".into()),
            "R1".into(),
            "r1".into(),
            PathBuf::from("/r1"),
        )],
        agents: vec![
            Agent::new(
                AgentId("idle1".into()),
                RepositoryId("r1".into()),
                "Idle A".into(),
                PathBuf::from("/r1/idle1"),
            ),
            {
                let mut running = Agent::new(
                    AgentId("run1".into()),
                    RepositoryId("r1".into()),
                    "Running B".into(),
                    PathBuf::from("/r1/run1"),
                );
                running.status = AgentStatus::Running;
                running
            },
            {
                let mut running = Agent::new(
                    AgentId("run2".into()),
                    RepositoryId("r1".into()),
                    "Running C".into(),
                    PathBuf::from("/r1/run2"),
                );
                running.status = AgentStatus::Running;
                running
            },
        ],
        selected_repository_index: Some(0),
        selected_agent_index: Some(1),
        pane_focus: PaneFocus::Agents,
        ..AppState::default()
    };

    let hidden = state.apply(AppEvent::ToggleHideIdleRepositories);
    assert!(hidden.hide_idle_repositories);

    let repo_id = RepositoryId("r1".into());
    let visible_agents = hidden.visible_agents_for_repository(&repo_id);
    let visible_indices = hidden.agent_indices_for_repository(&repo_id);

    assert_eq!(
        visible_agents.len(),
        visible_indices.len(),
        "visible_agents_for_repository and agent_indices_for_repository must agree on count"
    );

    for agent in &visible_agents {
        assert!(
            agent.is_running(),
            "idle agent '{}' must not appear in visible list",
            agent.name
        );
    }
}

#[test]
fn selected_agent_local_index_matches_visible_agents_position() {
    let state = AppState {
        repositories: vec![Repository::new(
            RepositoryId("r1".into()),
            "R1".into(),
            "r1".into(),
            PathBuf::from("/r1"),
        )],
        agents: vec![
            Agent::new(
                AgentId("idle1".into()),
                RepositoryId("r1".into()),
                "Idle A".into(),
                PathBuf::from("/r1/idle1"),
            ),
            {
                let mut running = Agent::new(
                    AgentId("run1".into()),
                    RepositoryId("r1".into()),
                    "Running B".into(),
                    PathBuf::from("/r1/run1"),
                );
                running.status = AgentStatus::Running;
                running
            },
            Agent::new(
                AgentId("idle2".into()),
                RepositoryId("r1".into()),
                "Idle C".into(),
                PathBuf::from("/r1/idle2"),
            ),
            {
                let mut running = Agent::new(
                    AgentId("run2".into()),
                    RepositoryId("r1".into()),
                    "Running D".into(),
                    PathBuf::from("/r1/run2"),
                );
                running.status = AgentStatus::Running;
                running
            },
        ],
        selected_repository_index: Some(0),
        selected_agent_index: Some(1),
        pane_focus: PaneFocus::Agents,
        ..AppState::default()
    };

    let hidden = state.apply(AppEvent::ToggleHideIdleRepositories);
    assert!(hidden.hide_idle_repositories);

    let repo_id = RepositoryId("r1".into());
    let visible_agents = hidden.visible_agents_for_repository(&repo_id);
    let local_idx = hidden
        .selected_agent_local_index()
        .test_unwrap("test unwrap");
    let selected = hidden.selected_agent().test_unwrap("test unwrap");

    assert_eq!(
        visible_agents[local_idx].id, selected.id,
        "indexing visible_agents with selected_agent_local_index must yield the selected agent"
    );
}

#[test]
fn visible_agents_returns_all_when_filter_disabled() {
    let state = AppState {
        repositories: vec![Repository::new(
            RepositoryId("r1".into()),
            "R1".into(),
            "r1".into(),
            PathBuf::from("/r1"),
        )],
        agents: vec![
            Agent::new(
                AgentId("idle1".into()),
                RepositoryId("r1".into()),
                "Idle A".into(),
                PathBuf::from("/r1/idle1"),
            ),
            {
                let mut running = Agent::new(
                    AgentId("run1".into()),
                    RepositoryId("r1".into()),
                    "Running B".into(),
                    PathBuf::from("/r1/run1"),
                );
                running.status = AgentStatus::Running;
                running
            },
        ],
        selected_repository_index: Some(0),
        selected_agent_index: Some(0),
        pane_focus: PaneFocus::Agents,
        hide_idle_repositories: false,
        ..AppState::default()
    };

    let repo_id = RepositoryId("r1".into());
    let visible_agents = state.visible_agents_for_repository(&repo_id);
    assert_eq!(
        visible_agents.len(),
        2,
        "with filter off, all agents should be visible"
    );
}

#[test]
fn delete_targets_correct_agent_when_idle_hidden() {
    let state = AppState {
        repositories: vec![repository("r1")],
        agents: vec![
            agent("idle1", "Idle A", AgentStatus::Queued),
            agent("target", "Target Agent", AgentStatus::Running),
            agent("other", "Other Agent", AgentStatus::Running),
        ],
        selected_repository_index: Some(0),
        selected_agent_index: Some(1),
        pane_focus: PaneFocus::Agents,
        ..AppState::default()
    };

    let hidden = state.apply(AppEvent::ToggleHideIdleRepositories);
    let repo_id = RepositoryId("r1".into());
    let visible_agents = hidden.visible_agents_for_repository(&repo_id);
    let local_idx = hidden
        .selected_agent_local_index()
        .test_unwrap("selected agent local index should exist");
    let selected_id = hidden
        .selected_agent()
        .test_unwrap("selected agent should exist")
        .id
        .clone();

    assert_eq!(visible_agents[local_idx].id, selected_id);
    assert_eq!(selected_id, AgentId("target".into()));

    let with_modal = hidden.apply(AppEvent::OpenDeleteAgent(selected_id));
    let ModalState::ConfirmDeleteAgent { id, .. } = &with_modal.modal else {
        panic!("expected ConfirmDeleteAgent, got {:?}", with_modal.modal);
    };
    assert_eq!(
        *id,
        AgentId("target".into()),
        "delete must target the highlighted agent, not an adjacent one"
    );
}

#[test]
fn visible_agent_count_includes_all_when_filter_off() {
    let state = AppState {
        repositories: vec![Repository::new(
            RepositoryId("r1".into()),
            "R1".into(),
            "r1".into(),
            PathBuf::from("/r1"),
        )],
        agents: vec![
            Agent::new(
                AgentId("idle1".into()),
                RepositoryId("r1".into()),
                "Idle A".into(),
                PathBuf::from("/r1/idle1"),
            ),
            {
                let mut a = Agent::new(
                    AgentId("run1".into()),
                    RepositoryId("r1".into()),
                    "Running B".into(),
                    PathBuf::from("/r1/run1"),
                );
                a.status = AgentStatus::Running;
                a
            },
        ],
        selected_repository_index: Some(0),
        selected_agent_index: Some(0),
        pane_focus: PaneFocus::Agents,
        ..AppState::default()
    };

    assert_eq!(state.visible_agent_count(), 2);
    assert_eq!(
        state.visible_agent_count_for_repository(&RepositoryId("r1".into())),
        2
    );
}

#[test]
fn visible_agent_count_excludes_inactive_when_filter_on() {
    let state = AppState {
        repositories: vec![Repository::new(
            RepositoryId("r1".into()),
            "R1".into(),
            "r1".into(),
            PathBuf::from("/r1"),
        )],
        agents: vec![
            Agent::new(
                AgentId("idle1".into()),
                RepositoryId("r1".into()),
                "Idle A".into(),
                PathBuf::from("/r1/idle1"),
            ),
            {
                let mut a = Agent::new(
                    AgentId("run1".into()),
                    RepositoryId("r1".into()),
                    "Running B".into(),
                    PathBuf::from("/r1/run1"),
                );
                a.status = AgentStatus::Running;
                a
            },
        ],
        selected_repository_index: Some(0),
        selected_agent_index: Some(1),
        pane_focus: PaneFocus::Agents,
        ..AppState::default()
    };

    let hidden = state.apply(AppEvent::ToggleHideIdleRepositories);
    assert_eq!(hidden.visible_agent_count(), 1);
    assert_eq!(
        hidden.visible_agent_count_for_repository(&RepositoryId("r1".into())),
        1
    );
}

#[test]
fn visible_repo_count_matches_visible_repository_indices() {
    let state = AppState {
        repositories: vec![
            Repository::new(
                RepositoryId("r1".into()),
                "R1".into(),
                "r1".into(),
                PathBuf::from("/r1"),
            ),
            Repository::new(
                RepositoryId("r2".into()),
                "R2".into(),
                "r2".into(),
                PathBuf::from("/r2"),
            ),
        ],
        agents: vec![
            {
                let mut a = Agent::new(
                    AgentId("run1".into()),
                    RepositoryId("r1".into()),
                    "Running A".into(),
                    PathBuf::from("/r1/run1"),
                );
                a.status = AgentStatus::Running;
                a
            },
            Agent::new(
                AgentId("idle1".into()),
                RepositoryId("r2".into()),
                "Idle B".into(),
                PathBuf::from("/r2/idle1"),
            ),
        ],
        selected_repository_index: Some(0),
        selected_agent_index: Some(0),
        pane_focus: PaneFocus::Repositories,
        ..AppState::default()
    };

    // Filter off: both repos visible
    assert_eq!(state.visible_repository_indices().len(), 2);

    // Filter on: only r1 visible (has running agent)
    let hidden = state.apply(AppEvent::ToggleHideIdleRepositories);
    assert_eq!(hidden.visible_repository_indices().len(), 1);
    assert_eq!(hidden.visible_repository_indices()[0], 0);
}

// =============================================================================
// Sticky dead-agent visibility (issue #116)
//
// When hide_idle_repositories is ON and the user kills an agent, the dead
// agent should remain visible until ANY UI navigation occurs. This prevents
// the user from losing their place when the agent they were viewing dies.
// =============================================================================

fn running_agent(id: &str, name: &str, repo_id: &str) -> Agent {
    let mut a = Agent::new(
        AgentId(id.into()),
        RepositoryId(repo_id.into()),
        name.into(),
        PathBuf::from(format!("/{repo_id}/{id}")),
    );
    a.status = AgentStatus::Running;
    a
}

/// Test 1: With hide_idle_repositories=true, kill the selected running agent.
/// The agent should still be visible and selected, and the repo should still
/// be in visible_repository_indices.
#[test]
fn kill_agent_in_active_only_mode_stays_visible() {
    let mut state = AppState {
        repositories: vec![repository("r1")],
        agents: vec![running_agent("a1", "Agent One", "r1")],
        selected_repository_index: Some(0),
        selected_agent_index: Some(0),
        pane_focus: PaneFocus::Agents,
        hide_idle_repositories: true,
        ..AppState::default()
    };
    state.normalize_selection_indices();

    let killed = state.apply(AppEvent::KillAgent(AgentId("a1".into())));

    // The agent is Dead but should still be in the visible set (sticky).
    let repo_id = RepositoryId("r1".into());
    let visible_agents = killed.visible_agents_for_repository(&repo_id);
    assert!(
        visible_agents.iter().any(|a| a.id == AgentId("a1".into())),
        "killed agent should remain visible via sticky until navigation"
    );

    // The agent should still be selected.
    let selected = killed.selected_agent();
    assert!(
        selected.is_some_and(|a| a.id == AgentId("a1".into())),
        "killed agent should still be selected (sticky keeps it visible)"
    );

    // The repo should still be visible.
    let visible_repos = killed.visible_repository_indices();
    assert!(
        visible_repos.contains(&0),
        "repo r1 should still be visible (sticky dead agent keeps it alive)"
    );
}

/// Test 2: After killing (sticky), navigating away should clear the sticky
/// list and the dead agent should be filtered out.
#[test]
fn navigate_after_kill_filters_dead_agent() {
    let mut state = AppState {
        repositories: vec![repository("r1"), repository("r2")],
        agents: vec![
            running_agent("a1", "Agent One", "r1"),
            running_agent("a2", "Agent Two", "r2"),
        ],
        selected_repository_index: Some(0),
        selected_agent_index: Some(0),
        pane_focus: PaneFocus::Agents,
        hide_idle_repositories: true,
        ..AppState::default()
    };
    state.normalize_selection_indices();

    let killed = state.apply(AppEvent::KillAgent(AgentId("a1".into())));

    // Navigate down — this should clear the sticky list.
    let after_nav = killed.apply(AppEvent::NavigateDown);

    let repo_id = RepositoryId("r1".into());
    let visible_agents = after_nav.visible_agents_for_repository(&repo_id);
    assert!(
        !visible_agents.iter().any(|a| a.id == AgentId("a1".into())),
        "after navigation, the dead agent should be filtered out"
    );
}

/// Test 3: Kill the last running agent in a repo. The repo should stay visible
/// (sticky). After navigating away, the repo should be filtered out.
#[test]
fn kill_last_running_agent_keeps_repo_visible() {
    let mut state = AppState {
        repositories: vec![repository("r1"), repository("r2")],
        agents: vec![
            running_agent("a1", "Agent One", "r1"),
            running_agent("a2", "Agent Two", "r2"),
        ],
        selected_repository_index: Some(0),
        selected_agent_index: Some(0),
        pane_focus: PaneFocus::Repositories,
        hide_idle_repositories: true,
        ..AppState::default()
    };
    state.normalize_selection_indices();

    // Kill the only running agent in r1.
    let killed = state.apply(AppEvent::KillAgent(AgentId("a1".into())));

    // r1 should still be visible because of the sticky dead agent.
    let visible_repos = killed.visible_repository_indices();
    assert!(
        visible_repos.contains(&0),
        "repo r1 should still be visible after killing its last running agent (sticky)"
    );

    // Navigate down — clears sticky, r1 should now be filtered out.
    let after_nav = killed.apply(AppEvent::NavigateDown);
    let visible_repos_after = after_nav.visible_repository_indices();
    assert!(
        !visible_repos_after.contains(&0),
        "after navigation, repo r1 should be filtered out (no running agents)"
    );
}

/// Test 4: AgentStatusChanged(Dead) should NOT trigger sticky behavior.
/// Only an explicit KillAgent action should be sticky.
#[test]
fn agent_status_changed_does_not_trigger_sticky() {
    let mut state = AppState {
        repositories: vec![repository("r1")],
        agents: vec![running_agent("a1", "Agent One", "r1")],
        selected_repository_index: Some(0),
        selected_agent_index: Some(0),
        pane_focus: PaneFocus::Agents,
        hide_idle_repositories: true,
        ..AppState::default()
    };
    state.normalize_selection_indices();

    // Use AgentStatusChanged (external status update) instead of KillAgent.
    let after = state.apply(AppEvent::AgentStatusChanged(
        AgentId("a1".into()),
        AgentStatus::Dead,
    ));

    let repo_id = RepositoryId("r1".into());
    let visible_agents = after.visible_agents_for_repository(&repo_id);
    assert!(
        !visible_agents.iter().any(|a| a.id == AgentId("a1".into())),
        "AgentStatusChanged(Dead) should NOT be sticky — agent should be filtered immediately"
    );
}

/// Test 5: Kill with filter OFF (sticky is set), then toggle filter ON.
/// Toggling the filter is a display setting, not a navigation, so it should
/// NOT clear the sticky list. The dead agent remains visible until the user
/// actually navigates away.
#[test]
fn kill_with_filter_off_then_toggle_on_keeps_sticky() {
    let mut state = AppState {
        repositories: vec![repository("r1")],
        agents: vec![running_agent("a1", "Agent One", "r1")],
        selected_repository_index: Some(0),
        selected_agent_index: Some(0),
        pane_focus: PaneFocus::Agents,
        hide_idle_repositories: false,
        ..AppState::default()
    };
    state.normalize_selection_indices();

    // Kill while filter is OFF — sticky list should still be populated.
    let killed = state.apply(AppEvent::KillAgent(AgentId("a1".into())));

    // Toggle filter ON — this is a display toggle, NOT navigation; sticky persists.
    let toggled = killed.apply(AppEvent::ToggleHideIdleRepositories);
    assert!(toggled.hide_idle_repositories);

    let repo_id = RepositoryId("r1".into());
    let visible_agents = toggled.visible_agents_for_repository(&repo_id);
    assert!(
        visible_agents.iter().any(|a| a.id == AgentId("a1".into())),
        "toggling filter ON should NOT clear sticky — dead agent stays visible"
    );

    // Now navigate down — this clears sticky, and the dead agent is filtered out.
    let navigated = toggled.apply(AppEvent::NavigateDown);
    let visible_after_nav = navigated.visible_agents_for_repository(&repo_id);
    assert!(
        !visible_after_nav
            .iter()
            .any(|a| a.id == AgentId("a1".into())),
        "after navigation, sticky is cleared and dead agent is filtered out"
    );
}

/// Test 6: Kill multiple agents in the same repo. All should be sticky until
/// navigation clears them.
#[test]
fn multiple_kills_all_sticky() {
    let mut state = AppState {
        repositories: vec![repository("r1")],
        agents: vec![
            running_agent("a1", "Agent One", "r1"),
            running_agent("a2", "Agent Two", "r1"),
        ],
        selected_repository_index: Some(0),
        selected_agent_index: Some(0),
        pane_focus: PaneFocus::Agents,
        hide_idle_repositories: true,
        ..AppState::default()
    };
    state.normalize_selection_indices();

    let killed_a = state.apply(AppEvent::KillAgent(AgentId("a1".into())));
    let killed_b = killed_a.apply(AppEvent::KillAgent(AgentId("a2".into())));

    let repo_id = RepositoryId("r1".into());
    let visible_agents = killed_b.visible_agents_for_repository(&repo_id);
    assert!(
        visible_agents.iter().any(|a| a.id == AgentId("a1".into())),
        "agent a1 should be sticky-visible"
    );
    assert!(
        visible_agents.iter().any(|a| a.id == AgentId("a2".into())),
        "agent a2 should be sticky-visible"
    );

    // Navigate away — both should be filtered.
    let after_nav = killed_b.apply(AppEvent::NavigateDown);
    let visible_after = after_nav.visible_agents_for_repository(&repo_id);
    assert!(
        visible_after.is_empty(),
        "after navigation, both dead agents should be filtered out"
    );
}

/// Test 7: Kill agent, then SelectRepository (even to the same repo) should
/// clear the sticky list.
#[test]
fn sticky_cleared_on_select_repository() {
    let mut state = AppState {
        repositories: vec![repository("r1"), repository("r2")],
        agents: vec![
            running_agent("a1", "Agent One", "r1"),
            running_agent("a2", "Agent Two", "r2"),
        ],
        selected_repository_index: Some(0),
        selected_agent_index: Some(0),
        pane_focus: PaneFocus::Repositories,
        hide_idle_repositories: true,
        ..AppState::default()
    };
    state.normalize_selection_indices();

    let killed = state.apply(AppEvent::KillAgent(AgentId("a1".into())));

    // SelectRepository is a navigation message — clears sticky.
    let after_select = killed.apply(AppEvent::SelectRepository(0));

    let repo_id = RepositoryId("r1".into());
    let visible_agents = after_select.visible_agents_for_repository(&repo_id);
    assert!(
        !visible_agents.iter().any(|a| a.id == AgentId("a1".into())),
        "SelectRepository should clear sticky and filter out the dead agent"
    );
}

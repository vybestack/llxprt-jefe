//! Dashboard reorder ("grab") behavior tests.
//!
//! Verifies the select-then-move interaction: Space to grab the highlighted
//! item, arrows to move it, Space/Enter to drop it. Works for both
//! repositories (within the visible set) and agents (within their repository).
//!
//! Issue #118.

use jefe::domain::{Agent, AgentId, AgentStatus, Repository, RepositoryId};
use jefe::state::{AppEvent, AppState, DashboardGrabPane, PaneFocus, ScreenMode};
use std::path::PathBuf;

/// Build a dashboard state with three repositories, each with one running
/// agent so they stay visible under the idle filter.
fn create_dashboard_test_state() -> AppState {
    let repo1 = Repository::new(
        RepositoryId("repo-a".into()),
        "alpha".into(),
        "alpha".into(),
        PathBuf::from("/projects/alpha"),
    );
    let repo2 = Repository::new(
        RepositoryId("repo-b".into()),
        "bravo".into(),
        "bravo".into(),
        PathBuf::from("/projects/bravo"),
    );
    let repo3 = Repository::new(
        RepositoryId("repo-c".into()),
        "charlie".into(),
        "charlie".into(),
        PathBuf::from("/projects/charlie"),
    );

    let mut a1 = Agent::new(
        AgentId("a1".into()),
        repo1.id.clone(),
        "alpha-1".into(),
        PathBuf::from("/projects/alpha/a1"),
    );
    a1.status = AgentStatus::Running;
    let mut a2 = Agent::new(
        AgentId("a2".into()),
        repo2.id.clone(),
        "bravo-1".into(),
        PathBuf::from("/projects/bravo/a2"),
    );
    a2.status = AgentStatus::Running;
    let mut a3 = Agent::new(
        AgentId("a3".into()),
        repo3.id.clone(),
        "charlie-1".into(),
        PathBuf::from("/projects/charlie/a3"),
    );
    a3.status = AgentStatus::Running;

    AppState {
        screen_mode: ScreenMode::Dashboard,
        repositories: vec![repo1, repo2, repo3],
        agents: vec![a1, a2, a3],
        selected_repository_index: Some(0),
        selected_agent_index: Some(0),
        ..Default::default()
    }
}

// ============================================================================
// Enter / Exit Grab
// ============================================================================

#[test]
fn space_grabs_repository_in_repositories_pane() {
    let mut state = create_dashboard_test_state();
    state.pane_focus = PaneFocus::Repositories;
    state.selected_repository_index = Some(0);

    state = state.apply(AppEvent::EnterDashboardGrab);

    assert_eq!(
        state.dashboard_grab,
        Some(DashboardGrabPane::Repository { visible_index: 0 })
    );
}

#[test]
fn space_grabs_agent_in_agents_pane() {
    let mut state = create_dashboard_test_state();
    state.pane_focus = PaneFocus::Agents;
    state.selected_repository_index = Some(0);
    state.selected_agent_index = Some(0);

    state = state.apply(AppEvent::EnterDashboardGrab);

    assert_eq!(
        state.dashboard_grab,
        Some(DashboardGrabPane::Agent {
            repository_id: RepositoryId("repo-a".into()),
            local_index: 0
        })
    );
}

#[test]
fn enter_grab_is_noop_in_terminal_pane() {
    let mut state = create_dashboard_test_state();
    state.pane_focus = PaneFocus::Terminal;

    state = state.apply(AppEvent::EnterDashboardGrab);

    assert_eq!(state.dashboard_grab, None);
}

#[test]
fn exit_grab_clears_grab_state() {
    let mut state = create_dashboard_test_state();
    state.dashboard_grab = Some(DashboardGrabPane::Repository { visible_index: 1 });

    state = state.apply(AppEvent::ExitDashboardGrab);

    assert_eq!(state.dashboard_grab, None);
}

// ============================================================================
// Repository Reorder
// ============================================================================

#[test]
fn grab_move_up_reorders_repository() {
    let mut state = create_dashboard_test_state();
    // repos: [alpha, bravo, charlie], grab bravo at visible_index 1
    state.selected_repository_index = Some(1);
    state.dashboard_grab = Some(DashboardGrabPane::Repository { visible_index: 1 });

    state = state.apply(AppEvent::DashboardGrabMoveUp);

    // Expected order: [bravo, alpha, charlie]
    assert_eq!(state.repositories[0].name, "bravo");
    assert_eq!(state.repositories[1].name, "alpha");
    assert_eq!(
        state.dashboard_grab,
        Some(DashboardGrabPane::Repository { visible_index: 0 })
    );
    assert_eq!(state.selected_repository_index, Some(0));
}

#[test]
fn grab_move_down_reorders_repository() {
    let mut state = create_dashboard_test_state();
    // repos: [alpha, bravo, charlie], grab alpha at visible_index 0
    state.selected_repository_index = Some(0);
    state.dashboard_grab = Some(DashboardGrabPane::Repository { visible_index: 0 });

    state = state.apply(AppEvent::DashboardGrabMoveDown);

    // Expected order: [bravo, alpha, charlie]
    assert_eq!(state.repositories[0].name, "bravo");
    assert_eq!(state.repositories[1].name, "alpha");
    assert_eq!(
        state.dashboard_grab,
        Some(DashboardGrabPane::Repository { visible_index: 1 })
    );
    assert_eq!(state.selected_repository_index, Some(1));
}

#[test]
fn grab_move_up_at_top_stays() {
    let mut state = create_dashboard_test_state();
    state.selected_repository_index = Some(0);
    state.dashboard_grab = Some(DashboardGrabPane::Repository { visible_index: 0 });

    state = state.apply(AppEvent::DashboardGrabMoveUp);

    assert_eq!(
        state.dashboard_grab,
        Some(DashboardGrabPane::Repository { visible_index: 0 })
    );
    assert_eq!(state.repositories[0].name, "alpha");
}

#[test]
fn grab_move_down_at_bottom_stays() {
    let mut state = create_dashboard_test_state();
    state.selected_repository_index = Some(2);
    state.dashboard_grab = Some(DashboardGrabPane::Repository { visible_index: 2 });

    state = state.apply(AppEvent::DashboardGrabMoveDown);

    assert_eq!(
        state.dashboard_grab,
        Some(DashboardGrabPane::Repository { visible_index: 2 })
    );
    assert_eq!(state.repositories[2].name, "charlie");
}

#[test]
fn full_reorder_flow_repository_grab_move_drop_preserves_order() {
    let mut state = create_dashboard_test_state();
    state.pane_focus = PaneFocus::Repositories;
    state.selected_repository_index = Some(0);

    state = state.apply(AppEvent::EnterDashboardGrab);
    assert!(state.dashboard_grab.is_some());

    state = state.apply(AppEvent::DashboardGrabMoveDown);
    state = state.apply(AppEvent::ExitDashboardGrab);

    assert_eq!(state.dashboard_grab, None);
    assert_eq!(state.repositories[0].name, "bravo");
    assert_eq!(state.repositories[1].name, "alpha");
}

// ============================================================================
// Agent Reorder (within repository)
// ============================================================================

fn create_multi_agent_dashboard_state() -> AppState {
    let repo = Repository::new(
        RepositoryId("repo-a".into()),
        "alpha".into(),
        "alpha".into(),
        PathBuf::from("/projects/alpha"),
    );

    let mut a1 = Agent::new(
        AgentId("a1".into()),
        repo.id.clone(),
        "agent-one".into(),
        PathBuf::from("/projects/alpha/a1"),
    );
    a1.status = AgentStatus::Running;
    let mut a2 = Agent::new(
        AgentId("a2".into()),
        repo.id.clone(),
        "agent-two".into(),
        PathBuf::from("/projects/alpha/a2"),
    );
    a2.status = AgentStatus::Running;
    let mut a3 = Agent::new(
        AgentId("a3".into()),
        repo.id.clone(),
        "agent-three".into(),
        PathBuf::from("/projects/alpha/a3"),
    );
    a3.status = AgentStatus::Running;

    AppState {
        screen_mode: ScreenMode::Dashboard,
        repositories: vec![repo],
        agents: vec![a1, a2, a3],
        selected_repository_index: Some(0),
        selected_agent_index: Some(0),
        ..Default::default()
    }
}

#[test]
fn grab_move_up_reorders_agent_within_repository() {
    let mut state = create_multi_agent_dashboard_state();
    state.pane_focus = PaneFocus::Agents;
    // agents: [agent-one, agent-two, agent-three], grab agent-two at local 1
    state.selected_agent_index = Some(1);
    state.dashboard_grab = Some(DashboardGrabPane::Agent {
        repository_id: RepositoryId("repo-a".into()),
        local_index: 1,
    });

    state = state.apply(AppEvent::DashboardGrabMoveUp);

    // Expected order: [agent-two, agent-one, agent-three]
    assert_eq!(state.agents[0].name, "agent-two");
    assert_eq!(state.agents[1].name, "agent-one");
    assert_eq!(
        state.dashboard_grab,
        Some(DashboardGrabPane::Agent {
            repository_id: RepositoryId("repo-a".into()),
            local_index: 0
        })
    );
    assert_eq!(state.selected_agent_index, Some(0));
}

#[test]
fn grab_move_down_reorders_agent_within_repository() {
    let mut state = create_multi_agent_dashboard_state();
    state.pane_focus = PaneFocus::Agents;
    // grab agent-one at local 0
    state.selected_agent_index = Some(0);
    state.dashboard_grab = Some(DashboardGrabPane::Agent {
        repository_id: RepositoryId("repo-a".into()),
        local_index: 0,
    });

    state = state.apply(AppEvent::DashboardGrabMoveDown);

    // Expected order: [agent-two, agent-one, agent-three]
    assert_eq!(state.agents[0].name, "agent-two");
    assert_eq!(state.agents[1].name, "agent-one");
    assert_eq!(
        state.dashboard_grab,
        Some(DashboardGrabPane::Agent {
            repository_id: RepositoryId("repo-a".into()),
            local_index: 1
        })
    );
    assert_eq!(state.selected_agent_index, Some(1));
}

#[test]
fn full_reorder_flow_agent_grab_move_drop_preserves_order() {
    let mut state = create_multi_agent_dashboard_state();
    state.pane_focus = PaneFocus::Agents;
    state.selected_agent_index = Some(0);

    state = state.apply(AppEvent::EnterDashboardGrab);
    assert!(state.dashboard_grab.is_some());

    state = state.apply(AppEvent::DashboardGrabMoveDown);
    state = state.apply(AppEvent::ExitDashboardGrab);

    assert_eq!(state.dashboard_grab, None);
    assert_eq!(state.agents[0].name, "agent-two");
    assert_eq!(state.agents[1].name, "agent-one");
}

// ============================================================================
// Visibility-filtered reorder
// ============================================================================

#[test]
fn grab_mode_uses_visible_index_space_when_idle_repositories_hidden() {
    let mut state = create_dashboard_test_state();
    state.hide_idle_repositories = true;

    let repo1_id = state.repositories[0].id.clone();
    let repo2_id = state.repositories[1].id.clone();
    let repo3_id = state.repositories[2].id.clone();

    // repo2's agent is idle (not running) → hidden under the idle filter.
    state.agents[1].status = AgentStatus::Queued;
    // repo3 has a running agent, so visible set is [repo1, repo3].
    state.selected_repository_index = Some(2);

    state = state.apply(AppEvent::EnterDashboardGrab);
    // repo3 is visible_index 1.
    assert_eq!(
        state.dashboard_grab,
        Some(DashboardGrabPane::Repository { visible_index: 1 })
    );

    state = state.apply(AppEvent::DashboardGrabMoveUp);

    // repo3 swaps with repo1 in the visible set; repo2 stays in place globally.
    assert_eq!(state.repositories[0].id, repo3_id);
    assert_eq!(state.repositories[1].id, repo2_id);
    assert_eq!(state.repositories[2].id, repo1_id);
    assert_eq!(
        state.dashboard_grab,
        Some(DashboardGrabPane::Repository { visible_index: 0 })
    );
    assert_eq!(state.selected_repository_index, Some(0));
}

// ============================================================================
// Agent grab scope isolation
// ============================================================================

#[test]
fn agent_grab_only_affects_agents_within_selected_repository() {
    let repo1 = Repository::new(
        RepositoryId("repo-a".into()),
        "alpha".into(),
        "alpha".into(),
        PathBuf::from("/projects/alpha"),
    );
    let repo2 = Repository::new(
        RepositoryId("repo-b".into()),
        "bravo".into(),
        "bravo".into(),
        PathBuf::from("/projects/bravo"),
    );

    let mut a1 = Agent::new(
        AgentId("a1".into()),
        repo1.id.clone(),
        "alpha-one".into(),
        PathBuf::from("/projects/alpha/a1"),
    );
    a1.status = AgentStatus::Running;
    let mut a2 = Agent::new(
        AgentId("a2".into()),
        repo1.id.clone(),
        "alpha-two".into(),
        PathBuf::from("/projects/alpha/a2"),
    );
    a2.status = AgentStatus::Running;
    let mut b1 = Agent::new(
        AgentId("b1".into()),
        repo2.id.clone(),
        "bravo-one".into(),
        PathBuf::from("/projects/bravo/b1"),
    );
    b1.status = AgentStatus::Running;
    let mut b2 = Agent::new(
        AgentId("b2".into()),
        repo2.id.clone(),
        "bravo-two".into(),
        PathBuf::from("/projects/bravo/b2"),
    );
    b2.status = AgentStatus::Running;

    let mut state = AppState {
        screen_mode: ScreenMode::Dashboard,
        repositories: vec![repo1, repo2],
        agents: vec![a1, a2, b1, b2],
        selected_repository_index: Some(0),
        selected_agent_index: Some(1),
        ..Default::default()
    };
    state.pane_focus = PaneFocus::Agents;
    // Grab alpha-two (local 1) and move up.
    state.dashboard_grab = Some(DashboardGrabPane::Agent {
        repository_id: RepositoryId("repo-a".into()),
        local_index: 1,
    });

    state = state.apply(AppEvent::DashboardGrabMoveUp);

    // alpha agents swapped; bravo agents unchanged.
    assert_eq!(state.agents[0].name, "alpha-two");
    assert_eq!(state.agents[1].name, "alpha-one");
    assert_eq!(state.agents[2].name, "bravo-one");
    assert_eq!(state.agents[3].name, "bravo-two");
}

// ============================================================================
// Grab clearing on pane/mode transitions
// ============================================================================

#[test]
fn cycle_pane_focus_clears_dashboard_grab() {
    let mut state = create_dashboard_test_state();
    state.pane_focus = PaneFocus::Repositories;
    state.dashboard_grab = Some(DashboardGrabPane::Repository { visible_index: 0 });

    state = state.apply(AppEvent::CyclePaneFocus);

    assert_eq!(state.pane_focus, PaneFocus::Agents);
    assert_eq!(state.dashboard_grab, None);
}

#[test]
fn move_pane_focus_left_clears_dashboard_grab() {
    let mut state = create_dashboard_test_state();
    state.pane_focus = PaneFocus::Agents;
    state.dashboard_grab = Some(DashboardGrabPane::Agent {
        repository_id: RepositoryId("repo-a".into()),
        local_index: 0,
    });

    state = state.apply(AppEvent::NavigateLeft);

    assert_eq!(state.pane_focus, PaneFocus::Repositories);
    assert_eq!(state.dashboard_grab, None);
}

#[test]
fn move_pane_focus_right_clears_dashboard_grab() {
    let mut state = create_dashboard_test_state();
    state.pane_focus = PaneFocus::Repositories;
    state.dashboard_grab = Some(DashboardGrabPane::Repository { visible_index: 0 });

    state = state.apply(AppEvent::NavigateRight);

    assert_eq!(state.pane_focus, PaneFocus::Agents);
    assert_eq!(state.dashboard_grab, None);
}

#[test]
fn enter_split_mode_clears_dashboard_grab() {
    let mut state = create_dashboard_test_state();
    state.dashboard_grab = Some(DashboardGrabPane::Repository { visible_index: 0 });

    state = state.apply(AppEvent::EnterSplitMode);

    assert_eq!(state.screen_mode, ScreenMode::Split);
    assert_eq!(state.dashboard_grab, None);
}

// ============================================================================
// Persistence: reordered Vec order survives to_persisted_state mapping
// ============================================================================

#[test]
fn reordered_repository_vec_order_matches_persisted_order() {
    use jefe::persistence::State as PersistedState;

    let mut state = create_dashboard_test_state();
    state.pane_focus = PaneFocus::Repositories;
    state.selected_repository_index = Some(0);
    state.dashboard_grab = Some(DashboardGrabPane::Repository { visible_index: 0 });

    state = state.apply(AppEvent::DashboardGrabMoveDown);

    // Manually mirror to_persisted_state (private to app_input) and verify the
    // Vec order is preserved — persistence derives directly from the Vec.
    let persisted = PersistedState {
        schema_version: jefe::persistence::STATE_SCHEMA_VERSION,
        repositories: state.repositories.clone(),
        agents: state.agents.clone(),
        selected_repository_index: state.selected_repository_index,
        selected_agent_index: state.selected_agent_index,
        hide_idle_repositories: state.hide_idle_repositories,
        last_selected_agent_by_repo: state.last_selected_agent_by_repo.clone(),
        pane_focus: String::new(),
        terminal_focused: false,
        user_preferences: jefe::domain::UserPreferences::default(),
    };

    assert_eq!(persisted.repositories[0].name, "bravo");
    assert_eq!(persisted.repositories[1].name, "alpha");
    assert_eq!(persisted.repositories[2].name, "charlie");
}

// ============================================================================
// Agent grab boundary conditions (MEDIUM-9)
// ============================================================================

#[test]
fn agent_grab_move_up_at_top_stays() {
    let mut state = create_multi_agent_dashboard_state();
    state.pane_focus = PaneFocus::Agents;
    // agents: [agent-one, agent-two, agent-three], grab agent-one at local 0
    state.selected_agent_index = Some(0);
    state.dashboard_grab = Some(DashboardGrabPane::Agent {
        repository_id: RepositoryId("repo-a".into()),
        local_index: 0,
    });

    state = state.apply(AppEvent::DashboardGrabMoveUp);

    // Already at the top — no change.
    assert_eq!(state.agents[0].name, "agent-one");
    assert_eq!(state.agents[1].name, "agent-two");
    assert_eq!(state.agents[2].name, "agent-three");
    assert_eq!(
        state.dashboard_grab,
        Some(DashboardGrabPane::Agent {
            repository_id: RepositoryId("repo-a".into()),
            local_index: 0
        })
    );
}

#[test]
fn agent_grab_move_down_at_bottom_stays() {
    let mut state = create_multi_agent_dashboard_state();
    state.pane_focus = PaneFocus::Agents;
    // agents: [agent-one, agent-two, agent-three], grab agent-three at local 2
    state.selected_agent_index = Some(2);
    state.dashboard_grab = Some(DashboardGrabPane::Agent {
        repository_id: RepositoryId("repo-a".into()),
        local_index: 2,
    });

    state = state.apply(AppEvent::DashboardGrabMoveDown);

    // Already at the bottom — no change.
    assert_eq!(state.agents[0].name, "agent-one");
    assert_eq!(state.agents[1].name, "agent-two");
    assert_eq!(state.agents[2].name, "agent-three");
    assert_eq!(
        state.dashboard_grab,
        Some(DashboardGrabPane::Agent {
            repository_id: RepositoryId("repo-a".into()),
            local_index: 2
        })
    );
}

// ============================================================================
// Stale-grab edge cases (MEDIUM-8)
// ============================================================================

#[test]
fn toggle_hide_idle_clears_dashboard_grab() {
    let mut state = create_dashboard_test_state();
    state.pane_focus = PaneFocus::Repositories;
    state.dashboard_grab = Some(DashboardGrabPane::Repository { visible_index: 0 });
    assert!(state.dashboard_grab.is_some());

    state = state.apply(AppEvent::ToggleHideIdleRepositories);

    assert_eq!(state.dashboard_grab, None);
}

#[test]
fn navigation_clears_dashboard_grab() {
    let mut state = create_dashboard_test_state();
    state.pane_focus = PaneFocus::Repositories;
    state.dashboard_grab = Some(DashboardGrabPane::Repository { visible_index: 0 });
    assert!(state.dashboard_grab.is_some());

    state = state.apply(AppEvent::NavigateDown);

    assert_eq!(state.dashboard_grab, None);
}

#[test]
fn select_repository_clears_dashboard_grab() {
    let mut state = create_dashboard_test_state();
    state.pane_focus = PaneFocus::Repositories;
    state.dashboard_grab = Some(DashboardGrabPane::Repository { visible_index: 0 });
    assert!(state.dashboard_grab.is_some());

    state = state.apply(AppEvent::SelectRepository(1));

    assert_eq!(state.dashboard_grab, None);
}

#[test]
fn agent_grab_carries_repository_id() {
    let mut state = create_multi_agent_dashboard_state();
    state.pane_focus = PaneFocus::Agents;
    state.selected_repository_index = Some(0);
    state.selected_agent_index = Some(1);

    state = state.apply(AppEvent::EnterDashboardGrab);

    match state.dashboard_grab {
        Some(DashboardGrabPane::Agent {
            repository_id,
            local_index,
        }) => {
            assert_eq!(repository_id, RepositoryId("repo-a".into()));
            assert_eq!(local_index, 1);
        }
        other => panic!("expected Agent grab with repository_id, got {other:?}"),
    }
}

// ============================================================================
// finalize_message validates stale grab (CodeRabbit finding)
// ============================================================================

#[test]
fn enter_issues_mode_clears_dashboard_grab_via_finalize() {
    let mut state = create_dashboard_test_state();
    state.dashboard_grab = Some(DashboardGrabPane::Repository { visible_index: 0 });

    // EnterIssuesMode changes screen_mode away from Dashboard; finalize_message
    // validation must clear the stale grab.
    state = state.apply(AppEvent::EnterIssuesMode);

    assert_ne!(state.screen_mode, ScreenMode::Dashboard);
    assert_eq!(state.dashboard_grab, None);
}

#[test]
fn repository_deletion_clears_stale_grab_via_finalize() {
    let mut state = create_dashboard_test_state();
    // repos: [alpha, bravo, charlie] — grab charlie at visible_index 2
    state.dashboard_grab = Some(DashboardGrabPane::Repository { visible_index: 2 });

    // Deleting a repository shrinks the Vec; finalize_message validates the
    // visible_index is still in bounds and clears if not.
    // Here we simulate the state change by applying a persistence/system event
    // that triggers finalize. OpenHelp is a modal event that goes through finalize.
    state = state.apply(AppEvent::OpenHelp);

    // With 3 repos, visible_index 2 is still valid, so grab should survive.
    assert_eq!(
        state.dashboard_grab,
        Some(DashboardGrabPane::Repository { visible_index: 2 })
    );

    // Now manually shrink the repositories and trigger finalize via another event.
    state.repositories.pop();
    state = state.apply(AppEvent::CloseModal);

    // visible_index 2 is now out of bounds (only 2 repos left) — grab cleared.
    assert_eq!(state.dashboard_grab, None);
}

#[test]
fn agent_deletion_clears_stale_agent_grab_via_finalize() {
    let mut state = create_multi_agent_dashboard_state();
    // agents: [agent-one, agent-two, agent-three] — grab at local_index 2
    let repo_id = state.repositories[0].id.clone();
    state.dashboard_grab = Some(DashboardGrabPane::Agent {
        repository_id: repo_id,
        local_index: 2,
    });

    // Remove the last agent so local_index 2 is out of bounds.
    state.agents.pop();
    state = state.apply(AppEvent::OpenHelp);

    assert_eq!(state.dashboard_grab, None);
}

#[test]
fn agent_grab_for_deleted_repository_clears_via_finalize() {
    let mut state = create_dashboard_test_state();
    let deleted_repo_id = state.repositories[2].id.clone();
    state.dashboard_grab = Some(DashboardGrabPane::Agent {
        repository_id: deleted_repo_id,
        local_index: 0,
    });

    // Remove the repository the grab points to.
    state.repositories.pop();
    state = state.apply(AppEvent::OpenHelp);

    // repository_id no longer exists → grab cleared.
    assert_eq!(state.dashboard_grab, None);
}

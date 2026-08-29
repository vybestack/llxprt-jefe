//! Agent visibility filter and display–selection consistency tests (issue #41).

use crate::support::TestOptionExt;

use std::path::PathBuf;

use jefe::domain::{Agent, AgentId, AgentStatus, Repository, RepositoryId};
use jefe::state::screen_overlays::ConfirmationRequest;
use jefe::state::transition::TransitionExt;
use jefe::state::{AppEvent, AppState, PaneFocus};

fn repository(id: &str) -> Repository {
    Repository::new(
        RepositoryId(id.into()),
        jefe::domain::shipped_agent_type(3),
        jefe::domain::TypedMap::new(),
        id.to_uppercase(),
        id.into(),
        PathBuf::from(format!("/{id}")),
    )
}

fn agent(id: &str, name: &str, status: AgentStatus) -> Agent {
    let mut agent = Agent::new(
        AgentId(id.into()),
        RepositoryId("r1".into()),
        jefe::domain::shipped_agent_type(3),
        jefe::domain::TypedMap::new(),
        name.into(),
        PathBuf::from(format!("/r1/{id}")),
    );
    agent.status = status;
    agent
}

fn visibility_state(agents: Vec<Agent>, selected_agent_index: usize) -> AppState {
    {
        let mut state = crate::common_app_state::app_state();
        state.repositories = vec![repository("r1")];
        state.agents = agents;
        state.selected_repository_index = Some(0);
        state.selected_agent_index = Some(selected_agent_index);
        state.pane_focus = PaneFocus::Agents;
        state
    }
}

#[test]
fn visible_agents_matches_agent_indices_when_idle_hidden() {
    let state = visibility_state(
        vec![
            agent("idle1", "Idle A", AgentStatus::Queued),
            agent("run1", "Running B", AgentStatus::Running),
            agent("run2", "Running C", AgentStatus::Running),
        ],
        1,
    );

    let hidden = state
        .apply(AppEvent::ToggleHideIdleRepositories)
        .committed_pure();
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
    let state = visibility_state(
        vec![
            agent("idle1", "Idle A", AgentStatus::Queued),
            agent("run1", "Running B", AgentStatus::Running),
            agent("idle2", "Idle C", AgentStatus::Queued),
            agent("run2", "Running D", AgentStatus::Running),
        ],
        1,
    );

    let hidden = state
        .apply(AppEvent::ToggleHideIdleRepositories)
        .committed_pure();
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
    let state = {
        let mut state = crate::common_app_state::app_state();
        state.repositories = vec![Repository::new(
            RepositoryId("r1".into()),
            jefe::domain::shipped_agent_type(3),
            jefe::domain::TypedMap::new(),
            "R1".into(),
            "r1".into(),
            PathBuf::from("/r1"),
        )];
        state.agents = vec![
            Agent::new(
                AgentId("idle1".into()),
                RepositoryId("r1".into()),
                jefe::domain::shipped_agent_type(3),
                jefe::domain::TypedMap::new(),
                "Idle A".into(),
                PathBuf::from("/r1/idle1"),
            ),
            {
                let mut running = Agent::new(
                    AgentId("run1".into()),
                    RepositoryId("r1".into()),
                    jefe::domain::shipped_agent_type(3),
                    jefe::domain::TypedMap::new(),
                    "Running B".into(),
                    PathBuf::from("/r1/run1"),
                );
                running.status = AgentStatus::Running;
                running
            },
        ];
        state.selected_repository_index = Some(0);
        state.selected_agent_index = Some(0);
        state.pane_focus = PaneFocus::Agents;
        state.hide_idle_repositories = false;
        state
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
    let state = {
        let mut state = crate::common_app_state::app_state();
        state.repositories = vec![repository("r1")];
        state.agents = vec![
            agent("idle1", "Idle A", AgentStatus::Queued),
            agent("target", "Target Agent", AgentStatus::Running),
            agent("other", "Other Agent", AgentStatus::Running),
        ];
        state.selected_repository_index = Some(0);
        state.selected_agent_index = Some(1);
        state.pane_focus = PaneFocus::Agents;
        state
    };

    let hidden = state
        .apply(AppEvent::ToggleHideIdleRepositories)
        .committed_pure();
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

    let with_modal = hidden
        .apply(AppEvent::OpenDeleteAgent(selected_id))
        .committed_pure();
    let Some(ConfirmationRequest::DeleteAgent { id, .. }) =
        with_modal.nav.current().overlays().generic_confirmation()
    else {
        panic!("expected exact-instance delete-agent confirmation");
    };
    assert_eq!(
        *id,
        AgentId("target".into()),
        "delete must target the highlighted agent, not an adjacent one"
    );
}

#[test]
fn visible_agent_count_includes_all_when_filter_off() {
    let state = {
        let mut state = crate::common_app_state::app_state();
        state.repositories = vec![Repository::new(
            RepositoryId("r1".into()),
            jefe::domain::shipped_agent_type(3),
            jefe::domain::TypedMap::new(),
            "R1".into(),
            "r1".into(),
            PathBuf::from("/r1"),
        )];
        state.agents = vec![
            Agent::new(
                AgentId("idle1".into()),
                RepositoryId("r1".into()),
                jefe::domain::shipped_agent_type(3),
                jefe::domain::TypedMap::new(),
                "Idle A".into(),
                PathBuf::from("/r1/idle1"),
            ),
            {
                let mut a = Agent::new(
                    AgentId("run1".into()),
                    RepositoryId("r1".into()),
                    jefe::domain::shipped_agent_type(3),
                    jefe::domain::TypedMap::new(),
                    "Running B".into(),
                    PathBuf::from("/r1/run1"),
                );
                a.status = AgentStatus::Running;
                a
            },
        ];
        state.selected_repository_index = Some(0);
        state.selected_agent_index = Some(0);
        state.pane_focus = PaneFocus::Agents;
        state
    };

    assert_eq!(state.visible_agent_count(), 2);
    assert_eq!(
        state.visible_agent_count_for_repository(&RepositoryId("r1".into())),
        2
    );
}

#[test]
fn visible_agent_count_excludes_inactive_when_filter_on() {
    let state = {
        let mut state = crate::common_app_state::app_state();
        state.repositories = vec![Repository::new(
            RepositoryId("r1".into()),
            jefe::domain::shipped_agent_type(3),
            jefe::domain::TypedMap::new(),
            "R1".into(),
            "r1".into(),
            PathBuf::from("/r1"),
        )];
        state.agents = vec![
            Agent::new(
                AgentId("idle1".into()),
                RepositoryId("r1".into()),
                jefe::domain::shipped_agent_type(3),
                jefe::domain::TypedMap::new(),
                "Idle A".into(),
                PathBuf::from("/r1/idle1"),
            ),
            {
                let mut a = Agent::new(
                    AgentId("run1".into()),
                    RepositoryId("r1".into()),
                    jefe::domain::shipped_agent_type(3),
                    jefe::domain::TypedMap::new(),
                    "Running B".into(),
                    PathBuf::from("/r1/run1"),
                );
                a.status = AgentStatus::Running;
                a
            },
        ];
        state.selected_repository_index = Some(0);
        state.selected_agent_index = Some(1);
        state.pane_focus = PaneFocus::Agents;
        state
    };

    let hidden = state
        .apply(AppEvent::ToggleHideIdleRepositories)
        .committed_pure();
    assert_eq!(hidden.visible_agent_count(), 1);
    assert_eq!(
        hidden.visible_agent_count_for_repository(&RepositoryId("r1".into())),
        1
    );
}

#[test]
fn visible_repo_count_matches_visible_repository_indices() {
    let state = {
        let mut state = crate::common_app_state::app_state();
        state.repositories = vec![
            Repository::new(
                RepositoryId("r1".into()),
                jefe::domain::shipped_agent_type(3),
                jefe::domain::TypedMap::new(),
                "R1".into(),
                "r1".into(),
                PathBuf::from("/r1"),
            ),
            Repository::new(
                RepositoryId("r2".into()),
                jefe::domain::shipped_agent_type(3),
                jefe::domain::TypedMap::new(),
                "R2".into(),
                "r2".into(),
                PathBuf::from("/r2"),
            ),
        ];
        state.agents = vec![
            {
                let mut a = Agent::new(
                    AgentId("run1".into()),
                    RepositoryId("r1".into()),
                    jefe::domain::shipped_agent_type(3),
                    jefe::domain::TypedMap::new(),
                    "Running A".into(),
                    PathBuf::from("/r1/run1"),
                );
                a.status = AgentStatus::Running;
                a
            },
            Agent::new(
                AgentId("idle1".into()),
                RepositoryId("r2".into()),
                jefe::domain::shipped_agent_type(3),
                jefe::domain::TypedMap::new(),
                "Idle B".into(),
                PathBuf::from("/r2/idle1"),
            ),
        ];
        state.selected_repository_index = Some(0);
        state.selected_agent_index = Some(0);
        state.pane_focus = PaneFocus::Repositories;
        state
    };

    // Filter off: both repos visible
    assert_eq!(state.visible_repository_indices().len(), 2);

    // Filter on: only r1 visible (has running agent)
    let hidden = state
        .apply(AppEvent::ToggleHideIdleRepositories)
        .committed_pure();
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
        jefe::domain::shipped_agent_type(3),
        jefe::domain::TypedMap::new(),
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
    let mut state = {
        let mut state = crate::common_app_state::app_state();
        state.repositories = vec![repository("r1")];
        state.agents = vec![running_agent("a1", "Agent One", "r1")];
        state.selected_repository_index = Some(0);
        state.selected_agent_index = Some(0);
        state.pane_focus = PaneFocus::Agents;
        state.hide_idle_repositories = true;
        state
    };
    state.normalize_selection_indices();

    let killed = state
        .apply(AppEvent::KillAgent(AgentId("a1".into())))
        .committed_discarding_effects();

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
    let mut state = {
        let mut state = crate::common_app_state::app_state();
        state.repositories = vec![repository("r1"), repository("r2")];
        state.agents = vec![
            running_agent("a1", "Agent One", "r1"),
            running_agent("a2", "Agent Two", "r2"),
        ];
        state.selected_repository_index = Some(0);
        state.selected_agent_index = Some(0);
        state.pane_focus = PaneFocus::Agents;
        state.hide_idle_repositories = true;
        state
    };
    state.normalize_selection_indices();

    let killed = state
        .apply(AppEvent::KillAgent(AgentId("a1".into())))
        .committed_discarding_effects();

    // Navigate down — this should clear the sticky list.
    let after_nav = killed.apply(AppEvent::NavigateDown).committed_pure();

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
    let mut state = {
        let mut state = crate::common_app_state::app_state();
        state.repositories = vec![repository("r1"), repository("r2")];
        state.agents = vec![
            running_agent("a1", "Agent One", "r1"),
            running_agent("a2", "Agent Two", "r2"),
        ];
        state.selected_repository_index = Some(0);
        state.selected_agent_index = Some(0);
        state.pane_focus = PaneFocus::Repositories;
        state.hide_idle_repositories = true;
        state
    };
    state.normalize_selection_indices();

    // Kill the only running agent in r1.
    let killed = state
        .apply(AppEvent::KillAgent(AgentId("a1".into())))
        .committed_discarding_effects();

    // r1 should still be visible because of the sticky dead agent.
    let visible_repos = killed.visible_repository_indices();
    assert!(
        visible_repos.contains(&0),
        "repo r1 should still be visible after killing its last running agent (sticky)"
    );

    // Navigate down — clears sticky, r1 should now be filtered out.
    let after_nav = killed.apply(AppEvent::NavigateDown).committed_pure();
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
    let mut state = {
        let mut state = crate::common_app_state::app_state();
        state.repositories = vec![repository("r1")];
        state.agents = vec![running_agent("a1", "Agent One", "r1")];
        state.selected_repository_index = Some(0);
        state.selected_agent_index = Some(0);
        state.pane_focus = PaneFocus::Agents;
        state.hide_idle_repositories = true;
        state
    };
    state.normalize_selection_indices();

    // Use AgentStatusChanged (external status update) instead of KillAgent.
    let after = state
        .apply(AppEvent::AgentStatusChanged(
            AgentId("a1".into()),
            AgentStatus::Dead,
        ))
        .committed_pure();

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
    let mut state = {
        let mut state = crate::common_app_state::app_state();
        state.repositories = vec![repository("r1")];
        state.agents = vec![running_agent("a1", "Agent One", "r1")];
        state.selected_repository_index = Some(0);
        state.selected_agent_index = Some(0);
        state.pane_focus = PaneFocus::Agents;
        state.hide_idle_repositories = false;
        state
    };
    state.normalize_selection_indices();

    // Kill while filter is OFF — sticky list should still be populated.
    let killed = state
        .apply(AppEvent::KillAgent(AgentId("a1".into())))
        .committed_discarding_effects();

    // Toggle filter ON — this is a display toggle, NOT navigation; sticky persists.
    let toggled = killed
        .apply(AppEvent::ToggleHideIdleRepositories)
        .committed_pure();
    assert!(toggled.hide_idle_repositories);

    let repo_id = RepositoryId("r1".into());
    let visible_agents = toggled.visible_agents_for_repository(&repo_id);
    assert!(
        visible_agents.iter().any(|a| a.id == AgentId("a1".into())),
        "toggling filter ON should NOT clear sticky — dead agent stays visible"
    );

    // Now navigate down — this clears sticky, and the dead agent is filtered out.
    let navigated = toggled.apply(AppEvent::NavigateDown).committed_pure();
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
    let mut state = {
        let mut state = crate::common_app_state::app_state();
        state.repositories = vec![repository("r1")];
        state.agents = vec![
            running_agent("a1", "Agent One", "r1"),
            running_agent("a2", "Agent Two", "r1"),
        ];
        state.selected_repository_index = Some(0);
        state.selected_agent_index = Some(0);
        state.pane_focus = PaneFocus::Agents;
        state.hide_idle_repositories = true;
        state
    };
    state.normalize_selection_indices();

    let killed_a = state
        .apply(AppEvent::KillAgent(AgentId("a1".into())))
        .committed_discarding_effects();
    let killed_b = killed_a
        .apply(AppEvent::KillAgent(AgentId("a2".into())))
        .committed_discarding_effects();

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
    let after_nav = killed_b.apply(AppEvent::NavigateDown).committed_pure();
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
    let mut state = {
        let mut state = crate::common_app_state::app_state();
        state.repositories = vec![repository("r1"), repository("r2")];
        state.agents = vec![
            running_agent("a1", "Agent One", "r1"),
            running_agent("a2", "Agent Two", "r2"),
        ];
        state.selected_repository_index = Some(0);
        state.selected_agent_index = Some(0);
        state.pane_focus = PaneFocus::Repositories;
        state.hide_idle_repositories = true;
        state
    };
    state.normalize_selection_indices();

    let killed = state
        .apply(AppEvent::KillAgent(AgentId("a1".into())))
        .committed_discarding_effects();

    // SelectRepository is a navigation message — clears sticky.
    let after_select = killed.apply(AppEvent::SelectRepository(0)).committed_pure();

    let repo_id = RepositoryId("r1".into());
    let visible_agents = after_select.visible_agents_for_repository(&repo_id);
    assert!(
        !visible_agents.iter().any(|a| a.id == AgentId("a1".into())),
        "SelectRepository should clear sticky and filter out the dead agent"
    );
}

// =============================================================================
// Sticky empty-repository visibility (issue #404)
//
// When hide_idle_repositories is ON and the user creates a new repository
// (which has no agents), the new repo should remain visible until ANY UI
// navigation occurs — mirroring the sticky-dead-agent behavior (issue #116).
// Without this, the freshly created repo disappears immediately because it
// has no running agents.
// =============================================================================

/// Drive the real New Repository form submit path: open the modal, type a
/// name, submit. Returns the state after submit with the new repo appended.
fn submit_new_repository(mut state: AppState, name: &str) -> AppState {
    state = state.apply(AppEvent::OpenNewRepository).committed_pure();
    for c in name.chars() {
        state = state.apply(AppEvent::FormChar(c)).committed_pure();
    }
    state.apply(AppEvent::SubmitForm).committed_pure()
}

/// Test 1 (A1): With hide_idle_repositories=true, submit the New Repository
/// form. The new repo has no agents, but it must remain visible and be the
/// selected repository so the user lands on it.
#[test]
fn new_repository_stays_visible_when_active_only_on() {
    // Start with one repo that has a running agent so active-only is useful.
    let mut state = {
        let mut state = crate::common_app_state::app_state();
        state.repositories = vec![repository("r1")];
        state.agents = vec![running_agent("a1", "Agent One", "r1")];
        state.selected_repository_index = Some(0);
        state.selected_agent_index = Some(0);
        state.pane_focus = PaneFocus::Repositories;
        state.hide_idle_repositories = true;
        state
    };
    state.normalize_selection_indices();

    let after = submit_new_repository(state, "NewRepo");

    // The new repo is the last one pushed.
    let new_repo_idx = after.repositories.len() - 1;
    let visible_repos = after.visible_repository_indices();
    assert!(
        visible_repos.contains(&new_repo_idx),
        "newly created empty repo must remain visible under active-only mode (issue #404)"
    );

    // And it should be the selected repository (form submit selects it).
    assert_eq!(
        after.selected_repository_index,
        Some(new_repo_idx),
        "new repo should be selected after submit"
    );
}

/// Test 2 (A2): After creating a sticky empty repo, navigating away should
/// clear the sticky set and the empty repo should be filtered out.
#[test]
fn navigate_after_new_repo_filters_empty_repo() {
    let mut state = {
        let mut state = crate::common_app_state::app_state();
        state.repositories = vec![repository("r1"), repository("r2")];
        state.agents = vec![running_agent("a1", "Agent One", "r1")];
        state.selected_repository_index = Some(0);
        state.selected_agent_index = Some(0);
        state.pane_focus = PaneFocus::Repositories;
        state.hide_idle_repositories = true;
        state
    };
    state.normalize_selection_indices();

    let created = submit_new_repository(state, "NewRepo");
    let new_repo_idx = created.repositories.len() - 1;
    assert!(created.visible_repository_indices().contains(&new_repo_idx));

    // Navigate down — clears sticky, empty new repo should be filtered out.
    let after_nav = created.apply(AppEvent::NavigateDown).committed_pure();
    let visible_after = after_nav.visible_repository_indices();
    assert!(
        !visible_after.contains(&new_repo_idx),
        "after navigation, the empty new repo should be filtered out"
    );
}

/// Test 3 (A3): Create the repo while the filter is OFF, then toggle the
/// filter ON. Toggling is a display change, not navigation, so the sticky
/// set must persist and the empty repo stays visible.
#[test]
fn new_repo_with_filter_off_then_toggle_on_keeps_sticky() {
    let mut state = {
        let mut state = crate::common_app_state::app_state();
        state.repositories = vec![repository("r1")];
        state.agents = vec![running_agent("a1", "Agent One", "r1")];
        state.selected_repository_index = Some(0);
        state.selected_agent_index = Some(0);
        state.pane_focus = PaneFocus::Repositories;
        state.hide_idle_repositories = false;
        state
    };
    state.normalize_selection_indices();

    // Create while filter OFF.
    let created = submit_new_repository(state, "NewRepo");
    let new_repo_idx = created.repositories.len() - 1;

    // Toggle filter ON — must NOT clear sticky.
    let toggled = created
        .apply(AppEvent::ToggleHideIdleRepositories)
        .committed_pure();
    assert!(toggled.hide_idle_repositories);
    assert!(
        toggled.visible_repository_indices().contains(&new_repo_idx),
        "toggling filter ON should NOT clear sticky — empty new repo stays visible"
    );

    // Now navigate — sticky clears, empty repo filtered out.
    let navigated = toggled.apply(AppEvent::NavigateDown).committed_pure();
    assert!(
        !navigated
            .visible_repository_indices()
            .contains(&new_repo_idx),
        "after navigation, sticky is cleared and empty new repo is filtered out"
    );
}

/// Test 4 (A4): SelectRepository (even to the same index) is a navigation
/// message and must clear the sticky set.
#[test]
fn sticky_cleared_on_select_repository_after_new_repo() {
    let mut state = {
        let mut state = crate::common_app_state::app_state();
        state.repositories = vec![repository("r1")];
        state.agents = vec![running_agent("a1", "Agent One", "r1")];
        state.selected_repository_index = Some(0);
        state.selected_agent_index = Some(0);
        state.pane_focus = PaneFocus::Repositories;
        state.hide_idle_repositories = true;
        state
    };
    state.normalize_selection_indices();

    let created = submit_new_repository(state, "NewRepo");
    let new_repo_idx = created.repositories.len() - 1;

    // SelectRepository is navigation — clears sticky.
    let after_select = created
        .apply(AppEvent::SelectRepository(new_repo_idx))
        .committed_pure();
    assert!(
        !after_select
            .visible_repository_indices()
            .contains(&new_repo_idx),
        "SelectRepository should clear sticky and filter out the empty new repo"
    );
}

/// Test 5 (A5): Create multiple empty repos in succession. All should be
/// sticky until navigation clears them.
#[test]
fn multiple_new_repos_all_sticky() {
    let mut state = {
        let mut state = crate::common_app_state::app_state();
        state.repositories = vec![repository("r1")];
        state.agents = vec![running_agent("a1", "Agent One", "r1")];
        state.selected_repository_index = Some(0);
        state.selected_agent_index = Some(0);
        state.pane_focus = PaneFocus::Repositories;
        state.hide_idle_repositories = true;
        state
    };
    state.normalize_selection_indices();

    let first = submit_new_repository(state, "NewOne");
    let first_idx = first.repositories.len() - 1;
    let second = submit_new_repository(first, "NewTwo");
    let second_idx = second.repositories.len() - 1;

    let visible = second.visible_repository_indices();
    assert!(
        visible.contains(&first_idx),
        "first new repo should be sticky-visible"
    );
    assert!(
        visible.contains(&second_idx),
        "second new repo should be sticky-visible"
    );

    // Navigate away — both filtered out (only r1 with its running agent remains).
    let after_nav = second.apply(AppEvent::NavigateDown).committed_pure();
    let visible_after = after_nav.visible_repository_indices();
    assert!(
        !visible_after.contains(&first_idx) && !visible_after.contains(&second_idx),
        "after navigation, both empty new repos should be filtered out"
    );
}

/// Test 6 (A6): The new-repo sticky mechanism must not interfere with the
/// existing sticky-dead-agent behavior. Kill an agent (sticky-dead), then
/// create a new repo (sticky-empty). Both stickies must hold until nav.
#[test]
fn new_repo_sticky_coexists_with_sticky_dead_agent() {
    let mut state = {
        let mut state = crate::common_app_state::app_state();
        state.repositories = vec![repository("r1")];
        state.agents = vec![running_agent("a1", "Agent One", "r1")];
        state.selected_repository_index = Some(0);
        state.selected_agent_index = Some(0);
        state.pane_focus = PaneFocus::Repositories;
        state.hide_idle_repositories = true;
        state
    };
    state.normalize_selection_indices();

    // Kill the agent (sticky-dead keeps r1 visible). KillAgent stages a
    // runtime effect; this contract covers visibility semantics only.
    let killed = state
        .apply(AppEvent::KillAgent(AgentId("a1".into())))
        .committed_discarding_effects();
    // Create a new empty repo (sticky-empty keeps it visible).
    let created = submit_new_repository(killed, "NewRepo");
    let new_repo_idx = created.repositories.len() - 1;

    let visible = created.visible_repository_indices();
    assert!(
        visible.contains(&0),
        "r1 should remain visible via sticky-dead-agent"
    );
    assert!(
        visible.contains(&new_repo_idx),
        "new empty repo should be visible via sticky-empty-repo"
    );

    // Navigate — both stickies clear. r1 has a dead agent, new repo is empty.
    let after_nav = created.apply(AppEvent::NavigateDown).committed_pure();
    let visible_after = after_nav.visible_repository_indices();
    assert!(
        visible_after.is_empty(),
        "after navigation, both stickies clear and no repo has a running agent"
    );
}

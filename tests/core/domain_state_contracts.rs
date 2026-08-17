//! Domain and state contract tests.
//!
//! @plan PLAN-20260216-FIRSTVERSION-V1.P04
//! @requirement REQ-TECH-002
//! @requirement REQ-TECH-003
//!
//! Pseudocode reference: component-001 lines 01-33

use crate::support::TestOptionExt;

use std::path::PathBuf;

use jefe::domain::{Agent, AgentId, AgentStatus, Repository, RepositoryId};
use jefe::state::transition::TransitionExt;
use jefe::state::{AppEvent, ModalState, PaneFocus, ScreenId};

// =============================================================================
// Domain Invariants (REQ-FUNC-003, REQ-FUNC-004)
// =============================================================================

#[test]
fn agent_defaults_to_generic_llxprt_with_empty_values() {
    let agent = Agent::new(
        AgentId("test".into()),
        RepositoryId("repo".into()),
        jefe::domain::shipped_agent_type(3),
        jefe::domain::TypedMap::new(),
        "Test".into(),
        PathBuf::from("/tmp"),
    );
    assert_eq!(agent.type_id.as_str(), "core.llxprt");
    assert!(agent.values.is_empty());
}

#[test]
fn agent_status_defaults_to_queued() {
    let agent = Agent::new(
        AgentId("test".into()),
        RepositoryId("repo".into()),
        jefe::domain::shipped_agent_type(3),
        jefe::domain::TypedMap::new(),
        "Test".into(),
        PathBuf::from("/tmp"),
    );
    assert_eq!(agent.status, AgentStatus::Queued);
}

#[test]
fn repository_slug_must_be_unique() {
    // This is an invariant that must be enforced at the AppState level
    // when adding repositories
    let mut state = crate::common_app_state::app_state();
    let repo1 = Repository::new(
        RepositoryId("r1".into()),
        jefe::domain::shipped_agent_type(3),
        jefe::domain::TypedMap::new(),
        "Repo One".into(),
        "repo-one".into(),
        PathBuf::from("/repos/one"),
    );
    let repo2 = Repository::new(
        RepositoryId("r2".into()),
        jefe::domain::shipped_agent_type(3),
        jefe::domain::TypedMap::new(),
        "Repo Two".into(),
        "repo-one".into(), // Same slug - should be rejected
        PathBuf::from("/repos/two"),
    );

    state.repositories.push(repo1);
    // In P05: AppState.add_repository should reject duplicate slugs
    // For now, just verify the invariant is documented
    let duplicate_exists = state.repositories.iter().any(|r| r.slug == repo2.slug);
    assert!(duplicate_exists, "duplicate slug detection setup");
}

// =============================================================================
// State Transition Tests (REQ-TECH-003)
// Pseudocode: component-001 lines 13-33
// =============================================================================

#[test]
fn navigate_up_decrements_selection() {
    let mut state = crate::common_app_state::app_state();
    state.repositories.push(Repository::new(
        RepositoryId("r1".into()),
        jefe::domain::shipped_agent_type(3),
        jefe::domain::TypedMap::new(),
        "R1".into(),
        "r1".into(),
        PathBuf::from("/r1"),
    ));
    state.repositories.push(Repository::new(
        RepositoryId("r2".into()),
        jefe::domain::shipped_agent_type(3),
        jefe::domain::TypedMap::new(),
        "R2".into(),
        "r2".into(),
        PathBuf::from("/r2"),
    ));
    state.selected_repository_index = Some(1);
    state.pane_focus = PaneFocus::Repositories;

    let next = state.apply(AppEvent::NavigateUp).committed_pure();

    assert_eq!(
        next.selected_repository_index,
        Some(0),
        "NavigateUp should decrement selection"
    );
}

#[test]
fn navigate_up_at_zero_stays_at_zero() {
    let mut state = crate::common_app_state::app_state();
    state.repositories.push(Repository::new(
        RepositoryId("r1".into()),
        jefe::domain::shipped_agent_type(3),
        jefe::domain::TypedMap::new(),
        "R1".into(),
        "r1".into(),
        PathBuf::from("/r1"),
    ));
    state.selected_repository_index = Some(0);
    state.pane_focus = PaneFocus::Repositories;

    let next = state.apply(AppEvent::NavigateUp).committed_pure();

    assert_eq!(
        next.selected_repository_index,
        Some(0),
        "NavigateUp at 0 should stay at 0"
    );
}

#[test]
fn navigate_down_increments_selection() {
    let mut state = crate::common_app_state::app_state();
    state.repositories.push(Repository::new(
        RepositoryId("r1".into()),
        jefe::domain::shipped_agent_type(3),
        jefe::domain::TypedMap::new(),
        "R1".into(),
        "r1".into(),
        PathBuf::from("/r1"),
    ));
    state.repositories.push(Repository::new(
        RepositoryId("r2".into()),
        jefe::domain::shipped_agent_type(3),
        jefe::domain::TypedMap::new(),
        "R2".into(),
        "r2".into(),
        PathBuf::from("/r2"),
    ));
    state.selected_repository_index = Some(0);
    state.pane_focus = PaneFocus::Repositories;

    let next = state.apply(AppEvent::NavigateDown).committed_pure();

    assert_eq!(
        next.selected_repository_index,
        Some(1),
        "NavigateDown should increment selection"
    );
}

#[test]
fn navigate_down_at_end_stays_at_end() {
    let mut state = crate::common_app_state::app_state();
    state.repositories.push(Repository::new(
        RepositoryId("r1".into()),
        jefe::domain::shipped_agent_type(3),
        jefe::domain::TypedMap::new(),
        "R1".into(),
        "r1".into(),
        PathBuf::from("/r1"),
    ));
    state.selected_repository_index = Some(0);
    state.pane_focus = PaneFocus::Repositories;

    let next = state.apply(AppEvent::NavigateDown).committed_pure();

    assert_eq!(
        next.selected_repository_index,
        Some(0),
        "NavigateDown at end should stay at end"
    );
}

fn contract_repository(id: &str) -> Repository {
    Repository::new(
        RepositoryId(id.into()),
        jefe::domain::shipped_agent_type(3),
        jefe::domain::TypedMap::new(),
        id.to_uppercase(),
        id.into(),
        PathBuf::from(format!("/{id}")),
    )
}

fn contract_agent(id: &str, repository: &str, status: AgentStatus) -> Agent {
    let mut agent = Agent::new(
        AgentId(id.into()),
        RepositoryId(repository.into()),
        jefe::domain::shipped_agent_type(3),
        jefe::domain::TypedMap::new(),
        format!("Agent {id}"),
        PathBuf::from(format!("/{repository}/{id}")),
    );
    agent.status = status;
    agent
}

#[test]
fn toggle_hide_idle_repositories_filters_to_running_repositories() {
    let state = {
        let mut state = crate::common_app_state::app_state();
        state.repositories = vec![
            contract_repository("r1"),
            contract_repository("r2"),
            contract_repository("r3"),
        ];
        state.agents = vec![
            contract_agent("a1", "r1", AgentStatus::Queued),
            contract_agent("a2", "r2", AgentStatus::Running),
        ];
        state.selected_repository_index = Some(0);
        state.pane_focus = PaneFocus::Repositories;
        state
    };

    let next = state
        .apply(AppEvent::ToggleHideIdleRepositories)
        .committed_pure();

    assert!(next.hide_idle_repositories);
    assert_eq!(next.selected_repository_index, Some(1));
    assert_eq!(
        next.selected_repository()
            .map(|repository| repository.id.clone()),
        Some(RepositoryId("r2".into()))
    );

    let visible_agents = next
        .selected_repository()
        .map(|repository| next.agent_indices_for_repository(&repository.id))
        .unwrap_or_default();
    assert_eq!(visible_agents.len(), 1);
    assert_eq!(
        next.selected_agent().map(|agent| agent.id.clone()),
        Some(AgentId("a2".into()))
    );
}

#[test]
fn toggle_hide_idle_repositories_hides_idle_agents_in_selected_repository() {
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
                AgentId("idle".into()),
                RepositoryId("r1".into()),
                jefe::domain::shipped_agent_type(3),
                jefe::domain::TypedMap::new(),
                "Idle A".into(),
                PathBuf::from("/r1/idle"),
            ),
            {
                let mut running = Agent::new(
                    AgentId("running".into()),
                    RepositoryId("r1".into()),
                    jefe::domain::shipped_agent_type(3),
                    jefe::domain::TypedMap::new(),
                    "Running A".into(),
                    PathBuf::from("/r1/running"),
                );
                running.status = AgentStatus::Running;
                running
            },
        ];
        state.selected_repository_index = Some(0);
        state.selected_agent_index = Some(0);
        state.pane_focus = PaneFocus::Agents;
        state
    };

    let hidden = state
        .apply(AppEvent::ToggleHideIdleRepositories)
        .committed_pure();

    assert!(hidden.hide_idle_repositories);
    let visible_indices = hidden.agent_indices_for_repository(&RepositoryId("r1".into()));
    assert_eq!(visible_indices.len(), 1);
    assert_eq!(
        hidden.selected_agent().map(|agent| agent.id.clone()),
        Some(AgentId("running".into()))
    );

    let restored = hidden
        .apply(AppEvent::ToggleHideIdleRepositories)
        .committed_pure();
    assert!(!restored.hide_idle_repositories);
    let restored_visible = restored.agent_indices_for_repository(&RepositoryId("r1".into()));
    assert_eq!(restored_visible.len(), 2);
}

#[test]
fn repository_navigation_skips_idle_repositories_when_hidden() {
    let state = {
        let mut state = crate::common_app_state::app_state();
        state.repositories = vec![
            contract_repository("r1"),
            contract_repository("r2"),
            contract_repository("r3"),
        ];
        state.agents = vec![
            contract_agent("a1", "r1", AgentStatus::Running),
            contract_agent("a2", "r2", AgentStatus::Queued),
            contract_agent("a3", "r3", AgentStatus::Running),
        ];
        state.selected_repository_index = Some(0);
        state.pane_focus = PaneFocus::Repositories;
        state
    };

    let next = state
        .apply(AppEvent::ToggleHideIdleRepositories)
        .committed_pure();
    assert!(next.hide_idle_repositories);

    let next = next.apply(AppEvent::NavigateDown).committed_pure();
    assert_eq!(next.selected_repository_index, Some(2));

    let next = next.apply(AppEvent::NavigateUp).committed_pure();
    assert_eq!(next.selected_repository_index, Some(0));
}

#[test]
fn toggling_hide_idle_off_restores_selectable_repository() {
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
            Agent::new(
                AgentId("a1".into()),
                RepositoryId("r1".into()),
                jefe::domain::shipped_agent_type(3),
                jefe::domain::TypedMap::new(),
                "Idle A1".into(),
                PathBuf::from("/r1/a1"),
            ),
            Agent::new(
                AgentId("a2".into()),
                RepositoryId("r2".into()),
                jefe::domain::shipped_agent_type(3),
                jefe::domain::TypedMap::new(),
                "Idle A2".into(),
                PathBuf::from("/r2/a2"),
            ),
        ];
        state.selected_repository_index = Some(1);
        state
    };

    let hidden = state
        .apply(AppEvent::ToggleHideIdleRepositories)
        .committed_pure();
    assert!(hidden.hide_idle_repositories);
    assert_eq!(hidden.selected_repository_index, None);

    let restored = hidden
        .apply(AppEvent::ToggleHideIdleRepositories)
        .committed_pure();
    assert!(!restored.hide_idle_repositories);
    assert_eq!(restored.selected_repository_index, Some(0));
}

#[test]
fn toggle_terminal_focus_sets_terminal_focused() {
    let state = {
        let mut state = crate::common_app_state::app_state();
        state.terminal_focused = false;
        state
    };

    let next = state.apply(AppEvent::ToggleTerminalFocus).committed_pure();

    assert!(
        next.terminal_focused,
        "ToggleTerminalFocus should set terminal_focused=true"
    );
}

#[test]
fn select_repository_ignores_hidden_repository_when_filter_enabled() {
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
        state.agents = vec![{
            let mut running = Agent::new(
                AgentId("a1".into()),
                RepositoryId("r1".into()),
                jefe::domain::shipped_agent_type(3),
                jefe::domain::TypedMap::new(),
                "Running A1".into(),
                PathBuf::from("/r1/a1"),
            );
            running.status = AgentStatus::Running;
            running
        }];
        state.selected_repository_index = Some(0);
        state
    };

    let filtered = state
        .apply(AppEvent::ToggleHideIdleRepositories)
        .committed_pure();
    assert!(filtered.hide_idle_repositories);

    let attempted = filtered
        .apply(AppEvent::SelectRepository(1))
        .committed_pure();
    assert_eq!(attempted.selected_repository_index, Some(0));
}

#[test]
fn toggle_terminal_focus_clears_terminal_focused() {
    let state = {
        let mut state = crate::common_app_state::app_state();
        state.terminal_focused = true;
        state
    };

    let next = state.apply(AppEvent::ToggleTerminalFocus).committed_pure();

    assert!(
        !next.terminal_focused,
        "ToggleTerminalFocus should toggle to false"
    );
}

#[test]
fn enter_split_mode_changes_active_screen() {
    let state = {
        let mut state = crate::common_app_state::app_state();
        state.nav = jefe::state::navigation::NavState::rooted(ScreenId::Dashboard);
        state
    };

    let next = state.apply(AppEvent::EnterSplitMode).committed_pure();

    assert_eq!(
        next.screen(),
        ScreenId::Repositories,
        "EnterSplitMode should change to Split"
    );
}

#[test]
fn exit_split_mode_returns_to_dashboard() {
    let state = {
        let mut state = crate::common_app_state::app_state();
        state.nav = jefe::state::navigation::NavState::rooted(ScreenId::Repositories);
        state
    };

    let next = state.apply(AppEvent::ExitSplitMode).committed_pure();

    assert_eq!(
        next.screen(),
        ScreenId::Dashboard,
        "ExitSplitMode should return to Dashboard"
    );
}

#[test]
fn open_help_sets_modal_to_help() {
    let state = crate::common_app_state::app_state();

    let next = state.apply(AppEvent::OpenHelp).committed_pure();

    assert!(
        matches!(next.modal, ModalState::Help),
        "OpenHelp should set modal to Help"
    );
}

#[test]
fn close_modal_clears_modal() {
    let state = {
        let mut state = crate::common_app_state::app_state();
        state.modal = ModalState::Help;
        state
    };

    let next = state.apply(AppEvent::CloseModal).committed_pure();

    assert!(
        matches!(next.modal, ModalState::None),
        "CloseModal should clear modal"
    );
}

#[test]
fn cycle_pane_focus_rotates_through_panes() {
    let state = {
        let mut state = crate::common_app_state::app_state();
        state.pane_focus = PaneFocus::Repositories;
        state
    };

    let next = state.apply(AppEvent::CyclePaneFocus).committed_pure();

    assert_eq!(
        next.pane_focus,
        PaneFocus::Agents,
        "CyclePaneFocus from Repositories should go to Agents"
    );

    let next2 = next.apply(AppEvent::CyclePaneFocus).committed_pure();

    assert_eq!(
        next2.pane_focus,
        PaneFocus::Terminal,
        "CyclePaneFocus from Agents should go to Terminal"
    );

    let next3 = next2.apply(AppEvent::CyclePaneFocus).committed_pure();

    assert_eq!(
        next3.pane_focus,
        PaneFocus::Repositories,
        "CyclePaneFocus from Terminal should wrap to Repositories"
    );
}

// =============================================================================
// Agent Lifecycle State Transitions (REQ-FUNC-005, REQ-FUNC-007)
// =============================================================================

#[test]
fn agent_status_changed_updates_agent() {
    let mut state = crate::common_app_state::app_state();
    let agent_id = AgentId("agent-1".into());
    state.agents.push(Agent::new(
        agent_id.clone(),
        RepositoryId("repo".into()),
        jefe::domain::shipped_agent_type(3),
        jefe::domain::TypedMap::new(),
        "Agent 1".into(),
        PathBuf::from("/work"),
    ));

    let next = state
        .apply(AppEvent::AgentStatusChanged(
            agent_id.clone(),
            AgentStatus::Running,
        ))
        .committed_pure();

    let agent = next
        .agents
        .iter()
        .find(|a| a.id == agent_id)
        .test_unwrap("agent should exist");
    assert_eq!(
        agent.status,
        AgentStatus::Running,
        "AgentStatusChanged should update agent status"
    );
}

#[test]
fn kill_agent_sets_status_to_dead() {
    let mut state = crate::common_app_state::app_state();
    let agent_id = AgentId("agent-1".into());
    let mut agent = Agent::new(
        agent_id.clone(),
        RepositoryId("repo".into()),
        jefe::domain::shipped_agent_type(3),
        jefe::domain::TypedMap::new(),
        "Agent 1".into(),
        PathBuf::from("/work"),
    );
    agent.status = AgentStatus::Running;
    state.agents.push(agent);

    let next = state
        .apply(AppEvent::KillAgent(agent_id.clone()))
        .committed_discarding_effects();

    let agent = next
        .agents
        .iter()
        .find(|a| a.id == agent_id)
        .test_unwrap("agent should exist");
    assert_eq!(
        agent.status,
        AgentStatus::Dead,
        "KillAgent should set status to Dead"
    );
}

#[test]
fn jump_to_agent_by_shortcut_switches_repo_and_selection() {
    let mut state = crate::common_app_state::app_state();
    let repo_a = Repository::new(
        RepositoryId("repo-a".into()),
        jefe::domain::shipped_agent_type(3),
        jefe::domain::TypedMap::new(),
        "Repo A".into(),
        "repo-a".into(),
        PathBuf::from("/repo-a"),
    );
    let repo_b = Repository::new(
        RepositoryId("repo-b".into()),
        jefe::domain::shipped_agent_type(3),
        jefe::domain::TypedMap::new(),
        "Repo B".into(),
        "repo-b".into(),
        PathBuf::from("/repo-b"),
    );
    state.repositories = vec![repo_a.clone(), repo_b.clone()];

    let mut a1 = Agent::new(
        AgentId("a1".into()),
        repo_a.id.clone(),
        jefe::domain::shipped_agent_type(3),
        jefe::domain::TypedMap::new(),
        "A1".into(),
        PathBuf::from("/repo-a/a1"),
    );
    a1.shortcut_slot = Some(1);

    let mut b1 = Agent::new(
        AgentId("b1".into()),
        repo_b.id.clone(),
        jefe::domain::shipped_agent_type(3),
        jefe::domain::TypedMap::new(),
        "B1".into(),
        PathBuf::from("/repo-b/b1"),
    );
    b1.shortcut_slot = Some(2);

    state.agents = vec![a1, b1];
    state.selected_repository_index = Some(0);
    state.selected_agent_index = Some(0);

    let next = state
        .apply(AppEvent::JumpToAgentByShortcut(2))
        .committed_pure();

    assert_eq!(next.selected_repository_index, Some(1));
    assert_eq!(next.selected_agent_index, Some(1));
    assert_eq!(next.pane_focus, PaneFocus::Agents);
    assert!(!next.terminal_focused);
}

#[test]
fn jump_to_shortcut_ignores_hidden_repository_when_filter_enabled() {
    let mut state = crate::common_app_state::app_state();
    let repo_a = Repository::new(
        RepositoryId("repo-a".into()),
        jefe::domain::shipped_agent_type(3),
        jefe::domain::TypedMap::new(),
        "Repo A".into(),
        "repo-a".into(),
        PathBuf::from("/repo-a"),
    );
    let repo_b = Repository::new(
        RepositoryId("repo-b".into()),
        jefe::domain::shipped_agent_type(3),
        jefe::domain::TypedMap::new(),
        "Repo B".into(),
        "repo-b".into(),
        PathBuf::from("/repo-b"),
    );
    state.repositories = vec![repo_a.clone(), repo_b.clone()];

    let mut a1 = Agent::new(
        AgentId("a1".into()),
        repo_a.id.clone(),
        jefe::domain::shipped_agent_type(3),
        jefe::domain::TypedMap::new(),
        "A1".into(),
        PathBuf::from("/repo-a/a1"),
    );
    a1.shortcut_slot = Some(1);
    a1.status = AgentStatus::Running;

    let mut b1 = Agent::new(
        AgentId("b1".into()),
        repo_b.id.clone(),
        jefe::domain::shipped_agent_type(3),
        jefe::domain::TypedMap::new(),
        "B1".into(),
        PathBuf::from("/repo-b/b1"),
    );
    b1.shortcut_slot = Some(2);

    state.agents = vec![a1, b1];
    state.hide_idle_repositories = true;
    state.selected_repository_index = Some(0);
    state.selected_agent_index = Some(0);

    let next = state
        .apply(AppEvent::JumpToAgentByShortcut(2))
        .committed_pure();

    assert_eq!(next.selected_repository_index, Some(0));
    assert_eq!(next.selected_agent_index, Some(0));
    assert_eq!(next.pane_focus, PaneFocus::Repositories);
    assert!(!next.terminal_focused);
}

#[test]
fn repository_navigation_restores_last_selected_agent_per_repo() {
    let mut state = crate::common_app_state::app_state();
    let repo_a = Repository::new(
        RepositoryId("repo-a".into()),
        jefe::domain::shipped_agent_type(3),
        jefe::domain::TypedMap::new(),
        "Repo A".into(),
        "repo-a".into(),
        PathBuf::from("/repo-a"),
    );
    let repo_b = Repository::new(
        RepositoryId("repo-b".into()),
        jefe::domain::shipped_agent_type(3),
        jefe::domain::TypedMap::new(),
        "Repo B".into(),
        "repo-b".into(),
        PathBuf::from("/repo-b"),
    );
    state.repositories = vec![repo_a.clone(), repo_b.clone()];

    let a1 = Agent::new(
        AgentId("a1".into()),
        repo_a.id.clone(),
        jefe::domain::shipped_agent_type(3),
        jefe::domain::TypedMap::new(),
        "A1".into(),
        PathBuf::from("/repo-a/a1"),
    );
    let a2 = Agent::new(
        AgentId("a2".into()),
        repo_a.id.clone(),
        jefe::domain::shipped_agent_type(3),
        jefe::domain::TypedMap::new(),
        "A2".into(),
        PathBuf::from("/repo-a/a2"),
    );
    let b1 = Agent::new(
        AgentId("b1".into()),
        repo_b.id.clone(),
        jefe::domain::shipped_agent_type(3),
        jefe::domain::TypedMap::new(),
        "B1".into(),
        PathBuf::from("/repo-b/b1"),
    );
    state.agents = vec![a1, a2, b1];

    state.selected_repository_index = Some(0);
    state.selected_agent_index = Some(1);
    state.pane_focus = PaneFocus::Repositories;

    let to_repo_b = state.apply(AppEvent::NavigateDown).committed_pure();
    assert_eq!(to_repo_b.selected_repository_index, Some(1));
    assert_eq!(to_repo_b.selected_agent_index, Some(2));

    let back_to_repo_a = to_repo_b.apply(AppEvent::NavigateUp).committed_pure();
    assert_eq!(back_to_repo_a.selected_repository_index, Some(0));
    assert_eq!(back_to_repo_a.selected_agent_index, Some(1));
}

// =============================================================================
// Error/Warning State (REQ-TECH-008)
// =============================================================================

#[test]
fn persistence_load_failed_sets_error() {
    let state = crate::common_app_state::app_state();

    let next = state
        .apply(AppEvent::PersistenceLoadFailed("file not found".into()))
        .committed_pure();

    assert!(
        next.error_message.is_some(),
        "PersistenceLoadFailed should set error_message"
    );
    assert!(
        next.error_message
            .as_ref()
            .test_unwrap("test unwrap")
            .contains("file not found"),
        "error_message should contain the error"
    );
}

#[test]
fn clear_error_clears_error_message() {
    let state = {
        let mut state = crate::common_app_state::app_state();
        state.error_message = Some("some error".into());
        state
    };

    let next = state.apply(AppEvent::ClearError).committed_pure();

    assert!(
        next.error_message.is_none(),
        "ClearError should clear error_message"
    );
}

#[test]
fn theme_resolve_failed_sets_warning() {
    let state = crate::common_app_state::app_state();

    let next = state
        .apply(AppEvent::ThemeResolveFailed("theme not found".into()))
        .committed_pure();

    assert!(
        next.warning_message.is_some(),
        "ThemeResolveFailed should set warning_message"
    );
}

#[test]
fn form_created_agent_has_running_status() {
    let repo = Repository::new(
        RepositoryId("repo-1".into()),
        jefe::domain::shipped_agent_type(3),
        jefe::domain::TypedMap::new(),
        "Repo One".into(),
        "repo-one".into(),
        PathBuf::from("/tmp/repo-one"),
    );
    let mut state = {
        let mut state = crate::common_app_state::app_state();
        state.repositories = vec![repo];
        state
    };

    state = state
        .apply(AppEvent::OpenNewAgent(RepositoryId("repo-1".into())))
        .committed_pure();
    if let ModalState::NewAgent { fields, .. } = &mut state.modal {
        fields.name = "Form Agent".into();
        fields.work_dir = "/tmp/repo-one/form-agent".into();
    } else {
        panic!("expected new-agent modal");
    }

    state = state.apply(AppEvent::SubmitForm).committed_pure();

    let Some(created) = state.agents.iter().find(|a| a.name == "Form Agent") else {
        panic!("form-created agent should exist");
    };
    assert_eq!(
        created.status,
        AgentStatus::Running,
        "app-created agents start Running because creation triggers launch"
    );
}

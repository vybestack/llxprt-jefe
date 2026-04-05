//! Agent visibility filter and display–selection consistency tests (issue #41).

#![allow(clippy::expect_used)]
#![allow(clippy::unwrap_used)]

use std::path::PathBuf;

use jefe::domain::{Agent, AgentId, AgentStatus, Repository, RepositoryId};
use jefe::state::{AppEvent, AppState, ModalState, PaneFocus};

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
    let local_idx = hidden.selected_agent_local_index().unwrap();
    let selected = hidden.selected_agent().unwrap();

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
                    AgentId("target".into()),
                    RepositoryId("r1".into()),
                    "Target Agent".into(),
                    PathBuf::from("/r1/target"),
                );
                running.status = AgentStatus::Running;
                running
            },
            {
                let mut running = Agent::new(
                    AgentId("other".into()),
                    RepositoryId("r1".into()),
                    "Other Agent".into(),
                    PathBuf::from("/r1/other"),
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

    let repo_id = RepositoryId("r1".into());
    let visible_agents = hidden.visible_agents_for_repository(&repo_id);
    let local_idx = hidden.selected_agent_local_index().unwrap();
    let selected = hidden.selected_agent().unwrap();

    assert_eq!(visible_agents[local_idx].id, selected.id);
    assert_eq!(selected.id, AgentId("target".into()));

    let delete_event = AppEvent::OpenDeleteAgent(selected.id.clone());
    let with_modal = hidden.apply(delete_event);
    match &with_modal.modal {
        ModalState::ConfirmDeleteAgent { id, .. } => {
            assert_eq!(
                *id,
                AgentId("target".into()),
                "delete must target the highlighted agent, not an adjacent one"
            );
        }
        other => panic!("expected ConfirmDeleteAgent, got {other:?}"),
    }
}

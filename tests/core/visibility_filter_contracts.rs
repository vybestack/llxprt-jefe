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

//! Workbench card-grid host-panel projection tests (issue #706 slice B).
//!
//! The cards screen's agent grid is retained by the cutover (maintainer
//! decision on #706): it migrates onto the shared host-panel runtime as a
//! list control over the workbench's own filtered order, exactly the order
//! the legacy grid renders. These tests pin the projection and the input
//! routing: one item per visible agent, bucket order, selection following
//! the app's selected agent, Enter attaching, and paging advancing the
//! workbench page.

use crate::domain::observation::{
    AgentObservation, FieldState, NativeActivityState, NativeActivityValue, ObservationHealth,
    Provenance, Wait, WaitReason,
};
use crate::domain::{Agent, AgentId, AgentStatus, AgentTypeId, Repository, RepositoryId, TypedMap};
use crate::host_controls::{ControlAction, ControlKind};
use crate::host_panel_models::project_host_panel;
use crate::runtime::provider::protocol::PanelBody;
use crate::state::AppState;
use crate::workbench::HostPanelModelSource;
use crate::workbench_view::StatusBucket;
use std::path::PathBuf;

fn repository(id: &str) -> Repository {
    Repository::new(
        RepositoryId(format!("repo-{id}")),
        AgentTypeId::default(),
        TypedMap::default(),
        format!("Repo {id}"),
        format!("repo-{id}"),
        PathBuf::from("/tmp"),
    )
}

fn agent(name: &str, repository_id: &str, status: AgentStatus) -> Agent {
    let mut agent = Agent::new(
        AgentId(name.to_owned()),
        RepositoryId(repository_id.to_owned()),
        AgentTypeId::default(),
        TypedMap::default(),
        name.to_owned(),
        PathBuf::from("/tmp"),
    );
    agent.status = status;
    agent
}

fn working_observation() -> AgentObservation {
    AgentObservation {
        health: ObservationHealth::Live,
        activity: FieldState::known(
            Provenance::Authoritative,
            NativeActivityValue {
                state: NativeActivityState::Acting,
            },
        ),
        ..AgentObservation::default()
    }
}

fn ready_observation() -> AgentObservation {
    AgentObservation {
        health: ObservationHealth::Live,
        activity: FieldState::known(
            Provenance::Authoritative,
            NativeActivityValue {
                state: NativeActivityState::Idle,
            },
        ),
        wait: FieldState::known(Provenance::Authoritative, None),
        turn: FieldState::known(Provenance::Authoritative, None),
        terminal: FieldState::known(Provenance::Authoritative, None),
        ..AgentObservation::default()
    }
}

fn waiting_observation() -> AgentObservation {
    AgentObservation {
        health: ObservationHealth::Live,
        wait: FieldState::known(
            Provenance::Authoritative,
            Some(Wait {
                reason: WaitReason::Permission,
            }),
        ),
        ..AgentObservation::default()
    }
}

/// Three agents spanning three buckets, the way the legacy grid sorted them.
fn state_with_agents_across_buckets() -> AppState {
    let mut state = AppState::new(crate::test_support::published_workbench());
    state.repositories = vec![repository("one")];
    let working = agent("working", "repo-one", AgentStatus::Running);
    let ready = agent("ready", "repo-one", AgentStatus::Running);
    let waiting = agent("waiting", "repo-one", AgentStatus::Running);
    state
        .observations
        .insert(working.id.clone(), working_observation());
    state
        .observations
        .insert(ready.id.clone(), ready_observation());
    state
        .observations
        .insert(waiting.id.clone(), waiting_observation());
    state.agents = vec![working, ready, waiting];
    state
}

fn card_items(
    model: &crate::host_panel_models::HostPanelModel,
) -> &Vec<crate::runtime::provider::protocol::ListItem> {
    let PanelBody::List(body) = &model.body else {
        panic!(
            "workbench cards must project a list body, got {:?}",
            model.body.kind()
        );
    };
    &body.items
}

/// The declared capability for the repositories screen's cards panel.
fn cards_capability() -> crate::workbench::HostPanelCapability {
    let registry = crate::workbench::screens::builtin_screens()
        .unwrap_or_else(|error| unreachable!("compiled screens are valid: {error}"));
    let descriptor = registry
        .get_identity(crate::workbench::ScreenIdentity::Compiled(
            crate::state::ScreenId::Repositories,
        ))
        .unwrap_or_else(|| panic!("repositories descriptor must be published"));
    let panel_id = crate::workbench::PanelId::parse("cards")
        .unwrap_or_else(|error| unreachable!("valid panel id: {error}"));
    descriptor
        .panel(&panel_id)
        .and_then(crate::workbench::descriptor::PanelDescriptor::host_capability)
        .unwrap_or_else(|| panic!("cards grid must be a declared host control"))
}

#[test]
fn workbench_cards_list_agents_in_bucket_order() {
    let state = state_with_agents_across_buckets();

    let model = project_host_panel(&state, HostPanelModelSource::WorkbenchCards);

    assert_eq!(model.title, "Workbench");
    let items = card_items(&model);
    assert_eq!(
        items
            .iter()
            .map(|item| item.label.as_str())
            .collect::<Vec<_>>(),
        ["waiting", "working", "ready"],
        "cards follow the projection order: needs-you, working, ready"
    );
    assert_eq!(
        items
            .iter()
            .map(|item| item.status.as_deref())
            .collect::<Vec<_>>(),
        [
            Some(StatusBucket::NeedsYou.label()),
            Some(StatusBucket::Working.label()),
            Some(StatusBucket::Ready.label()),
        ],
        "each card carries its bucket label"
    );
    assert!(
        items.iter().all(|item| item.description.is_none()),
        "one row per card: no description second rows"
    );
    assert!(
        body_of(&model).next_page_token.is_some(),
        "paging stays reachable while any agent is visible"
    );
}

#[test]
fn workbench_cards_respect_the_status_mask() {
    let mut state = state_with_agents_across_buckets();
    let mask = state
        .workbench
        .status_filter
        .mask()
        .with(StatusBucket::Working, false);
    state.workbench.status_filter = crate::state::WorkbenchStatusFilter(mask);

    let model = project_host_panel(&state, HostPanelModelSource::WorkbenchCards);

    let items = card_items(&model);
    assert_eq!(
        items
            .iter()
            .map(|item| item.label.as_str())
            .collect::<Vec<_>>(),
        ["waiting", "ready"],
        "a toggled-off bucket's agents leave the grid"
    );
}

#[test]
fn workbench_cards_selection_follows_the_selected_agent() {
    let mut state = state_with_agents_across_buckets();
    state.selected_repository_index = Some(0);
    state.selected_agent_index = Some(0);

    let model = project_host_panel(&state, HostPanelModelSource::WorkbenchCards);

    // Agent 0 is "working", which the order places second.
    let expected =
        crate::domain::Id::internal_indexed(crate::domain::InternalId::WorkbenchCardItem, 1);
    assert_eq!(model.selected_id, Some(expected));
}

#[test]
fn workbench_cards_empty_when_no_agents_pass_the_filter() {
    let mut state = state_with_agents_across_buckets();
    state.agents.clear();

    let model = project_host_panel(&state, HostPanelModelSource::WorkbenchCards);

    assert!(card_items(&model).is_empty());
    assert_eq!(model.selected_id, None);
    assert!(
        body_of(&model).next_page_token.is_none(),
        "nothing to page through"
    );
}

#[test]
fn workbench_cards_selection_moves_through_the_grid_order() {
    let mut state = state_with_agents_across_buckets();
    let capability = cards_capability();
    assert_eq!(capability.control_kind(), ControlKind::List);
    assert_eq!(
        state.selected_agent().map(|agent| agent.name.as_str()),
        None,
        "fixture starts with no selection"
    );

    // No selection yet: the shared list contract implicitly selects the first
    // card, so Next moves to the second.
    assert!(state.apply_host_panel_action(capability, ControlAction::Next, 40));
    assert_eq!(
        state.selected_agent().map(|agent| agent.name.as_str()),
        Some("working")
    );

    assert!(state.apply_host_panel_action(capability, ControlAction::Next, 40));
    assert_eq!(
        state.selected_agent().map(|agent| agent.name.as_str()),
        Some("ready")
    );

    // Host list semantics wrap at the ends; the legacy grid clamped. The
    // wrap is the shared control contract, so it is pinned here deliberately.
    assert!(state.apply_host_panel_action(capability, ControlAction::Next, 40));
    assert_eq!(
        state.selected_agent().map(|agent| agent.name.as_str()),
        Some("waiting")
    );

    assert!(state.apply_host_panel_action(capability, ControlAction::Previous, 40));
    assert_eq!(
        state.selected_agent().map(|agent| agent.name.as_str()),
        Some("ready")
    );
}

#[test]
fn workbench_cards_activate_attaches_to_the_selected_agent() {
    let mut state = state_with_agents_across_buckets();
    let capability = cards_capability();
    state.apply_host_panel_action(capability, ControlAction::Next, 40);
    state.split_grab_index = Some(3);

    assert!(state.apply_host_panel_action(capability, ControlAction::Activate, 40));

    assert_eq!(
        state.pane_focus,
        crate::state::PaneFocus::Terminal,
        "attach lands on the terminal pane"
    );
    assert!(state.terminal_focused);
    assert_eq!(
        state.split_grab_index, None,
        "attach drops any split grab state"
    );
    assert!(
        state.compiled_screen().is_none(),
        "attach leaves the workbench for the dashboard"
    );
}

#[test]
fn workbench_cards_page_next_advances_the_workbench_page() {
    let mut state = state_with_agents_across_buckets();
    let capability = cards_capability();
    assert_eq!(state.workbench.page, 0);

    assert!(state.apply_host_panel_action(capability, ControlAction::PageNext, 40));
    assert_eq!(state.workbench.page, 1);
}

fn body_of(
    model: &crate::host_panel_models::HostPanelModel,
) -> &crate::runtime::provider::protocol::ListBody {
    let PanelBody::List(body) = &model.body else {
        unreachable!("cards model is a list body");
    };
    body
}

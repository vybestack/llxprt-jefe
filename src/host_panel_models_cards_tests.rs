//! Workbench card-grid host-panel projection tests (issue #706 slice B).
//!
//! The cards screen's agent grid is retained by the cutover (maintainer
//! decision on #706): it migrates onto the shared host-panel runtime as a
//! list control over the workbench's own filtered order, exactly the order
//! the legacy grid renders. These tests pin the projection and the input
//! routing: one item per visible agent, bucket order, selection following
//! the app's selected agent, Enter attaching, and paging advancing the
//! workbench page.

use crate::domain::AgentStatus;
use crate::host_controls::{ControlAction, ControlKind};
use crate::host_panel_models::project_host_panel;
use crate::runtime::provider::protocol::PanelBody;
use crate::state::AppState;
use crate::test_support::{
    host_panel_agent, host_panel_repository, ready_observation, waiting_observation,
    working_observation,
};
use crate::workbench::HostPanelModelSource;
use crate::workbench_view::StatusBucket;

/// Three agents spanning three buckets, the way the legacy grid sorted them.
fn state_with_agents_across_buckets() -> AppState {
    let mut state = AppState::new(crate::test_support::published_workbench());
    state.repositories = vec![host_panel_repository("one")];
    let working = host_panel_agent("working", "repo-one", AgentStatus::Running);
    let ready = host_panel_agent("ready", "repo-one", AgentStatus::Running);
    let waiting = host_panel_agent("waiting", "repo-one", AgentStatus::Running);
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
        .get_identity(crate::workbench::REPOSITORIES_IDENTITY)
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
    assert!(state.apply_host_panel_action(capability, ControlAction::Next, 80, 40));
    assert_eq!(
        state.selected_agent().map(|agent| agent.name.as_str()),
        Some("working")
    );

    assert!(state.apply_host_panel_action(capability, ControlAction::Next, 80, 40));
    assert_eq!(
        state.selected_agent().map(|agent| agent.name.as_str()),
        Some("ready")
    );

    // Host list semantics wrap at the ends; the legacy grid clamped. The
    // wrap is the shared control contract, so it is pinned here deliberately.
    assert!(state.apply_host_panel_action(capability, ControlAction::Next, 80, 40));
    assert_eq!(
        state.selected_agent().map(|agent| agent.name.as_str()),
        Some("waiting")
    );

    assert!(state.apply_host_panel_action(capability, ControlAction::Previous, 80, 40));
    assert_eq!(
        state.selected_agent().map(|agent| agent.name.as_str()),
        Some("ready")
    );
}

#[test]
fn workbench_cards_activate_attaches_to_the_selected_agent() {
    let mut state = state_with_agents_across_buckets();
    let capability = cards_capability();
    state.apply_host_panel_action(capability, ControlAction::Next, 80, 40);
    state.split_grab_index = Some(3);

    assert!(state.apply_host_panel_action(capability, ControlAction::Activate, 80, 40));

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
    // Commit a frame whose effective render size is (80, 12): in windowed
    // mode `effective_render_size` subtracts 2 per axis, so resolve at
    // (82, 14). The display-basis page count then matches the geometry
    // the test exercises (issue #706).
    state.nav = crate::state::navigation::NavState::rooted_definition(
        crate::workbench::REPOSITORIES_IDENTITY,
        crate::workbench::RouteId::from_static("repositories"),
        crate::workbench::PanelId::from_static("repositories"),
    );
    state.resolved_layout = crate::screen_layout::resolve_screen(&state, 82, 14);
    assert!(
        state.resolved_layout.is_some(),
        "fixture must resolve a layout at 82x14"
    );
    let capability = cards_capability();
    assert_eq!(state.workbench.page, 0);

    // Three agents at 80x12: one column, one row per page → three pages.
    assert!(state.apply_host_panel_action(capability, ControlAction::PageNext, 80, 12));
    assert_eq!(state.workbench.page, 1);
    assert!(state.apply_host_panel_action(capability, ControlAction::PageNext, 80, 12));
    assert_eq!(state.workbench.page, 2, "the last page is reachable");
    assert!(state.apply_host_panel_action(capability, ControlAction::PageNext, 80, 12));
    assert_eq!(
        state.workbench.page, 2,
        "paging never advances past the real page count"
    );
}

#[test]
fn workbench_cards_page_next_clamps_on_a_single_page_grid() {
    // Issue #706: the retained page counter must never advance past the
    // last page the grid can show, or PreviousPage looks unresponsive until
    // the display clamp saturates back.
    let mut state = state_with_agents_across_buckets();
    state.nav = crate::state::navigation::NavState::rooted_definition(
        crate::workbench::REPOSITORIES_IDENTITY,
        crate::workbench::RouteId::from_static("repositories"),
        crate::workbench::PanelId::from_static("repositories"),
    );
    state.resolved_layout = crate::screen_layout::resolve_screen(&state, 82, 42);
    assert!(
        state.resolved_layout.is_some(),
        "fixture must resolve a layout at 82x42"
    );
    let capability = cards_capability();
    assert_eq!(state.workbench.page, 0);

    for _ in 0..3 {
        assert!(state.apply_host_panel_action(capability, ControlAction::PageNext, 80, 40));
    }
    assert_eq!(
        state.workbench.page, 0,
        "a single-page grid never advances the page"
    );
}

#[test]
fn workbench_cards_select_routes_the_card_id_to_the_agent() {
    // Issue #706: the advertised input contract routes a card item id
    // through the host-panel path to the workbench selection.
    let mut state = state_with_agents_across_buckets();
    let capability = cards_capability();
    let inputs = state
        .agents
        .iter()
        .map(|agent| crate::workbench_view::AgentInput {
            agent,
            git_info: None,
            observation: state.observations.get(&agent.id),
        })
        .collect::<Vec<_>>();
    let ordered = crate::workbench_view::ordered_agent_ids(
        &inputs,
        state.workbench.status_filter.mask(),
        state
            .split_filter
            .as_ref()
            .map(|repository| repository.0.as_str()),
    );
    let target_index = 1;
    let Some(target_card) = ordered.get(target_index) else {
        panic!("fixture must order a card at {target_index}");
    };
    let expected = target_card.0.clone();

    let id = crate::domain::Id::internal_indexed(
        crate::domain::InternalId::WorkbenchCardItem,
        target_index,
    );
    assert!(state.apply_host_panel_action(capability, ControlAction::Select(id), 80, 40));
    assert_eq!(
        state.selected_agent().map(|agent| agent.id.0.as_str()),
        Some(expected.as_str()),
        "selecting the card id selects that agent"
    );
}

fn body_of(
    model: &crate::host_panel_models::HostPanelModel,
) -> &crate::runtime::provider::protocol::ListBody {
    let PanelBody::List(body) = &model.body else {
        unreachable!("cards model is a list body");
    };
    body
}

//! Workbench STATUS-block host-panel projection tests (issue #706).
//!
//! The cards screen's STATUS block is retained by the cutover (maintainer
//! decision on #706): it migrates onto the shared host-panel runtime as a
//! list control, exactly like the legacy keymap treated it ("the status rail
//! is the navigable list"). These tests pin the projection against the
//! legacy left-rail behavior: four buckets in filter order, checkbox from
//! the active mask, live counts computed before filtering, one row per
//! bucket.

use crate::domain::AgentStatus;
use crate::domain::observation::{AgentObservation, ObservationHealth};
use crate::host_controls::ControlAction;
use crate::host_panel_models::project_host_panel;
use crate::runtime::provider::protocol::{ListItem, PanelBody};
use crate::state::AppState;
use crate::test_support::{
    host_panel_agent, host_panel_repository, ready_observation, waiting_observation,
    working_observation,
};
use crate::workbench::HostPanelModelSource;
use crate::workbench_view::StatusBucket;

fn stale_observation() -> AgentObservation {
    AgentObservation {
        health: ObservationHealth::Stale,
        ..AgentObservation::default()
    }
}

/// One agent per bucket, the way the legacy STATUS rail counted them.
fn state_with_one_agent_per_bucket() -> AppState {
    let mut state = AppState::new(crate::test_support::published_workbench());
    state.repositories = vec![host_panel_repository("one")];
    let waiting = host_panel_agent("waiting", "repo-one", AgentStatus::Running);
    let working = host_panel_agent("working", "repo-one", AgentStatus::Running);
    let ready = host_panel_agent("ready", "repo-one", AgentStatus::Running);
    let dead = host_panel_agent("stale", "repo-one", AgentStatus::Dead);
    state
        .observations
        .insert(waiting.id.clone(), waiting_observation());
    state
        .observations
        .insert(working.id.clone(), working_observation());
    state
        .observations
        .insert(ready.id.clone(), ready_observation());
    state
        .observations
        .insert(dead.id.clone(), stale_observation());
    state.agents = vec![waiting, working, ready, dead];
    state
}

/// The content width the resolver hands the STATUS pane on the shipped split:
/// a 22-column rail less `LIST_PANE_CHROME`'s two side borders
/// (`src/workbench/screens.rs`). Rows are asserted at that width, not at a
/// comfortable one, because that is where #752's folded count was lost (#745).
const STATUS_PANE_WIDTH: usize = 20;

/// The rows the shared list control paints for this model at `width`.
fn rendered_rows(model: &crate::host_panel_models::HostPanelModel, width: usize) -> Vec<String> {
    crate::host_controls::project_control_body(
        &model.body,
        &model.action_affordances,
        model.selected_id.as_ref(),
        None,
        width,
    )
    .into_iter()
    .map(|row| row.text)
    .collect()
}

fn status_items(model: &crate::host_panel_models::HostPanelModel) -> &Vec<ListItem> {
    let PanelBody::List(body) = &model.body else {
        panic!(
            "workbench status must project a list body, got {:?}",
            model.body.kind()
        );
    };
    &body.items
}

#[test]
fn status_block_lists_four_buckets_in_filter_order_with_live_counts() {
    let state = state_with_one_agent_per_bucket();

    let model = project_host_panel(&state, HostPanelModelSource::WorkbenchStatus);

    assert_eq!(model.title, "STATUS");
    let items = status_items(&model);
    assert_eq!(
        items
            .iter()
            .map(|item| (item.label.as_str(), item.count))
            .collect::<Vec<_>>(),
        [
            ("[x] Needs you", Some(1)),
            ("[x] Working", Some(1)),
            ("[x] Ready", Some(1)),
            ("[x] Stale", Some(1))
        ],
        "default mask is all-on, buckets in filter order, counts typed"
    );
    assert_eq!(
        rendered_rows(&model, STATUS_PANE_WIDTH),
        [
            ">> [x] Needs you (1)",
            "   [x] Working (1)",
            "   [x] Ready (1)",
            "   [x] Stale (1)"
        ],
        "and the rows the pane paints are the corpus form"
    );
    assert!(
        items.iter().all(|item| item.status.is_none()),
        "a count is not a status word, so the shared `[value]` suffix stays clear: {items:?}"
    );
    assert!(
        items.iter().all(|item| item.description.is_none()),
        "one row per bucket: no description second rows"
    );
}

#[test]
fn status_block_checkbox_reflects_the_mask_while_counts_stay_prefilter() {
    let mut state = state_with_one_agent_per_bucket();
    let mask = state
        .workbench
        .status_filter
        .mask()
        .with(StatusBucket::Working, false);
    state.workbench.status_filter = crate::state::WorkbenchStatusFilter(mask);

    let model = project_host_panel(&state, HostPanelModelSource::WorkbenchStatus);

    let items = status_items(&model);
    assert_eq!(
        (items[1].label.as_str(), items[1].count),
        ("[ ] Working", Some(1)),
        "a toggled-off bucket is unchecked and counts ignore the active \
         filter, like the legacy rail"
    );
    assert_eq!(
        (items[0].label.as_str(), items[0].count),
        ("[x] Needs you", Some(1))
    );
    assert_eq!(
        rendered_rows(&model, STATUS_PANE_WIDTH).get(1).cloned(),
        Some("   [ ] Working (1)".to_owned()),
        "and the unchecked row still paints its count"
    );
}

#[test]
fn status_block_selection_follows_the_filter_cursor() {
    let mut state = state_with_one_agent_per_bucket();
    state.workbench.filter_cursor = 2;

    let model = project_host_panel(&state, HostPanelModelSource::WorkbenchStatus);

    let expected =
        crate::domain::Id::internal_indexed(crate::domain::InternalId::StatusBucketItem, 2);
    assert_eq!(model.selected_id, Some(expected));
}

/// The declared capability of the repositories screen's status panel.
fn status_block_capability() -> crate::workbench::HostPanelCapability {
    let registry = crate::workbench::screens::builtin_screens()
        .unwrap_or_else(|error| unreachable!("compiled screens are valid: {error}"));
    let descriptor = registry
        .get_identity(crate::workbench::REPOSITORIES_IDENTITY)
        .unwrap_or_else(|| panic!("repositories descriptor must be published"));
    let panel_id = crate::workbench::PanelId::parse("status")
        .unwrap_or_else(|error| unreachable!("valid panel id: {error}"));
    descriptor
        .panel(&panel_id)
        .and_then(crate::workbench::descriptor::PanelDescriptor::host_capability)
        .unwrap_or_else(|| panic!("status block must be a declared host control"))
}

#[test]
fn status_block_cursor_moves_through_the_host_panel_input_path() {
    let mut state = state_with_one_agent_per_bucket();
    state.workbench.filter_cursor = 0;
    state.workbench.page = 2;

    let capability = status_block_capability();
    assert!(state.apply_host_panel_action(capability, ControlAction::Next, 80, 5));
    assert_eq!(state.workbench.filter_cursor, 1);
    assert_eq!(state.workbench.page, 2, "cursor moves never reset the page");
    assert!(state.apply_host_panel_action(capability, ControlAction::Previous, 80, 5));
    assert_eq!(state.workbench.filter_cursor, 0);
    // The shared list contract cycles, where the legacy rail clamped at the
    // ends: through the host runtime the block behaves like every other
    // list control.
    assert!(state.apply_host_panel_action(capability, ControlAction::Previous, 80, 5));
    assert_eq!(state.workbench.filter_cursor, 3);
}

#[test]
fn status_block_activation_toggles_the_bucket_under_the_cursor() {
    let mut state = state_with_one_agent_per_bucket();
    state.workbench.filter_cursor = 1; // Working
    state.workbench.page = 3;

    let capability = status_block_capability();
    assert!(state.apply_host_panel_action(capability, ControlAction::Activate, 80, 5));

    let mask = state.workbench.status_filter.mask();
    assert!(
        !mask.allows(StatusBucket::Working),
        "activating the cursor row toggles its bucket off"
    );
    assert!(
        mask.allows(StatusBucket::NeedsYou) && mask.allows(StatusBucket::Ready),
        "other buckets are untouched"
    );
    assert_eq!(
        state.workbench.page, 0,
        "a toggle resets the page so a shrinking card list cannot strand it"
    );
    let model = project_host_panel(&state, HostPanelModelSource::WorkbenchStatus);
    let items = status_items(&model);
    assert_eq!(
        (items[1].label.as_str(), items[1].count),
        ("[ ] Working", Some(1)),
        "the count stays live for a toggled-off bucket"
    );
}

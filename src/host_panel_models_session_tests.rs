//! Terminal-manager session-list host-panel projection tests (issue #706).
//!
//! The shell list migrated onto the shared host-panel runtime as a list
//! control. These tests pin its selection projection, especially the clamp
//! that keeps a stale selected index pointing at a row that still exists.

use crate::domain::{AgentId, Id, InternalId};
use crate::host_panel_models::project_host_panel;
use crate::runtime::provider::protocol::PanelBody;
use crate::state::AppState;
use crate::workbench::HostPanelModelSource;

fn session_body(
    model: &crate::host_panel_models::HostPanelModel,
) -> &crate::runtime::provider::protocol::ListBody {
    let PanelBody::List(body) = &model.body else {
        panic!("the session model is a list body");
    };
    body
}

#[test]
fn session_list_clamps_a_stale_selected_index_to_the_last_row() {
    let mut state = AppState::new(crate::test_support::published_workbench());
    state.shell_inventory.record(AgentId("alpha".to_owned()));
    state.shell_inventory.record(AgentId("beta".to_owned()));
    state.terminal_manager.selected_index = Some(9);

    let model = project_host_panel(&state, HostPanelModelSource::SessionList);
    let body = session_body(&model);

    assert_eq!(body.items.len(), 2);
    assert_eq!(
        body.selected_id,
        Some(Id::internal_indexed(InternalId::SessionItem, 1)),
        "a stale index beyond the row count clamps to the last row"
    );
}

#[test]
fn session_list_carries_no_selection_when_the_row_list_empties() {
    let mut state = AppState::new(crate::test_support::published_workbench());
    state.terminal_manager.selected_index = Some(0);

    let model = project_host_panel(&state, HostPanelModelSource::SessionList);
    let body = session_body(&model);

    assert!(body.items.is_empty());
    assert_eq!(
        body.selected_id, None,
        "an empty row list cannot carry a selection"
    );
}

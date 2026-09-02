//! Dashboard chrome host-panel projection tests (issue #723).
//!
//! The dashboard preview and sidebar rows are host-owned models, so their
//! content contracts are proven here: the preview must carry the retained
//! preview_view field set, and an agent restored without a name value must
//! fall back to its id instead of rendering a blank sidebar row.

use crate::domain::AgentStatus;
use crate::host_panel_models::project_host_panel;
use crate::runtime::provider::protocol::PanelBody;
use crate::state::AppState;
use crate::test_support::{host_panel_agent, host_panel_repository};
use crate::workbench::HostPanelModelSource;

fn state_with_selected_agent(agent: crate::domain::Agent) -> AppState {
    let mut state = AppState::new(crate::test_support::published_workbench());
    let repository = host_panel_repository("alpha");
    state.repositories = vec![repository];
    state.agents = vec![agent];
    state.selected_repository_index = Some(0);
    state.selected_agent_index = Some(0);
    state
}

/// Issue #723 fix 3: the dashboard agent-preview panel renders the retained
/// preview_view field set — Name, Status, Repo, Branch, Dir — instead of the
/// two-line Status/Work directory stub #715 left behind.
#[test]
fn dashboard_preview_projects_the_full_preview_view_field_set() {
    let agent = host_panel_agent("zed", "repo-alpha", AgentStatus::Dead);
    let state = state_with_selected_agent(agent);

    let model = project_host_panel(&state, HostPanelModelSource::AgentPreview);
    let PanelBody::Detail(body) = &model.body else {
        panic!(
            "agent preview must project a detail body, got {:?}",
            model.body.kind()
        );
    };
    let labels: Vec<&str> = body.metadata.iter().map(|row| row.label.as_str()).collect();
    for expected in ["Name", "Status", "Repo", "Branch", "Dir"] {
        assert!(
            labels.contains(&expected),
            "preview must carry the `{expected}:` field, got {labels:?}"
        );
    }
}

/// Issue #723 fix 5: a restored schema-2 agent with empty values yields no
/// display name; the sidebar row must fall back to the agent id so the row
/// is never blank.
#[test]
fn dashboard_sidebar_falls_back_to_the_agent_id_when_values_have_no_name() {
    // A schema-2 restore rebuilds the agent with an id and no `name` value,
    // so the fixture pins exactly that shape: named id, empty display name.
    let mut agent = host_panel_agent("agent-x", "repo-alpha", AgentStatus::Dead);
    agent.name = String::new();
    let state = state_with_selected_agent(agent);

    let model = project_host_panel(&state, HostPanelModelSource::AgentList);
    let PanelBody::List(body) = &model.body else {
        panic!(
            "agent sidebar must project a list body, got {:?}",
            model.body.kind()
        );
    };
    assert_eq!(
        body.items.first().map(|item| item.label.as_str()),
        Some("agent-x"),
        "a blank display name must fall back to the agent id"
    );
}

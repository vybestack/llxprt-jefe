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
    for expected in ["Name", "Status", "Repo", "Branch", "Dir"] {
        assert!(
            body.metadata.iter().any(|row| row.label == expected),
            "preview must carry the `{expected}:` field, got {:?}",
            body.metadata
                .iter()
                .map(|row| row.label.as_str())
                .collect::<Vec<_>>()
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

/// Issue #723 OCR fix: the preview metadata comes from preview_view's
/// structured rows, and the fixed pane-width budget applies to the value
/// after the label/value split. A truncated value can therefore never eat a
/// delimiter or silently drop a row, and an over-width Dir still gets the
/// full 30-cell budget instead of the 25 the `Dir: ` prefix used to spend.
#[test]
fn dashboard_preview_metadata_budgets_values_after_the_label_split() {
    let mut agent = host_panel_agent("zed", "repo-alpha", AgentStatus::Dead);
    agent.work_dir =
        std::path::PathBuf::from("/tmp/jefe/workdirs/repo-alpha-very-long-checkout-path");
    let state = state_with_selected_agent(agent);

    let model = project_host_panel(&state, HostPanelModelSource::AgentPreview);
    let PanelBody::Detail(body) = &model.body else {
        panic!(
            "agent preview must project a detail body, got {:?}",
            model.body.kind()
        );
    };
    let labels: Vec<&str> = body.metadata.iter().map(|row| row.label.as_str()).collect();
    assert_eq!(labels, ["Name", "Status", "Repo", "Branch", "Dir"]);
    let dir = &body.metadata[4];
    assert_eq!(dir.label, "Dir");
    assert!(
        dir.value.starts_with("/tmp/jefe/workdirs"),
        "the Dir value keeps its leading path cells: {}",
        dir.value
    );
    assert_eq!(
        dir.value.chars().count(),
        30,
        "the width budget applies to the value alone, not the rendered row"
    );
    assert!(dir.value.ends_with('…'));
}

/// Issue #723 OCR fix: the structured accessor carries exactly the accepted
/// five-field set. The live "Turn elapsed" row stays a pane render concern:
/// it needs a clock, so it is not dashboard metadata.
#[test]
fn dashboard_preview_metadata_stays_five_rows_while_a_turn_is_active() {
    let agent = host_panel_agent("zed", "repo-alpha", AgentStatus::Running);
    let mut state = state_with_selected_agent(agent.clone());
    let mut observation = crate::test_support::working_observation();
    observation.turn = crate::domain::observation::FieldState::known(
        crate::domain::observation::Provenance::Authoritative,
        Some(crate::domain::observation::CurrentTurn { elapsed_ms: 5000 }),
    );
    state.observations.insert(agent.id.clone(), observation);

    let model = project_host_panel(&state, HostPanelModelSource::AgentPreview);
    let PanelBody::Detail(body) = &model.body else {
        panic!(
            "agent preview must project a detail body, got {:?}",
            model.body.kind()
        );
    };
    let labels: Vec<&str> = body.metadata.iter().map(|row| row.label.as_str()).collect();
    assert_eq!(
        labels,
        ["Name", "Status", "Repo", "Branch", "Dir"],
        "an active turn must not leak a sixth metadata row"
    );
}

//! Agent Types availability host-panel projection tests (issue #734).
//!
//! The pane is restored as a declared host control, so its rows come from the
//! same pure `agent_status_view` projection the retired renderer consumed. One
//! list item per observation, in observation order, carrying the status text
//! the scenario corpus boots on and the create-gating the pre-cutover pane
//! showed on the right of every row.

use crate::agent_status_view::AgentAvailabilityObservation;
use crate::domain::Id;
use crate::domain::InternalId;
use crate::domain::agent_definition::{AgentDefinition, Availability, ProbeErrorCode};
use crate::host_panel_models::project_host_panel;
use crate::runtime::provider::protocol::{ListBody, PanelBody};
use crate::state::AppState;
use crate::workbench::HostPanelModelSource;

fn definition(display_name: &str) -> AgentDefinition {
    AgentDefinition::shipped()
        .into_iter()
        .find(|candidate| candidate.display_name == display_name)
        .unwrap_or_else(|| panic!("shipped definition {display_name} must exist"))
}

fn observations() -> Vec<AgentAvailabilityObservation> {
    vec![
        AgentAvailabilityObservation::not_found(&definition("Claude Code"), true, 1),
        AgentAvailabilityObservation::new(
            &definition("Code Puppy"),
            true,
            Availability::InstalledCompatible {
                identity: "code-puppy 9.9.9".to_owned(),
                generation: 1,
            },
        ),
        AgentAvailabilityObservation::new(
            &definition("Codex CLI"),
            false,
            Availability::InstalledIncompatible {
                reason: "missing the resume capability".to_owned(),
                generation: 1,
            },
        ),
        AgentAvailabilityObservation::new(
            &definition("LLxprt"),
            true,
            Availability::ProbeError {
                code: ProbeErrorCode::Agte202,
                reason: "probe timed out".to_owned(),
                generation: 1,
            },
        ),
    ]
}

fn state_with(observations: Vec<AgentAvailabilityObservation>) -> AppState {
    let mut state = AppState::test_fixture();
    state.agent_type_availability = observations;
    state
}

fn availability_body(state: &AppState) -> ListBody {
    let model = project_host_panel(state, HostPanelModelSource::AgentTypeAvailability);
    assert_eq!(
        model.title, "Agent Types",
        "the pane's title is the literal the required scenario asserts"
    );
    let PanelBody::List(body) = model.body else {
        panic!("the availability pane must project a list body");
    };
    body
}

#[test]
fn every_observation_projects_one_row_in_probe_order() {
    let state = state_with(observations());

    let body = availability_body(&state);

    let labels: Vec<&str> = body.items.iter().map(|item| item.label.as_str()).collect();
    assert_eq!(
        labels,
        vec![
            "Claude Code  Not found, enabled",
            "Code Puppy  Installed, enabled",
            "Codex CLI  Incompatible, disabled",
            "LLxprt  Probe error, enabled",
        ],
        "one row per observation, in the order the probe published them"
    );
}

#[test]
fn each_row_carries_its_create_gating_and_its_reason() {
    let state = state_with(observations());

    let body = availability_body(&state);

    let gating: Vec<Option<&str>> = body
        .items
        .iter()
        .map(|item| item.status.as_deref())
        .collect();
    assert_eq!(
        gating,
        vec![
            Some("Create disabled"),
            Some("Create enabled"),
            Some("Create disabled"),
            Some("Create disabled"),
        ],
        "only an enabled, installed-compatible definition can be created from"
    );

    let reasons: Vec<Option<&str>> = body
        .items
        .iter()
        .map(|item| item.description.as_deref())
        .collect();
    // The authored-against release is the shipped definition's own value, so
    // this pins the composition rather than today's version literal.
    let authored_against = definition("Code Puppy").minimum_version;
    assert_eq!(
        reasons,
        vec![
            Some("no executable candidate resolved"),
            Some(
                format!("identity: code-puppy 9.9.9 (authored against {authored_against})")
                    .as_str()
            ),
            Some("missing the resume capability"),
            Some("AGT-E202  probe timed out"),
        ],
        "the reason line keeps its diagnostic code, exactly as the pane showed it"
    );
}

#[test]
fn a_pending_probe_reads_as_checking_with_no_reason() {
    let state = state_with(vec![AgentAvailabilityObservation::pending(
        &definition("LLxprt"),
        true,
        7,
        crate::agent_candidate::CandidateResolution::NotFound(Vec::new()),
    )]);

    let body = availability_body(&state);

    assert_eq!(body.items.len(), 1);
    assert_eq!(body.items[0].label, "LLxprt  Checking, enabled");
    assert_eq!(body.items[0].description, None);
}

#[test]
fn the_selected_definition_index_selects_its_row() {
    let mut state = state_with(observations());
    state.selected_agent_type_index = 2;

    let body = availability_body(&state);

    assert_eq!(
        body.selected_id,
        Some(Id::internal_indexed(InternalId::AgentTypeItem, 2)),
        "the pane's cursor is the state-owned definition index"
    );
}

#[test]
fn a_stale_selection_index_clamps_to_the_last_row() {
    // The probe can republish a shorter snapshot while the cursor sits past
    // its end; a selection that addresses nothing would render no marker at
    // all.
    let mut state = state_with(observations());
    state.selected_agent_type_index = 99;

    let body = availability_body(&state);

    assert_eq!(
        body.selected_id,
        Some(Id::internal_indexed(InternalId::AgentTypeItem, 3))
    );
}

#[test]
fn an_unpublished_probe_projects_an_empty_pane_with_no_selection() {
    let state = state_with(Vec::new());

    let body = availability_body(&state);

    assert!(body.items.is_empty());
    assert_eq!(body.selected_id, None);
    assert_eq!(body.next_page_token, None);
}

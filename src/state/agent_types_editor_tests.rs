//! Behavioral tests for the Agent Types editor projection (issue #388).
//!
//! @requirement CW08-01

use crate::domain::action_registry::Provenance;
use crate::domain::agent_definition::{AgentDefinition, Availability, ProbeErrorCode};
use crate::persistence::settings_document::PublishedSettings;

use super::{AgentAvailability, AgentEditorRow, project_agent_types};
use crate::agent_status_view::AgentAvailabilityObservation;

fn shipped() -> Vec<AgentDefinition> {
    AgentDefinition::shipped()
}

fn definition(id: &str) -> AgentDefinition {
    shipped()
        .into_iter()
        .find(|definition| definition.id.as_str() == id)
        .unwrap_or_else(|| panic!("shipped definition {id}"))
}

fn observation(
    id: &str,
    enabled: bool,
    availability: Availability,
) -> AgentAvailabilityObservation {
    AgentAvailabilityObservation::new(&definition(id), enabled, availability)
}

fn published(source: &str) -> PublishedSettings {
    let catalog = crate::config_owners::builtin_owner_catalog()
        .unwrap_or_else(|error| panic!("owner catalog fixture: {error}"));
    crate::persistence::migration::migrate_settings(source.as_bytes(), &catalog)
        .unwrap_or_else(|diagnostics| panic!("settings fixture: {diagnostics:?}"))
        .published()
        .clone()
}

fn row<'rows>(rows: &'rows [AgentEditorRow], id: &str) -> &'rows AgentEditorRow {
    rows.iter()
        .find(|row| row.type_id.as_str() == id)
        .unwrap_or_else(|| panic!("row for {id}"))
}

#[test]
fn every_known_agent_type_projects_exactly_one_row_in_registry_order() {
    let observations: Vec<_> = shipped()
        .iter()
        .map(|definition| {
            AgentAvailabilityObservation::new(definition, true, Availability::NotFound)
        })
        .collect();

    let rows = project_agent_types(&observations, &PublishedSettings::default());

    assert_eq!(rows.len(), observations.len());
    let projected: Vec<_> = rows.iter().map(|row| row.type_id.as_str()).collect();
    let expected: Vec<_> = observations
        .iter()
        .map(|observation| observation.type_id().as_str())
        .collect();
    assert_eq!(projected, expected, "rows keep the registry's own order");
}

#[test]
fn each_upstream_availability_projects_its_own_status_with_the_probes_exact_reason() {
    let observations = vec![
        observation(
            "core.llxprt",
            true,
            Availability::InstalledCompatible {
                identity: "llxprt".to_owned(),
                generation: 1,
            },
        ),
        observation(
            "core.claude-code",
            true,
            Availability::InstalledIncompatible {
                reason: "missing capability: prompt".to_owned(),
                generation: 2,
            },
        ),
        observation("core.codex", true, Availability::NotFound),
        observation(
            "core.code-puppy",
            true,
            Availability::ProbeError {
                code: ProbeErrorCode::Agte202,
                reason: "probe exceeded its deadline".to_owned(),
                generation: 3,
            },
        ),
    ];

    let rows = project_agent_types(&observations, &PublishedSettings::default());

    assert_eq!(
        row(&rows, "core.llxprt").availability,
        AgentAvailability::Compatible
    );
    assert_eq!(
        row(&rows, "core.claude-code").availability,
        AgentAvailability::Incompatible {
            reason: "missing capability: prompt".to_owned()
        }
    );
    assert_eq!(
        row(&rows, "core.codex").availability,
        AgentAvailability::NotFound
    );
    assert_eq!(
        row(&rows, "core.code-puppy").availability,
        AgentAvailability::ProbeError {
            code: ProbeErrorCode::Agte202.as_str().to_owned(),
            reason: "probe exceeded its deadline".to_owned(),
        }
    );
}

#[test]
fn an_agent_the_document_does_not_mention_is_enabled_and_inherits_its_provenance() {
    let observations = vec![observation("core.llxprt", true, Availability::NotFound)];

    let rows = project_agent_types(&observations, &published("settings_schema = 2\n"));

    let row = row(&rows, "core.llxprt");
    assert!(row.enabled, "an unmentioned agent type is offered");
    assert_eq!(row.provenance, Provenance::Compiled);
}

#[test]
fn an_agent_the_document_assigns_reports_the_documents_value_and_provenance() {
    // The observation was taken at startup and still says the type was offered;
    // the row must report what the *candidate* says, or the screen would show
    // the user's own unsaved change as not having happened.
    let observations = vec![observation("core.llxprt", true, Availability::NotFound)];
    let published = published("settings_schema = 2\n[agents.\"core.llxprt\"]\nenabled = false\n");

    let rows = project_agent_types(&observations, &published);

    let row = row(&rows, "core.llxprt");
    assert!(!row.enabled);
    assert!(
        matches!(row.provenance, Provenance::Settings { .. }),
        "an assigned value comes from the document, not the compiled default"
    );
}

#[test]
fn an_unavailable_agent_type_still_reports_the_enablement_the_document_drafts() {
    let observations = vec![observation("core.codex", false, Availability::NotFound)];
    let published = published("settings_schema = 2\n[agents.\"core.codex\"]\nenabled = true\n");

    let rows = project_agent_types(&observations, &published);

    let row = row(&rows, "core.codex");
    assert!(
        row.enabled,
        "enablement may be drafted for a type that is not installed"
    );
    assert_eq!(row.availability, AgentAvailability::NotFound);
}

#[test]
fn an_agent_the_document_names_without_a_definition_projects_no_row() {
    // Identity comes from the inventory. A document naming an owner no
    // definition declares is preserved byte for byte, but the editor has
    // nothing to say about it and must not invent a row.
    let observations = vec![observation("core.llxprt", true, Availability::NotFound)];
    let published = published("settings_schema = 2\n[agents.\"core.claude\"]\nenabled = false\n");

    let rows = project_agent_types(&observations, &published);

    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].type_id.as_str(), "core.llxprt");
}

#[test]
fn the_display_name_comes_from_the_definition_rather_than_the_identity() {
    let observations = vec![observation("core.llxprt", true, Availability::NotFound)];

    let rows = project_agent_types(&observations, &PublishedSettings::default());

    assert_eq!(rows[0].display_name, definition("core.llxprt").display_name);
}

#[test]
fn projecting_the_same_snapshot_twice_produces_the_same_rows() {
    let observations = vec![
        observation("core.llxprt", true, Availability::NotFound),
        observation("core.codex", false, Availability::NotFound),
    ];
    let published = published("settings_schema = 2\n");

    assert_eq!(
        project_agent_types(&observations, &published),
        project_agent_types(&observations, &published),
        "the projection is a pure function of its inputs"
    );
}

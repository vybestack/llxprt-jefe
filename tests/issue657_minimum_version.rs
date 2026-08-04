//! Issue #657: declared minimum versions are documentation, never a gate.

use jefe::agent_status_view::{AgentAvailabilityObservation, project_agent_type_statuses};
use jefe::domain::agent_definition::{AgentDefinition, Availability};

/// Every shipped definition records the release its mappings were authored
/// against, so a user can see what Jefe expects.
#[test]
fn every_shipped_definition_declares_a_minimum_version() {
    for definition in AgentDefinition::shipped() {
        assert!(
            !definition.minimum_version.trim().is_empty(),
            "{} must declare a minimum version",
            definition.id
        );
    }
}

/// The declared minimum is shown next to the resolved installation.
#[test]
fn status_reports_the_resolved_version_and_the_declared_minimum() {
    let definition = shipped("core.codex");
    let views = project_agent_type_statuses(&[AgentAvailabilityObservation::new(
        &definition,
        true,
        Availability::InstalledCompatible {
            identity: "codex-cli 0.1.0".to_string(),
            generation: 1,
        },
    )]);
    let view = views
        .into_iter()
        .next()
        .unwrap_or_else(|| panic!("one row"));
    let reason = view.reason.unwrap_or_else(|| panic!("status reason"));
    assert!(reason.contains("codex-cli 0.1.0"), "reason {reason:?}");
    assert!(
        reason.contains(&definition.minimum_version),
        "declared minimum must be visible: {reason:?}"
    );
}

/// A version far below the declared minimum is still fully usable: nothing
/// parses or compares it.
#[test]
fn a_version_below_the_declared_minimum_is_not_gated() {
    let definition = shipped("core.codex");
    let views = project_agent_type_statuses(&[AgentAvailabilityObservation::new(
        &definition,
        true,
        Availability::InstalledCompatible {
            identity: "codex-cli 0.0.1".to_string(),
            generation: 1,
        },
    )]);
    let view = views
        .into_iter()
        .next()
        .unwrap_or_else(|| panic!("one row"));
    assert_eq!(view.status_text, "Installed");
    assert!(view.create_enabled, "an old release is not blocked");
    assert_eq!(view.error_code, None);
}

fn shipped(id: &str) -> AgentDefinition {
    AgentDefinition::shipped()
        .into_iter()
        .find(|definition| definition.id.as_str() == id)
        .unwrap_or_else(|| panic!("shipped definition {id}"))
}

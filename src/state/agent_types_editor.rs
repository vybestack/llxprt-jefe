//! The Agent Types editor's pure projection (issue #388, CW-08).
//!
//! One immutable probe snapshot and one candidate document become one row per
//! known agent type. Nothing here decides whether a type may be enabled, probes
//! anything, or starts a provider: enablement is
//! [`crate::agent_registry::agent_type_enabled`]'s rule and availability is
//! whatever the probe boundary already observed.
//!
//! Identity comes from the inventory. A document naming an owner no definition
//! declares keeps its bytes — the lossless writer sees to that — but has no row
//! here, because the editor has nothing true to say about a type it cannot
//! name, describe, or probe.

use crate::agent_registry::agent_type_enabled;
use crate::agent_status_view::AgentAvailabilityObservation;
use crate::domain::action_registry::Provenance;
use crate::domain::agent_definition::{AgentTypeId, Availability};
use crate::persistence::settings_document::PublishedSettings;

#[cfg(test)]
#[path = "agent_types_editor_tests.rs"]
mod agent_types_editor_tests;

/// What the probe boundary found for one agent type, as the editor shows it.
///
/// This is the observation stated in the terms a row needs: whether the type
/// can be used, and the exact reason when it cannot. The probe generations and
/// capability lists the runtime reasons about are deliberately absent — a row
/// that carried them would invite the screen to draw conclusions the probe
/// boundary has already drawn.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AgentAvailability {
    /// The executable is present and has every required capability.
    Compatible,
    /// The executable is present but unusable, for this exact reason.
    Incompatible {
        /// The probe's own reason, verbatim.
        reason: String,
    },
    /// No candidate resolved to an executable.
    NotFound,
    /// The probe itself failed.
    ProbeError {
        /// The stable probe diagnostic code.
        code: String,
        /// The probe's own reason, verbatim.
        reason: String,
    },
}

/// One agent type as the editor presents it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AgentEditorRow {
    /// The type's stable identity.
    pub type_id: AgentTypeId,
    /// The type's display name, from its definition.
    pub display_name: String,
    /// Whether the candidate document offers this type.
    pub enabled: bool,
    /// What the probe boundary found.
    pub availability: AgentAvailability,
    /// Where the effective enablement came from.
    pub provenance: Provenance,
}

/// One typed intent the Agent Types editor emits.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AgentIntent {
    /// Offer this type, or stop offering it.
    SetEnabled {
        /// The type to change.
        type_id: AgentTypeId,
        /// Whether the type is offered.
        enabled: bool,
    },
    /// Remove the assignment so the compiled default is inherited again.
    Reset {
        /// The type to reset.
        type_id: AgentTypeId,
    },
}

/// Project one probe snapshot and one candidate document into editor rows.
///
/// Rows keep the observation order, which is the registry's own canonical
/// order, so the list does not reshuffle when a probe result changes.
#[must_use]
pub fn project_agent_types(
    observations: &[AgentAvailabilityObservation],
    published: &PublishedSettings,
) -> Vec<AgentEditorRow> {
    observations
        .iter()
        .map(|observation| project_row(observation, published))
        .collect()
}

fn project_row(
    observation: &AgentAvailabilityObservation,
    published: &PublishedSettings,
) -> AgentEditorRow {
    let type_id = observation.type_id().clone();
    AgentEditorRow {
        enabled: agent_type_enabled(published, &type_id),
        provenance: provenance(published, &type_id),
        availability: availability(observation.availability()),
        display_name: observation.display_name().to_owned(),
        type_id,
    }
}

/// Where the effective enablement came from.
///
/// The candidate is what a save would make authoritative, so an unsaved draft
/// already reads as the document's own value: showing the compiled default
/// while the draft says otherwise would present the user's change as not having
/// happened.
fn provenance(published: &PublishedSettings, type_id: &AgentTypeId) -> Provenance {
    let assigned = crate::domain::Id::parse(type_id.as_str())
        .ok()
        .and_then(|owner| published.agents.get(&owner))
        .and_then(|owner| owner.enabled)
        .is_some();
    if assigned {
        Provenance::Settings {
            source: "settings".to_owned(),
        }
    } else {
        Provenance::Compiled
    }
}

fn availability(observed: &Availability) -> AgentAvailability {
    match observed {
        Availability::InstalledCompatible { .. } => AgentAvailability::Compatible,
        Availability::InstalledIncompatible { reason, .. } => AgentAvailability::Incompatible {
            reason: reason.clone(),
        },
        Availability::NotFound => AgentAvailability::NotFound,
        Availability::ProbeError { code, reason, .. } => AgentAvailability::ProbeError {
            code: code.as_str().to_owned(),
            reason: reason.clone(),
        },
    }
}

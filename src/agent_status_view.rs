//! Pure, iocraft-free projection of immutable agent availability observations.

use crate::agent_candidate::{CandidateGenerationKey, CandidateResolution};
use crate::domain::agent_definition::type_id::AgentTypeId;
use crate::domain::agent_definition::{AgentDefinition, Availability};

/// One enabled-definition observation captured by the startup probe boundary.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AgentAvailabilityObservation {
    type_id: AgentTypeId,
    display_name: String,
    minimum_version: String,
    enabled: bool,
    availability: Availability,
    generation: u64,
    candidate_resolution: Option<CandidateResolution>,
    candidate_generation_key: Option<CandidateGenerationKey>,
    pending_generation: Option<u64>,
}

impl AgentAvailabilityObservation {
    /// Build an observation from an immutable definition and one probe result.
    #[must_use]
    pub fn new(definition: &AgentDefinition, enabled: bool, availability: Availability) -> Self {
        let generation = availability.generation().unwrap_or_default();
        Self {
            type_id: definition.id.clone(),
            display_name: definition.display_name.clone(),
            minimum_version: definition.minimum_version.clone(),
            enabled,
            availability,
            generation,
            candidate_resolution: None,
            candidate_generation_key: None,
            pending_generation: None,
        }
    }

    /// Publish an unresolved definition at an explicit state-owned generation.
    #[must_use]
    pub fn not_found(definition: &AgentDefinition, enabled: bool, generation: u64) -> Self {
        Self {
            type_id: definition.id.clone(),
            display_name: definition.display_name.clone(),
            minimum_version: definition.minimum_version.clone(),
            enabled,
            availability: Availability::NotFound,
            generation,
            candidate_resolution: None,
            candidate_generation_key: None,
            pending_generation: None,
        }
    }

    /// Publish a resolved definition before its process probe executes.
    #[must_use]
    pub fn pending(
        definition: &AgentDefinition,
        enabled: bool,
        generation: u64,
        resolution: CandidateResolution,
    ) -> Self {
        let candidate_generation_key = resolution
            .resolved()
            .map(|candidate| candidate.generation_key(definition));
        Self {
            type_id: definition.id.clone(),
            display_name: definition.display_name.clone(),
            minimum_version: definition.minimum_version.clone(),
            enabled,
            availability: Availability::NotFound,
            generation,
            candidate_resolution: Some(resolution),
            candidate_generation_key,
            pending_generation: Some(generation),
        }
    }

    /// Stable definition id.
    #[must_use]
    pub const fn type_id(&self) -> &AgentTypeId {
        &self.type_id
    }

    /// Definition display name.
    #[must_use]
    pub fn display_name(&self) -> &str {
        &self.display_name
    }

    /// Release the shipped mappings were authored against, for display only.
    #[must_use]
    pub fn minimum_version(&self) -> &str {
        &self.minimum_version
    }

    /// Durable definition enablement, separate from observed availability.
    #[must_use]
    pub const fn enabled(&self) -> bool {
        self.enabled
    }

    /// Runtime availability produced by the probe boundary.
    #[must_use]
    pub const fn availability(&self) -> &Availability {
        &self.availability
    }

    /// Last state-owned probe generation, including NotFound observations.
    #[must_use]
    pub const fn generation(&self) -> u64 {
        self.generation
    }

    /// Candidate resolution captured by the state-owned observation boundary.
    #[must_use]
    pub const fn candidate_resolution(&self) -> Option<&CandidateResolution> {
        self.candidate_resolution.as_ref()
    }

    /// Candidate identity used to advance a generation only when evidence changes.
    #[must_use]
    pub const fn candidate_generation_key(&self) -> Option<&CandidateGenerationKey> {
        self.candidate_generation_key.as_ref()
    }

    /// Generation currently awaiting a correlated probe completion.
    #[must_use]
    pub const fn pending_generation(&self) -> Option<u64> {
        self.pending_generation
    }

    /// Apply a result only to the generation that is still pending.
    pub fn apply_probe_result(&mut self, generation: u64, availability: Availability) -> bool {
        if self.pending_generation != Some(generation) {
            return false;
        }
        self.availability = availability;
        self.pending_generation = None;
        true
    }
}

/// Plain display row consumed by the thin Agent Types renderer.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AgentTypeStatusView {
    pub display_name: String,
    pub status_text: &'static str,
    pub enabled: bool,
    pub reason: Option<String>,
    pub error_code: Option<&'static str>,
    pub create_enabled: bool,
}

/// Project every input observation to exactly one display row, preserving order.
#[must_use]
pub fn project_agent_type_statuses(
    observations: &[AgentAvailabilityObservation],
) -> Vec<AgentTypeStatusView> {
    observations.iter().map(project_status).collect()
}

fn project_status(observation: &AgentAvailabilityObservation) -> AgentTypeStatusView {
    let (status_text, reason, error_code) = if observation.pending_generation().is_some() {
        ("Checking", None, None)
    } else {
        match observation.availability() {
            Availability::NotFound => (
                "Not found",
                Some("no executable candidate resolved".to_string()),
                None,
            ),
            Availability::InstalledCompatible { identity, .. } => (
                "Installed",
                Some(format!(
                    "identity: {identity} (authored against {})",
                    observation.minimum_version()
                )),
                None,
            ),
            Availability::InstalledIncompatible { reason, .. } => {
                ("Incompatible", Some(reason.clone()), None)
            }
            Availability::ProbeError { code, reason, .. } => {
                ("Probe error", Some(reason.clone()), Some(code.as_str()))
            }
        }
    };
    AgentTypeStatusView {
        display_name: observation.display_name().to_string(),
        status_text,
        enabled: observation.enabled(),
        reason,
        error_code,
        create_enabled: observation.enabled()
            && matches!(
                observation.availability(),
                Availability::InstalledCompatible { .. }
            ),
    }
}

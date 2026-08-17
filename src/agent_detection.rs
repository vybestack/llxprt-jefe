//! Session-cached detection of installed agent definitions.

use std::sync::OnceLock;

use crate::agent_candidate::{AgentCandidateResolver, CandidateResolution};
use crate::agent_candidate_path::PathSnapshot;
use crate::domain::agent_definition::{
    AgentDefinition, AgentTypeId, CandidateKind, shipped_preference_order,
};

static INSTALLED_AGENT_TYPES: OnceLock<Vec<AgentTypeId>> = OnceLock::new();

/// Enabled shipped definitions whose executable candidates resolve in this session.
#[must_use]
pub fn available_agent_type_ids() -> &'static [AgentTypeId] {
    INSTALLED_AGENT_TYPES.get_or_init(detect_available_agent_type_ids)
}

fn detect_available_agent_type_ids() -> Vec<AgentTypeId> {
    let snapshot = PathSnapshot::current();
    let repository_root = std::env::current_dir().unwrap_or_default();
    let resolver = AgentCandidateResolver::new(&snapshot, repository_root);
    AgentDefinition::shipped()
        .into_iter()
        .filter(|definition| {
            matches!(
                resolver.resolve(definition),
                CandidateResolution::Resolved(_)
            )
        })
        .map(|definition| definition.id)
        .collect()
}

/// Pure detection of shipped path-name candidates in explicit PATH directories.
#[must_use]
pub fn detect_agent_type_ids(directories: &[std::path::PathBuf]) -> Vec<AgentTypeId> {
    let snapshot = PathSnapshot::for_platform(
        crate::agent_candidate_path::AgentExecutablePlatform::current(),
        directories.to_vec(),
        std::env::var_os("PATHEXT"),
    );
    let mut detected = AgentDefinition::shipped()
        .into_iter()
        .filter(|definition| {
            definition.candidates.iter().any(|candidate| {
                let CandidateKind::PathName { name } = &candidate.kind else {
                    return false;
                };
                snapshot.resolve_binary(name).is_some()
            })
        })
        .map(|definition| definition.id)
        .collect::<Vec<_>>();
    detected.sort_by_key(shipped_display_order);
    detected
}

fn shipped_display_order(type_id: &AgentTypeId) -> usize {
    AgentDefinition::shipped()
        .into_iter()
        .position(|definition| definition.id == *type_id)
        .unwrap_or(usize::MAX)
}
/// Project enabled, compatible observations to stable definition identifiers.
///
/// Output is ordered by the product default-preference order (LLxprt first),
/// so `.first()` consumers select LLxprt whenever it is installed. Ids
/// outside the shipped set keep observation order after the shipped ones.
#[must_use]
pub fn compatible_agent_type_ids(
    observations: &[crate::agent_status_view::AgentAvailabilityObservation],
) -> Vec<AgentTypeId> {
    let mut compatible: Vec<AgentTypeId> = observations
        .iter()
        .filter(|observation| {
            observation.enabled()
                && observation.pending_generation().is_none()
                && matches!(
                    observation.availability(),
                    crate::domain::agent_definition::Availability::InstalledCompatible { .. }
                )
        })
        .map(|observation| observation.type_id().clone())
        .collect();
    compatible.sort_by_key(preference_rank);
    compatible
}

/// Rank of a shipped type id in the product default-preference order.
fn preference_rank(type_id: &AgentTypeId) -> usize {
    shipped_preference_order()
        .iter()
        .position(|preferred| preferred == type_id)
        .unwrap_or(usize::MAX)
}

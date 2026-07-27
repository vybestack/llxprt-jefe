//! Session-cached detection of installed agent runtimes.

use std::sync::OnceLock;

use crate::domain::AgentKind;
use crate::runtime::{AgentExecutablePlatform, AgentExecutableResolver};

static INSTALLED_AGENT_KINDS: OnceLock<Vec<AgentKind>> = OnceLock::new();

/// Agent kinds whose executable is present on PATH, detected once per session.
#[must_use]
pub fn installed_agent_kinds() -> &'static [AgentKind] {
    INSTALLED_AGENT_KINDS.get_or_init(detect_installed_agent_kinds)
}

fn detect_installed_agent_kinds() -> Vec<AgentKind> {
    detect_with_resolver(&AgentExecutableResolver::current())
}

/// Pure detection of which agent runtimes are installed, given an explicit
/// slice of PATH directories.
///
/// Returns the kinds whose executable is present and executable (on Unix) or
/// present as a file (on non-Unix) in any of the supplied directories. The
/// detection order follows the canonical kind order in the candidate list.
///
/// Extracted as a pure function so the detection logic is deterministically
/// testable without touching the real filesystem or `PATH` environment
/// variable.
#[must_use]
pub fn detect_agent_kinds(dirs: &[std::path::PathBuf]) -> Vec<AgentKind> {
    let resolver = AgentExecutableResolver::for_platform(
        AgentExecutablePlatform::current(),
        dirs.to_vec(),
        std::env::var_os("PATHEXT"),
    );
    detect_with_resolver(&resolver)
}

/// Bridge compatible definition observations into the legacy kind list still
/// consumed by forms and launch gates before the final cutover.
///
/// Matching is data-driven through candidate binary names; no product identity
/// is added outside shipped definitions.
#[must_use]
pub fn compatible_legacy_agent_kinds(
    observations: &[crate::agent_status_view::AgentAvailabilityObservation],
    definitions: &[crate::domain::agent_definition::AgentDefinition],
) -> Vec<AgentKind> {
    [AgentKind::Llxprt, AgentKind::CodePuppy]
        .into_iter()
        .filter(|kind| legacy_kind_is_compatible(*kind, observations, definitions))
        .collect()
}

fn legacy_kind_is_compatible(
    kind: AgentKind,
    observations: &[crate::agent_status_view::AgentAvailabilityObservation],
    definitions: &[crate::domain::agent_definition::AgentDefinition],
) -> bool {
    definitions.iter().any(|definition| {
        definition.candidates.iter().any(|candidate| {
            candidate.value.file_name() == Some(std::ffi::OsStr::new(kind.binary_name()))
        }) && observations.iter().any(|observation| {
            observation.type_id() == &definition.id
                && observation.enabled()
                && observation.pending_generation().is_none()
                && matches!(
                    observation.availability(),
                    crate::domain::agent_definition::Availability::InstalledCompatible { .. }
                )
        })
    })
}

fn detect_with_resolver(resolver: &AgentExecutableResolver) -> Vec<AgentKind> {
    [AgentKind::Llxprt, AgentKind::CodePuppy]
        .into_iter()
        .filter(|kind| resolver.resolve(*kind).is_ok())
        .collect()
}

#[cfg(test)]
mod compatibility_tests {
    use super::*;
    use crate::agent_status_view::AgentAvailabilityObservation;
    use crate::domain::agent_definition::{AgentDefinition, Availability};

    fn observation(
        definition: &AgentDefinition,
        enabled: bool,
        availability: Availability,
    ) -> AgentAvailabilityObservation {
        AgentAvailabilityObservation::new(definition, enabled, availability)
    }

    fn compatible(generation: u64) -> Availability {
        Availability::InstalledCompatible {
            identity: "fixture identity".to_string(),
            capabilities: Vec::new(),
            generation,
        }
    }

    #[test]
    fn legacy_bridge_requires_enabled_compatible_observation() {
        let definitions = AgentDefinition::shipped();
        let llxprt = definitions
            .iter()
            .find(|definition| definition.id.as_str() == "core.llxprt")
            .unwrap_or_else(|| panic!("LLxprt definition must be shipped"));
        let code_puppy = definitions
            .iter()
            .find(|definition| definition.id.as_str() == "core.code-puppy")
            .unwrap_or_else(|| panic!("Code Puppy definition must be shipped"));
        let observations = vec![
            observation(llxprt, false, compatible(1)),
            observation(
                code_puppy,
                true,
                Availability::InstalledIncompatible {
                    reason: "missing required capability: interactive".to_string(),
                    generation: 2,
                },
            ),
        ];

        assert!(
            compatible_legacy_agent_kinds(&observations, &definitions).is_empty(),
            "disabled and incompatible definitions must not enter legacy launch gates"
        );

        let enabled = vec![observation(llxprt, true, compatible(3))];
        assert_eq!(
            compatible_legacy_agent_kinds(&enabled, &definitions),
            vec![AgentKind::Llxprt]
        );
    }
}

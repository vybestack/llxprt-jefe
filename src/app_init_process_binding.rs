//! Atomic process binding selection during startup restoration.
//!
//! PID and process-instance identity are one observation. The resolver chooses
//! fresh or persisted evidence as a unit and never fills a missing fresh field
//! from stale persistence.

use jefe::domain::ProcessIdentity;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct ProcessBindingObservation {
    pub pid: Option<u32>,
    pub identity: Option<ProcessIdentity>,
}

impl ProcessBindingObservation {
    #[must_use]
    pub(super) const fn new(pid: Option<u32>, identity: Option<ProcessIdentity>) -> Self {
        Self { pid, identity }
    }
}

#[must_use]
pub(super) fn resolve_process_binding(
    fresh: ProcessBindingObservation,
    persisted: ProcessBindingObservation,
) -> ProcessBindingObservation {
    let selected = if fresh.pid.is_some() || fresh.identity.is_some() {
        fresh
    } else {
        persisted
    };
    normalize_process_binding(selected)
}

fn normalize_process_binding(observation: ProcessBindingObservation) -> ProcessBindingObservation {
    match (observation.pid, observation.identity) {
        (Some(pid), Some(identity)) if pid == identity.pid => observation,
        (None, Some(identity)) => {
            ProcessBindingObservation::new(Some(identity.pid), Some(identity))
        }
        (Some(pid), None) => ProcessBindingObservation::new(Some(pid), None),
        (None, None) | (Some(_), Some(_)) => ProcessBindingObservation::new(None, None),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn observation(
        pid: Option<u32>,
        identity: Option<ProcessIdentity>,
    ) -> ProcessBindingObservation {
        ProcessBindingObservation::new(pid, identity)
    }

    #[test]
    fn fresh_pid_never_borrows_persisted_identity() {
        let fresh = observation(Some(42), None);
        let persisted = observation(Some(41), Some(ProcessIdentity::new(41, 900)));
        assert_eq!(resolve_process_binding(fresh, persisted), fresh);
    }

    #[test]
    fn fresh_identity_supplies_its_own_pid_without_persisted_fields() {
        let identity = ProcessIdentity::new(42, 901);
        let fresh = observation(None, Some(identity));
        let persisted = observation(Some(41), Some(ProcessIdentity::new(41, 900)));
        assert_eq!(
            resolve_process_binding(fresh, persisted),
            observation(Some(42), Some(identity))
        );
    }

    #[test]
    fn absent_fresh_evidence_preserves_one_coherent_persisted_observation() {
        let persisted = observation(Some(41), Some(ProcessIdentity::new(41, 900)));
        assert_eq!(
            resolve_process_binding(observation(None, None), persisted),
            persisted
        );
    }

    #[test]
    fn legacy_pid_only_binding_remains_supported() {
        let persisted = observation(Some(41), None);
        assert_eq!(
            resolve_process_binding(observation(None, None), persisted),
            persisted
        );
    }

    #[test]
    fn internally_mismatched_observation_is_discarded() {
        let mismatched = observation(Some(42), Some(ProcessIdentity::new(41, 900)));
        assert_eq!(
            resolve_process_binding(mismatched, observation(None, None)),
            observation(None, None)
        );
    }

    fn applied_runtime_binding(
        fresh: ProcessBindingObservation,
        persisted: ProcessBindingObservation,
    ) -> jefe::domain::RuntimeBinding {
        use jefe::domain::{Agent, AgentId, AgentStatus, Repository, RepositoryId};
        use jefe::state::AppState;

        let repository_id = RepositoryId("binding-repo".to_owned());
        let repository = Repository::new(
            repository_id.clone(),
            "Binding Repo".to_owned(),
            "binding-repo".to_owned(),
            std::path::PathBuf::from("/tmp/binding-repo"),
        );
        let mut agent = Agent::new(
            AgentId("binding-agent".to_owned()),
            repository_id,
            "Binding Agent".to_owned(),
            std::path::PathBuf::from("/tmp/binding-agent"),
        );
        agent.status = AgentStatus::Running;
        let signature = super::super::launch_signature_for_agent(&agent, &repository);
        let agent_id = agent.id.clone();
        let resolved = resolve_process_binding(fresh, persisted);
        let mut state = AppState {
            agents: vec![agent],
            repositories: vec![repository],
            ..AppState::default()
        };

        super::super::apply_restored_state(
            &mut state,
            vec![(agent_id.clone(), signature, resolved.pid, resolved.identity)],
            Vec::new(),
            None,
        );
        let Some(binding) = state
            .agents
            .iter()
            .find(|agent| agent.id == agent_id)
            .and_then(|agent| agent.runtime_binding.clone())
        else {
            panic!("restored agent must have a runtime binding");
        };
        binding
    }

    #[test]
    fn fresh_pid_only_is_applied_without_stale_persisted_identity() {
        let binding = applied_runtime_binding(
            observation(Some(42), None),
            observation(Some(41), Some(ProcessIdentity::new(41, 900))),
        );
        assert_eq!(binding.pid, Some(42));
        assert_eq!(binding.process_identity, None);
    }

    #[test]
    fn fresh_identity_only_is_applied_as_a_coherent_binding() {
        let identity = ProcessIdentity::new(42, 901);
        let binding = applied_runtime_binding(
            observation(None, Some(identity)),
            observation(Some(41), Some(ProcessIdentity::new(41, 900))),
        );
        assert_eq!(binding.pid, Some(42));
        assert_eq!(binding.process_identity, Some(identity));
    }
}

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
}

//! Orphan-worker reconciliation for startup (issue #332).
//!
//! Extracted from `app_init` so the parent module stays under the source-size
//! limit. Computes orphan evidence for startup classification and performs the
//! best-effort reap of a dead-launcher orphan tree before Dead-marking.

use jefe::domain::{Agent, ProcessIdentity};

use crate::app_init::{BindingEvidence, SessionEvidence};

/// Compute orphan evidence for startup classification (issue #332).
pub(super) fn orphan_evidence(
    session: SessionEvidence,
    remote: bool,
    worker_identities: Option<&Vec<ProcessIdentity>>,
) -> jefe::runtime::OrphanClassification {
    use jefe::runtime::{OrphanClassification as Oc, PaneLiveness};

    if remote {
        return Oc::NoOrphan;
    }
    let identities = worker_identities.map(Vec::as_slice).unwrap_or(&[]);
    if identities.is_empty() {
        return Oc::NoOrphan;
    }
    let pane = match session {
        SessionEvidence::Alive => PaneLiveness::Alive,
        SessionEvidence::Missing => PaneLiveness::Dead,
        SessionEvidence::Unavailable => PaneLiveness::Unavailable,
    };
    let observed: Vec<jefe::runtime::ObservedDescendant> = identities
        .iter()
        .map(|identity| jefe::runtime::ObservedDescendant {
            recorded: *identity,
            liveness: if jefe::runtime::descendant_still_matches_anchor(*identity) {
                jefe::runtime::ProcessLiveness::Alive
            } else {
                jefe::runtime::ProcessLiveness::Dead
            },
        })
        .collect();
    let session_exists = session != SessionEvidence::Missing;
    jefe::runtime::classify_orphan_state(pane, session_exists, &observed)
}

/// Best-effort reap of a dead-launcher orphan: terminate the validated worker
/// descendant tree and remove the stale multiplexer session (issue #332).
///
/// All failures are logged and swallowed inside [`reap_orphan_session`] so
/// startup is never aborted by a reap/kill error.
pub(super) fn reap_orphaned_agent(agent: &Agent) {
    let Some(binding) = agent.runtime_binding.as_ref() else {
        return;
    };
    jefe::runtime::reap_orphan_session(&binding.worker_identities, &binding.session_name);
}

#[cfg(test)]
mod tests {
    use super::super::{
        BindingEvidence, ProcessLiveness, SessionEvidence, StartupClassification, classify_startup,
    };

    #[test]
    fn dead_pane_with_orphans_is_orphaned_not_recoverable() {
        // AC10: session missing/dead + validated live descendants must route to
        // Orphaned, NOT Recoverable, so the caller reaps instead of leaving the
        // worker stranded (issue #332).
        assert_eq!(
            classify_startup(
                SessionEvidence::Missing,
                BindingEvidence::Coherent,
                false,
                ProcessLiveness::Alive,
                jefe::runtime::OrphanClassification::DeadPaneWithOrphans,
            ),
            StartupClassification::Orphaned
        );
        // Also when the session exists but the pane is dead.
        assert_eq!(
            classify_startup(
                SessionEvidence::Unavailable,
                BindingEvidence::Coherent,
                false,
                ProcessLiveness::Alive,
                jefe::runtime::OrphanClassification::DeadPaneWithOrphans,
            ),
            StartupClassification::Orphaned
        );
    }

    #[test]
    fn alive_pane_with_orphan_evidence_stays_running() {
        // AC11: a genuinely live/attachable session is never an orphan, even if
        // orphan evidence is present (#323/#324/#326 behavior preserved).
        assert_eq!(
            classify_startup(
                SessionEvidence::Alive,
                BindingEvidence::Coherent,
                false,
                ProcessLiveness::Alive,
                jefe::runtime::OrphanClassification::DeadPaneWithOrphans,
            ),
            StartupClassification::Running
        );
        assert_eq!(
            classify_startup(
                SessionEvidence::Alive,
                BindingEvidence::Coherent,
                false,
                ProcessLiveness::Alive,
                jefe::runtime::OrphanClassification::NoOrphan,
            ),
            StartupClassification::Running
        );
    }

    #[test]
    fn dead_pane_without_orphans_is_not_orphaned() {
        // A dead pane with no surviving orphans must not take the Orphaned path.
        assert_eq!(
            classify_startup(
                SessionEvidence::Missing,
                BindingEvidence::Coherent,
                false,
                ProcessLiveness::Alive,
                jefe::runtime::OrphanClassification::DeadPaneNoWorker,
            ),
            StartupClassification::Recoverable
        );
    }
}

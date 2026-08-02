//! Startup evidence reconciliation for persisted runtime bindings.

use jefe::domain::{AgentId, PaneWorkerTopology, RuntimeBinding};
use jefe::runtime::{OrphanClassification, ProcessLiveness, RuntimeSession, SessionLiveness};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum SessionEvidence {
    Alive,
    Missing,
    Unavailable,
}

impl From<SessionLiveness> for SessionEvidence {
    fn from(value: SessionLiveness) -> Self {
        match value {
            SessionLiveness::Alive => Self::Alive,
            SessionLiveness::Missing => Self::Missing,
            SessionLiveness::Unavailable => Self::Unavailable,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum BindingEvidence {
    Coherent,
    Legacy,
    Inconsistent,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum StartupClassification {
    Running,
    Stopped,
    Stale,
    Recoverable,
    Inconsistent,
    Orphaned,
    /// A probe did not answer, so startup has no basis for any classification.
    ///
    /// Distinct from [`Self::Recoverable`], which is a conclusion drawn from
    /// evidence that did arrive. Held means the evidence never arrived, so the
    /// agent must be left exactly as persisted and re-probed later rather than
    /// classified now (issue #541).
    Held,
}

/// Ownership evidence for one persisted binding.
///
/// Configuration content deliberately does not participate. Whether jefe still
/// owns a live process is answered by the deterministic session name and the
/// recorded process identity alone; what the user has since typed into the
/// agent's fields is a statement about the *next* launch and cannot revoke
/// ownership of a process that is already running (issue #583).
#[must_use]
pub(super) fn binding_evidence(
    binding: Option<&RuntimeBinding>,
    agent_id: &AgentId,
) -> BindingEvidence {
    let Some(binding) = binding else {
        return BindingEvidence::Legacy;
    };
    if binding.session_name != RuntimeSession::session_name_for(agent_id) {
        return BindingEvidence::Inconsistent;
    }
    binding_process_evidence(binding)
}

fn binding_process_evidence(binding: &RuntimeBinding) -> BindingEvidence {
    let Some(worker) = binding.worker_identity else {
        return BindingEvidence::Legacy;
    };
    // A worker identity that claims the pane leader's PID on a platform where
    // the worker runs *below* the pane is precisely the conflation this issue
    // forbids. Treat it as inconsistent evidence rather than trusting it
    // (issue #543).
    if !PaneWorkerTopology::current().pane_determines_worker()
        && binding
            .pane_identity
            .is_some_and(|pane| pane.pid() == worker.pid())
    {
        return BindingEvidence::Inconsistent;
    }
    // Without a creation token the anchor cannot reject PID reuse; that is the
    // legacy shape a restored document produces.
    if worker.started_at().is_none() {
        return BindingEvidence::Legacy;
    }
    BindingEvidence::Coherent
}

#[must_use]
pub(super) fn classify_startup(
    session: SessionEvidence,
    binding: BindingEvidence,
    remote: bool,
    process: ProcessLiveness,
    orphan: OrphanClassification,
) -> StartupClassification {
    if session != SessionEvidence::Alive && orphan == OrphanClassification::DeadPaneWithOrphans {
        return StartupClassification::Orphaned;
    }
    if binding == BindingEvidence::Inconsistent {
        return StartupClassification::Inconsistent;
    }
    if !remote && process == ProcessLiveness::ReusedPid {
        return StartupClassification::Stale;
    }
    match session {
        SessionEvidence::Alive => StartupClassification::Running,
        // The session probe did not answer. Every classification below this
        // point reads the *absence* of a session as meaningful, which it is
        // not when the question was never answered (issue #541, #537).
        SessionEvidence::Unavailable => StartupClassification::Held,
        SessionEvidence::Missing if remote => StartupClassification::Stopped,
        SessionEvidence::Missing => classify_missing_local_process(process),
    }
}

fn classify_missing_local_process(process: ProcessLiveness) -> StartupClassification {
    match process {
        ProcessLiveness::Dead => StartupClassification::Stopped,
        ProcessLiveness::ReusedPid => StartupClassification::Stale,
        ProcessLiveness::MalformedIdentity => StartupClassification::Inconsistent,
        // The process query itself failed or was refused, so liveness is
        // unknown rather than recoverable.
        ProcessLiveness::Inaccessible | ProcessLiveness::ProbeFailure => {
            StartupClassification::Held
        }
        ProcessLiveness::Alive => StartupClassification::Recoverable,
    }
}

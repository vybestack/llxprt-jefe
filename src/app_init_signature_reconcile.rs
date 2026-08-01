//! Startup evidence reconciliation for persisted runtime bindings.

use jefe::domain::{
    Agent, AgentId, AgentLaunchRequest, LaunchSignatureV1, PaneWorkerTopology, Repository,
    RuntimeBinding,
};
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
    DefinitionDrift,
    Inconsistent,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum DurableSignatureEvidence {
    Match,
    DefinitionDrift,
    Inconsistent,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum StartupClassification {
    Running,
    DefinitionDrift,
    Stopped,
    Stale,
    Recoverable,
    Inconsistent,
    Orphaned,
}

#[must_use]
pub(super) fn binding_evidence(
    binding: Option<&RuntimeBinding>,
    agent_id: &AgentId,
    signature: &AgentLaunchRequest,
    persisted_signature: Option<&LaunchSignatureV1>,
    durable_signature: DurableSignatureEvidence,
) -> BindingEvidence {
    if durable_signature == DurableSignatureEvidence::Inconsistent {
        return BindingEvidence::Inconsistent;
    }
    let Some(binding) = binding else {
        return match durable_signature {
            DurableSignatureEvidence::Match => BindingEvidence::Legacy,
            DurableSignatureEvidence::DefinitionDrift | DurableSignatureEvidence::Inconsistent => {
                BindingEvidence::Inconsistent
            }
        };
    };
    if binding.session_name != RuntimeSession::session_name_for(agent_id)
        || !binding_signature_matches(binding, signature, persisted_signature, durable_signature)
    {
        return BindingEvidence::Inconsistent;
    }
    let process_evidence = binding_process_evidence(binding);
    if process_evidence == BindingEvidence::Inconsistent {
        return process_evidence;
    }
    match durable_signature {
        DurableSignatureEvidence::DefinitionDrift => BindingEvidence::DefinitionDrift,
        DurableSignatureEvidence::Match => process_evidence,
        DurableSignatureEvidence::Inconsistent => BindingEvidence::Inconsistent,
    }
}

fn binding_signature_matches(
    binding: &RuntimeBinding,
    signature: &AgentLaunchRequest,
    persisted_signature: Option<&LaunchSignatureV1>,
    durable_signature: DurableSignatureEvidence,
) -> bool {
    match durable_signature {
        DurableSignatureEvidence::Match => {
            jefe::runtime::launch_compose::launch_signature_from_request(signature)
                .is_ok_and(|current| current == binding.launch_signature)
        }
        DurableSignatureEvidence::DefinitionDrift => {
            persisted_signature == Some(&binding.launch_signature)
        }
        DurableSignatureEvidence::Inconsistent => false,
    }
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
    if binding == BindingEvidence::DefinitionDrift {
        if remote || session != SessionEvidence::Alive {
            return StartupClassification::Inconsistent;
        }
        if process == ProcessLiveness::Dead {
            return StartupClassification::Inconsistent;
        }
        return StartupClassification::DefinitionDrift;
    }
    match session {
        SessionEvidence::Alive => StartupClassification::Running,
        SessionEvidence::Unavailable => StartupClassification::Recoverable,
        SessionEvidence::Missing if remote => StartupClassification::Stopped,
        SessionEvidence::Missing => classify_missing_local_process(process),
    }
}

fn classify_missing_local_process(process: ProcessLiveness) -> StartupClassification {
    match process {
        ProcessLiveness::Dead => StartupClassification::Stopped,
        ProcessLiveness::ReusedPid => StartupClassification::Stale,
        ProcessLiveness::MalformedIdentity => StartupClassification::Inconsistent,
        ProcessLiveness::Alive | ProcessLiveness::Inaccessible | ProcessLiveness::ProbeFailure => {
            StartupClassification::Recoverable
        }
    }
}

pub(super) fn durable_signature_evidence(
    agent: &Agent,
    repository: &Repository,
) -> DurableSignatureEvidence {
    match agent.persisted_launch_signature.as_ref() {
        None => DurableSignatureEvidence::Match,
        Some(persisted) => jefe::state::durable_projection::current_launch_signature(
            agent, repository,
        )
        .map_or(DurableSignatureEvidence::Inconsistent, |current| {
            if current == *persisted {
                DurableSignatureEvidence::Match
            } else if current.version == persisted.version
                && current.typed_value_hash == persisted.typed_value_hash
                && current.target_fingerprint == persisted.target_fingerprint
            {
                DurableSignatureEvidence::DefinitionDrift
            } else {
                DurableSignatureEvidence::Inconsistent
            }
        }),
    }
}

//! Persistent action-provider candidate lifecycle (issue #390 CW-10, Slice C2).
//!
//! A persistent provider candidate is started once during candidate startup in
//! deterministic plugin-id order, handshakes to `ready` (`hello`/`hello-ack` →
//! `configure`/`ready` with per-stage bounds), and is then kept alive for the
//! lifetime of the host. Every required candidate must reach `ready` before one
//! atomic publication; if any spawn/handshake/protocol/capability/timeout phase
//! fails, every previously started and the failing candidate is stopped and
//! reaped, no publication is returned, and the typed rollback evidence records
//! that each was reaped. There is no auto-restart.
//!
//! The persistent lifecycle owns its processes, pipes, drains, and reaping
//! entirely inside this module's [`PersistentSupervisor`]; no `Child`, pipe, or
//! thread handle ever leaves it. It reuses the Slice C1 framing, environment
//! construction, secret resolution/redaction, drains, process-tree spawn, and
//! staged reap helpers, but keeps its lifecycle ownership and types distinct
//! from the one-shot supervisor. The supervisor exposes only typed readiness,
//! health, publication, and shutdown values.

use std::collections::BTreeSet;
use std::fmt;
use std::io;
use std::io::Write as _;
use std::path::PathBuf;
use std::process::{ChildStdin, ExitStatus};
use std::sync::mpsc;

use crate::domain::{CanonicalSemver, Id};

use super::candidate::{StartOutcome, start_prepared};
use super::drains::{StderrDrain, StdoutDrain, StdoutEvent, final_stdout_drain};
use super::driver;
use super::dto::{Capability, ConfigurePayload, ShutdownReason};
use super::encode::encode_shutdown;
use super::environment::{
    EnvironmentError, HostEnv, ProcessEnv, ProviderEnvironment, Redactor, build_process_env,
    resolve_configure_secrets,
};
use super::error;
use super::identifiers::RequestId;
use super::process_tree::ProviderProcess;
use super::protocol::{LifecycleOrder, MessageKind};
use super::redaction;
use super::supervisor::{
    CleanupFailure, ShutdownOutcome, SupervisorBounds, SupervisorFailure, collect_retained_stderr,
    compose_cleanup_failure, signal_cleanup_evidence, staged_shutdown,
};

/// One persistent candidate startup request (the inputs to `run_persistent_startup`).
#[derive(Debug, Clone)]
pub struct PersistentStartup {
    /// The candidates to start, in any order; startup observes plugin-id order.
    pub candidates: Vec<PersistentCandidate>,
}

/// One persistent candidate description (a handle-free, data-only request).
#[derive(Debug, Clone)]
pub struct PersistentCandidate {
    /// The plugin package id; startup order and de-duplication use this.
    pub plugin_id: Id,
    /// The plugin package version sent in `hello`.
    pub plugin_version: CanonicalSemver,
    /// The selected provider binary.
    pub binary: PathBuf,
    /// Arguments to pass to the binary.
    pub arguments: Vec<String>,
    /// Contained working directory.
    pub working_dir: PathBuf,
    /// Environment specification (CW10-14).
    pub environment: ProviderEnvironment,
    /// Contained `HOME`.
    pub home: PathBuf,
    /// Contained `TMPDIR`.
    pub tmpdir: PathBuf,
    /// Locale (`LC_ALL`/`LANG`).
    pub locale: String,
    /// Host API identifier sent in `hello`.
    pub host_api: String,
    /// Fixed positive generation for this candidate's process.
    pub generation: u64,
    /// Host request id for this candidate's handshake.
    pub request_id: RequestId,
    /// Base `configure` payload; the supervisor merges resolved secrets in.
    pub configure: ConfigurePayload,
    /// Capabilities the manifest declares this provider may report at `ready`.
    pub declared_capabilities: Vec<Capability>,
}

/// One candidate's fully resolved launch inputs: the contained process
/// environment, the `Configure` payload with every secret merged in, and the
/// redactor scrubbed against those secret values (issue #704 S2).
///
/// Built once, before any provider spawns, by
/// [`prepare_candidate_environment`]; the same values drive spawn and
/// handshake, so a mutable host input resolved during preparation is never
/// read a second time.
pub(crate) struct PreparedEnvironment {
    /// The contained environment the provider command is spawned with.
    pub(crate) env: ProcessEnv,
    /// The `Configure` payload with resolved secrets merged in.
    pub(crate) configure: ConfigurePayload,
    /// Scrubs every resolved secret value out of observation surfaces.
    pub(crate) redactor: Redactor,
}

/// Resolve one candidate's contained environment and `Configure` secrets.
///
/// This is the single preparation authority shared by preflight and startup:
/// it builds the empty-based contained environment, resolves every declared
/// secret source against the host environment exactly once, rejects
/// caller-supplied `Configure` secrets, and returns the redactor covering
/// every resolved value. It reads, but never mutates, the host environment
/// and spawns nothing.
///
/// # Errors
///
/// Returns [`EnvironmentError::UnresolvedSecret`] when a declared secret
/// source is absent from the host environment, or
/// [`EnvironmentError::UndeclaredConfigureSecret`] when the caller supplied a
/// `Configure` secret the manifest did not declare. No secret value is ever
/// carried in an error.
pub(crate) fn prepare_candidate_environment<E: HostEnv>(
    candidate: &PersistentCandidate,
    host_env: &E,
) -> Result<PreparedEnvironment, EnvironmentError> {
    let env = build_process_env(
        &candidate.environment,
        &candidate.home,
        &candidate.tmpdir,
        &candidate.locale,
        host_env,
    )?;
    let secrets = resolve_configure_secrets(&candidate.environment, host_env)?;
    reject_caller_secrets(&candidate.configure)?;
    let mut configure = candidate.configure.clone();
    for (binding, value) in secrets {
        configure.secrets.insert(binding, value);
    }
    let redactor = env.redactor();
    Ok(PreparedEnvironment {
        env,
        configure,
        redactor,
    })
}

/// The supervisor is the sole `Configure`-secret resolver: reject any
/// caller-supplied secret.
fn reject_caller_secrets(configure: &ConfigurePayload) -> Result<(), EnvironmentError> {
    if let Some((binding, _)) = configure.secrets.first_key_value() {
        return Err(EnvironmentError::UndeclaredConfigureSecret {
            binding: binding.to_string(),
        });
    }
    Ok(())
}

/// The atomic candidate startup boundary.
///
/// On success, [`Self::Started`] returns a supervisor owning every ready process
/// plus a separate typed publication snapshot. On failure,
/// [`Self::Failed`] returns only typed failure and rollback evidence — no
/// handles and no publication.
pub enum PersistentStartupResult {
    /// Every required candidate reached `ready`; publication is atomic.
    Started {
        /// The supervisor owning every ready candidate process.
        supervisor: PersistentSupervisor,
        /// A separate, data-only publication snapshot.
        publication: PersistentPublication,
    },
    /// At least one candidate failed; every started candidate was reaped.
    Failed(PersistentStartupFailure),
}

impl fmt::Debug for PersistentStartupResult {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Started { .. } => formatter
                .debug_struct("PersistentStartupResult::Started")
                .finish_non_exhaustive(),
            Self::Failed(failure) => formatter
                .debug_tuple("PersistentStartupResult::Failed")
                .field(failure)
                .finish(),
        }
    }
}

/// Typed evidence returned when candidate startup fails.
#[derive(Debug, Clone)]
pub struct PersistentStartupFailure {
    /// Which candidate/phase failed.
    pub failure: StartupFailure,
    /// Every previously started and the failing candidate, in start order, each
    /// with evidence of whether it was reaped.
    pub rollback: Vec<ReapedCandidate>,
}

/// Why persistent startup failed.
#[derive(Debug, Clone)]
pub enum StartupFailure {
    /// One candidate failed a spawn/handshake/protocol/capability/timeout phase.
    Candidate(CandidateFailure),
    /// Two candidates shared a plugin id; rejected before any spawn.
    DuplicatePluginId {
        /// The repeated plugin id.
        plugin_id: Id,
    },
}

impl StartupFailure {
    /// The stable operator code. A duplicate plugin id is a closed-protocol
    /// contract violation (`PLG-E502`); a candidate failure delegates to its
    /// underlying supervisor failure.
    #[must_use]
    pub fn code(&self) -> &'static str {
        match self {
            Self::Candidate(candidate) => candidate.code(),
            Self::DuplicatePluginId { .. } => error::PROTOCOL_FAILURE_CODE,
        }
    }
}

/// One candidate's startup failure, with the phase that failed.
#[derive(Debug, Clone)]
pub struct CandidateFailure {
    /// The plugin id of the failing candidate.
    pub plugin_id: Id,
    /// The lifecycle phase that failed.
    pub phase: PersistentPhase,
    /// The typed supervisor failure.
    pub failure: SupervisorFailure,
}

impl CandidateFailure {
    /// The stable operator code, delegated to the underlying failure.
    #[must_use]
    pub fn code(&self) -> &'static str {
        self.failure.code()
    }
}

/// One persistent candidate startup phase.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PersistentPhase {
    /// Process spawn or environment construction.
    Spawn,
    /// Awaiting `hello-ack`.
    HelloAck,
    /// Sending/awaiting `configure`.
    Configure,
    /// Awaiting `ready`.
    Ready,
    /// `ready` capabilities were not a subset of the manifest declaration.
    Capability,
}

/// Evidence that one candidate was reaped during rollback.
///
/// A clean tree cleanup requires the leader reaped **and** a bounded stdout
/// EOF/disconnection **and** a bounded stderr completion; otherwise
/// [`Self::cleanup_failure`] carries the typed reason (analogous to the
/// one-shot `CleanupFailure`). No cleanup evidence is discarded: the final
/// stdout/stderr drain outcomes are composed into this single typed value.
#[derive(Debug, Clone)]
pub struct ReapedCandidate {
    /// The plugin id of the reaped candidate.
    pub plugin_id: Id,
    /// Whether the candidate's tree cleanup was **clean**: leader reaped **and**
    /// stdout reached EOF **and** stderr completed within the bound. A lingering
    /// descendant that holds an inherited pipe makes this `false`.
    pub reaped: bool,
    /// Typed cleanup evidence. `None` only when the leader reaped, stdout
    /// reached EOF, and stderr completed within the bound. A lingering
    /// descendant holding the pipes surfaces as `DrainTimeout`, not a clean reap.
    pub cleanup_failure: Option<CleanupFailure>,
}

/// A data-only publication snapshot: every ready candidate, in plugin-id order.
#[derive(Debug, Clone)]
pub struct PersistentPublication {
    candidates: Vec<ReadyCandidate>,
}

impl PersistentPublication {
    /// The ready candidates, in canonical plugin-id order.
    #[must_use]
    pub fn ready(&self) -> &[ReadyCandidate] {
        &self.candidates
    }
}

/// One ready candidate in the publication snapshot.
#[derive(Debug, Clone)]
pub struct ReadyCandidate {
    /// The plugin package id.
    pub plugin_id: Id,
    /// The plugin package version.
    pub plugin_version: CanonicalSemver,
    /// The `ready` capabilities, a subset of the manifest declaration.
    pub capabilities: Vec<Capability>,
}

/// The typed health of one candidate process.
///
/// Health probes fail fast: a `try_wait` OS error is never reported `Ready`,
/// and unexpected stdout data after `Ready` (a protocol fault) marks the
/// candidate unavailable without an exit and triggers no restart. No variant
/// carries provider text, so no secret can leak through it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CandidateHealth {
    /// The process is still `ready`.
    Ready {
        /// The ready capabilities.
        capabilities: Vec<Capability>,
    },
    /// The process exited on its own (no auto-restart follows).
    Exited {
        /// The observed exit code, if any.
        exit_code: Option<i32>,
    },
    /// The process state could not be probed (`try_wait` returned an OS error).
    /// Never reported `Ready`: the candidate is unavailable.
    ProbeFailed {
        /// The OS error reported by `try_wait` (an errno string, never provider text).
        error: String,
    },
    /// The candidate emitted unexpected stdout after `Ready` (protocol fault).
    ProtocolFault {
        /// Typed evidence of the illegal stdout; never carries provider text.
        evidence: IllegalStdout,
    },
}

/// Why a ready candidate's stdout was illegal during a health probe. Carries no
/// provider text, so it cannot leak a secret.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum IllegalStdout {
    /// An unexpected protocol frame arrived after `Ready`.
    Frame,
    /// An oversize line or non-frame fault arrived after `Ready`.
    Fault,
    /// stdout closed while the process was still alive.
    Closed,
}

/// One candidate health observation.
#[derive(Debug, Clone)]
pub struct CandidateHealthSnapshot {
    /// The plugin id.
    pub plugin_id: Id,
    /// The current health.
    pub health: CandidateHealth,
}

/// One candidate's explicit-shutdown evidence.
///
/// A clean tree cleanup requires the leader reaped **and** a bounded stdout EOF
/// **and** a bounded stderr completion; otherwise [`Self::cleanup_failure`]
/// carries the typed reason.
#[derive(Debug, Clone)]
pub struct CandidateShutdown {
    /// The plugin id.
    pub plugin_id: Id,
    /// Whether the candidate's tree cleanup was **clean**: leader reaped **and**
    /// stdout reached EOF **and** stderr completed within the bound. A lingering
    /// descendant that holds an inherited pipe makes this `false`.
    pub process_reaped: bool,
    /// Typed cleanup evidence, `None` only for a clean tree cleanup.
    pub cleanup_failure: Option<CleanupFailure>,
}

// ---------------------------------------------------------------------------
// Public entry point
// ---------------------------------------------------------------------------

/// Start every persistent candidate in deterministic plugin-id order.
///
/// Every candidate's environment and `Configure` secrets are resolved first,
/// in plugin-id order, before any process spawns, so a resolution defect in
/// any candidate prevents every spawn. Each candidate then performs
/// `hello`/`hello-ack`/`configure`/`ready` with per-stage handshake bounds,
/// and its `ready` capabilities must be a subset of the manifest-declared
/// capabilities. Every required candidate must reach `ready` before one
/// atomic publication. On any failure, every previously started and the
/// failing candidate is stopped/reaped and no publication is returned. There
/// is no auto-restart.
///
/// Duplicate plugin ids are rejected before any spawn.
pub fn run_persistent_startup<E: HostEnv>(
    startup: &PersistentStartup,
    bounds: &SupervisorBounds,
    host_env: &E,
) -> PersistentStartupResult {
    if let Some(plugin_id) = duplicate_plugin_id(startup.candidates.iter()) {
        return PersistentStartupResult::Failed(PersistentStartupFailure {
            failure: StartupFailure::DuplicatePluginId { plugin_id },
            rollback: Vec::new(),
        });
    }
    match prepare_pairs(&startup.candidates, host_env) {
        Ok(pairs) => run_prepared_startup(pairs, bounds),
        Err(failure) => PersistentStartupResult::Failed(failure),
    }
}

/// Resolve every candidate's launch inputs in deterministic plugin-id order,
/// before any spawn. Returns candidate/prepared pairs in plugin-id order.
fn prepare_pairs<E: HostEnv>(
    candidates: &[PersistentCandidate],
    host_env: &E,
) -> Result<Vec<(PersistentCandidate, PreparedEnvironment)>, PersistentStartupFailure> {
    let mut pairs = Vec::with_capacity(candidates.len());
    for index in startup_order(candidates) {
        match prepare_candidate_environment(&candidates[index], host_env) {
            Ok(environment) => pairs.push((candidates[index].clone(), environment)),
            Err(error) => {
                return Err(PersistentStartupFailure {
                    failure: StartupFailure::Candidate(CandidateFailure {
                        plugin_id: candidates[index].plugin_id.clone(),
                        phase: PersistentPhase::Spawn,
                        failure: SupervisorFailure::Environment(error),
                    }),
                    rollback: Vec::new(),
                });
            }
        }
    }
    Ok(pairs)
}

/// Start every already-prepared candidate in deterministic plugin-id order
/// and publish atomically (issue #704 S2).
///
/// Each pair carries the candidate and its [`PreparedEnvironment`] resolved
/// exactly once; spawn consumes the prepared values and never re-reads a
/// mutable host input. Duplicate plugin ids are rejected before any spawn.
pub(crate) fn run_prepared_startup(
    pairs: Vec<(PersistentCandidate, PreparedEnvironment)>,
    bounds: &SupervisorBounds,
) -> PersistentStartupResult {
    let candidates: Vec<&PersistentCandidate> =
        pairs.iter().map(|(candidate, _)| candidate).collect();
    if let Some(plugin_id) = duplicate_plugin_id(candidates) {
        return PersistentStartupResult::Failed(PersistentStartupFailure {
            failure: StartupFailure::DuplicatePluginId { plugin_id },
            rollback: Vec::new(),
        });
    }
    let mut ordered = pairs;
    ordered.sort_by(|left, right| left.0.plugin_id.as_str().cmp(right.0.plugin_id.as_str()));
    let mut owned: Vec<OwnedCandidate> = Vec::new();
    let mut ready: Vec<ReadyCandidate> = Vec::new();
    for (candidate, environment) in ordered {
        match start_prepared(&candidate, environment, bounds) {
            StartOutcome::Started(candidate, publication) => {
                owned.push(*candidate);
                ready.push(publication);
            }
            StartOutcome::Failed { failure, reaped } => {
                return rollback_failure(owned, bounds, failure, reaped);
            }
        }
    }
    publish(owned, ready, *bounds)
}

/// Reap every previously started candidate in start order, then append the
/// failing candidate's reap evidence.
fn rollback_failure(
    owned: Vec<OwnedCandidate>,
    bounds: &SupervisorBounds,
    failure: CandidateFailure,
    reaped: Option<ReapedCandidate>,
) -> PersistentStartupResult {
    let mut rollback: Vec<ReapedCandidate> = owned
        .into_iter()
        .map(|candidate| reap_owned(candidate, bounds))
        .collect();
    if let Some(failing) = reaped {
        rollback.push(failing);
    }
    PersistentStartupResult::Failed(PersistentStartupFailure {
        failure: StartupFailure::Candidate(failure),
        rollback,
    })
}

/// Build the atomic publication and the supervisor owning every ready process.
fn publish(
    owned: Vec<OwnedCandidate>,
    ready: Vec<ReadyCandidate>,
    bounds: SupervisorBounds,
) -> PersistentStartupResult {
    let publication = PersistentPublication { candidates: ready };
    let snapshot = publication.clone();
    let supervisor = PersistentSupervisor {
        candidates: owned,
        bounds,
        shut_down: false,
        publication,
    };
    PersistentStartupResult::Started {
        supervisor,
        publication: snapshot,
    }
}

// ---------------------------------------------------------------------------
// Pure ordering and capability helpers
// ---------------------------------------------------------------------------

/// The first repeated plugin id, if any. Checked before any spawn.
pub(super) fn duplicate_plugin_id<'a>(
    candidates: impl IntoIterator<Item = &'a PersistentCandidate>,
) -> Option<Id> {
    let mut seen: BTreeSet<Id> = BTreeSet::new();
    for candidate in candidates {
        if !seen.insert(candidate.plugin_id.clone()) {
            return Some(candidate.plugin_id.clone());
        }
    }
    None
}

/// Candidate indices in canonical plugin-id text order.
pub(super) fn startup_order(candidates: &[PersistentCandidate]) -> Vec<usize> {
    let mut indices: Vec<usize> = (0..candidates.len()).collect();
    indices.sort_by(|&left, &right| {
        candidates[left]
            .plugin_id
            .as_str()
            .cmp(candidates[right].plugin_id.as_str())
    });
    indices
}

/// The first ready capability the manifest did not declare, if any.
pub(super) fn first_undeclared_capability(
    declared: &[Capability],
    ready: &[Capability],
) -> Option<Capability> {
    ready
        .iter()
        .copied()
        .find(|capability| !declared.contains(capability))
}

/// The typed protocol failure for an undeclared ready capability.
pub(super) fn capability_mismatch_failure(capability: Capability) -> SupervisorFailure {
    SupervisorFailure::Protocol(error::ProviderError::InvalidValue {
        path: "ready.capabilities".to_owned(),
        reason: format!(
            "provider reported capability {} not declared by the manifest",
            capability.as_str()
        ),
    })
}

// ---------------------------------------------------------------------------
// Supervisor: process ownership, health, shutdown, Drop
// ---------------------------------------------------------------------------

/// The sole owner of every ready persistent candidate process.
///
/// No `Child`, pipe, or thread handle leaves this type. It exposes only typed
/// readiness, health, publication, and shutdown values. [`Drop`] performs a
/// bounded staged reap of any still-owned candidates so a supervisor that is
/// dropped without an explicit [`Self::shutdown`] cannot orphan a process.
pub struct PersistentSupervisor {
    candidates: Vec<OwnedCandidate>,
    bounds: SupervisorBounds,
    shut_down: bool,
    publication: PersistentPublication,
}

impl fmt::Debug for PersistentSupervisor {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("PersistentSupervisor")
            .field("candidates", &self.candidates.len())
            .field("shut_down", &self.shut_down)
            .finish_non_exhaustive()
    }
}

impl PersistentSupervisor {
    /// The atomic publication snapshot.
    #[must_use]
    pub fn publication(&self) -> &PersistentPublication {
        &self.publication
    }

    /// The number of owned candidate processes.
    #[must_use]
    pub fn candidate_count(&self) -> usize {
        self.candidates.len()
    }

    /// Consume the supervisor, moving each ready candidate into a dedicated
    /// command-owner thread that drives repeated same-PID invocations.
    ///
    /// The supervisor is fully consumed: its candidates become owned by the
    /// returned [`PersistentSessionOwner`], and the supervisor's `Drop` is a
    /// no-op afterward. If an owner cannot start, every transferred and pending
    /// candidate is reaped before the typed error returns. No `Child`, pipe, or
    /// thread handle enters `AppState` — the session owner stays at the runtime
    /// boundary.
    pub fn into_sessions(
        mut self,
    ) -> Result<
        super::persistent_session::PersistentSessionOwner,
        super::persistent_session::PersistentOwnerStartFailure,
    > {
        self.shut_down = true;
        let candidates = std::mem::take(&mut self.candidates);
        super::persistent_session::PersistentSessionOwner::from_candidates(candidates, self.bounds)
    }

    /// Observe each candidate's health. A `ready` process that has since exited
    /// is reported as [`CandidateHealth::Exited`]; no auto-restart follows.
    pub fn health(&mut self) -> Vec<CandidateHealthSnapshot> {
        self.candidates
            .iter_mut()
            .map(|candidate| CandidateHealthSnapshot {
                plugin_id: candidate.plugin_id.clone(),
                health: candidate_health(candidate),
            })
            .collect()
    }

    /// Explicitly shut down every candidate using the staged process-tree/drain
    /// mechanics. Returns reap evidence per candidate. Idempotent.
    pub fn shutdown(&mut self) -> Vec<CandidateShutdown> {
        if self.shut_down {
            return Vec::new();
        }
        self.shut_down = true;
        let candidates = std::mem::take(&mut self.candidates);
        reap_all(candidates, &self.bounds)
    }
}

impl Drop for PersistentSupervisor {
    fn drop(&mut self) {
        if self.shut_down {
            return;
        }
        let candidates = std::mem::take(&mut self.candidates);
        // Drop cannot return evidence, so invoke the bounded staged reap
        // explicitly (never silenced) so a dropped supervisor cannot orphan a
        // candidate process. Each candidate is reaped within the shutdown bounds.
        reap_all(candidates, &self.bounds);
    }
}

/// The owned process and its drains.
pub(super) struct OwnedCandidate {
    pub(super) plugin_id: Id,
    pub(super) capabilities: Vec<Capability>,
    pub(super) process: ProviderProcess,
    pub(super) stdin: Option<ChildStdin>,
    pub(super) stdout_drain: StdoutDrain,
    pub(super) stderr_drain: StderrDrain,
    pub(super) pid: u32,
    pub(super) request_id: RequestId,
    pub(super) generation: u64,
    pub(super) exited: bool,
    /// Scrubs every resolved secret value out of cleanup evidence collected
    /// during this candidate's shutdown/rollback reap.
    pub(super) redactor: Redactor,
    /// The live lifecycle validator, advanced to `ready`. A healthy shutdown
    /// advances it through the outbound `shutdown` and observes the ack.
    pub(super) lifecycle: LifecycleOrder,
    /// Whether the candidate reached `ready` without faulting. Only a healthy
    /// candidate strictly validates the shutdown-ack; an unhealthy one is
    /// best-effort reaped.
    pub(super) healthy: bool,
    /// A sticky protocol fault observed after `ready`. Once illegal stdout is
    /// seen, the candidate remains faulted until it is reaped, even after the
    /// offending bytes are drained from the channel.
    pub(super) fault: Option<IllegalStdout>,
}

/// Probe one candidate's process and update its exited/health flags. Fails fast:
/// illegal stdout after `Ready` is a protocol fault, and a `try_wait` OS error
/// is `ProbeFailed`, never `Ready`.
///
/// A protocol fault is **sticky**: once illegal stdout is observed the candidate
/// is marked faulted until it is reaped, even after the offending bytes are
/// drained from the channel, so repeated probes cannot flip it back to `Ready`.
pub(super) fn candidate_health(candidate: &mut OwnedCandidate) -> CandidateHealth {
    if let Some(evidence) = candidate.fault.clone() {
        return CandidateHealth::ProtocolFault { evidence };
    }
    let probe = probe_stdout(&candidate.stdout_drain.receiver);
    let wait = candidate.process.try_wait();
    let health = classify_health(probe, wait, &candidate.capabilities);
    match &health {
        CandidateHealth::Exited { .. } => candidate.exited = true,
        CandidateHealth::ProbeFailed { .. } => candidate.healthy = false,
        CandidateHealth::ProtocolFault { evidence } => {
            candidate.healthy = false;
            candidate.fault = Some(evidence.clone());
        }
        CandidateHealth::Ready { .. } => {}
    }
    health
}

/// One non-blocking stdout probe outcome during a health check.
pub(super) enum StdoutProbe {
    /// No data is available.
    Idle,
    /// An unexpected frame or non-frame fault arrived after `Ready`.
    Illegal(IllegalStdout),
    /// The stdout channel disconnected (the drain ended).
    Closed,
}

/// Non-blockingly probe the candidate's stdout channel for illegal post-`Ready`
/// data. A healthy `ready` provider emits nothing; any frame/fault is a protocol
/// violation, and a closed channel while the process is alive is also illegal.
pub(super) fn probe_stdout(receiver: &mpsc::Receiver<StdoutEvent>) -> StdoutProbe {
    match receiver.try_recv() {
        Ok(StdoutEvent::Frame(_)) => StdoutProbe::Illegal(IllegalStdout::Frame),
        Ok(StdoutEvent::Oversize(_) | StdoutEvent::ReadError) => {
            StdoutProbe::Illegal(IllegalStdout::Fault)
        }
        Err(mpsc::TryRecvError::Empty) => StdoutProbe::Idle,
        Err(mpsc::TryRecvError::Disconnected) => StdoutProbe::Closed,
    }
}

/// Whether a candidate's process group may still be signalled by its pid.
///
/// Only while the leader is unreaped. Once it has been waited on, the pid is
/// the kernel's to reuse and `-pid` names someone else's process group
/// (issue #390).
pub(super) const fn may_signal_group(leader_already_reaped: bool) -> bool {
    !leader_already_reaped
}

/// Classify one candidate's health from its stdout probe and `try_wait` result.
///
/// Explicit priority cascade so a normally-exited process whose stdout channel
/// has disconnected is `Exited`, not a closed-while-alive fault: illegal stdout
/// after `Ready` wins first (fail-fast); a `try_wait` OS error is `ProbeFailed`;
/// a process that has exited wins over a closed stdout channel; only a
/// still-alive process with a closed channel broke the closed protocol.
pub(super) fn classify_health(
    stdout_probe: StdoutProbe,
    wait_result: io::Result<Option<ExitStatus>>,
    capabilities: &[Capability],
) -> CandidateHealth {
    if let StdoutProbe::Illegal(evidence) = stdout_probe {
        return CandidateHealth::ProtocolFault { evidence };
    }
    // A process exit (Ok(Some)) wins over a normally-closed stdout channel; the
    // closed channel is only a fault when the process is still alive (Ok(None)).
    match wait_result {
        Ok(Some(status)) => CandidateHealth::Exited {
            exit_code: status.code(),
        },
        Err(error) => CandidateHealth::ProbeFailed {
            error: error.to_string(),
        },
        Ok(None) => match stdout_probe {
            StdoutProbe::Closed => CandidateHealth::ProtocolFault {
                evidence: IllegalStdout::Closed,
            },
            StdoutProbe::Idle => CandidateHealth::Ready {
                capabilities: capabilities.to_vec(),
            },
            StdoutProbe::Illegal(_) => {
                unreachable!("illegal stdout is classified as a protocol fault above")
            }
        },
    }
}

/// Reap every candidate in start order, returning shutdown evidence that
/// carries the composed cleanup failure.
fn reap_all(candidates: Vec<OwnedCandidate>, bounds: &SupervisorBounds) -> Vec<CandidateShutdown> {
    candidates
        .into_iter()
        .map(|candidate| {
            let reaped = reap_owned(candidate, bounds);
            CandidateShutdown {
                plugin_id: reaped.plugin_id,
                process_reaped: reaped.reaped,
                cleanup_failure: reaped.cleanup_failure,
            }
        })
        .collect()
}

/// Reap one owned candidate using the staged process-tree/drain mechanics, and
/// compose typed cleanup evidence from the independent reap, ack, stdout, and
/// stderr signals. A clean cleanup requires the leader reaped and both pipes
/// closed; a healthy candidate strictly validates the `shutdown-ack`. A process
/// already observed reaped by [`PersistentSupervisor::health`] is not short-cut:
/// the group is still force-killed so descendants holding inherited pipes are
/// terminated and their closure observed within the bound.
pub(super) fn reap_owned(mut owned: OwnedCandidate, bounds: &SupervisorBounds) -> ReapedCandidate {
    let plugin_id = owned.plugin_id.clone();
    let redactor = owned.redactor.clone();
    // Snapshot descendants before the shutdown frame gives a provider the
    // opportunity to exit and orphan an escaped process-group member.
    let descendant_observation_error = owned.process.observe_descendants().err();

    // A self-terminated candidate needs no shutdown handshake; only a still-
    // healthy candidate strictly validates the shutdown-ack.
    let ack_failure = if owned.exited {
        None
    } else {
        observe_healthy_shutdown(&mut owned, bounds)
    };

    // Escalate/reap the leader. Both branches collect terminate/force-kill
    // errors (a benign ESRCH is filtered by `signal_cleanup_evidence`).
    let (leader_reaped, mut signal_errors): (bool, Vec<io::Error>) =
        if may_signal_group(owned.exited) {
            let (outcome, errors) =
                staged_shutdown(&mut owned.process, owned.stdin.take(), bounds, owned.pid);
            (matches!(outcome, ShutdownOutcome::Exited(_)), errors)
        } else {
            // The leader PID may have been recycled, so its old process group is
            // never named. Exact descendants captured while parentage was live
            // remain owned cleanup targets.
            owned.stdin.take();
            let errors = owned
                .process
                .force_kill_descendants()
                .err()
                .into_iter()
                .collect();
            (true, errors)
        };
    if let Some(error) = descendant_observation_error {
        signal_errors.push(error);
    }

    let stdout_final = final_stdout_drain(&owned.stdout_drain.receiver, bounds.final_drain);
    let (_retained, _truncated, stderr_timed_out) =
        collect_retained_stderr(owned.stderr_drain, bounds.final_drain);
    let raw_failure =
        compose_cleanup_failure(leader_reaped, ack_failure, stdout_final, stderr_timed_out);
    // A signal error is recorded only when the reap and drains are otherwise
    // clean, so a benign ESRCH never dirties a clean cleanup.
    let signal_evidence = signal_cleanup_evidence(&signal_errors);
    let reaped = raw_failure.is_none() && signal_evidence.is_none();
    let cleanup_failure = raw_failure
        .or_else(|| signal_evidence.map(CleanupFailure::Io))
        .map(|failure| redaction::redact_cleanup_failure(failure, &redactor));
    ReapedCandidate {
        plugin_id,
        reaped,
        cleanup_failure,
    }
}

/// Send a best-effort `shutdown` frame and, for a healthy candidate, strictly
/// observe the `shutdown-ack`. Returns a cleanup failure if the frame could not
/// be written/flushed, or if a healthy candidate's ack is wrong/missing/
/// malformed/out-of-order or preceded by EOF.
fn observe_healthy_shutdown(
    owned: &mut OwnedCandidate,
    bounds: &SupervisorBounds,
) -> Option<CleanupFailure> {
    let write_result =
        send_shutdown_frame(&owned.request_id, owned.generation, owned.stdin.as_mut());
    // An unhealthy candidate may already have crashed, so its signal is best-
    // effort: a write failure is expected and the bounded reap is authoritative.
    if !owned.healthy {
        return None;
    }
    // A healthy candidate was expected alive; a write/flush failure (it closed
    // its stdin or exited before the host signalled) is typed I/O evidence.
    if let Err(error) = write_result {
        return Some(CleanupFailure::Io(format!(
            "shutdown frame write failed: {error}"
        )));
    }
    if let Err(error) = owned
        .lifecycle
        .observe(MessageKind::Shutdown, owned.generation)
    {
        return Some(CleanupFailure::ShutdownAck(error));
    }
    driver::await_shutdown_ack(
        &owned.stdout_drain,
        &mut owned.lifecycle,
        bounds.shutdown_ack,
    )
}

/// Write the graceful `shutdown` frame before the staged reap, returning a typed
/// error if the write or flush failed (the provider closed its stdin or exited).
fn send_shutdown_frame(
    request_id: &RequestId,
    generation: u64,
    stdin: Option<&mut ChildStdin>,
) -> io::Result<()> {
    let Some(stdin) = stdin else { return Ok(()) };
    let frame = encode_shutdown(request_id, generation, ShutdownReason::HostExit);
    stdin.write_all(&frame)?;
    stdin.flush()?;
    Ok(())
}

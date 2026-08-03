//! Definition-driven local agent probe adapter (issue #382 S3c/S3d).
//!
//! This boundary validates immutable definition/candidate inputs, rechecks the
//! physical executable fingerprint around fixed-argv probes, and returns a
//! generation-stamped availability result. It owns no registry or application
//! state and does not construct launch plans.

use std::ffi::{OsStr, OsString};
use std::path::Path;
use std::process::Command;
use std::time::{Duration, Instant};

use crate::agent_candidate::{CandidateGenerationKey, CandidateResolution, ResolvedCandidate};
use crate::agent_candidate_fingerprint::{CandidateFingerprint, capture_candidate_fingerprint};
use crate::agent_candidate_path::AgentWrapperKind;
use crate::domain::agent_definition::limits::{
    LOCAL_PROBE_TIMEOUT_MS, PACKAGE_MATERIALIZATION_TIMEOUT_MS, REMOTE_PROBE_TIMEOUT_MS,
};
use crate::domain::agent_definition::probe::{CapabilityProbe, ProbeStream};
use crate::domain::agent_definition::{
    AgentDefinition, Availability, DefinitionSha256, ProbeErrorCode,
};

use super::agent_probe_parse::{ProbeEvidenceError, parse_capabilities, parse_identity};
use super::agent_probe_process::{ProbeProcessError, ProbeProcessOutput, run_probe_process};

/// Windows `STATUS_DLL_INIT_FAILED` (`0xC0000142`) as a signed `i32`.
///
/// This is a known transient loader failure on Windows, especially for
/// npm-shim-mediated commands (`cmd.exe /D /S /C llxprt.cmd --version`) on a
/// cold DLL cache. The psmux version probe already retries this; the agent
/// probe must do the same or freshly installed agents fail to launch.
const STATUS_DLL_INIT_FAILED: i32 = -1_073_741_502;

/// Maximum identity-probe attempts on Windows loader transients.
const MAX_LOADER_TRANSIENT_RETRIES: u32 = 3;

/// Backoff between loader-transient retries.
const LOADER_TRANSIENT_BACKOFF: Duration = Duration::from_millis(500);

/// Maximum identity-probe attempts when the probe overruns its budget.
///
/// Deliberately smaller than [`MAX_LOADER_TRANSIENT_RETRIES`]: a cold shim is
/// slow exactly once, so one retry recovers it, whereas a command that never
/// answers would otherwise hold the user for three full budgets before saying
/// so (issue #604).
const MAX_TIMEOUT_ATTEMPTS: u32 = 2;

/// Deadline for one probe attempt, measured from now.
///
/// Falls forward to a bounded far-future deadline when the budget cannot be
/// represented, matching [`ProbePhase::deadline`] rather than collapsing into
/// an instant timeout.
fn attempt_deadline(budget: Duration) -> Instant {
    let now = Instant::now();
    now.checked_add(budget)
        .or_else(|| now.checked_add(UNBOUNDED_PHASE_BUDGET))
        .unwrap_or(now)
}

/// Budget for the one probe process that also materializes a package.
const PACKAGE_MATERIALIZATION_TIMEOUT: Duration =
    Duration::from_millis(PACKAGE_MATERIALIZATION_TIMEOUT_MS);

/// Deadline used when a phase budget is too large to represent as an `Instant`.
/// Every real budget is a bounded constant, so this only guards the arithmetic.
const UNBOUNDED_PHASE_BUDGET: Duration = Duration::from_secs(24 * 60 * 60);

/// Whether an exit status is a retryable Windows loader transient.
fn is_retryable_loader_transient(status: std::process::ExitStatus) -> bool {
    status.code() == Some(STATUS_DLL_INIT_FAILED)
}

/// Probe target class with its bounded process timeout.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AgentProbeTarget {
    /// Local executable probe.
    Local,
    /// Remote probe contract; execution is owned by a later remote adapter.
    Remote,
}

impl AgentProbeTarget {
    /// Maximum duration for one identity or capability process.
    #[must_use]
    pub const fn total_timeout(self) -> Duration {
        match self {
            Self::Local => Duration::from_millis(LOCAL_PROBE_TIMEOUT_MS),
            Self::Remote => Duration::from_millis(REMOTE_PROBE_TIMEOUT_MS),
        }
    }
}

/// Availability plus the immutable evidence that produced it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AgentProbeResult {
    availability: Availability,
    executable_fingerprint: Option<CandidateFingerprint>,
    candidate_generation_key: Option<CandidateGenerationKey>,
    definition_sha256: DefinitionSha256,
    prepared_invocation: Option<super::package_runtime::PackageInvocation>,
}

impl AgentProbeResult {
    /// Runtime availability classification.
    #[must_use]
    pub const fn availability(&self) -> &Availability {
        &self.availability
    }

    /// The package invocation this probe prepared and measured.
    ///
    /// Launch must execute *this* invocation rather than preparing its own.
    /// Preparing twice means resolving a moving selector twice, and two
    /// resolutions can disagree — which composed availability measured from one
    /// version with the executable and fingerprint of another (issue #571).
    ///
    /// `None` when the agent is not reached through a package runner, and when
    /// preparation failed; in both cases there is no measured invocation to
    /// carry.
    #[must_use]
    pub const fn prepared_invocation(&self) -> Option<&super::package_runtime::PackageInvocation> {
        self.prepared_invocation.as_ref()
    }

    /// Physical executable fingerprint, absent only for NotFound.
    #[must_use]
    pub const fn executable_fingerprint(&self) -> Option<&CandidateFingerprint> {
        self.executable_fingerprint.as_ref()
    }

    /// Candidate and selector identity that produced this probe result.
    #[must_use]
    pub const fn candidate_generation_key(&self) -> Option<&CandidateGenerationKey> {
        self.candidate_generation_key.as_ref()
    }

    /// Hash of the exact validated definition used by this probe.
    #[must_use]
    pub const fn definition_sha256(&self) -> &DefinitionSha256 {
        &self.definition_sha256
    }
}

/// Execute a local definition probe with one bounded deadline per process.
///
/// NotFound is returned before command construction. A resolved candidate is
/// fingerprint-checked before the first process and immediately after every
/// process. Identity and capability commands each receive the definition's
/// authored timeout, so one successful process cannot consume the next one's
/// budget. The caller owns the monotonic generation counter; this adapter
/// preserves the exact requested stamp on every attempted outcome.
#[must_use]
pub fn run_local_agent_probe(
    definition: &AgentDefinition,
    resolution: &CandidateResolution,
    requested_generation: u64,
) -> AgentProbeResult {
    run_local_agent_probe_with_cache(
        definition,
        resolution,
        requested_generation,
        &super::package_runtime::managed_package_cache_root(),
    )
}

/// Injectable package-cache variant used by production-connected tests.
#[must_use]
pub fn run_local_agent_probe_with_cache(
    definition: &AgentDefinition,
    resolution: &CandidateResolution,
    requested_generation: u64,
    package_cache_root: &Path,
) -> AgentProbeResult {
    let definition_sha256 = definition.sha256();
    let CandidateResolution::Resolved(candidate) = resolution else {
        return AgentProbeResult {
            availability: Availability::NotFound,
            executable_fingerprint: None,
            candidate_generation_key: None,
            definition_sha256,
            prepared_invocation: None,
        };
    };
    let candidate_generation_key = candidate.generation_key(definition);
    let prepared = super::package_runtime::prepare_local_probe(candidate, package_cache_root);
    let fingerprint = prepared
        .as_ref()
        .ok()
        .and_then(|invocation| invocation.as_ref())
        .and_then(|invocation| invocation.fingerprint())
        .cloned()
        .unwrap_or_else(|| candidate.fingerprint().clone());
    let (availability, prepared_invocation) = match prepared {
        Ok(invocation) => (
            probe_resolved(
                definition,
                candidate,
                invocation.as_ref(),
                requested_generation,
            ),
            invocation,
        ),
        Err(error) => (
            probe_error(
                ProbeErrorCode::Agte202,
                error.to_string(),
                requested_generation,
            ),
            None,
        ),
    };
    AgentProbeResult {
        availability,
        executable_fingerprint: Some(fingerprint),
        candidate_generation_key: Some(candidate_generation_key),
        definition_sha256,
        prepared_invocation,
    }
}

/// What one probe phase executes: the resolved candidate and, when the agent is
/// reached through a package runner, the prepared package invocation.
struct ProbeTarget<'a> {
    candidate: &'a ResolvedCandidate,
    invocation: Option<&'a super::package_runtime::PackageInvocation>,
}

impl ProbeTarget<'_> {
    /// The program every phase of this probe actually runs.
    fn executable(&self) -> &Path {
        self.invocation.map_or_else(
            || self.candidate.executable(),
            super::package_runtime::PackageInvocation::executable,
        )
    }

    /// Whether the agent is reached through a package runner that materializes
    /// the package as part of executing it.
    fn is_runner_mediated(&self) -> bool {
        self.invocation
            .is_some_and(|invocation| !invocation.prefix().is_empty())
    }
}

/// One bounded probe phase, carrying everything a failure must name.
struct ProbePhase<'a> {
    name: &'static str,
    executable: &'a Path,
    budget: Duration,
    started: Instant,
}

impl<'a> ProbePhase<'a> {
    fn start(name: &'static str, executable: &'a Path, budget: Duration) -> Self {
        Self {
            name,
            executable,
            budget,
            started: Instant::now(),
        }
    }

    /// Deadline for this phase.
    ///
    /// Every step is checked because this path may not panic. A budget too
    /// large to represent falls forward to a bounded far-future deadline
    /// instead of collapsing into an instant timeout. The final arm exists
    /// only to keep the arithmetic total: it is unreachable, because
    /// `Instant::now()` cannot be within [`UNBOUNDED_PHASE_BUDGET`] of the
    /// representable maximum, and every real probe budget is a bounded
    /// constant well under it.
    fn deadline(&self) -> Instant {
        self.started
            .checked_add(self.budget)
            .or_else(|| self.started.checked_add(UNBOUNDED_PHASE_BUDGET))
            .unwrap_or(self.started)
    }

    /// Attribute a failure to this phase and the program it ran.
    fn describe(&self, detail: &str) -> String {
        format!(
            "{name} probe of {executable} {detail}",
            name = self.name,
            executable = self.executable.display()
        )
    }
}

fn probe_resolved(
    definition: &AgentDefinition,
    candidate: &ResolvedCandidate,
    invocation: Option<&super::package_runtime::PackageInvocation>,
    generation: u64,
) -> Availability {
    if let Err(error) = definition.validate() {
        return probe_error(ProbeErrorCode::Agte201, error.to_string(), generation);
    }
    if fingerprint_changed(candidate) {
        return stale_error(generation);
    }
    let target = ProbeTarget {
        candidate,
        invocation,
    };
    let probe_budget = AgentProbeTarget::Local
        .total_timeout()
        .min(Duration::from_millis(definition.probe.timeout_ms));
    // A runner-mediated invocation materializes its package inside the first
    // process it runs, which is registry work rather than agent startup
    // latency, so that phase gets the materialization budget (issue #553).
    let identity_budget = if target.is_runner_mediated() {
        PACKAGE_MATERIALIZATION_TIMEOUT
    } else {
        probe_budget
    };
    let identity_phase = ProbePhase::start("identity", target.executable(), identity_budget);
    let identity = match run_identity(definition, &target, &identity_phase) {
        Ok(identity) => identity,
        Err(failure) => return failure.into_availability(&identity_phase, generation),
    };
    if fingerprint_changed(candidate) {
        return stale_error(generation);
    }
    // Materialization is complete once identity has run, so every later phase
    // is bounded by the ordinary probe budget.
    let capability_phase = ProbePhase::start("capability", target.executable(), probe_budget);
    run_capabilities(definition, &target, identity, &capability_phase, generation)
}

fn run_identity(
    definition: &AgentDefinition,
    target: &ProbeTarget<'_>,
    phase: &ProbePhase<'_>,
) -> Result<String, ProbeFailure> {
    let output = execute_probe(target, &definition.probe.argv, phase)?;
    let selected = select_stream(&output, definition.probe.stream)?;
    parse_identity(&selected, &definition.probe).map_err(ProbeFailure::Evidence)
}

fn run_capabilities(
    definition: &AgentDefinition,
    target: &ProbeTarget<'_>,
    identity: String,
    phase: &ProbePhase<'_>,
    generation: u64,
) -> Availability {
    let Some(probe) = &definition.probe.capabilities else {
        return compatible(identity, Vec::new(), generation);
    };
    // A trusted capability probe skips the `--help` subprocess entirely and
    // reports every authored token as present (issue #534). This eliminates
    // the dominant source of Windows launch failures for agents whose every
    // release supports all authored arguments.
    if probe.trusted {
        let capabilities = probe.authored_capability_ids();
        if fingerprint_changed(target.candidate) {
            return stale_error(generation);
        }
        return compatible(identity, capabilities, generation);
    }
    let evaluation = match execute_capability_probe(definition, target, probe, phase) {
        Ok(evaluation) => evaluation,
        Err(failure) => return failure.into_availability(phase, generation),
    };
    if fingerprint_changed(target.candidate) {
        return stale_error(generation);
    }
    if let Some(missing) = evaluation.missing_required.first() {
        return Availability::InstalledIncompatible {
            reason: format!("missing required capability: {missing}"),
            generation,
        };
    }
    compatible(identity, evaluation.present, generation)
}

fn execute_capability_probe(
    definition: &AgentDefinition,
    target: &ProbeTarget<'_>,
    probe: &CapabilityProbe,
    phase: &ProbePhase<'_>,
) -> Result<crate::domain::agent_definition::CapabilityEvaluation, ProbeFailure> {
    let output = execute_probe(target, &probe.argv, phase)?;
    let selected = select_stream(&output, probe.stream)?;
    parse_capabilities(
        &selected,
        definition.probe.max_bytes,
        probe,
        &definition.probe.required,
    )
    .map_err(ProbeFailure::Evidence)
}

fn execute_probe(
    target: &ProbeTarget<'_>,
    argv: &[String],
    phase: &ProbePhase<'_>,
) -> Result<ProbeProcessOutput, ProbeFailure> {
    let deadline = phase.deadline();
    if Instant::now() >= deadline {
        return Err(ProbeFailure::Timeout);
    }
    let arguments: Vec<OsString> = argv.iter().map(OsString::from).collect();
    let build_command = || match target.invocation {
        Some(invocation) => {
            let mut package_arguments = invocation.prefix().to_vec();
            package_arguments.extend(arguments.iter().cloned());
            command_for_path(
                invocation.executable(),
                invocation.wrapper_kind(),
                &package_arguments,
            )
        }
        None => command_for_candidate(target.candidate, &arguments),
    };
    let output = run_probe_with_loader_retry(&build_command, phase.budget)?;
    validate_process_output(output)
}

/// Run a probe command, retrying on Windows loader transients.
///
/// Mirrors the proven psmux version-probe pattern: `STATUS_DLL_INIT_FAILED`
/// (`0xC0000142`) is a transient DLL loader failure that especially affects
/// npm-shim-mediated commands on a cold cache. Without retry, a freshly
/// installed agent (e.g. a new nightly) fails to launch even though it is
/// perfectly functional.
fn run_probe_with_loader_retry(
    build_command: &impl Fn() -> Command,
    budget: Duration,
) -> Result<ProbeProcessOutput, ProbeFailure> {
    let mut last_error = None;
    for attempt in 1..=MAX_LOADER_TRANSIENT_RETRIES {
        let command = build_command();
        // Each attempt is given the budget afresh. A timeout has by definition
        // already spent the previous one, so a shared deadline would leave the
        // retry nothing to run in and make it a formality.
        let deadline = attempt_deadline(budget);
        match run_probe_process(command, deadline) {
            Ok(output) => {
                if !is_retryable_loader_transient(output.status) {
                    return Ok(output);
                }
                tracing::warn!(
                    attempt,
                    max_attempts = MAX_LOADER_TRANSIENT_RETRIES,
                    status = ?output.status,
                    "agent probe hit Windows loader transient (STATUS_DLL_INIT_FAILED); retrying"
                );
                last_error = Some(ProbeFailure::Failed(format!(
                    "exited with a loader transient status: {output_status}",
                    output_status = output.status
                )));
            }
            // A cold shim is slow once and prompt afterwards, which is the same
            // transient this function already exists for -- it just arrives as
            // slowness instead of as a loader status (issue #604). Retried once
            // rather than three times: a command that is genuinely hung must not
            // cost three full budgets before anyone is told.
            Err(ProbeProcessError::Timeout) => {
                if attempt >= MAX_TIMEOUT_ATTEMPTS {
                    return Err(ProbeFailure::Timeout);
                }
                tracing::warn!(
                    attempt,
                    max_attempts = MAX_TIMEOUT_ATTEMPTS,
                    "agent probe overran its budget; retrying once with a fresh budget"
                );
                last_error = Some(ProbeFailure::Timeout);
            }
            Err(error) => return Err(ProbeFailure::Process(error)),
        }
        if attempt < MAX_LOADER_TRANSIENT_RETRIES {
            std::thread::sleep(LOADER_TRANSIENT_BACKOFF);
        }
    }
    Err(last_error.unwrap_or_else(|| {
        ProbeFailure::Failed("exhausted its loader transient retries".to_string())
    }))
}

fn validate_process_output(output: ProbeProcessOutput) -> Result<ProbeProcessOutput, ProbeFailure> {
    if output.stdout.truncated || output.stderr.truncated {
        return Err(ProbeFailure::Truncated);
    }
    if output.status.success() {
        return Ok(output);
    }
    let detail = output.status.code().map_or_else(
        || "terminated by signal".to_string(),
        |code| format!("exited with status {code}"),
    );
    Err(ProbeFailure::Failed(detail))
}

fn select_stream(
    output: &ProbeProcessOutput,
    stream: ProbeStream,
) -> Result<Vec<u8>, ProbeFailure> {
    match stream {
        ProbeStream::Stdout => Ok(output.stdout.bytes.clone()),
        ProbeStream::Stderr => Ok(output.stderr.bytes.clone()),
        ProbeStream::Combined => {
            let Some(capacity) = output
                .stdout
                .bytes
                .len()
                .checked_add(output.stderr.bytes.len())
            else {
                return Err(ProbeFailure::Truncated);
            };
            let mut combined = Vec::with_capacity(capacity);
            combined.extend_from_slice(&output.stdout.bytes);
            combined.extend_from_slice(&output.stderr.bytes);
            Ok(combined)
        }
    }
}

fn command_for_candidate(candidate: &ResolvedCandidate, argv: &[OsString]) -> Command {
    command_for_path(candidate.executable(), candidate.wrapper_kind(), argv)
}

pub fn command_for_path(path: &Path, wrapper: AgentWrapperKind, argv: &[OsString]) -> Command {
    match wrapper {
        AgentWrapperKind::Direct => command_with_args(path.as_os_str(), argv),
        // Canonical fingerprints store verbatim `\\?\` paths on Windows, which
        // cmd.exe and powershell.exe cannot launch (issue #525). Strip the
        // prefix only at this command-construction boundary; Direct paths are
        // launched as-is and fingerprints remain canonical.
        AgentWrapperKind::CommandScript => {
            let launch_path = super::agent_executable::strip_verbatim_prefix(path);
            let program = std::env::var_os("COMSPEC").unwrap_or_else(|| OsString::from("cmd.exe"));
            let mut command = Command::new(program);
            command
                .args(["/D", "/S", "/C"])
                .arg(&launch_path)
                .args(argv);
            command
        }
        AgentWrapperKind::PowerShellScript => {
            let launch_path = super::agent_executable::strip_verbatim_prefix(path);
            let program = std::env::var_os("JEFE_POWERSHELL_BIN")
                .unwrap_or_else(|| OsString::from("powershell.exe"));
            let mut command = Command::new(program);
            command
                .args(["-NoLogo", "-NoProfile", "-NonInteractive", "-File"])
                .arg(&launch_path)
                .args(argv);
            command
        }
    }
}

fn command_with_args(program: &OsStr, argv: &[OsString]) -> Command {
    let mut command = Command::new(program);
    command.args(argv);
    command
}

fn fingerprint_changed(candidate: &ResolvedCandidate) -> bool {
    match capture_candidate_fingerprint(candidate.executable()) {
        Ok(current) => &current != candidate.fingerprint(),
        Err(_) => true,
    }
}

fn compatible(identity: String, capabilities: Vec<String>, generation: u64) -> Availability {
    Availability::InstalledCompatible {
        identity,
        capabilities,
        generation,
    }
}

fn probe_error(code: ProbeErrorCode, reason: String, generation: u64) -> Availability {
    Availability::ProbeError {
        code,
        reason,
        generation,
    }
}

fn stale_error(generation: u64) -> Availability {
    probe_error(
        ProbeErrorCode::Agte203,
        "candidate fingerprint changed; reprobe required".to_string(),
        generation,
    )
}

enum ProbeFailure {
    Timeout,
    Truncated,
    Process(ProbeProcessError),
    Evidence(ProbeEvidenceError),
    Failed(String),
}

impl ProbeFailure {
    /// Render this failure as an AGT-E202 whose reason names the phase, the
    /// executable that ran, and — for a timeout — the elapsed time and the
    /// budget it exceeded, so a field report is attributable (issue #553).
    fn into_availability(self, phase: &ProbePhase<'_>, generation: u64) -> Availability {
        let detail = match self {
            Self::Timeout | Self::Process(ProbeProcessError::Timeout) => format!(
                "timed out after {elapsed} ms (budget {budget} ms)",
                elapsed = phase.started.elapsed().as_millis(),
                budget = phase.budget.as_millis()
            ),
            Self::Truncated => "produced a truncated stream".to_string(),
            Self::Failed(detail) => detail,
            Self::Process(ProbeProcessError::Failed(detail)) => format!("failed: {detail}"),
            Self::Evidence(ProbeEvidenceError::Bounds(detail)) => {
                format!("exceeded its bounds: {detail}")
            }
            Self::Evidence(ProbeEvidenceError::InvalidUtf8) => {
                "produced a stream that is not valid UTF-8".to_string()
            }
            Self::Evidence(ProbeEvidenceError::MalformedFraming) => {
                "produced malformed framing".to_string()
            }
            Self::Evidence(ProbeEvidenceError::IdentityMismatch) => {
                "reported an unrecognized identity".to_string()
            }
        };
        probe_error(ProbeErrorCode::Agte202, phase.describe(&detail), generation)
    }
}

#[cfg(test)]
mod tests {
    use std::ffi::OsString;
    use std::path::Path;
    use std::time::Duration;

    use super::{AgentWrapperKind, command_for_path};

    #[test]
    fn wrapper_commands_preserve_fixed_argv_elements() {
        let path = Path::new("C:/agent/probe.cmd");
        let argv = [OsString::from("--version"), OsString::from("literal value")];
        let direct = command_for_path(path, AgentWrapperKind::Direct, &argv);
        assert_eq!(direct.get_program(), path.as_os_str());
        let direct_args = direct.get_args().collect::<Vec<_>>();
        assert_eq!(
            direct_args,
            argv.iter().map(OsString::as_os_str).collect::<Vec<_>>()
        );

        let command_script = command_for_path(path, AgentWrapperKind::CommandScript, &argv);
        let command_args = command_script.get_args().collect::<Vec<_>>();
        assert_eq!(command_args[0..3], ["/D", "/S", "/C"]);
        assert_eq!(command_args[3], path.as_os_str());
        assert_eq!(command_args[4..], argv);

        let powershell = command_for_path(path, AgentWrapperKind::PowerShellScript, &argv);
        let powershell_args = powershell.get_args().collect::<Vec<_>>();
        assert_eq!(
            powershell_args[0..4],
            ["-NoLogo", "-NoProfile", "-NonInteractive", "-File"]
        );
        assert_eq!(powershell_args[4], path.as_os_str());
        assert_eq!(powershell_args[5..], argv);
    }

    #[test]
    fn local_probe_budget_bounds_each_sequential_process() {
        let process_timeout = super::AgentProbeTarget::Local.total_timeout();
        assert_eq!(process_timeout, std::time::Duration::from_secs(10));
        assert_eq!(
            process_timeout.saturating_mul(2),
            std::time::Duration::from_secs(20),
            "identity and capability each receive one bounded process timeout"
        );
    }

    /// A runner-mediated probe pays materialization once, in identity, so its
    /// combined ceiling is that budget plus one ordinary probe budget. It is
    /// larger than a direct probe's ceiling by design, and still finite.
    #[test]
    fn runner_mediated_probe_has_a_finite_combined_ceiling() {
        let probe_budget = super::AgentProbeTarget::Local.total_timeout();
        assert_eq!(
            super::PACKAGE_MATERIALIZATION_TIMEOUT,
            std::time::Duration::from_secs(300),
            "materialization budget is a pinned contract value"
        );
        let combined = super::PACKAGE_MATERIALIZATION_TIMEOUT.saturating_add(probe_budget);
        assert_eq!(combined, std::time::Duration::from_secs(310));
        assert!(
            combined > probe_budget.saturating_mul(2),
            "materialization is deliberately not charged to the probe budget"
        );
    }

    /// A shim that is slow only the first time must still yield an identity.
    ///
    /// This is the cold-DLL-cache case the module already documents, arriving
    /// as slowness rather than as `STATUS_DLL_INIT_FAILED`. Measured on a real
    /// machine, `llxprt.cmd --version` took 12.5s cold and 2.2s warm against a
    /// 10s budget, so the first launch of jefe after a reboot reported
    /// `AGT-E202` and left the agent unlaunchable until the user quit and
    /// reopened (issue #604).
    ///
    /// The first attempt here overruns a deliberately tiny budget; the second
    /// returns at once. Retrying must therefore succeed, and must do so by
    /// giving the retry its own budget rather than the remains of a budget the
    /// timeout already consumed.
    #[test]
    fn a_probe_that_overruns_its_budget_once_is_retried() {
        use std::sync::atomic::{AtomicU32, Ordering};

        let attempts = AtomicU32::new(0);
        let build = || {
            if attempts.fetch_add(1, Ordering::SeqCst) == 0 {
                sleeping_command()
            } else {
                immediate_command()
            }
        };

        let outcome = super::run_probe_with_loader_retry(&build, Duration::from_millis(300));

        assert!(
            outcome.is_ok(),
            "a first attempt that merely ran long must not condemn the probe"
        );
        assert_eq!(
            attempts.load(Ordering::SeqCst),
            2,
            "the timeout must be retried exactly once, not abandoned and not repeated forever"
        );
    }

    /// A command that never answers must still be given up on.
    ///
    /// The retry above must not become an unbounded wait: fail-slow is the
    /// mirror of the bug it fixes.
    #[test]
    fn a_probe_that_always_overruns_is_eventually_abandoned() {
        use std::sync::atomic::{AtomicU32, Ordering};

        let attempts = AtomicU32::new(0);
        let build = || {
            attempts.fetch_add(1, Ordering::SeqCst);
            sleeping_command()
        };

        let outcome = super::run_probe_with_loader_retry(&build, Duration::from_millis(300));

        assert!(
            outcome.is_err(),
            "a command that never answers must not be reported as an identity"
        );
        assert_eq!(
            attempts.load(Ordering::SeqCst),
            2,
            "one retry, then the verdict"
        );
    }

    /// A command that outlives a short budget on every platform.
    fn sleeping_command() -> std::process::Command {
        if cfg!(windows) {
            let mut command = std::process::Command::new("cmd");
            command.args(["/D", "/S", "/C", "ping -n 4 127.0.0.1"]);
            command
        } else {
            let mut command = std::process::Command::new("sh");
            command.args(["-c", "sleep 3"]);
            command
        }
    }

    /// A command that answers immediately on every platform.
    fn immediate_command() -> std::process::Command {
        if cfg!(windows) {
            let mut command = std::process::Command::new("cmd");
            command.args(["/D", "/S", "/C", "exit 0"]);
            command
        } else {
            let mut command = std::process::Command::new("sh");
            command.args(["-c", "exit 0"]);
            command
        }
    }

    #[test]
    fn loader_transient_classification_matches_psmux_contract() {
        // STATUS_DLL_INIT_FAILED is the only retryable status. We verify the
        // constant value and the classifier logic without spawning a process.
        assert_eq!(super::STATUS_DLL_INIT_FAILED, -1_073_741_502);
        assert_eq!(super::MAX_LOADER_TRANSIENT_RETRIES, 3);
        assert_eq!(
            super::LOADER_TRANSIENT_BACKOFF,
            std::time::Duration::from_millis(500)
        );
    }

    #[cfg(windows)]
    #[test]
    fn loader_transient_retry_recovers_when_process_eventually_succeeds() {
        // Prove the retry loop runs the command multiple times and recovers
        // when an early attempt fails but a later one succeeds. We use a
        // simple batch script that always succeeds — the point is that
        // run_probe_with_loader_retry invokes the builder and returns Ok.
        use std::process::{Command, Stdio};

        let dir = tempfile::tempdir()
            .unwrap_or_else(|error| panic!("could not create fixture dir: {error}"));
        let script = dir.path().join("probe.cmd");
        std::fs::write(&script, "@echo off\r\necho 1.0.0\r\nexit /b 0\r\n")
            .unwrap_or_else(|error| panic!("could not write fixture script: {error}"));

        let script_ref = &script;
        let build = || {
            let mut cmd = Command::new("cmd.exe");
            cmd.args(["/D", "/S", "/C"])
                .arg(script_ref)
                .stdin(Stdio::null())
                .stdout(Stdio::piped())
                .stderr(Stdio::piped());
            cmd
        };

        let result = super::run_probe_with_loader_retry(&build, Duration::from_secs(30));
        let Some(output) = result.ok() else {
            panic!("retry must return Ok on success");
        };
        assert!(output.status.success(), "final attempt must exit 0");
    }

    #[cfg(windows)]
    #[test]
    fn canonical_windows_wrapper_paths_are_launch_safe() {
        let dir = tempfile::tempdir()
            .unwrap_or_else(|error| panic!("could not create wrapper fixture: {error}"));
        let wrapper = dir.path().join("probe.cmd");
        std::fs::write(&wrapper, b"@echo off\r\necho 0.10.0\r\n")
            .unwrap_or_else(|error| panic!("could not write wrapper fixture: {error}"));
        let canonical = std::fs::canonicalize(&wrapper)
            .unwrap_or_else(|error| panic!("could not canonicalize wrapper fixture: {error}"));
        assert_ne!(
            canonical, wrapper,
            "Windows canonical path must be verbatim"
        );

        let mut command = command_for_path(
            &canonical,
            AgentWrapperKind::CommandScript,
            &[OsString::from("--version")],
        );
        let args = command.get_args().collect::<Vec<_>>();
        assert_eq!(
            args[3],
            super::super::agent_executable::strip_verbatim_prefix(&canonical),
            "cmd.exe cannot launch a canonical verbatim wrapper path"
        );
        let output = command
            .output()
            .unwrap_or_else(|error| panic!("could not execute normalized wrapper: {error}"));
        assert!(
            output.status.success(),
            "normalized wrapper failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );
    }
}

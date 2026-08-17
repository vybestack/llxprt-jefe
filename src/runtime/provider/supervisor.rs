//! One-shot provider process supervisor (issue #390 CW-10, Slice C1).
//!
//! The supervisor is the sole owner of a provider [`Child`], its process group,
//! its pipes, its drain threads, its outbound queue, its timeouts, and its
//! reaping. It drives one fresh one-shot lifecycle end to end — `hello`/
//! `hello-ack` → `configure`/`ready` → `invoke-action` → `0..256` progress →
//! exactly one `outcome`/`error` → `shutdown`/`shutdown-ack` → EOF/reap — and
//! returns only typed domain values: an outcome, a typed failure, a transcript,
//! redacted retained stderr, and a reap flag. No [`Child`], pipe, or thread
//! handle ever leaves this module.
//!
//! Timeout and shutdown bounds are injectable as pure values
//! ([`SupervisorBounds`]); [`SupervisorBounds::PRODUCTION`] holds the exact
//! contract defaults while tests inject small values.
//!
//! [`Child`]: std::process::Child

use std::io;
use std::process::{ChildStdin, Command, ExitStatus};
use std::sync::atomic::AtomicBool;
use std::sync::mpsc;
use std::time::{Duration, Instant};

use crate::domain::{CanonicalSemver, Id};

use super::drains::{
    FinalStdoutOutcome, StderrDrain, StderrOutcome, StdoutDrain, final_stdout_drain,
};
use super::driver;
use super::dto;
use super::environment::{
    EnvironmentError, HostEnv, ProcessEnv, ProviderEnvironment, Redactor, build_process_env,
    resolve_configure_secrets,
};
use super::identifiers::RequestId;
use super::outbound::OutboundQueue;
pub use super::outcome::{
    CleanupFailure, LifecycleTranscript, OneShotOutcome, OneShotResult, SupervisorFailure,
    TranscriptEntry,
};
use super::process_tree::{self, ProviderProcess};
use super::protocol::{LifecycleOrder, ProgressTracker};
use super::redaction;

/// The poll interval used while waiting for a process to exit.
const EXIT_POLL: Duration = Duration::from_millis(10);

/// Injectable timeout and shutdown bounds.
///
/// Production defaults are exact: a 5 s handshake stage, a 60 s invocation, and
/// the 2 s / 2 s / 2 s staged shutdown. Tests inject small values.
#[derive(Debug, Clone, Copy)]
pub struct SupervisorBounds {
    /// Per handshake stage (hello-ack, configure/ready).
    pub handshake: Duration,
    /// Invocation: progress + terminal.
    pub invocation: Duration,
    /// Stage A: wait for `shutdown-ack` and process exit after `shutdown`.
    pub shutdown_ack: Duration,
    /// Stage B: after closing stdin and terminating the group.
    pub stdin_close: Duration,
    /// Stage C: after force-killing and reaping the tree.
    pub final_drain: Duration,
}

impl SupervisorBounds {
    /// The exact production defaults.
    pub const PRODUCTION: Self = Self {
        handshake: Duration::from_secs(5),
        invocation: Duration::from_secs(60),
        shutdown_ack: Duration::from_secs(2),
        stdin_close: Duration::from_secs(2),
        final_drain: Duration::from_secs(2),
    };
}

impl Default for SupervisorBounds {
    fn default() -> Self {
        Self::PRODUCTION
    }
}

impl SupervisorBounds {
    /// Production bounds with a specific invocation timeout (S15).
    ///
    /// The descriptor-selected `timeout_seconds` (validated 1..=600 at startup
    /// composition) is carried exactly into the invocation deadline. All other
    /// bounds remain at production defaults.
    #[must_use]
    pub fn for_invocation(timeout_seconds: u32) -> Self {
        Self {
            invocation: Duration::from_secs(u64::from(timeout_seconds)),
            ..Self::PRODUCTION
        }
    }
}

/// One fresh one-shot provider invocation request.
#[derive(Debug, Clone)]
pub struct OneShotRequest {
    /// The selected provider binary.
    pub binary: std::path::PathBuf,
    /// Arguments to pass to the binary.
    pub arguments: Vec<String>,
    /// Contained working directory.
    pub working_dir: std::path::PathBuf,
    /// Environment specification (CW10-14).
    pub environment: ProviderEnvironment,
    /// Contained `HOME`.
    pub home: std::path::PathBuf,
    /// Contained `TMPDIR`.
    pub tmpdir: std::path::PathBuf,
    /// Locale (`LC_ALL`/`LANG`).
    pub locale: String,
    /// Host API identifier sent in `hello`.
    pub host_api: String,
    /// Plugin package id.
    pub plugin_id: Id,
    /// Plugin package version.
    pub plugin_version: CanonicalSemver,
    /// Fixed positive generation for this invocation.
    pub generation: u64,
    /// Host request id for this invocation.
    pub request_id: RequestId,
    /// Base `configure` payload; the supervisor merges resolved secrets in.
    pub configure: dto::ConfigurePayload,
    /// The `invoke-action` payload.
    pub invocation: dto::InvokeActionPayload,
}

/// Run one fresh one-shot provider lifecycle.
///
/// The supervisor spawns the provider, drives the closed handshake and
/// invocation, then performs the staged shutdown/reap. It returns typed values
/// only. Secrets are resolved from declared host references through `host_env`
/// and redacted from every provider-owned observation surface.
pub fn run_one_shot<E: HostEnv>(
    request: &OneShotRequest,
    bounds: &SupervisorBounds,
    host_env: &E,
) -> OneShotResult {
    let mut transcript = LifecycleTranscript::default();
    let (env, configure, redactor) = match prepare_invocation(request, host_env) {
        Prepared::Ready {
            env,
            configure,
            redactor,
        } => (env, configure, redactor),
        Prepared::Failed(result) => return result,
    };

    let live = LiveHooks {
        redactor: &redactor,
        progress: None,
        cancel: None,
    };
    let raw = supervise(request, bounds, env, configure, &mut transcript, &live);
    finish_result(raw, transcript, redactor)
}

/// Run one fresh one-shot provider lifecycle with **live streaming** (S16/S17).
///
/// Each accepted progress payload is forwarded through `progress` as it arrives
/// (redacted against resolved secrets), before the terminal result. The
/// `cancel` flag is checked between reads; when set, the driver writes a cancel
/// frame and stops, preserving first-terminal semantics at the reducer layer.
///
/// The returned [`OneShotResult`] carries the same typed values as
/// [`run_one_shot`], including the cleanup diagnostic.
pub fn run_one_shot_streaming<E: HostEnv>(
    request: &OneShotRequest,
    bounds: &SupervisorBounds,
    host_env: &E,
    progress: Option<&mpsc::Sender<dto::ProgressPayload>>,
    cancel: Option<&AtomicBool>,
) -> OneShotResult {
    let mut transcript = LifecycleTranscript::default();

    let (env, configure, redactor) = match prepare_invocation(request, host_env) {
        Prepared::Ready {
            env,
            configure,
            redactor,
        } => (env, configure, redactor),
        Prepared::Failed(result) => return result,
    };

    let live = LiveHooks {
        redactor: &redactor,
        progress,
        cancel,
    };
    let raw = supervise(request, bounds, env, configure, &mut transcript, &live);
    finish_result(raw, transcript, redactor)
}

/// The common pre-spawn preparation shared by the blocking and streaming
/// entry points. Resolves the environment and Configure secrets and rejects a
/// caller-supplied Configure secret.
enum Prepared {
    Ready {
        env: ProcessEnv,
        configure: dto::ConfigurePayload,
        redactor: Redactor,
    },
    Failed(OneShotResult),
}

fn prepare_invocation<E: HostEnv>(request: &OneShotRequest, host_env: &E) -> Prepared {
    let env = match build_process_env(
        &request.environment,
        &request.home,
        &request.tmpdir,
        &request.locale,
        host_env,
    ) {
        Ok(env) => env,
        Err(err) => {
            return Prepared::Failed(OneShotResult::pre_spawn(SupervisorFailure::Environment(
                err,
            )));
        }
    };
    let configure_secrets = match resolve_configure_secrets(&request.environment, host_env) {
        Ok(secrets) => secrets,
        Err(err) => {
            return Prepared::Failed(OneShotResult::pre_spawn(SupervisorFailure::Environment(
                err,
            )));
        }
    };
    let redactor = env.redactor();

    if let Some((binding, _)) = request.configure.secrets.first_key_value() {
        return Prepared::Failed(OneShotResult::pre_spawn(SupervisorFailure::Environment(
            EnvironmentError::UndeclaredConfigureSecret {
                binding: binding.to_string(),
            },
        )));
    }

    let mut configure = request.configure.clone();
    for (binding, value) in configure_secrets {
        configure.secrets.insert(binding, value);
    }
    for directory in [&request.working_dir, &request.home, &request.tmpdir] {
        if let Err(error) = std::fs::create_dir_all(directory) {
            return Prepared::Failed(OneShotResult::pre_spawn(SupervisorFailure::Containment {
                path: directory.clone(),
                error: error.to_string(),
            }));
        }
    }
    Prepared::Ready {
        env,
        configure,
        redactor,
    }
}

/// Apply redaction and assemble the final typed result from the raw internals.
fn finish_result(
    raw: RawResult,
    mut transcript: LifecycleTranscript,
    redactor: Redactor,
) -> OneShotResult {
    transcript.redact_progress(&redactor);
    OneShotResult {
        outcome: redaction::redact_one_shot_outcome(raw.outcome, &redactor),
        transcript,
        retained_stderr: raw.retained_stderr,
        stderr_truncated: raw.stderr_truncated,
        process_reaped: raw.process_reaped,
        exit_code: raw.exit_code,
        cleanup_failure: raw
            .cleanup_failure
            .map(|failure| redaction::redact_cleanup_failure(failure, &redactor)),
    }
}

/// The internally-collected result before outcome redaction.
struct RawResult {
    outcome: OneShotOutcome,
    retained_stderr: String,
    stderr_truncated: bool,
    process_reaped: bool,
    exit_code: Option<i32>,
    cleanup_failure: Option<CleanupFailure>,
}

fn supervise(
    request: &OneShotRequest,
    bounds: &SupervisorBounds,
    env: ProcessEnv,
    configure: dto::ConfigurePayload,
    transcript: &mut LifecycleTranscript,
    live: &LiveHooks<'_>,
) -> RawResult {
    let command = build_command(request, &env);
    let (mut process, mut stdin, stdout, stderr) = match process_tree::spawn(command) {
        Ok(spawned) => spawned,
        Err(error) => {
            return RawResult {
                outcome: OneShotOutcome::Failed(SupervisorFailure::Spawn(error.to_string())),
                retained_stderr: String::new(),
                stderr_truncated: false,
                process_reaped: false,
                exit_code: None,
                cleanup_failure: None,
            };
        }
    };
    let pid = process.id();

    // Drain thread spawn failures are propagated as typed failures after
    // force-killing/reaping the process, never discarded into a later
    // ambiguous EOF.
    let stdout_drain = spawn_stdout_drain(&mut process, stdout, bounds, transcript);
    let stdout_drain = match stdout_drain {
        StdoutDrainSpawn::Ok(drain) => drain,
        StdoutDrainSpawn::Failed(raw) => return *raw,
    };
    let stderr_drain = spawn_stderr_drain(&mut process, stderr, bounds, transcript);
    let stderr_drain = match stderr_drain {
        StderrDrainSpawn::Ok(drain) => drain,
        StderrDrainSpawn::Failed(raw) => return *raw,
    };

    let req = DriveReq { request, bounds };
    let (outcome, cleanup_ack_failure) = drive_lifecycle(
        &req,
        &configure,
        &mut stdin,
        &stdout_drain,
        transcript,
        live,
    );

    complete_lifecycle(
        &mut process,
        Some(stdin),
        transcript,
        LifecycleTail {
            bounds,
            pid,
            stdout_drain: &stdout_drain,
            stderr_drain,
            redactor: live.redactor,
            outcome,
            ack_failure: cleanup_ack_failure,
        },
    )
}

/// Run the staged shutdown, bounded final drains, and cleanup-failure
/// composition after the lifecycle terminal has been reached.
///
/// Extracted from [`supervise`] so the lifecycle and its completion remain
/// separately readable and each stays under the source-size guidance.
fn complete_lifecycle(
    process: &mut ProviderProcess,
    stdin: Option<ChildStdin>,
    transcript: &mut LifecycleTranscript,
    tail: LifecycleTail<'_>,
) -> RawResult {
    // Staged shutdown always runs, regardless of the outcome. The stdout drain
    // is detached (its handle dropped) rather than joined: the lifecycle is
    // complete and the bounded reaper closed the pipe, so joining would be an
    // unbounded wait for no additional information.
    let (shutdown_outcome, _signal_errors) = staged_shutdown(process, stdin, tail.bounds, tail.pid);
    let process_reaped = matches!(shutdown_outcome, ShutdownOutcome::Exited(_));
    let exit_code = match shutdown_outcome {
        ShutdownOutcome::Exited(code) => code,
        ShutdownOutcome::NotReaped => None,
    };
    // Bounded final stdout drain: after process exit/kill, observe whether
    // stdout actually reached EOF (channel disconnection), reject any data
    // buffered after the ack, and surface an explicit DrainTimeout/protocol
    // cleanup failure if EOF was not observed. EOF is recorded only when it
    // was actually observed, never unconditionally.
    let stdout_final = final_stdout_drain(&tail.stdout_drain.receiver, tail.bounds.final_drain);
    if matches!(stdout_final, FinalStdoutOutcome::Eof) {
        transcript.push(TranscriptEntry::Eof);
    }
    if process_reaped {
        transcript.push(TranscriptEntry::Reaped);
    }

    finish_cleanup(
        tail.stderr_drain,
        tail.redactor,
        FinishCleanup {
            final_drain: tail.bounds.final_drain,
            process_reaped,
            exit_code,
            ack_failure: tail.ack_failure,
            stdout_final,
            outcome: tail.outcome,
        },
    )
}

/// Inputs for [`complete_lifecycle`], grouping its references under the clippy
/// argument limit by analogy with [`FinishCleanup`].
struct LifecycleTail<'a> {
    bounds: &'a SupervisorBounds,
    pid: u32,
    stdout_drain: &'a StdoutDrain,
    stderr_drain: StderrDrain,
    redactor: &'a Redactor,
    outcome: OneShotOutcome,
    ack_failure: Option<CleanupFailure>,
}

/// Spawn the stdout drain, returning a typed failure if the thread cannot start.
fn spawn_stdout_drain(
    process: &mut ProviderProcess,
    stdout: std::process::ChildStdout,
    bounds: &SupervisorBounds,
    transcript: &mut LifecycleTranscript,
) -> StdoutDrainSpawn {
    match StdoutDrain::spawn(stdout) {
        Ok(drain) => StdoutDrainSpawn::Ok(drain),
        Err(error) => StdoutDrainSpawn::Failed(Box::new(reap_on_drain_failure(
            process,
            transcript,
            bounds,
            SupervisorFailure::Io(format!("stdout drain spawn failed: {error}")),
        ))),
    }
}

/// Spawn the stderr drain, returning a typed failure if the thread cannot start.
fn spawn_stderr_drain(
    process: &mut ProviderProcess,
    stderr: std::process::ChildStderr,
    bounds: &SupervisorBounds,
    transcript: &mut LifecycleTranscript,
) -> StderrDrainSpawn {
    match StderrDrain::spawn(stderr) {
        Ok(drain) => StderrDrainSpawn::Ok(drain),
        Err(error) => StderrDrainSpawn::Failed(Box::new(reap_on_drain_failure(
            process,
            transcript,
            bounds,
            SupervisorFailure::Io(format!("stderr drain spawn failed: {error}")),
        ))),
    }
}

/// Result of spawning the stdout drain: either the drain or a pre-reaped failure.
enum StdoutDrainSpawn {
    /// The drain started.
    Ok(StdoutDrain),
    /// The thread could not start; the process was force-killed and reaped.
    Failed(Box<RawResult>),
}

/// Result of spawning the stderr drain: either the drain or a pre-reaped failure.
enum StderrDrainSpawn {
    /// The drain started.
    Ok(StderrDrain),
    /// The thread could not start; the process was force-killed and reaped.
    Failed(Box<RawResult>),
}

/// Build the fully-configured provider command from the request and the
/// constructed environment.
fn build_command(request: &OneShotRequest, env: &ProcessEnv) -> Command {
    let mut command = Command::new(&request.binary);
    command.args(&request.arguments);
    command.current_dir(&request.working_dir);
    command.env_clear();
    for (key, value) in env.vars() {
        command.env(key, value);
    }
    command
}

/// Force-kill and reap a process whose drain could not start, recording only
/// evidence that was actually observed.
///
/// The drain threads could not start, so there is no stdout/stderr channel to
/// observe EOF through. Force-kill signals are issued without an unbounded
/// wait and the leader is reaped within the final-drain bound. An EOF is never
/// claimed (it could not be observed); `Reaped` is recorded only when the
/// bounded reap actually observed an exit.
fn reap_on_drain_failure(
    process: &mut ProviderProcess,
    transcript: &mut LifecycleTranscript,
    bounds: &SupervisorBounds,
    failure: SupervisorFailure,
) -> RawResult {
    // Signal-delivery errors during this best-effort force-kill are not fatal
    // on their own; the bounded reap below is the reap authority and a failure
    // to reap surfaces as `NotReaped`.
    drop(process.force_kill_tree());
    let reaped = wait_for_exit(process, bounds.final_drain).is_some();
    if reaped {
        transcript.push(TranscriptEntry::Reaped);
    }
    RawResult {
        outcome: OneShotOutcome::Failed(failure),
        retained_stderr: String::new(),
        stderr_truncated: false,
        process_reaped: reaped,
        exit_code: None,
        cleanup_failure: if reaped {
            None
        } else {
            Some(CleanupFailure::NotReaped)
        },
    }
}

/// Drive the closed lifecycle and the strict shutdown-ack observation,
/// returning the terminal outcome and the pending cleanup-ack failure.
///
/// When `live.progress` is present, each accepted progress payload is streamed
/// live (S16) and a host cancel signal is checked between reads (S17).
fn drive_lifecycle(
    req: &DriveReq<'_>,
    configure: &dto::ConfigurePayload,
    stdin: &mut ChildStdin,
    stdout_drain: &StdoutDrain,
    transcript: &mut LifecycleTranscript,
    live: &LiveHooks<'_>,
) -> (OneShotOutcome, Option<CleanupFailure>) {
    let mut queue = OutboundQueue::new();
    let mut lifecycle = LifecycleOrder::new();
    let mut progress = ProgressTracker::new();
    let mut healthy = true;
    let mut cleanup_ack_failure: Option<CleanupFailure> = None;
    let outcome = {
        let mut driver = driver::Driver {
            request: req.request,
            bounds: req.bounds,
            stdin,
            queue: &mut queue,
            stdout: stdout_drain,
            lifecycle: &mut lifecycle,
            progress: &mut progress,
            transcript,
            healthy: &mut healthy,
            configure,
            cleanup_ack_failure: &mut cleanup_ack_failure,
            progress_sink: live.progress,
            cancel: live.cancel,
            redactor: live.redactor,
        };
        driver.run()
    };
    queue.close();
    (outcome, cleanup_ack_failure)
}

/// The immutable one-shot invocation parameters bundled for [`drive_lifecycle`]
/// to keep its argument count under the clippy limit.
struct DriveReq<'a> {
    request: &'a OneShotRequest,
    bounds: &'a SupervisorBounds,
}

/// Live streaming context for one invocation (S16/S17).
///
/// Always constructed: the `redactor` is required for both blocking and
/// streaming paths; `progress`/`cancel` are present only for the streaming
/// entry point.
struct LiveHooks<'a> {
    redactor: &'a Redactor,
    progress: Option<&'a mpsc::Sender<dto::ProgressPayload>>,
    cancel: Option<&'a AtomicBool>,
}

/// Inputs for [`finish_cleanup`], keeping its signature at the clippy argument
/// limit by grouping the per-call outcome and reap evidence.
struct FinishCleanup {
    final_drain: Duration,
    process_reaped: bool,
    exit_code: Option<i32>,
    ack_failure: Option<CleanupFailure>,
    stdout_final: FinalStdoutOutcome,
    outcome: OneShotOutcome,
}

/// Collect the bounded stderr, redact it, and compose the final cleanup failure
/// from the independent lifecycle, drain, and reap signals.
fn finish_cleanup(
    stderr_drain: StderrDrain,
    redactor: &Redactor,
    inputs: FinishCleanup,
) -> RawResult {
    // Stderr completion is bounded by the final-drain bound: a blocking recv or
    // join would be an unbounded wait if a descendant holds an inherited pipe.
    let (retained, truncated, stderr_timed_out) =
        collect_retained_stderr(stderr_drain, inputs.final_drain);
    let retained_stderr = redactor.redact(&retained).into_owned();
    let cleanup_failure = compose_cleanup_failure(
        inputs.process_reaped,
        inputs.ack_failure,
        inputs.stdout_final,
        stderr_timed_out,
    );
    RawResult {
        outcome: inputs.outcome,
        retained_stderr,
        stderr_truncated: truncated,
        process_reaped: inputs.process_reaped,
        exit_code: inputs.exit_code,
        cleanup_failure,
    }
}

/// Compose the single cleanup failure from the independent reap, ack, stdout,
/// and stderr signals. A clean cleanup requires all four: the leader reaped, a
/// valid shutdown-ack, an observed stdout EOF, and a closed stderr drain. Reap
/// is the most actionable; a shutdown-ack fault is next; a surviving descendant
/// (stdout/stderr did not close) is last. Descendants are never assumed reaped
/// merely because the leader reaped — closure of both pipes is required.
pub(super) fn compose_cleanup_failure(
    process_reaped: bool,
    ack_failure: Option<CleanupFailure>,
    stdout_final: FinalStdoutOutcome,
    stderr_timed_out: bool,
) -> Option<CleanupFailure> {
    if !process_reaped {
        return Some(CleanupFailure::NotReaped);
    }
    if let Some(ack) = ack_failure {
        return Some(ack);
    }
    match stdout_final {
        FinalStdoutOutcome::Eof => {}
        FinalStdoutOutcome::DataAfterAck => {
            return Some(CleanupFailure::ShutdownAck(driver::ack_data_after()));
        }
        FinalStdoutOutcome::Fault => {
            return Some(CleanupFailure::ShutdownAck(driver::ack_read_fault()));
        }
        FinalStdoutOutcome::Timeout => return Some(CleanupFailure::DrainTimeout),
    }
    if stderr_timed_out {
        return Some(CleanupFailure::DrainTimeout);
    }
    None
}

/// ESRCH (no such process): signal target already reaped (benign on Unix).
const EXITED_PROCESS_ERRNO: i32 = 3;
/// Preserve the first non-benign signal-delivery error (ESRCH is benign) so a
/// real failure (e.g. permission) is not discarded when cleanup is clean.
pub(super) fn signal_cleanup_evidence(errors: &[io::Error]) -> Option<String> {
    // On non-Unix "process already gone" is not a stable errno, so signal errors
    // are not classified; the bounded reap/drains remain authoritative.
    if !cfg!(unix) {
        return None;
    }
    errors.iter().find_map(|error| {
        let benign = error.raw_os_error() == Some(EXITED_PROCESS_ERRNO);
        (!benign).then(|| error.to_string())
    })
}

/// The outcome of the staged shutdown.
#[derive(Debug, Clone, Copy)]
pub(super) enum ShutdownOutcome {
    /// The process tree exited; its exit code, if any.
    Exited(Option<i32>),
    /// The process tree could not be observed reaped within the bound.
    NotReaped,
}

/// Perform the escalating staged shutdown and reap (CW10-11).
///
/// Stage A waits the shutdown-ack bound for a self-exit. Stage B closes stdin,
/// terminates the group, and waits the stdin-close bound. Stage C force-kills the
/// tree and reaps within the final-drain bound; if it cannot be observed reaped,
/// [`NotReaped`] is returned — never a reaped flag without an observed exit.
/// Kill signals are issued without an unbounded `wait`: the bounded poll is the
/// sole reap authority. Signal-delivery errors are collected (not discarded); a
/// benign already-reaped (ESRCH) result is filtered by [`signal_cleanup_evidence`].
///
/// [`NotReaped`]: ShutdownOutcome::NotReaped
pub(super) fn staged_shutdown(
    process: &mut ProviderProcess,
    stdin: Option<ChildStdin>,
    bounds: &SupervisorBounds,
    pid: u32,
) -> (ShutdownOutcome, Vec<io::Error>) {
    let mut signals: Vec<io::Error> = Vec::new();
    let mut leader_status = None;
    let mut observation_error = None;
    if wait_for_tree_exit(
        process,
        bounds.shutdown_ack,
        &mut leader_status,
        &mut observation_error,
    ) {
        append_observation_error(&mut signals, observation_error);
        return (
            ShutdownOutcome::Exited(leader_status.and_then(|status| status.code())),
            signals,
        );
    }
    drop(stdin);
    // A reaped leader's process-group id may already have been recycled. Signal
    // its group only while the owned Child is still live; exact descendant PIDs
    // were captured while parentage was authoritative and remain safe targets.
    if leader_status.is_none()
        && let Err(error) = process_tree::terminate_process_tree(pid)
    {
        signals.push(error);
    }
    if let Err(error) = process.terminate_descendants() {
        signals.push(error);
    }
    if wait_for_tree_exit(
        process,
        bounds.stdin_close,
        &mut leader_status,
        &mut observation_error,
    ) {
        append_observation_error(&mut signals, observation_error);
        return (
            ShutdownOutcome::Exited(leader_status.and_then(|status| status.code())),
            signals,
        );
    }
    // Stage C: force-kill only exact targets and retain every signal error. The
    // leader group is never named after the leader has been observed reaped.
    let force_result = if leader_status.is_some() {
        process.force_kill_descendants()
    } else {
        process.force_kill_tree()
    };
    if let Err(error) = force_result {
        signals.push(error);
    }
    let exited = wait_for_tree_exit(
        process,
        bounds.final_drain,
        &mut leader_status,
        &mut observation_error,
    );
    append_observation_error(&mut signals, observation_error);
    let outcome = if exited {
        ShutdownOutcome::Exited(leader_status.and_then(|status| status.code()))
    } else {
        ShutdownOutcome::NotReaped
    };
    (outcome, signals)
}

fn append_observation_error(errors: &mut Vec<io::Error>, error: Option<io::Error>) {
    if let Some(error) = error {
        errors.push(error);
    }
}

fn wait_for_tree_exit(
    process: &mut ProviderProcess,
    bound: Duration,
    leader_status: &mut Option<ExitStatus>,
    observation_error: &mut Option<io::Error>,
) -> bool {
    let deadline = Instant::now() + bound;
    loop {
        if let Err(error) = process.observe_descendants()
            && observation_error.is_none()
        {
            *observation_error = Some(error);
        }
        if leader_status.is_none()
            && let Ok(Some(status)) = process.try_wait()
        {
            *leader_status = Some(status);
        }
        if leader_status.is_some() && !process.descendants_alive() {
            return true;
        }
        if Instant::now() >= deadline {
            return false;
        }
        std::thread::sleep(EXIT_POLL);
    }
}

/// Poll for process exit up to `bound`, returning the status if it exits.
pub(super) fn wait_for_exit(process: &mut ProviderProcess, bound: Duration) -> Option<ExitStatus> {
    let deadline = Instant::now() + bound;
    loop {
        if let Ok(Some(status)) = process.try_wait() {
            return Some(status);
        }
        if Instant::now() >= deadline {
            return None;
        }
        std::thread::sleep(EXIT_POLL);
    }
}

/// Collect the retained stderr within the final-drain bound.
///
/// The stderr drain handle is detached (dropped) rather than joined: joining is
/// an unbounded wait, and this bounded `recv_timeout` is the completion signal.
/// Returns the retained bytes, whether the retention cap was hit, and whether
/// the drain did not close within the bound.
pub(super) fn collect_retained_stderr(
    stderr: StderrDrain,
    bound: Duration,
) -> (String, bool, bool) {
    match stderr.receiver.recv_timeout(bound) {
        Ok(StderrOutcome::Retained { bytes, truncated }) => (
            String::from_utf8_lossy(&bytes).into_owned(),
            truncated,
            false,
        ),
        Err(_) => (String::new(), false, true),
    }
}

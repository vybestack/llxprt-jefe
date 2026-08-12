//! Provisional pre-Configure provider migration transaction (issue #391,
//! Slice E).
//!
//! This module is the smallest extension of the existing supervisor/process
//! ownership that drives a *provisional* provider process through the
//! pre-Configure migration handshake only:
//! `spawn → hello/hello-ack → migrate-config/migrated-config →
//! shutdown/shutdown-ack → reap`. It performs absolutely no `configure`,
//! `ready`, `invoke-action`, publication, or configure-secret resolution, and
//! it never resolves a secret value: the migrated `config` carries references
//! only and is forwarded verbatim.
//!
//! The provisional process is owned entirely inside [`run_migration`]; no
//! [`Child`], pipe, or thread handle leaves this module. It reuses the sole
//! process-tree spawn, the live pipe drains, the incremental framing/decoder
//! ([`parse_message`], the single wire authority), the closed encoder, and the
//! existing staged shutdown/reap plus cleanup-failure composition — never a
//! second supervisor, process manager, JSON parser, dependency, or settings
//! state.
//!
//! It returns only typed values: a [`MigratedConfigPayload`] on success, an
//! existing [`SupervisorFailure`] on failure, the ordered lifecycle
//! transcript, redacted retained stderr, and reaped/cleanup evidence. Lifecycle
//! order (hello-ack, then migrated-config, then shutdown-ack) is validated by
//! the driver's sequential phase structure and explicit per-phase kind checks,
//! reusing [`parse_message`] as the sole authority for kind, direction,
//! request-origin, positive generation, and closed payload validation, and
//! reusing the driver's ack-read classification ([`probe_ack`]) for the
//! shutdown-ack.
//!
//! [`Child`]: std::process::Child

use std::io::Write as _;
use std::process::{ChildStdin, Command};
use std::sync::mpsc::RecvTimeoutError;
use std::time::Duration;

use crate::domain::{CanonicalSemver, Id};

use super::drains::{
    FinalStdoutOutcome, StderrDrain, StdoutDrain, StdoutEvent, final_stdout_drain,
};
use super::driver::{AckProbe, ack_eof, ack_missing, ack_read_fault, probe_ack, unexpected_kind};
use super::dto::{ParsedMessage, ProviderMessage, ShutdownReason};
use super::encode::{encode_hello, encode_migrate_config, encode_shutdown};
use super::environment::{HostEnv, ProcessEnv, ProviderEnvironment, Redactor, build_process_env};
use super::error::ProviderError;
use super::identifiers::{Direction, MessageKind, RequestId, RequestOrigin};
use super::panel_model::{MigrateConfigPayload, MigratedConfigPayload};
use super::process_tree::{self, ProviderProcess};
use super::protocol::parse_message;
use super::redaction;
use super::supervisor::{
    CleanupFailure, LifecycleTranscript, ShutdownOutcome, SupervisorBounds, SupervisorFailure,
    TranscriptEntry, collect_retained_stderr, compose_cleanup_failure, staged_shutdown,
    wait_for_exit,
};

/// The graceful shutdown reason a completed migration uses: the transaction is
/// finished, so the provisional process is asked to exit cleanly.
const SHUTDOWN_REASON: ShutdownReason = ShutdownReason::Completed;

/// One provisional pre-Configure migration invocation request.
///
/// The primitive spawns the selected provider binary, drives the closed
/// migration handshake, and reaps the provisional process. It performs no
/// configure, so `environment` builds only the contained process environment;
/// no configure-secret sources are resolved. The `migrate.config` carries
/// references only and is forwarded verbatim to the provider.
#[derive(Debug, Clone)]
pub struct MigrationRequest {
    /// The selected provider binary.
    pub binary: std::path::PathBuf,
    /// Arguments to pass to the binary.
    pub arguments: Vec<String>,
    /// Contained working directory.
    pub working_dir: std::path::PathBuf,
    /// Contained process environment specification (CW10-14). Migration
    /// performs no configure, so no configure-secret sources are resolved.
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
    /// Fixed positive generation for this invocation (the process generation).
    pub generation: u64,
    /// Host request id for this invocation. Must be host-originated; the
    /// `migrated-config` response echoes it exactly.
    pub request_id: RequestId,
    /// The `migrate-config` payload. Its `config` carries references only; the
    /// primitive forwards it verbatim and never resolves secrets.
    pub migrate: MigrateConfigPayload,
}

/// The terminal outcome of a provisional migration transaction.
#[derive(Debug, Clone)]
pub enum MigrationOutcome {
    /// The provider returned a valid `migrated-config` that echoed the exact
    /// migration identity (versions, source config, and draft token) on the
    /// bound process generation and host request id.
    Migrated(MigratedConfigPayload),
    /// A supervisor-level failure (pre-spawn, spawn, protocol, timeout, crash,
    /// or I/O). The transcript and reaped/cleanup evidence explain the failure.
    Failed(SupervisorFailure),
}

/// The complete typed result of a provisional migration transaction.
#[derive(Debug, Clone)]
pub struct MigrationResult {
    /// The terminal outcome (first-terminal semantics).
    pub outcome: MigrationOutcome,
    /// The ordered lifecycle transcript.
    pub transcript: LifecycleTranscript,
    /// Retained stderr, redacted against any process-env secrets, capped.
    pub retained_stderr: String,
    /// Whether retained stderr was truncated at the retention cap.
    pub stderr_truncated: bool,
    /// Whether the process tree was reaped within the final-drain bound.
    pub process_reaped: bool,
    /// The provider exit code, if observed.
    pub exit_code: Option<i32>,
    /// A bounded cleanup failure observed after the terminal result, if the
    /// full shutdown/ack/EOF/reap lifecycle did not complete cleanly. Never
    /// replaces [`Self::outcome`] (first-terminal semantics).
    pub cleanup_failure: Option<CleanupFailure>,
}

impl MigrationResult {
    /// Build a result for a failure that occurred before any process existed.
    fn pre_spawn(failure: SupervisorFailure) -> Self {
        Self {
            outcome: MigrationOutcome::Failed(failure),
            transcript: LifecycleTranscript::default(),
            retained_stderr: String::new(),
            stderr_truncated: false,
            process_reaped: false,
            exit_code: None,
            cleanup_failure: None,
        }
    }
}

/// Run one provisional pre-Configure migration transaction.
///
/// Spawns the provider, drives the closed migration handshake
/// (`hello`/`hello-ack` then `migrate-config`/`migrated-config`), then performs
/// the staged shutdown/reap. Returns typed values only. The first terminal
/// frame is authoritative: a later shutdown-ack fault is reported as a
/// [`MigrationResult::cleanup_failure`], never replacing the migration outcome.
///
/// No `configure`, `ready`, `invoke-action`, publication, or configure-secret
/// resolution occurs. [`parse_message`] is the sole wire authority; lifecycle
/// order is validated by the sequential phase structure and per-phase kind,
/// generation, request-id, and echo checks.
pub fn run_migration<E: HostEnv>(
    request: &MigrationRequest,
    bounds: &SupervisorBounds,
    host_env: &E,
) -> MigrationResult {
    let mut transcript = LifecycleTranscript::default();

    if let Some(failure) = validate_request(request) {
        return MigrationResult::pre_spawn(failure);
    }

    let env = match build_process_env(
        &request.environment,
        &request.home,
        &request.tmpdir,
        &request.locale,
        host_env,
    ) {
        Ok(env) => env,
        Err(err) => return MigrationResult::pre_spawn(SupervisorFailure::Environment(err)),
    };
    let redactor = env.redactor();

    let command = build_command(request, &env);
    let (mut process, mut stdin, stdout, stderr) = match process_tree::spawn(command) {
        Ok(spawned) => spawned,
        Err(err) => {
            return MigrationResult::pre_spawn(SupervisorFailure::Spawn(err.to_string()));
        }
    };
    let pid = process.id();

    let stdout_drain = match StdoutDrain::spawn(stdout) {
        Ok(drain) => drain,
        Err(err) => {
            return reap_on_drain_failure(
                &mut process,
                transcript,
                bounds,
                SupervisorFailure::Io(format!("stdout drain spawn failed: {err}")),
            );
        }
    };
    let stderr_drain = match StderrDrain::spawn(stderr) {
        Ok(drain) => drain,
        Err(err) => {
            return reap_on_drain_failure(
                &mut process,
                transcript,
                bounds,
                SupervisorFailure::Io(format!("stderr drain spawn failed: {err}")),
            );
        }
    };

    let driven = drive_migration(request, bounds, &mut stdin, &stdout_drain, &mut transcript);
    complete_migration(
        &mut process,
        Some(stdin),
        transcript,
        pid,
        driven,
        CleanupContext {
            bounds,
            stdout_drain: &stdout_drain,
            stderr_drain,
            redactor: &redactor,
        },
    )
}

/// Validate the migration request before any process is spawned.
///
/// The positive generation and host-originated request id are migration
/// identity inputs the wire layer cannot supply later: a zero generation or a
/// provider-originated id cannot be correlated with a `migrated-config` echo.
fn validate_request(request: &MigrationRequest) -> Option<SupervisorFailure> {
    if request.generation == 0 {
        return Some(SupervisorFailure::Protocol(
            ProviderError::InvalidGeneration { value: 0 },
        ));
    }
    if request.request_id.origin() != RequestOrigin::Host {
        return Some(SupervisorFailure::Protocol(
            ProviderError::InvalidRequestOrigin {
                raw: request.request_id.as_str(),
                stream: Direction::HostToProvider.as_str().to_owned(),
            },
        ));
    }
    None
}

/// Build the fully-configured provider command from the request and environment.
fn build_command(request: &MigrationRequest, env: &ProcessEnv) -> Command {
    let mut command = Command::new(&request.binary);
    command.args(&request.arguments);
    command.current_dir(&request.working_dir);
    command.env_clear();
    for (key, value) in env.vars() {
        command.env(key, value);
    }
    command
}

/// The terminal outcome plus the pending cleanup-ack failure produced by
/// [`drive_migration`].
struct Driven {
    outcome: MigrationOutcome,
    ack_failure: Option<CleanupFailure>,
}

/// Drive the closed migration lifecycle.
///
/// Order is enforced by reading exactly one inbound frame per phase and
/// checking its kind, generation, request id (for `migrated-config`), and
/// echoed identity. [`parse_message`] validates direction, request-origin,
/// positive generation, and the closed payload before this function checks the
/// per-phase expectations.
fn drive_migration(
    request: &MigrationRequest,
    bounds: &SupervisorBounds,
    stdin: &mut ChildStdin,
    stdout: &StdoutDrain,
    transcript: &mut LifecycleTranscript,
) -> Driven {
    let generation = request.generation;

    // Phase 1: hello -> hello-ack.
    if let Err(error) = send(
        stdin,
        &encode_hello(
            &request.request_id,
            generation,
            &request.host_api,
            &request.plugin_id,
            &request.plugin_version,
        ),
    ) {
        return failure_outcome(SupervisorFailure::Io(error.to_string()));
    }
    transcript.push(TranscriptEntry::Sent(MessageKind::Hello));
    match read_bounded(
        stdout,
        bounds.handshake,
        MessageKind::HelloAck,
        generation,
        SupervisorFailure::HandshakeTimeout,
    ) {
        Ok(()) => {}
        Err(failure) => return failure_outcome(failure),
    }
    transcript.push(TranscriptEntry::Received(MessageKind::HelloAck));

    // Phase 2: migrate-config -> migrated-config (the migration terminal).
    if let Err(error) = send(
        stdin,
        &encode_migrate_config(&request.request_id, generation, &request.migrate),
    ) {
        return failure_outcome(SupervisorFailure::Io(error.to_string()));
    }
    transcript.push(TranscriptEntry::Sent(MessageKind::MigrateConfig));
    let migrated = match read_migrated_config(stdout, bounds.invocation, request) {
        Ok(payload) => payload,
        Err(failure) => return failure_outcome(failure),
    };
    transcript.push(TranscriptEntry::Received(MessageKind::MigratedConfig));
    let outcome = MigrationOutcome::Migrated(migrated);

    // Phase 3: shutdown -> shutdown-ack. The terminal is already bound; a
    // shutdown/ack fault is cleanup evidence, never a new outcome.
    let ack_failure = send_shutdown_and_observe_ack(stdin, stdout, request, bounds, transcript);
    Driven {
        outcome,
        ack_failure,
    }
}

/// Send the graceful `shutdown` frame and, for a healthy provider, observe the
/// `shutdown-ack`. A write failure or a wrong/missing/late/malformed ack is
/// cleanup evidence; the migration outcome is already terminal.
fn send_shutdown_and_observe_ack(
    stdin: &mut ChildStdin,
    stdout: &StdoutDrain,
    request: &MigrationRequest,
    bounds: &SupervisorBounds,
    transcript: &mut LifecycleTranscript,
) -> Option<CleanupFailure> {
    if let Err(error) = send(
        stdin,
        &encode_shutdown(&request.request_id, request.generation, SHUTDOWN_REASON),
    ) {
        return Some(CleanupFailure::Io(format!(
            "shutdown frame write failed: {error}"
        )));
    }
    transcript.push(TranscriptEntry::Sent(MessageKind::Shutdown));
    match observe_shutdown_ack(stdout, bounds.shutdown_ack, request.generation) {
        Ok(()) => {
            transcript.push(TranscriptEntry::Received(MessageKind::ShutdownAck));
            None
        }
        Err(fault) => Some(CleanupFailure::ShutdownAck(fault)),
    }
}

/// Build a [`Driven`] for a pre-terminal failure (no cleanup-ack failure).
fn failure_outcome(failure: SupervisorFailure) -> Driven {
    Driven {
        outcome: MigrationOutcome::Failed(failure),
        ack_failure: None,
    }
}

/// Write one encoded frame and flush.
fn send(stdin: &mut ChildStdin, frame: &[u8]) -> Result<(), std::io::Error> {
    stdin.write_all(frame)?;
    stdin.flush()?;
    Ok(())
}

/// Read one inbound frame, validate its kind and generation, and return a
/// typed failure. `on_timeout` distinguishes the handshake stage from the
/// migration invocation stage.
fn read_bounded(
    stdout: &StdoutDrain,
    bound: Duration,
    expected: MessageKind,
    generation: u64,
    on_timeout: SupervisorFailure,
) -> Result<(), SupervisorFailure> {
    let parsed = read_parsed(stdout, bound, on_timeout)?;
    check_kind_and_generation(&parsed, expected, generation)
}

/// Read one inbound frame and parse it, mapping read/parse/EOF/timeout outcomes
/// to supervisor failures.
fn read_parsed(
    stdout: &StdoutDrain,
    bound: Duration,
    on_timeout: SupervisorFailure,
) -> Result<ParsedMessage, SupervisorFailure> {
    match classify_inbound(stdout, bound) {
        Inbound::Message(parsed) => Ok(parsed),
        Inbound::Protocol(fault) => Err(SupervisorFailure::Protocol(fault)),
        Inbound::Timeout => Err(on_timeout),
        Inbound::ReadError => Err(SupervisorFailure::Io(
            "stdout read failed awaiting migration frame".to_owned(),
        )),
        Inbound::Eof => Err(SupervisorFailure::Crashed { exit: None }),
    }
}

/// Read the `migrated-config` terminal and validate its kind, generation,
/// request-id echo, and echoed migration identity.
fn read_migrated_config(
    stdout: &StdoutDrain,
    bound: Duration,
    request: &MigrationRequest,
) -> Result<MigratedConfigPayload, SupervisorFailure> {
    let parsed = read_parsed(stdout, bound, SupervisorFailure::InvocationTimeout)?;
    check_kind_and_generation(&parsed, MessageKind::MigratedConfig, request.generation)?;
    // parse_message already validated the request-id origin is Host; verify it
    // echoes the exact migrate-config request id.
    if parsed.request_id != request.request_id {
        return Err(SupervisorFailure::Protocol(ProviderError::InvalidValue {
            path: "migrated-config.request_id".to_owned(),
            reason: "does not echo the migrate-config request id".to_owned(),
        }));
    }
    let ProviderMessage::MigratedConfig(payload) = parsed.message else {
        return Err(SupervisorFailure::Protocol(unexpected_kind(
            MessageKind::MigratedConfig,
            MessageKind::MigratedConfig,
        )));
    };
    verify_echo(request, &payload)?;
    Ok(payload)
}

/// Verify the migrated-config echoes the migrate-config identity exactly:
/// versions, source config, and draft token.
fn verify_echo(
    request: &MigrationRequest,
    payload: &MigratedConfigPayload,
) -> Result<(), SupervisorFailure> {
    if payload.from_version != request.migrate.from_version {
        return Err(echo_mismatch("from_version"));
    }
    if payload.to_version != request.migrate.to_version {
        return Err(echo_mismatch("to_version"));
    }
    if payload.draft_token != request.migrate.draft_token {
        return Err(echo_mismatch("draft_token"));
    }
    if payload.config != request.migrate.config {
        return Err(SupervisorFailure::Protocol(ProviderError::InvalidValue {
            path: "migrated-config.config".to_owned(),
            reason: "does not echo the migrate-config source config".to_owned(),
        }));
    }
    Ok(())
}

/// A migrated-config echo mismatch for a scalar identity field.
fn echo_mismatch(field: &'static str) -> SupervisorFailure {
    SupervisorFailure::Protocol(ProviderError::InvalidValue {
        path: format!("migrated-config.{field}"),
        reason: format!("does not echo the migrate-config {field}"),
    })
}

/// Check the parsed frame's kind and generation against the phase expectation.
fn check_kind_and_generation(
    parsed: &ParsedMessage,
    expected: MessageKind,
    generation: u64,
) -> Result<(), SupervisorFailure> {
    if parsed.kind() != expected {
        return Err(SupervisorFailure::Protocol(unexpected_kind(
            expected,
            parsed.kind(),
        )));
    }
    if parsed.generation != generation {
        return Err(SupervisorFailure::Protocol(
            ProviderError::InvalidGeneration {
                value: parsed.generation,
            },
        ));
    }
    Ok(())
}

/// Observe the `shutdown-ack`, reusing the driver's ack-read classification
/// ([`probe_ack`]). A wrong/missing/late/malformed/out-of-order ack or a
/// preceding EOF is a typed cleanup fault.
fn observe_shutdown_ack(
    stdout: &StdoutDrain,
    bound: Duration,
    generation: u64,
) -> Result<(), ProviderError> {
    match probe_ack(stdout, bound) {
        AckProbe::Message(parsed) => {
            if parsed.kind() != MessageKind::ShutdownAck {
                return Err(unexpected_kind(MessageKind::ShutdownAck, parsed.kind()));
            }
            if parsed.generation != generation {
                return Err(ProviderError::InvalidGeneration {
                    value: parsed.generation,
                });
            }
            Ok(())
        }
        AckProbe::Protocol(error) => Err(error),
        AckProbe::Timeout => Err(ack_missing()),
        AckProbe::ReadError => Err(ack_read_fault()),
        AckProbe::Eof => Err(ack_eof()),
    }
}

/// The classification of one bounded inbound read (mirrors the driver's
/// `AckProbe` for the handshake/invocation phases).
enum Inbound {
    /// A parsed, well-formed frame.
    Message(ParsedMessage),
    /// A framing/shape/direction fault.
    Protocol(ProviderError),
    /// No frame arrived within the bound.
    Timeout,
    /// The stdout read failed.
    ReadError,
    /// stdout reached EOF (the provider closed/exited).
    Eof,
}

/// Classify one bounded inbound stdout read.
fn classify_inbound(stdout: &StdoutDrain, bound: Duration) -> Inbound {
    match stdout.receiver.recv_timeout(bound) {
        Ok(StdoutEvent::Frame(frame)) => match parse_message(&frame, Direction::ProviderToHost) {
            Ok(parsed) => Inbound::Message(parsed),
            Err(fault) => Inbound::Protocol(fault),
        },
        Ok(StdoutEvent::Oversize(fault)) => Inbound::Protocol(fault),
        Ok(StdoutEvent::ReadError) => Inbound::ReadError,
        Err(RecvTimeoutError::Timeout) => Inbound::Timeout,
        Err(RecvTimeoutError::Disconnected) => Inbound::Eof,
    }
}

/// Drain and redaction context shared by the final cleanup phase.
struct CleanupContext<'a> {
    bounds: &'a SupervisorBounds,
    stdout_drain: &'a StdoutDrain,
    stderr_drain: StderrDrain,
    redactor: &'a Redactor,
}

/// Run the staged shutdown, bounded final drains, and cleanup-failure
/// composition after the migration terminal has been reached.
fn complete_migration(
    process: &mut ProviderProcess,
    stdin: Option<ChildStdin>,
    mut transcript: LifecycleTranscript,
    pid: u32,
    driven: Driven,
    cleanup: CleanupContext<'_>,
) -> MigrationResult {
    // Staged shutdown always runs. The stdout drain is detached (handle
    // dropped): the lifecycle is complete and the bounded reaper closed the
    // pipe, so joining would be an unbounded wait for no new information.
    let (shutdown_outcome, _signal_errors) = staged_shutdown(process, stdin, cleanup.bounds, pid);
    let process_reaped = matches!(shutdown_outcome, ShutdownOutcome::Exited(_));
    let exit_code = match shutdown_outcome {
        ShutdownOutcome::Exited(code) => code,
        ShutdownOutcome::NotReaped => None,
    };

    let stdout_final =
        final_stdout_drain(&cleanup.stdout_drain.receiver, cleanup.bounds.final_drain);
    if matches!(stdout_final, FinalStdoutOutcome::Eof) {
        transcript.push(TranscriptEntry::Eof);
    }
    if process_reaped {
        transcript.push(TranscriptEntry::Reaped);
    }

    let (retained, truncated, stderr_timed_out) =
        collect_retained_stderr(cleanup.stderr_drain, cleanup.bounds.final_drain);
    let retained_stderr = cleanup.redactor.redact(&retained).into_owned();
    let cleanup_failure = compose_cleanup_failure(
        process_reaped,
        driven.ack_failure,
        stdout_final,
        stderr_timed_out,
    );
    MigrationResult {
        outcome: driven.outcome,
        transcript,
        retained_stderr,
        stderr_truncated: truncated,
        process_reaped,
        exit_code,
        cleanup_failure: cleanup_failure
            .map(|failure| redaction::redact_cleanup_failure(failure, cleanup.redactor)),
    }
}

/// Force-kill and reap a process whose drain could not start, recording only
/// evidence that was actually observed (no EOF claim; `Reaped` only when the
/// bounded reap observed an exit).
fn reap_on_drain_failure(
    process: &mut ProviderProcess,
    mut transcript: LifecycleTranscript,
    bounds: &SupervisorBounds,
    failure: SupervisorFailure,
) -> MigrationResult {
    // Best-effort force-kill; the bounded reap below is the sole reap authority.
    drop(process.force_kill_tree());
    let reaped = wait_for_exit(process, bounds.final_drain).is_some();
    if reaped {
        transcript.push(TranscriptEntry::Reaped);
    }
    MigrationResult {
        outcome: MigrationOutcome::Failed(failure),
        transcript,
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

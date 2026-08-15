//! Per-candidate persistent startup (issue #390 CW-10, Slice C2).
//!
//! This private module owns the mechanics of starting one persistent candidate
//! process from its already-resolved launch inputs — spawn, drains, the closed
//! `hello`/`hello-ack` → `configure`/`ready` handshake with per-stage bounds,
//! the manifest-declared capability-subset check, and the rollback reap of a
//! candidate that failed after spawn. Environment construction and secret
//! resolution happen once, before any spawn, in [`super::persistent`]'s
//! preparation phase; this module reuses the Slice C1 framing, drains,
//! process-tree spawn, and staged-reap helpers and keeps no application
//! state.
//!
//! It hands back a typed [`StartOutcome`] (an owned ready process or a typed
//! failure plus reap evidence) to [`super::persistent`], which sequences
//! candidates in plugin-id order and performs the atomic all-or-nothing
//! publication. No `Child`, pipe, or thread handle leaves the provider module.

use std::io::Write as _;
use std::process::{ChildStdin, Command};
use std::sync::mpsc;
use std::time::Duration;

use crate::domain::Id;

use super::drains::{StderrDrain, StdoutDrain, StdoutEvent};
use super::driver::unexpected_kind;
use super::dto::{Capability, ConfigurePayload, ParsedMessage, ProviderMessage};
use super::encode::{encode_configure, encode_hello};
use super::environment::{ProcessEnv, Redactor};
use super::error;
use super::persistent::{
    CandidateFailure, OwnedCandidate, PersistentCandidate, PersistentPhase, PreparedEnvironment,
    ReadyCandidate, ReapedCandidate, capability_mismatch_failure, first_undeclared_capability,
    reap_owned,
};
use super::process_tree::{self, ProviderProcess};
use super::protocol::{Direction, LifecycleOrder, MessageKind, parse_message};
use super::redaction;
use super::supervisor::{CleanupFailure, SupervisorBounds, SupervisorFailure, wait_for_exit};

/// The outcome of starting one candidate.
pub(super) enum StartOutcome {
    /// The candidate reached `ready` and is owned by the supervisor. The owned
    /// process is boxed to keep this enum small (it travels through a
    /// `Result`'s `Err` slot during rollback).
    Started(Box<OwnedCandidate>, ReadyCandidate),
    /// The candidate failed; `reaped` is `Some` when a process existed.
    Failed {
        /// Which candidate/phase failed.
        failure: CandidateFailure,
        /// Reap evidence for the failing candidate, if a process existed.
        reaped: Option<ReapedCandidate>,
    },
}

/// Start one candidate from its already-prepared environment: spawn, drains,
/// handshake, capability check.
///
/// The [`PreparedEnvironment`] was resolved once before any spawn; its values
/// drive the process environment, the `Configure` payload, and every
/// redaction, so mutable host inputs are never read a second time.
pub(super) fn start_prepared(
    candidate: &PersistentCandidate,
    prepared: PreparedEnvironment,
    bounds: &SupervisorBounds,
) -> StartOutcome {
    let plugin_id = candidate.plugin_id.clone();
    let PreparedEnvironment {
        env,
        configure,
        redactor,
    } = prepared;
    let spawned = match spawn_with_drains(candidate, &env, bounds, &redactor) {
        Ok(spawned) => spawned,
        Err(outcome) => return *outcome,
    };
    finalize_start(candidate, plugin_id, configure, spawned, bounds, &redactor)
}

/// Spawn the process and its drain threads. The error is boxed to keep the
/// `Result` small (a spawn failure is rare, and the full `StartOutcome` carries
/// owned strings). Every spawn/drain diagnostic is redacted against the resolved
/// secrets so no configure secret or explicit secret-env value leaks.
fn spawn_with_drains(
    candidate: &PersistentCandidate,
    env: &ProcessEnv,
    bounds: &SupervisorBounds,
    redactor: &Redactor,
) -> Result<Spawned, Box<StartOutcome>> {
    let command = build_command(candidate, env);
    let (process, stdin, stdout, stderr) = match process_tree::spawn(command) {
        Ok(spawned) => spawned,
        Err(err) => {
            return Err(Box::new(failed_before_spawn(
                candidate.plugin_id.clone(),
                PersistentPhase::Spawn,
                redaction::redact_supervisor_failure(
                    SupervisorFailure::Spawn(err.to_string()),
                    redactor,
                ),
            )));
        }
    };
    let pid = process.id();
    let stdout_drain = match StdoutDrain::spawn(stdout) {
        Ok(drain) => drain,
        Err(err) => {
            return Err(Box::new(failed_after_spawn(
                candidate.plugin_id.clone(),
                PersistentPhase::Spawn,
                redaction::redact_supervisor_failure(
                    SupervisorFailure::Io(format!("stdout drain spawn failed: {err}")),
                    redactor,
                ),
                process,
                Some(stdin),
                bounds,
            )));
        }
    };
    let stderr_drain = match StderrDrain::spawn(stderr) {
        Ok(drain) => drain,
        Err(err) => {
            return Err(Box::new(failed_after_spawn(
                candidate.plugin_id.clone(),
                PersistentPhase::Spawn,
                redaction::redact_supervisor_failure(
                    SupervisorFailure::Io(format!("stderr drain spawn failed: {err}")),
                    redactor,
                ),
                process,
                Some(stdin),
                bounds,
            )));
        }
    };
    Ok(Spawned {
        process,
        stdin,
        stdout_drain,
        stderr_drain,
        pid,
    })
}

/// Build the fully-configured provider command from the request and environment.
fn build_command(candidate: &PersistentCandidate, env: &ProcessEnv) -> Command {
    let mut command = Command::new(&candidate.binary);
    command.args(&candidate.arguments);
    command.current_dir(&candidate.working_dir);
    command.env_clear();
    for (key, value) in env.vars() {
        command.env(key, value);
    }
    command
}

/// A spawned candidate's live process and drains, before the handshake.
struct Spawned {
    process: ProviderProcess,
    stdin: ChildStdin,
    stdout_drain: StdoutDrain,
    stderr_drain: StderrDrain,
    pid: u32,
}

/// Runtime context threaded from the handshake to an owned candidate: the
/// resolved-secret redactor, the live lifecycle validator (advanced to `ready`
/// for a healthy candidate, fresh for an unhealthy one), and the health flag.
struct OwnedContext {
    redactor: Redactor,
    lifecycle: LifecycleOrder,
    healthy: bool,
}

/// Drive the handshake and capability check; on success own the ready process.
fn finalize_start(
    candidate: &PersistentCandidate,
    plugin_id: Id,
    configure: ConfigurePayload,
    mut spawned: Spawned,
    bounds: &SupervisorBounds,
    redactor: &Redactor,
) -> StartOutcome {
    let handshake = drive_handshake(
        candidate,
        &mut spawned.stdin,
        &spawned.stdout_drain,
        bounds,
        &configure,
    );
    match handshake {
        Ok(outcome) => finalize_ready(candidate, plugin_id, outcome, spawned, bounds, redactor),
        Err(fault) => finalize_failure(candidate, plugin_id, fault, spawned, bounds, redactor),
    }
}

/// Own a healthy `ready` candidate, or reject it for an undeclared capability.
fn finalize_ready(
    candidate: &PersistentCandidate,
    plugin_id: Id,
    outcome: HandshakeOutcome,
    spawned: Spawned,
    bounds: &SupervisorBounds,
    redactor: &Redactor,
) -> StartOutcome {
    let HandshakeOutcome {
        capabilities,
        lifecycle,
    } = outcome;
    if let Some(offender) =
        first_undeclared_capability(&candidate.declared_capabilities, &capabilities)
    {
        let context = OwnedContext {
            redactor: redactor.clone(),
            lifecycle,
            healthy: true,
        };
        let owned = owned_from(candidate, plugin_id.clone(), capabilities, spawned, context);
        return StartOutcome::Failed {
            failure: CandidateFailure {
                plugin_id,
                phase: PersistentPhase::Capability,
                failure: redaction::redact_supervisor_failure(
                    capability_mismatch_failure(offender),
                    redactor,
                ),
            },
            reaped: Some(reap_owned(owned, bounds)),
        };
    }
    let ready = ReadyCandidate {
        plugin_id: plugin_id.clone(),
        plugin_version: candidate.plugin_version.clone(),
        capabilities: capabilities.clone(),
    };
    let context = OwnedContext {
        redactor: redactor.clone(),
        lifecycle,
        healthy: true,
    };
    let owned = owned_from(candidate, plugin_id, capabilities, spawned, context);
    StartOutcome::Started(Box::new(owned), ready)
}

/// Own and reap a candidate that failed its handshake (unhealthy: best-effort).
fn finalize_failure(
    candidate: &PersistentCandidate,
    plugin_id: Id,
    fault: HandshakeFault,
    spawned: Spawned,
    bounds: &SupervisorBounds,
    redactor: &Redactor,
) -> StartOutcome {
    // The candidate failed before reaching ready, so it is not healthy: its
    // rollback reap performs best-effort signalling (no ack is expected). A
    // fresh lifecycle validator is carried harmlessly for the unhealthy reap.
    let context = OwnedContext {
        redactor: redactor.clone(),
        lifecycle: LifecycleOrder::new(),
        healthy: false,
    };
    let owned = owned_from(candidate, plugin_id.clone(), Vec::new(), spawned, context);
    let reaped = reap_owned(owned, bounds);
    StartOutcome::Failed {
        failure: CandidateFailure {
            plugin_id,
            phase: fault.phase,
            failure: redaction::redact_supervisor_failure(fault.failure, redactor),
        },
        reaped: Some(reaped),
    }
}

/// Build an owned candidate from the spawned parts, carrying the redactor and
/// live lifecycle/health state it needs for its later shutdown/rollback reap.
fn owned_from(
    candidate: &PersistentCandidate,
    plugin_id: Id,
    capabilities: Vec<Capability>,
    spawned: Spawned,
    context: OwnedContext,
) -> OwnedCandidate {
    OwnedCandidate {
        plugin_id,
        capabilities,
        process: spawned.process,
        stdin: Some(spawned.stdin),
        stdout_drain: spawned.stdout_drain,
        stderr_drain: spawned.stderr_drain,
        pid: spawned.pid,
        request_id: candidate.request_id.clone(),
        generation: candidate.generation,
        exited: false,
        redactor: context.redactor,
        lifecycle: context.lifecycle,
        healthy: context.healthy,
        fault: None,
    }
}

/// A failure that occurred before any process existed (no reap evidence).
fn failed_before_spawn(
    plugin_id: Id,
    phase: PersistentPhase,
    failure: SupervisorFailure,
) -> StartOutcome {
    StartOutcome::Failed {
        failure: CandidateFailure {
            plugin_id,
            phase,
            failure,
        },
        reaped: None,
    }
}

/// A failure that occurred after spawn but before the drains were live;
/// force-kill and bounded-reap the leader. There are no live drains, so pipe
/// closure cannot be observed; the cleanup evidence is reap-only, mirroring the
/// one-shot drain-spawn-failure path.
fn failed_after_spawn(
    plugin_id: Id,
    phase: PersistentPhase,
    failure: SupervisorFailure,
    mut process: ProviderProcess,
    stdin: Option<ChildStdin>,
    bounds: &SupervisorBounds,
) -> StartOutcome {
    drop(stdin);
    let _ = process.force_kill_tree();
    let reaped = wait_for_exit(&mut process, bounds.final_drain).is_some();
    let cleanup_failure = if reaped {
        None
    } else {
        Some(CleanupFailure::NotReaped)
    };
    StartOutcome::Failed {
        failure: CandidateFailure {
            plugin_id: plugin_id.clone(),
            phase,
            failure,
        },
        reaped: Some(ReapedCandidate {
            plugin_id,
            reaped,
            cleanup_failure,
        }),
    }
}

/// The successful handshake outcome: the `ready` capabilities and the live
/// lifecycle validator advanced to `ready`.
struct HandshakeOutcome {
    capabilities: Vec<Capability>,
    lifecycle: LifecycleOrder,
}

/// A handshake failure: the phase that failed and the typed supervisor failure.
struct HandshakeFault {
    phase: PersistentPhase,
    failure: SupervisorFailure,
}

/// Drive `hello`/`hello-ack` then `configure`/`ready`.
///
/// Returns the ready capabilities and the live lifecycle validator (advanced to
/// `ready`), or the phase that failed.
fn drive_handshake(
    candidate: &PersistentCandidate,
    stdin: &mut ChildStdin,
    stdout: &StdoutDrain,
    bounds: &SupervisorBounds,
    configure: &ConfigurePayload,
) -> Result<HandshakeOutcome, HandshakeFault> {
    let mut lifecycle = LifecycleOrder::new();
    let generation = candidate.generation;

    send_and_observe(
        stdin,
        encode_hello(
            &candidate.request_id,
            generation,
            &candidate.host_api,
            &candidate.plugin_id,
            &candidate.plugin_version,
        ),
        &mut lifecycle,
        MessageKind::Hello,
        generation,
        PersistentPhase::HelloAck,
    )?;
    expect_inbound(
        stdout,
        &mut lifecycle,
        bounds.handshake,
        MessageKind::HelloAck,
        PersistentPhase::HelloAck,
    )?;

    send_and_observe(
        stdin,
        encode_configure(&candidate.request_id, generation, configure),
        &mut lifecycle,
        MessageKind::Configure,
        generation,
        PersistentPhase::Configure,
    )?;
    let ready = expect_inbound(
        stdout,
        &mut lifecycle,
        bounds.handshake,
        MessageKind::Ready,
        PersistentPhase::Ready,
    )?;
    let ProviderMessage::Ready(payload) = ready.message else {
        return Err(HandshakeFault {
            phase: PersistentPhase::Ready,
            failure: SupervisorFailure::Protocol(unexpected_kind(MessageKind::Ready, ready.kind())),
        });
    };
    Ok(HandshakeOutcome {
        capabilities: payload.capabilities,
        lifecycle,
    })
}

/// Send one outbound frame and observe it in the lifecycle, mapping any failure
/// to the given handshake phase.
fn send_and_observe(
    stdin: &mut ChildStdin,
    frame: Vec<u8>,
    lifecycle: &mut LifecycleOrder,
    kind: MessageKind,
    generation: u64,
    phase: PersistentPhase,
) -> Result<(), HandshakeFault> {
    send(stdin, frame).map_err(|failure| HandshakeFault { phase, failure })?;
    lifecycle
        .observe(kind, generation)
        .map_err(|fault| HandshakeFault {
            phase,
            failure: SupervisorFailure::Protocol(fault),
        })
}

/// Read one inbound message within `bound`, verifying it is `expected`.
fn expect_inbound(
    stdout: &StdoutDrain,
    lifecycle: &mut LifecycleOrder,
    bound: Duration,
    expected: MessageKind,
    phase: PersistentPhase,
) -> Result<ParsedMessage, HandshakeFault> {
    let message = read_message(stdout, lifecycle, bound)
        .map_err(|failure| HandshakeFault { phase, failure })?;
    if message.kind() != expected {
        return Err(HandshakeFault {
            phase,
            failure: SupervisorFailure::Protocol(unexpected_kind(expected, message.kind())),
        });
    }
    Ok(message)
}

/// Write one encoded frame to the provider stdin.
fn send(stdin: &mut ChildStdin, frame: Vec<u8>) -> Result<(), SupervisorFailure> {
    stdin
        .write_all(&frame)
        .map_err(|err| SupervisorFailure::Io(err.to_string()))?;
    stdin
        .flush()
        .map_err(|err| SupervisorFailure::Io(err.to_string()))?;
    Ok(())
}

/// Read, validate, and observe one inbound provider message.
fn read_message(
    stdout: &StdoutDrain,
    lifecycle: &mut LifecycleOrder,
    bound: Duration,
) -> Result<ParsedMessage, SupervisorFailure> {
    match read_inbound(&stdout.receiver, bound) {
        Inbound::Message(parsed) => {
            lifecycle
                .observe(parsed.kind(), parsed.generation)
                .map_err(SupervisorFailure::Protocol)?;
            Ok(parsed)
        }
        Inbound::Protocol(fault) => Err(SupervisorFailure::Protocol(fault)),
        Inbound::Timeout => Err(SupervisorFailure::HandshakeTimeout),
        Inbound::ReadError => Err(SupervisorFailure::Io("stdout read failed".to_owned())),
        Inbound::Eof => Err(SupervisorFailure::Crashed { exit: None }),
    }
}

/// One classified inbound read result.
enum Inbound {
    Message(ParsedMessage),
    Protocol(error::ProviderError),
    Timeout,
    ReadError,
    Eof,
}

/// Read and classify one inbound stdout event within `bound`.
fn read_inbound(receiver: &mpsc::Receiver<StdoutEvent>, bound: Duration) -> Inbound {
    match receiver.recv_timeout(bound) {
        Ok(StdoutEvent::Frame(frame)) => match parse_message(&frame, Direction::ProviderToHost) {
            Ok(parsed) => Inbound::Message(parsed),
            Err(fault) => Inbound::Protocol(fault),
        },
        Ok(StdoutEvent::Oversize(fault)) => Inbound::Protocol(fault),
        Ok(StdoutEvent::ReadError) => Inbound::ReadError,
        Err(mpsc::RecvTimeoutError::Timeout) => Inbound::Timeout,
        Err(mpsc::RecvTimeoutError::Disconnected) => Inbound::Eof,
    }
}

//! Lifecycle driver for the one-shot provider supervisor
//! (issue #390 CW-10, Slice C1).
//!
//! [`Driver`] is the per-message state machine the supervisor constructs once a
//! provider process and its drains are live. It owns no process handle: it
//! borrows the stdin writer, the bounded outbound queue, the stdout drain
//! receiver, the lifecycle and progress validators, the transcript, and the
//! pending cleanup-ack slot, and drives the exact closed lifecycle —
//! hello/hello-ack → configure/ready → invoke-action → progress → the single
//! terminal → shutdown/shutdown-ack → clean EOF.
//!
//! Every protocol or lifecycle deviation is a typed failure; the strict
//! shutdown-ack validation produces a [`CleanupFailure`] that the supervisor
//! reports alongside (never replacing) the authoritative terminal result.
//!
//! No process, application state, effect, or persistence lives here.

use std::io::Write;
use std::process::ChildStdin;
use std::sync::mpsc::RecvTimeoutError;
use std::time::Duration;

use super::drains::{StdoutDrain, StdoutEvent};
use super::encode;
use super::identifiers::Direction;
use super::outbound::{OutboundError, OutboundQueue};
use super::protocol::{LifecycleOrder, MessageKind, ProgressTracker, parse_message};
use super::supervisor::{
    CleanupFailure, LifecycleTranscript, OneShotOutcome, OneShotRequest, SupervisorBounds,
    SupervisorFailure, TranscriptEntry,
};
use super::{dto, error};

/// Mutable driver state shared by the lifecycle steps.
///
/// Constructed by the supervisor after spawn; each method advances one phase.
pub(super) struct Driver<'a> {
    /// The one-shot request (binary identity, generation, payloads).
    pub request: &'a OneShotRequest,
    /// Injectable timeout/shutdown bounds.
    pub bounds: &'a SupervisorBounds,
    /// The provider stdin writer.
    pub stdin: &'a mut ChildStdin,
    /// The bounded outbound queue.
    pub queue: &'a mut OutboundQueue,
    /// The stdout drain receiver.
    pub stdout: &'a StdoutDrain,
    /// The lifecycle order validator.
    pub lifecycle: &'a mut LifecycleOrder,
    /// The progress monotonicity validator.
    pub progress: &'a mut ProgressTracker,
    /// The ordered lifecycle transcript.
    pub transcript: &'a mut LifecycleTranscript,
    /// Whether the generation is still healthy (no outbound lifecycle fault).
    pub healthy: &'a mut bool,
    /// The merged configure payload.
    pub configure: &'a dto::ConfigurePayload,
    /// The pending shutdown-ack cleanup failure, if the lifecycle deviated.
    pub cleanup_ack_failure: &'a mut Option<CleanupFailure>,
}

impl Driver<'_> {
    /// Drive the full handshake and invocation to the single terminal result.
    pub(super) fn run(&mut self) -> OneShotOutcome {
        if let Some(failure) = self.handshake() {
            return OneShotOutcome::Failed(failure);
        }
        match self.invocation() {
            Ok(outcome) => outcome,
            Err(failure) => OneShotOutcome::Failed(failure),
        }
    }

    /// Drive hello/hello-ack, configure/ready. Returns a failure if any stage
    /// cannot complete; on success the process is steady.
    fn handshake(&mut self) -> Option<SupervisorFailure> {
        if let Err(failure) = self.send(encode::encode_hello(
            &self.request.request_id,
            self.request.generation,
            &self.request.host_api,
            &self.request.plugin_id,
            &self.request.plugin_version,
        )) {
            return Some(failure);
        }
        self.transcript
            .push(TranscriptEntry::Sent(MessageKind::Hello));
        self.observe_outbound(MessageKind::Hello);

        if let Some(failure) = self.expect_one(MessageKind::HelloAck, self.bounds.handshake) {
            return Some(failure);
        }

        if let Err(failure) = self.send(encode::encode_configure(
            &self.request.request_id,
            self.request.generation,
            self.configure,
        )) {
            return Some(failure);
        }
        self.transcript
            .push(TranscriptEntry::Sent(MessageKind::Configure));
        self.observe_outbound(MessageKind::Configure);

        self.expect_one(MessageKind::Ready, self.bounds.handshake)
    }

    /// Drive invoke-action, progress, the single terminal, and best-effort
    /// graceful shutdown.
    fn invocation(&mut self) -> Result<OneShotOutcome, SupervisorFailure> {
        self.send(encode::encode_invoke_action(
            &self.request.request_id,
            self.request.generation,
            &self.request.invocation,
        ))?;
        self.transcript
            .push(TranscriptEntry::Sent(MessageKind::InvokeAction));
        self.observe_outbound(MessageKind::InvokeAction);

        let outcome = self.drive_to_terminal(self.bounds.invocation)?;

        // Best-effort graceful shutdown; the staged reaper cleans up regardless.
        if self
            .send(encode::encode_shutdown(
                &self.request.request_id,
                self.request.generation,
                dto::ShutdownReason::Completed,
            ))
            .is_ok()
        {
            self.transcript
                .push(TranscriptEntry::Sent(MessageKind::Shutdown));
            self.observe_outbound(MessageKind::Shutdown);
        }
        *self.cleanup_ack_failure = self.observe_shutdown_ack();
        Ok(outcome)
    }

    /// Read progress events until the single terminal outcome/error (or a
    /// supervisor failure) is reached.
    fn drive_to_terminal(
        &mut self,
        timeout: Duration,
    ) -> Result<OneShotOutcome, SupervisorFailure> {
        loop {
            match self.read_inbound(timeout, SupervisorFailure::InvocationTimeout) {
                Inbound::Message(parsed) => match self.observe_inbound(&parsed) {
                    Ok(()) => match &parsed.message {
                        dto::ProviderMessage::Progress(_) => {}
                        dto::ProviderMessage::Outcome(outcome) => {
                            return Ok(OneShotOutcome::Completed(outcome.clone()));
                        }
                        dto::ProviderMessage::Error(error) => {
                            return Ok(OneShotOutcome::ProviderError(error.clone()));
                        }
                        _ => {
                            return Ok(OneShotOutcome::Failed(SupervisorFailure::Protocol(
                                unexpected_after_invoke(),
                            )));
                        }
                    },
                    Err(error) => {
                        return Ok(OneShotOutcome::Failed(SupervisorFailure::Protocol(error)));
                    }
                },
                Inbound::Protocol(error) => {
                    return Ok(OneShotOutcome::Failed(SupervisorFailure::Protocol(error)));
                }
                Inbound::Timeout(failure) => return Err(failure),
                Inbound::ReadError => {
                    return Err(SupervisorFailure::Io("stdout read failed".to_owned()));
                }
                Inbound::Eof => {
                    return Ok(OneShotOutcome::Failed(SupervisorFailure::Crashed {
                        exit: None,
                    }));
                }
            }
        }
    }

    /// Expect exactly one inbound message of `expected` within `timeout`.
    fn expect_one(
        &mut self,
        expected: MessageKind,
        timeout: Duration,
    ) -> Option<SupervisorFailure> {
        match self.read_inbound(timeout, SupervisorFailure::HandshakeTimeout) {
            Inbound::Message(parsed) => match self.observe_inbound(&parsed) {
                Ok(()) if parsed.kind() == expected => None,
                Ok(()) => Some(SupervisorFailure::Protocol(unexpected_kind(
                    expected,
                    parsed.kind(),
                ))),
                Err(error) => Some(SupervisorFailure::Protocol(error)),
            },
            Inbound::Protocol(error) => Some(SupervisorFailure::Protocol(error)),
            Inbound::Timeout(failure) => Some(failure),
            Inbound::ReadError => Some(SupervisorFailure::Io("stdout read failed".to_owned())),
            Inbound::Eof => Some(SupervisorFailure::Crashed { exit: None }),
        }
    }

    /// Strictly observe the `shutdown-ack`.
    ///
    /// The full lifecycle requires shutdown/ack/EOF/reap, but the EOF that must
    /// follow a valid ack is observed by the supervisor's bounded final stdout
    /// drain after process exit/kill — not here. This method validates only the
    /// ack itself: a wrong kind, a malformed line, a wrong generation or order,
    /// a missing (timeout) ack, an EOF before the ack, or a read fault each
    /// produce a [`CleanupFailure`]. After a valid ack it returns `None` and
    /// defers the EOF/data-after-ack observation to the final drain, so a
    /// timeout waiting for EOF is never treated as success. The first terminal
    /// result remains authoritative; this never replaces the outcome.
    fn observe_shutdown_ack(&mut self) -> Option<CleanupFailure> {
        match self.read_inbound(self.bounds.shutdown_ack, SupervisorFailure::ShutdownTimeout) {
            Inbound::Message(parsed) => {
                if parsed.kind() != MessageKind::ShutdownAck {
                    return Some(CleanupFailure::ShutdownAck(unexpected_kind(
                        MessageKind::ShutdownAck,
                        parsed.kind(),
                    )));
                }
                if let Err(error) = self.observe_inbound(&parsed) {
                    return Some(CleanupFailure::ShutdownAck(error));
                }
                // A valid shutdown-ack was observed. The bounded final stdout
                // drain owns the EOF observation and rejects any data-after-ack;
                // this method must not treat a later EOF timeout as success.
                None
            }
            Inbound::Protocol(error) => Some(CleanupFailure::ShutdownAck(error)),
            Inbound::Timeout(_) => Some(CleanupFailure::ShutdownAck(ack_missing())),
            Inbound::ReadError => Some(CleanupFailure::ShutdownAck(ack_read_fault())),
            Inbound::Eof => Some(CleanupFailure::ShutdownAck(ack_eof())),
        }
    }

    /// Observe one inbound message in the lifecycle and progress validators and
    /// record its transcript entry.
    fn observe_inbound(&mut self, parsed: &dto::ParsedMessage) -> Result<(), error::ProviderError> {
        self.lifecycle.observe(parsed.kind(), parsed.generation)?;
        match &parsed.message {
            dto::ProviderMessage::Progress(progress) => {
                self.progress
                    .observe(progress.sequence, progress.completed, progress.total)?;
                self.transcript
                    .push(TranscriptEntry::Progress(progress.sequence));
            }
            _ => self
                .transcript
                .push(TranscriptEntry::Received(parsed.kind())),
        }
        Ok(())
    }

    /// Observe one outbound host message (advancing the lifecycle phase) while
    /// the generation is still healthy.
    fn observe_outbound(&mut self, kind: MessageKind) {
        if *self.healthy
            && let Err(error) = self.lifecycle.observe(kind, self.request.generation)
        {
            *self.healthy = false;
            self.transcript.push(TranscriptEntry::Received(kind));
            let _ = error;
        }
    }

    /// Read and classify one inbound event.
    fn read_inbound(&self, timeout: Duration, timeout_failure: SupervisorFailure) -> Inbound {
        match self.stdout.receiver.recv_timeout(timeout) {
            Ok(StdoutEvent::Frame(frame)) => match parse_message(&frame, Direction::ProviderToHost)
            {
                Ok(parsed) => Inbound::Message(parsed),
                Err(error) => Inbound::Protocol(error),
            },
            Ok(StdoutEvent::Oversize(error)) => Inbound::Protocol(error),
            Ok(StdoutEvent::ReadError) => Inbound::ReadError,
            Err(RecvTimeoutError::Timeout) => Inbound::Timeout(timeout_failure),
            Err(RecvTimeoutError::Disconnected) => Inbound::Eof,
        }
    }

    /// Enqueue one encoded frame through the bounded outbound queue and flush it
    /// to the provider stdin.
    fn send(&mut self, frame: Vec<u8>) -> Result<(), SupervisorFailure> {
        self.queue.enqueue(frame).map_err(map_outbound_error)?;
        for pending in self.queue.drain() {
            self.stdin
                .write_all(&pending)
                .map_err(|err| SupervisorFailure::Io(err.to_string()))?;
        }
        self.stdin
            .flush()
            .map_err(|err| SupervisorFailure::Io(err.to_string()))?;
        Ok(())
    }
}

/// One classified inbound read result.
enum Inbound {
    Message(dto::ParsedMessage),
    Protocol(error::ProviderError),
    Timeout(SupervisorFailure),
    ReadError,
    Eof,
}

fn map_outbound_error(error: OutboundError) -> SupervisorFailure {
    SupervisorFailure::Protocol(error::ProviderError::InvalidValue {
        path: "outbound".to_owned(),
        reason: match error {
            OutboundError::Overflow => "queue overflow".to_owned(),
            OutboundError::Closed => "queue closed".to_owned(),
        },
    })
}

fn unexpected_after_invoke() -> error::ProviderError {
    error::ProviderError::OutOfOrder {
        phase: "ready".to_owned(),
        kind: "non-terminal".to_owned(),
    }
}

fn unexpected_kind(expected: MessageKind, observed: MessageKind) -> error::ProviderError {
    error::ProviderError::OutOfOrder {
        phase: expected.as_str().to_owned(),
        kind: observed.as_str().to_owned(),
    }
}

/// The shutdown-ack never arrived within the bound.
fn ack_missing() -> error::ProviderError {
    error::ProviderError::InvalidValue {
        path: "shutdown-ack".to_owned(),
        reason: "no shutdown-ack within the bound".to_owned(),
    }
}

/// stdout reached EOF before the shutdown-ack arrived.
fn ack_eof() -> error::ProviderError {
    error::ProviderError::OutOfOrder {
        phase: super::protocol::LifecyclePhase::AwaitShutdownAck
            .as_str()
            .to_owned(),
        kind: "eof".to_owned(),
    }
}

/// A parseable frame arrived after the shutdown-ack; only EOF may follow it.
pub(super) fn ack_data_after() -> error::ProviderError {
    error::ProviderError::OutOfOrder {
        phase: super::protocol::LifecyclePhase::Terminated
            .as_str()
            .to_owned(),
        kind: "data-after-ack".to_owned(),
    }
}

/// The stdout read failed while waiting for the shutdown-ack, or a non-frame
/// fault remained in the final drain after a valid ack.
pub(super) fn ack_read_fault() -> error::ProviderError {
    error::ProviderError::InvalidValue {
        path: "shutdown-ack".to_owned(),
        reason: "stdout read failed".to_owned(),
    }
}

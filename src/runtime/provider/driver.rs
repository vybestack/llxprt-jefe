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
use std::sync::atomic::AtomicBool;
use std::sync::mpsc::{self, RecvTimeoutError};
use std::time::{Duration, Instant};

use super::drains::{StdoutDrain, StdoutEvent};
use super::encode;
use super::environment::Redactor;
use super::identifiers::Direction;
use super::outbound::{OutboundError, OutboundQueue};
use super::protocol::{LifecycleOrder, MessageKind, ProgressTracker, parse_message};
use super::supervisor::{
    CleanupFailure, LifecycleTranscript, OneShotOutcome, OneShotRequest, SupervisorBounds,
    SupervisorFailure, TranscriptEntry,
};
use super::{dto, error};

/// Maximum time between cancel-signal checks while driving to the terminal
/// (S17). Each read during the invoke/progress phase is capped at this slice
/// so a host cancel is observed promptly even when the provider is silent,
/// rather than hiding behind the full invocation deadline (up to 600 s). The
/// overall invocation deadline is tracked separately and still fires exactly.
const CANCEL_POLL: Duration = Duration::from_millis(200);

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
    /// Live progress stream sink (S16). When present, each accepted progress
    /// payload is redacted and forwarded as it arrives, before the terminal.
    pub progress_sink: Option<&'a mpsc::Sender<dto::ProgressPayload>>,
    /// Live cancel observer (S17). When present, the driver checks this flag
    /// between reads; `true` means the host cancelled the invocation.
    pub cancel: Option<&'a AtomicBool>,
    /// Secret redactor for live progress streaming (S16).
    pub redactor: &'a Redactor,
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
    /// supervisor failure) is reached. Checks for a live cancel signal between
    /// reads (S17): a host cancel writes a cancel frame before the staged
    /// shutdown. The first terminal observed by this driver is authoritative.
    ///
    /// When a cancel hook is present, each read is capped at [`CANCEL_POLL`] so
    /// a cancel is observed promptly even during silence; the invocation
    /// deadline still fires exactly via a tracked [`Instant`]. Without a hook
    /// the full invocation timeout is used in one read, preserving the original
    /// behavior for the blocking entry point.
    fn drive_to_terminal(
        &mut self,
        timeout: Duration,
    ) -> Result<OneShotOutcome, SupervisorFailure> {
        let mut deadline: Option<Instant> = None;
        loop {
            if let Some(inbound) = self.try_read_inbound()
                && let Some(outcome) = self.accept_invocation_inbound(inbound)?
            {
                return Ok(outcome);
            }
            if let Some(cancel) = self.cancel
                && cancel.load(std::sync::atomic::Ordering::SeqCst)
            {
                self.write_cancel_frame();
                return Ok(OneShotOutcome::Cancelled);
            }
            let read_timeout = match (self.cancel, &mut deadline) {
                (Some(_), slot) => {
                    let deadline = slot.get_or_insert_with(|| Instant::now() + timeout);
                    let now = Instant::now();
                    if now >= *deadline {
                        return Err(SupervisorFailure::InvocationTimeout);
                    }
                    deadline.saturating_duration_since(now).min(CANCEL_POLL)
                }
                (None, _) => timeout,
            };
            if let Some(outcome) = self.accept_invocation_inbound(
                self.read_inbound(read_timeout, SupervisorFailure::InvocationTimeout),
            )? {
                return Ok(outcome);
            }
        }
    }

    fn accept_invocation_inbound(
        &mut self,
        inbound: Inbound,
    ) -> Result<Option<OneShotOutcome>, SupervisorFailure> {
        match inbound {
            Inbound::Message(parsed) => match self.observe_inbound(&parsed) {
                Ok(()) => match &parsed.message {
                    dto::ProviderMessage::Progress(_) => Ok(None),
                    dto::ProviderMessage::Outcome(outcome) => {
                        Ok(Some(OneShotOutcome::Completed(outcome.clone())))
                    }
                    dto::ProviderMessage::Error(error) => {
                        Ok(Some(OneShotOutcome::ProviderError(error.clone())))
                    }
                    _ => Ok(Some(OneShotOutcome::Failed(SupervisorFailure::Protocol(
                        unexpected_after_invoke(),
                    )))),
                },
                Err(error) => Ok(Some(OneShotOutcome::Failed(SupervisorFailure::Protocol(
                    error,
                )))),
            },
            Inbound::Protocol(error) => Ok(Some(OneShotOutcome::Failed(
                SupervisorFailure::Protocol(error),
            ))),
            // A bounded slice expired: loop to re-check cancel and the
            // invocation deadline. Without a hook this is the real invocation
            // timeout and is returned as a failure.
            Inbound::Timeout(SupervisorFailure::InvocationTimeout) if self.cancel.is_some() => {
                Ok(None)
            }
            Inbound::Timeout(failure) => Err(failure),
            Inbound::ReadError => Err(SupervisorFailure::Io("stdout read failed".to_owned())),
            Inbound::Eof => Ok(Some(OneShotOutcome::Failed(SupervisorFailure::Crashed {
                exit: None,
            }))),
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

    /// Strictly observe the `shutdown-ack` through the shared observer.
    ///
    /// The full lifecycle requires shutdown/ack/EOF/reap, but the EOF that must
    /// follow a valid ack is observed by the supervisor's bounded final stdout
    /// drain after process exit/kill — not here. The shared observer validates
    /// only the ack itself; this method additionally records the transcript
    /// entry for a valid ack. After a valid ack it returns `None` and defers the
    /// EOF/data-after-ack observation to the final drain, so a timeout waiting
    /// for EOF is never treated as success. The first terminal result remains
    /// authoritative; this never replaces the outcome.
    fn observe_shutdown_ack(&mut self) -> Option<CleanupFailure> {
        let probe = probe_ack(self.stdout, self.bounds.shutdown_ack);
        let (failure, ack) = validate_shutdown_ack(probe, self.lifecycle);
        if let Some(parsed) = ack {
            self.transcript
                .push(TranscriptEntry::Received(parsed.kind()));
        }
        failure
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
                self.transcript.push_progress(progress.clone());
                // Stream each accepted progress payload live, redacted against
                // resolved secrets, before the terminal result (S16). The
                // transcript copy is redacted separately by the supervisor's
                // finish step.
                if let Some(sink) = self.progress_sink {
                    let mut streamed = progress.clone();
                    streamed.message = self.redactor.redact(&progress.message).into_owned();
                    let _ = sink.send(streamed);
                }
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

    /// Consume a provider event that was already queued before host control.
    fn try_read_inbound(&self) -> Option<Inbound> {
        match self.stdout.receiver.try_recv() {
            Ok(event) => Some(classify_stdout_event(event)),
            Err(mpsc::TryRecvError::Empty) => None,
            Err(mpsc::TryRecvError::Disconnected) => Some(Inbound::Eof),
        }
    }

    /// Read and classify one inbound event.
    fn read_inbound(&self, timeout: Duration, timeout_failure: SupervisorFailure) -> Inbound {
        match self.stdout.receiver.recv_timeout(timeout) {
            Ok(event) => classify_stdout_event(event),
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

    /// Write a best-effort `cancel` frame to the provider (S17). Write/flush
    /// failures are not fatal: the staged reap is the authoritative cleanup.
    fn write_cancel_frame(&mut self) {
        let frame = encode::encode_cancel(
            &self.request.request_id,
            self.request.generation,
            &self.request.request_id,
        );
        let _ = self.send(frame);
    }
}

/// One classified inbound read result.
fn classify_stdout_event(event: StdoutEvent) -> Inbound {
    match event {
        StdoutEvent::Frame(frame) => match parse_message(&frame, Direction::ProviderToHost) {
            Ok(parsed) => Inbound::Message(parsed),
            Err(error) => Inbound::Protocol(error),
        },
        StdoutEvent::Oversize(error) => Inbound::Protocol(error),
        StdoutEvent::ReadError => Inbound::ReadError,
    }
}

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

pub(super) fn unexpected_after_invoke() -> error::ProviderError {
    error::ProviderError::OutOfOrder {
        phase: "ready".to_owned(),
        kind: "non-terminal".to_owned(),
    }
}

/// One classified inbound read while awaiting `shutdown-ack`.
///
/// Shared by the one-shot lifecycle driver and the persistent supervisor's
/// healthy shutdown/rollback path.
pub(super) enum AckProbe {
    /// A parsed frame arrived.
    Message(dto::ParsedMessage),
    /// A non-frame protocol fault (oversize, parse error, or malformed line).
    Protocol(error::ProviderError),
    /// No event arrived within the bound.
    Timeout,
    /// The stdout read failed.
    ReadError,
    /// stdout reached EOF before the ack.
    Eof,
}

/// Read and classify one bounded inbound event while awaiting `shutdown-ack`.
pub(super) fn probe_ack(stdout: &StdoutDrain, bound: Duration) -> AckProbe {
    match stdout.receiver.recv_timeout(bound) {
        Ok(StdoutEvent::Frame(frame)) => match parse_message(&frame, Direction::ProviderToHost) {
            Ok(parsed) => AckProbe::Message(parsed),
            Err(error) => AckProbe::Protocol(error),
        },
        Ok(StdoutEvent::Oversize(error)) => AckProbe::Protocol(error),
        Ok(StdoutEvent::ReadError) => AckProbe::ReadError,
        Err(RecvTimeoutError::Timeout) => AckProbe::Timeout,
        Err(RecvTimeoutError::Disconnected) => AckProbe::Eof,
    }
}

/// Validate one `shutdown-ack` probe against the lifecycle, returning a cleanup
/// failure if it is wrong/missing/malformed/out-of-order. On a valid ack the
/// parsed message is returned so the caller can record any transcript entry;
/// the EOF/data-after-ack observation is deferred to the bounded final drain.
pub(super) fn validate_shutdown_ack(
    probe: AckProbe,
    lifecycle: &mut LifecycleOrder,
) -> (Option<CleanupFailure>, Option<dto::ParsedMessage>) {
    match probe {
        AckProbe::Message(parsed) => {
            if parsed.kind() != MessageKind::ShutdownAck {
                return (
                    Some(CleanupFailure::ShutdownAck(unexpected_kind(
                        MessageKind::ShutdownAck,
                        parsed.kind(),
                    ))),
                    None,
                );
            }
            match lifecycle.observe(parsed.kind(), parsed.generation) {
                Ok(()) => (None, Some(parsed)),
                Err(error) => (Some(CleanupFailure::ShutdownAck(error)), None),
            }
        }
        AckProbe::Protocol(error) => (Some(CleanupFailure::ShutdownAck(error)), None),
        AckProbe::Timeout => (Some(CleanupFailure::ShutdownAck(ack_missing())), None),
        AckProbe::ReadError => (Some(CleanupFailure::ShutdownAck(ack_read_fault())), None),
        AckProbe::Eof => (Some(CleanupFailure::ShutdownAck(ack_eof())), None),
    }
}

/// Strictly await one `shutdown-ack` for a healthy candidate, returning a
/// cleanup failure if it is wrong/missing/malformed/out-of-order or preceded by
/// EOF. After a valid ack returns `None`; the bounded final stdout drain owns the
/// EOF/data-after-ack observation, so a later EOF timeout is never treated as
/// success. The caller must have advanced the lifecycle to `AwaitShutdownAck`.
pub(super) fn await_shutdown_ack(
    stdout: &StdoutDrain,
    lifecycle: &mut LifecycleOrder,
    bound: Duration,
) -> Option<CleanupFailure> {
    validate_shutdown_ack(probe_ack(stdout, bound), lifecycle).0
}

pub(super) fn unexpected_kind(
    expected: MessageKind,
    observed: MessageKind,
) -> error::ProviderError {
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

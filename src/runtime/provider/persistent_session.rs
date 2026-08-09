//! Persistent provider invocation sessions (issue #390 CW-10, Remediation E).
//!
//! Each already-Ready persistent candidate is owned by one command-owner thread
//! at the runtime boundary. The thread exposes typed command/event channels so
//! repeated invocations use the same process, live progress is observed before
//! the terminal, cancellation sends a live cancel envelope, and the descriptor
//! timeout applies. No handle enters `AppState` — the session owner stays in
//! the coordinator.
//!
//! The invocation reuses the one-shot driver's cancel-checked progress→terminal
//! loop but does NOT send shutdown after the terminal: the candidate stays alive
//! for the next invocation. Request IDs, the fixed wire generation, and
//! first-terminal semantics are preserved exactly.

use std::io::Write;
use std::process::ChildStdin;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, mpsc};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

use crate::domain::Id;

use super::drains::StdoutEvent;
use super::encode;
use super::identifiers::Direction;
use super::outcome::{OneShotOutcome, OneShotResult, SupervisorFailure};
use super::persistent::{
    CandidateHealth, CandidateHealthSnapshot, OwnedCandidate, candidate_health, reap_owned,
};
use super::protocol::{
    InvokeActionPayload, LifecycleOrder, MessageKind, ProgressPayload, ProgressTracker,
    ProviderMessage, parse_message,
};
use super::supervisor::SupervisorBounds;

/// Maximum time between cancel-signal checks while driving to the terminal.
const CANCEL_POLL: Duration = Duration::from_millis(200);
/// Health publication must never block the application context indefinitely.
const HEALTH_RESPONSE_TIMEOUT: Duration = Duration::from_millis(250);

// ---------------------------------------------------------------------------
// Public session types
// ---------------------------------------------------------------------------

/// One bounded invocation command sent to a persistent candidate's owner thread.
struct InvocationCommand {
    /// The host request id for this invocation.
    request_id: super::identifiers::RequestId,
    /// The `invoke-action` payload.
    payload: Box<InvokeActionPayload>,
    /// The descriptor-selected invocation timeout.
    timeout: Duration,
    /// Live progress stream sink.
    progress_tx: mpsc::SyncSender<ProgressPayload>,
    /// Live cancel flag. `true` means the host cancelled.
    cancel: Arc<AtomicBool>,
    /// Terminal result sink. The owner thread sends exactly one result.
    terminal_tx: mpsc::SyncSender<OneShotResult>,
    /// Completion flag. Set `true` after the terminal is sent.
    done: Arc<AtomicBool>,
}

/// Control commands have a separate lane so health and shutdown cannot be
/// trapped behind queued invocations.
enum ControlCommand {
    /// Probe the candidate's health.
    Health {
        /// Health reply sink.
        reply: mpsc::SyncSender<CandidateHealthSnapshot>,
    },
    /// Shut down the candidate and exit the owner thread.
    Shutdown,
}

/// One persistent candidate session (bounded command senders + thread join handle).
struct PersistentSession {
    plugin_id: Id,
    invocation_tx: Option<mpsc::SyncSender<InvocationCommand>>,
    control_tx: Option<mpsc::SyncSender<ControlCommand>>,
    thread: Option<JoinHandle<()>>,
}

/// The sole owner of every persistent candidate session.
///
/// Holds one command-owner thread per ready candidate. No `Child`, pipe, PID,
/// or thread handle leaves this type except through the typed command/event
/// channels. [`Drop`] drops each command sender (signaling the owner thread to
/// reap and exit) and joins every thread so a dropped owner cannot orphan a
/// process.
pub struct PersistentSessionOwner {
    sessions: Vec<PersistentSession>,
}

impl PersistentSessionOwner {
    /// Construct from ready candidates and bounds, spawning one owner thread
    /// per candidate.
    pub(super) fn from_candidates(
        candidates: Vec<OwnedCandidate>,
        bounds: SupervisorBounds,
    ) -> Self {
        let sessions = candidates
            .into_iter()
            .map(|candidate| spawn_owner_thread(candidate, bounds))
            .collect();
        Self { sessions }
    }

    /// Construct an empty owner (no sessions).
    #[must_use]
    pub fn empty() -> Self {
        Self {
            sessions: Vec::new(),
        }
    }

    /// Whether any persistent session is owned.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.sessions.is_empty()
    }

    /// The number of owned sessions.
    #[must_use]
    pub fn len(&self) -> usize {
        self.sessions.len()
    }

    /// Invoke one action on the ready candidate for `plugin_id`.
    ///
    /// Sends an `Invoke` command to the owning thread and returns a handle the
    /// caller polls for progress, cancel, and the terminal result. The
    /// invocation runs entirely in the owner thread; the caller never blocks on
    /// the provider.
    ///
    /// # Errors
    ///
    /// Returns [`PersistentInvokeError::NoSession`] when no session owns
    /// `plugin_id`, or [`PersistentInvokeError::SessionGone`] when the owner
    /// thread has exited.
    pub fn invoke(
        &self,
        plugin_id: &Id,
        request_id: super::identifiers::RequestId,
        payload: InvokeActionPayload,
        timeout: Duration,
    ) -> Result<PersistentInvocation, PersistentInvokeError> {
        let session = self
            .sessions
            .iter()
            .find(|session| &session.plugin_id == plugin_id)
            .ok_or(PersistentInvokeError::NoSession)?;
        let invocation_tx = session
            .invocation_tx
            .as_ref()
            .ok_or(PersistentInvokeError::SessionGone)?;
        let (progress_tx, progress_rx) = mpsc::sync_channel(super::PROGRESS_SEQUENCE_MAX.into());
        let (terminal_tx, terminal_rx) = mpsc::sync_channel(1);
        let cancel = Arc::new(AtomicBool::new(false));
        let done = Arc::new(AtomicBool::new(false));
        invocation_tx
            .try_send(InvocationCommand {
                request_id,
                payload: Box::new(payload),
                timeout,
                progress_tx,
                cancel: cancel.clone(),
                terminal_tx,
                done: done.clone(),
            })
            .map_err(|error| match error {
                mpsc::TrySendError::Full(_) => PersistentInvokeError::QueueFull,
                mpsc::TrySendError::Disconnected(_) => PersistentInvokeError::SessionGone,
            })?;
        Ok(PersistentInvocation {
            progress_rx,
            cancel,
            done,
            result_rx: terminal_rx,
        })
    }

    /// Probe every candidate's health. A `ready` process that has since exited
    /// is reported as [`CandidateHealth::Exited`]; no auto-restart follows.
    #[must_use]
    pub fn health(&self) -> Vec<CandidateHealthSnapshot> {
        self.sessions
            .iter()
            .filter_map(|session| {
                let control_tx = session.control_tx.as_ref()?;
                let (reply_tx, reply_rx) = mpsc::sync_channel(1);
                let plugin_id = session.plugin_id.clone();
                match control_tx.try_send(ControlCommand::Health { reply: reply_tx }) {
                    Ok(()) => {}
                    Err(mpsc::TrySendError::Disconnected(_)) => {
                        return Some(CandidateHealthSnapshot {
                            plugin_id,
                            health: CandidateHealth::Exited { exit_code: None },
                        });
                    }
                    Err(mpsc::TrySendError::Full(_)) => {
                        return Some(health_probe_failed(
                            plugin_id,
                            "persistent owner control queue is full",
                        ));
                    }
                }
                match reply_rx.recv_timeout(HEALTH_RESPONSE_TIMEOUT) {
                    Ok(snapshot) => Some(CandidateHealthSnapshot {
                        plugin_id,
                        health: snapshot.health,
                    }),
                    Err(mpsc::RecvTimeoutError::Timeout) => Some(health_probe_failed(
                        plugin_id,
                        "persistent owner health response timed out",
                    )),
                    Err(mpsc::RecvTimeoutError::Disconnected) => Some(CandidateHealthSnapshot {
                        plugin_id,
                        health: CandidateHealth::Exited { exit_code: None },
                    }),
                }
            })
            .collect()
    }

    /// Explicitly shut down every candidate. Idempotent. Sends `Shutdown` to
    /// each owner thread and joins it so all processes are reaped.
    pub fn shutdown(&mut self) {
        for session in &mut self.sessions {
            let _ = session.invocation_tx.take();
            if let Some(control_tx) = session.control_tx.take() {
                let _ = control_tx.send(ControlCommand::Shutdown);
            }
            if let Some(thread) = session.thread.take() {
                let _ = thread.join();
            }
        }
    }
}

impl Drop for PersistentSessionOwner {
    fn drop(&mut self) {
        for session in &mut self.sessions {
            let _ = session.invocation_tx.take();
            let _ = session.control_tx.take();
            if let Some(thread) = session.thread.take() {
                let _ = thread.join();
            }
        }
    }
}

impl std::fmt::Debug for PersistentSessionOwner {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("PersistentSessionOwner")
            .field("sessions", &self.sessions.len())
            .finish_non_exhaustive()
    }
}
fn health_probe_failed(plugin_id: Id, error: &str) -> CandidateHealthSnapshot {
    CandidateHealthSnapshot {
        plugin_id,
        health: CandidateHealth::ProbeFailed {
            error: error.to_owned(),
        },
    }
}

/// Spawn one command-owner thread for a candidate.
fn spawn_owner_thread(candidate: OwnedCandidate, bounds: SupervisorBounds) -> PersistentSession {
    let plugin_id = candidate.plugin_id.clone();
    let (invocation_tx, invocation_rx) = mpsc::sync_channel(super::MAX_QUEUED_ENVELOPES);
    let (control_tx, control_rx) = mpsc::sync_channel(super::MAX_QUEUED_ENVELOPES);
    let (candidate_tx, candidate_rx) = mpsc::sync_channel(1);
    let thread_result = thread::Builder::new()
        .name(format!("jefe-persistent-{plugin_id}"))
        .spawn(move || {
            if let Ok(candidate) = candidate_rx.recv() {
                run_owner_thread(candidate, bounds, invocation_rx, control_rx);
            }
        });

    match thread_result {
        Ok(thread) => match candidate_tx.send(candidate) {
            Ok(()) => PersistentSession {
                plugin_id,
                invocation_tx: Some(invocation_tx),
                control_tx: Some(control_tx),
                thread: Some(thread),
            },
            Err(error) => {
                reap_after_owner_start_failure(error.0, &bounds);
                let _ = thread.join();
                unavailable_session(plugin_id)
            }
        },
        Err(error) => {
            tracing::warn!(
                plugin_id = %plugin_id,
                error = %error,
                "failed to spawn persistent owner thread"
            );
            reap_after_owner_start_failure(candidate, &bounds);
            unavailable_session(plugin_id)
        }
    }
}

fn reap_after_owner_start_failure(candidate: OwnedCandidate, bounds: &SupervisorBounds) {
    let reaped = reap_owned(candidate, bounds);
    if !reaped.reaped {
        tracing::warn!(
            plugin_id = %reaped.plugin_id,
            cleanup_failure = ?reaped.cleanup_failure,
            "persistent candidate cleanup after owner-start failure was incomplete"
        );
    }
}

fn unavailable_session(plugin_id: Id) -> PersistentSession {
    PersistentSession {
        plugin_id,
        invocation_tx: None,
        control_tx: None,
        thread: None,
    }
}

/// The owner thread's main loop. Invocation traffic is bounded independently
/// from control traffic, so health and shutdown remain serviceable while a
/// provider action is live.
fn run_owner_thread(
    mut candidate: OwnedCandidate,
    bounds: SupervisorBounds,
    invocation_rx: mpsc::Receiver<InvocationCommand>,
    control_rx: mpsc::Receiver<ControlCommand>,
) {
    loop {
        if service_idle_control(&mut candidate, &control_rx) {
            reap_owned(candidate, &bounds);
            return;
        }
        match invocation_rx.recv_timeout(CANCEL_POLL) {
            Ok(command) => {
                let signals = InvocationSignals {
                    timeout: command.timeout,
                    progress_tx: &command.progress_tx,
                    cancel: &command.cancel,
                    control_rx: &control_rx,
                };
                let (result, shutdown) = drive_invocation(
                    &mut candidate,
                    &command.request_id,
                    &command.payload,
                    &signals,
                );
                let _ = command.terminal_tx.send(result);
                command.done.store(true, Ordering::SeqCst);
                if shutdown {
                    reap_owned(candidate, &bounds);
                    return;
                }
            }
            Err(mpsc::RecvTimeoutError::Timeout) => {}
            Err(mpsc::RecvTimeoutError::Disconnected) => {
                reap_owned(candidate, &bounds);
                return;
            }
        }
    }
}

fn service_idle_control(
    candidate: &mut OwnedCandidate,
    control_rx: &mpsc::Receiver<ControlCommand>,
) -> bool {
    loop {
        match control_rx.try_recv() {
            Ok(ControlCommand::Health { reply }) => send_health(candidate, reply),
            Ok(ControlCommand::Shutdown) | Err(mpsc::TryRecvError::Disconnected) => return true,
            Err(mpsc::TryRecvError::Empty) => return false,
        }
    }
}

fn send_health(candidate: &mut OwnedCandidate, reply: mpsc::SyncSender<CandidateHealthSnapshot>) {
    let health = candidate_health(candidate);
    let _ = reply.send(CandidateHealthSnapshot {
        plugin_id: candidate.plugin_id.clone(),
        health,
    });
}

/// A live handle to one persistent invocation.
///
/// The caller polls [`Self::progress_rx`] for live progress, sets
/// [`Self::cancel`] to request cancellation, checks [`Self::is_finished`] for
/// the terminal, then calls [`Self::finish`] to consume the typed result.
pub struct PersistentInvocation {
    /// Live progress events (redacted against resolved secrets).
    pub progress_rx: mpsc::Receiver<ProgressPayload>,
    /// Cancel flag. Set `true` to request cancellation while the invocation is
    /// still live.
    pub cancel: Arc<AtomicBool>,
    /// Completion flag. `true` once the owner thread has sent the terminal.
    pub done: Arc<AtomicBool>,
    /// Terminal result sink.
    pub result_rx: mpsc::Receiver<OneShotResult>,
}

impl PersistentInvocation {
    /// Whether the invocation has reached its terminal result.
    #[must_use]
    pub fn is_finished(&self) -> bool {
        self.done.load(Ordering::SeqCst)
    }

    /// Consume the typed terminal result. Blocks until the owner thread sends
    /// it. If the owner thread panicked or exited unexpectedly, returns a
    /// `Crashed` failure (first-terminal semantics are preserved by the
    /// reducer, not here).
    #[must_use]
    pub fn finish(self) -> OneShotResult {
        match self.result_rx.recv() {
            Ok(result) => result,
            Err(_) => OneShotResult::without_process(SupervisorFailure::Crashed { exit: None }),
        }
    }
}

enum ActiveInput {
    Stdout(StdoutEvent),
    StdoutDisconnected,
    Health(mpsc::SyncSender<CandidateHealthSnapshot>),
    Shutdown,
    Cancel,
    None,
}

/// Select one immediately ready invocation input with provider stdout first.
/// This ordering makes a terminal frame that was already queued authoritative
/// over a host cancel or shutdown observed in the same poll.
fn poll_active_input(
    stdout_rx: &mpsc::Receiver<StdoutEvent>,
    control_rx: &mpsc::Receiver<ControlCommand>,
    cancel: &AtomicBool,
) -> ActiveInput {
    match stdout_rx.try_recv() {
        Ok(event) => return ActiveInput::Stdout(event),
        Err(mpsc::TryRecvError::Disconnected) => return ActiveInput::StdoutDisconnected,
        Err(mpsc::TryRecvError::Empty) => {}
    }
    match control_rx.try_recv() {
        Ok(ControlCommand::Health { reply }) => return ActiveInput::Health(reply),
        Ok(ControlCommand::Shutdown) | Err(mpsc::TryRecvError::Disconnected) => {
            return ActiveInput::Shutdown;
        }
        Err(mpsc::TryRecvError::Empty) => {}
    }
    if cancel.load(Ordering::SeqCst) {
        ActiveInput::Cancel
    } else {
        ActiveInput::None
    }
}

/// Why one invocation could not be started on a persistent session.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PersistentInvokeError {
    /// No session owns the requested plugin id.
    NoSession,
    /// The owner thread has exited (process crashed during a previous
    /// invocation or the session was shut down).
    SessionGone,
    /// The provider already has 64 queued outbound invocation envelopes.
    QueueFull,
}

impl std::fmt::Display for PersistentInvokeError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NoSession => formatter.write_str("no persistent session for the plugin id"),
            Self::SessionGone => formatter.write_str("persistent session owner thread has exited"),
            Self::QueueFull => formatter.write_str("persistent provider outbound queue is full"),
        }
    }
}

impl std::error::Error for PersistentInvokeError {}

// ---------------------------------------------------------------------------
// Persistent invocation driver
// ---------------------------------------------------------------------------

struct InvocationSignals<'a> {
    timeout: Duration,
    progress_tx: &'a mpsc::SyncSender<ProgressPayload>,
    cancel: &'a AtomicBool,
    control_rx: &'a mpsc::Receiver<ControlCommand>,
}

/// Drive one persistent invocation: send `invoke-action`, then read progress
/// until the single terminal (or cancel/timeout), without sending shutdown.
///
/// The candidate stays alive for the next invocation. Each accepted progress
/// payload is forwarded live (redacted against resolved secrets). The cancel
/// flag is checked between reads so a host cancel is observed promptly. The
/// generation is the candidate's fixed wire generation; the request id is the
/// host-side allocation for this invocation.
fn drive_invocation(
    candidate: &mut OwnedCandidate,
    request_id: &super::identifiers::RequestId,
    payload: &InvokeActionPayload,
    signals: &InvocationSignals<'_>,
) -> (OneShotResult, bool) {
    if !candidate.healthy {
        return (
            OneShotResult::without_process(SupervisorFailure::Protocol(
                super::driver::unexpected_after_invoke(),
            )),
            false,
        );
    }
    let frame = encode::encode_invoke_action(request_id, candidate.generation, payload);
    if let Err(failure) = write_frame(candidate.stdin.as_mut(), &frame) {
        return (OneShotResult::without_process(failure), false);
    }
    observe_outbound(candidate, MessageKind::InvokeAction);
    let (outcome, shutdown) = drive_to_terminal(candidate, request_id, signals);
    let cleanup_failure = if matches!(outcome, OneShotOutcome::Cancelled) && !candidate.healthy {
        Some(super::outcome::CleanupFailure::PostTerminal(
            super::driver::unexpected_after_invoke(),
        ))
    } else {
        None
    };
    (
        OneShotResult {
            outcome,
            transcript: super::outcome::LifecycleTranscript::default(),
            retained_stderr: String::new(),
            stderr_truncated: false,
            process_reaped: false,
            exit_code: None,
            cleanup_failure,
        },
        shutdown,
    )
}

/// Read progress events until the single terminal outcome/error (or a
/// supervisor failure) is reached. Checks for a live cancel between reads
/// (S17): a host cancel stops the loop and writes a cancel frame before
/// returning `Cancelled`. Each read is capped at [`CANCEL_POLL`] so a cancel
/// is observed promptly even during silence; the invocation deadline still
/// fires exactly via a tracked [`Instant`].
fn drive_to_terminal(
    candidate: &mut OwnedCandidate,
    request_id: &super::identifiers::RequestId,
    signals: &InvocationSignals<'_>,
) -> (OneShotOutcome, bool) {
    let deadline = Instant::now() + signals.timeout;
    let mut progress = ProgressTracker::new();
    loop {
        match poll_active_input(
            &candidate.stdout_drain.receiver,
            signals.control_rx,
            signals.cancel,
        ) {
            ActiveInput::Stdout(event) => {
                if let Some(outcome) =
                    accept_invocation_event(candidate, &mut progress, signals.progress_tx, event)
                {
                    return (outcome, false);
                }
                continue;
            }
            ActiveInput::StdoutDisconnected => return disconnected_outcome(candidate),
            ActiveInput::Health(reply) => {
                send_health(candidate, reply);
                continue;
            }
            ActiveInput::Shutdown => return (OneShotOutcome::Cancelled, true),
            ActiveInput::Cancel => {
                write_cancel_frame(candidate, request_id);
                let shutdown = drain_after_cancel(candidate, signals.control_rx);
                return (OneShotOutcome::Cancelled, shutdown);
            }
            ActiveInput::None => {}
        }
        let now = Instant::now();
        if now >= deadline {
            return (
                OneShotOutcome::Failed(SupervisorFailure::InvocationTimeout),
                false,
            );
        }
        let read_timeout = deadline.saturating_duration_since(now).min(CANCEL_POLL);
        match candidate.stdout_drain.receiver.recv_timeout(read_timeout) {
            Ok(event) => {
                if let Some(outcome) =
                    accept_invocation_event(candidate, &mut progress, signals.progress_tx, event)
                {
                    return (outcome, false);
                }
            }
            Err(mpsc::RecvTimeoutError::Timeout) => {}
            Err(mpsc::RecvTimeoutError::Disconnected) => {
                return disconnected_outcome(candidate);
            }
        }
    }
}

fn disconnected_outcome(candidate: &mut OwnedCandidate) -> (OneShotOutcome, bool) {
    candidate.exited = true;
    (
        OneShotOutcome::Failed(SupervisorFailure::Crashed { exit: None }),
        false,
    )
}

/// Apply one provider stream event to the active invocation.
///
/// `None` means a progress frame was accepted and the invocation remains live;
/// every terminal or malformed event returns the authoritative result.
fn accept_invocation_event(
    candidate: &mut OwnedCandidate,
    progress: &mut ProgressTracker,
    progress_tx: &mpsc::SyncSender<ProgressPayload>,
    event: StdoutEvent,
) -> Option<OneShotOutcome> {
    let parsed = match event {
        StdoutEvent::Frame(frame) => match parse_message(&frame, Direction::ProviderToHost) {
            Ok(parsed) => parsed,
            Err(error) => {
                return Some(OneShotOutcome::Failed(SupervisorFailure::Protocol(error)));
            }
        },
        StdoutEvent::Oversize(error) => {
            return Some(OneShotOutcome::Failed(SupervisorFailure::Protocol(error)));
        }
        StdoutEvent::ReadError => {
            candidate.exited = true;
            return Some(OneShotOutcome::Failed(SupervisorFailure::Io(
                "stdout read failed".to_owned(),
            )));
        }
    };
    if observe_inbound(
        &mut candidate.lifecycle,
        progress,
        &parsed.message,
        candidate.generation,
    )
    .is_err()
    {
        return Some(OneShotOutcome::Failed(SupervisorFailure::Protocol(
            super::driver::unexpected_after_invoke(),
        )));
    }
    match parsed.message {
        ProviderMessage::Progress(mut payload) => {
            payload.message = candidate.redactor.redact(&payload.message).into_owned();
            let _ = progress_tx.send(payload);
            None
        }
        ProviderMessage::Outcome(outcome) => Some(OneShotOutcome::Completed(outcome)),
        ProviderMessage::Error(error) => Some(OneShotOutcome::ProviderError(error)),
        _ => Some(OneShotOutcome::Failed(SupervisorFailure::Protocol(
            super::driver::unexpected_after_invoke(),
        ))),
    }
}

/// Write one encoded frame to the provider stdin.
fn write_frame(stdin: Option<&mut ChildStdin>, frame: &[u8]) -> Result<(), SupervisorFailure> {
    let Some(stdin) = stdin else {
        return Err(SupervisorFailure::Io("provider stdin closed".to_owned()));
    };
    stdin
        .write_all(frame)
        .map_err(|err| SupervisorFailure::Io(err.to_string()))?;
    stdin
        .flush()
        .map_err(|err| SupervisorFailure::Io(err.to_string()))?;
    Ok(())
}

/// Observe the cancelled invocation's stream for the bounded shutdown-ack
/// phase. Cancel is already the authoritative terminal result. Any provider
/// byte after it is therefore a protocol diagnostic and makes this generation
/// non-reusable; consuming it here prevents it from becoming the next request's
/// result.
fn drain_after_cancel(
    candidate: &mut OwnedCandidate,
    control_rx: &mpsc::Receiver<ControlCommand>,
) -> bool {
    let deadline = Instant::now() + SupervisorBounds::PRODUCTION.shutdown_ack;
    loop {
        if service_idle_control(candidate, control_rx) {
            return true;
        }
        let now = Instant::now();
        if now >= deadline {
            return false;
        }
        let wait = deadline.saturating_duration_since(now).min(CANCEL_POLL);
        match candidate.stdout_drain.receiver.recv_timeout(wait) {
            Ok(_) | Err(mpsc::RecvTimeoutError::Disconnected) => {
                candidate.healthy = false;
                return false;
            }
            Err(mpsc::RecvTimeoutError::Timeout) => {}
        }
    }
}

/// Write a best-effort `cancel` frame to the provider (S17). The bounded
/// post-cancel drain decides whether this persistent generation remains reusable.
fn write_cancel_frame(candidate: &mut OwnedCandidate, request_id: &super::identifiers::RequestId) {
    let frame = encode::encode_cancel(request_id, candidate.generation, request_id);
    let _ = write_frame(candidate.stdin.as_mut(), &frame);
}

/// Observe one outbound host message (advancing the lifecycle phase) while the
/// generation is still healthy.
fn observe_outbound(candidate: &mut OwnedCandidate, kind: MessageKind) {
    if candidate.healthy
        && candidate
            .lifecycle
            .observe(kind, candidate.generation)
            .is_err()
    {
        candidate.healthy = false;
    }
}

/// Observe one inbound provider message in the lifecycle and progress
/// validators.
fn observe_inbound(
    lifecycle: &mut LifecycleOrder,
    progress: &mut ProgressTracker,
    message: &ProviderMessage,
    generation: u64,
) -> Result<(), super::error::ProviderError> {
    let kind = message_kind(message);
    lifecycle.observe(kind, generation)?;
    if let ProviderMessage::Progress(payload) = message {
        progress.observe(payload.sequence, payload.completed, payload.total)?;
    }
    Ok(())
}

/// Map a [`ProviderMessage`] to its [`MessageKind`].
fn message_kind(message: &ProviderMessage) -> MessageKind {
    match message {
        ProviderMessage::Hello(_) => MessageKind::Hello,
        ProviderMessage::HelloAck(_) => MessageKind::HelloAck,
        ProviderMessage::Configure(_) => MessageKind::Configure,
        ProviderMessage::Ready(_) => MessageKind::Ready,
        ProviderMessage::InvokeAction(_) => MessageKind::InvokeAction,
        ProviderMessage::Progress(_) => MessageKind::Progress,
        ProviderMessage::Outcome(_) => MessageKind::Outcome,
        ProviderMessage::Error(_) => MessageKind::Error,
        ProviderMessage::Cancel(_) => MessageKind::Cancel,

        ProviderMessage::Shutdown(_) => MessageKind::Shutdown,
        ProviderMessage::ShutdownAck => MessageKind::ShutdownAck,
    }
}

#[cfg(test)]
mod active_input_order_tests {
    use super::*;

    #[test]
    fn queued_provider_frame_wins_over_cancel_and_shutdown() {
        let (stdout_tx, stdout_rx) = mpsc::channel();
        let (control_tx, control_rx) = mpsc::sync_channel(1);
        let cancel = AtomicBool::new(true);
        let frame = br#"{"protocol":1,"type":"outcome"}\n"#.to_vec();
        assert!(stdout_tx.send(StdoutEvent::Frame(frame)).is_ok());
        assert!(control_tx.send(ControlCommand::Shutdown).is_ok());

        assert!(matches!(
            poll_active_input(&stdout_rx, &control_rx, &cancel),
            ActiveInput::Stdout(StdoutEvent::Frame(_))
        ));
    }

    #[test]
    fn queued_shutdown_wins_over_cancel_when_stdout_is_empty() {
        let (_stdout_tx, stdout_rx) = mpsc::channel();
        let (control_tx, control_rx) = mpsc::sync_channel(1);
        let cancel = AtomicBool::new(true);
        assert!(control_tx.send(ControlCommand::Shutdown).is_ok());

        assert!(matches!(
            poll_active_input(&stdout_rx, &control_rx, &cancel),
            ActiveInput::Shutdown
        ));
    }

    #[test]
    fn health_probe_is_bounded_when_an_owner_does_not_reply() {
        let (control_tx, control_rx) = mpsc::sync_channel(1);
        let plugin_id = Id::parse("vendor.health").unwrap_or_else(|error| panic!("id: {error:?}"));
        let owner = PersistentSessionOwner {
            sessions: vec![PersistentSession {
                plugin_id: plugin_id.clone(),
                invocation_tx: None,
                control_tx: Some(control_tx),
                thread: None,
            }],
        };
        let (result_tx, result_rx) = mpsc::sync_channel(1);
        let probe = thread::spawn(move || {
            let _ = result_tx.send(owner.health());
        });

        let result = result_rx.recv_timeout(Duration::from_secs(1));
        drop(control_rx);
        if let Err(payload) = probe.join() {
            std::panic::resume_unwind(payload);
        }
        let snapshots = result.unwrap_or_else(|error| panic!("health probe blocked: {error:?}"));
        assert!(matches!(
            snapshots.as_slice(),
            [CandidateHealthSnapshot {
                health: CandidateHealth::ProbeFailed { .. },
                ..
            }]
        ));
    }
}

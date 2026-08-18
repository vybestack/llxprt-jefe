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
use std::sync::{Arc, Mutex, mpsc};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

use crate::domain::Id;

use super::drains::StdoutEvent;
use super::dto::Capability;
use super::encode;
use super::error::ProviderError;
use super::identifiers::Direction;
use super::outcome::{OneShotOutcome, OneShotResult, SupervisorFailure};
use super::persistent::{
    CandidateHealth, CandidateHealthSnapshot, OwnedCandidate, reap_owned, session_candidate_health,
};
use super::protocol::{
    InvokeActionPayload, LifecycleOrder, MessageKind, ProgressPayload, ProgressTracker,
    ProviderMessage, parse_message,
};
use super::redaction::redact_panel_snapshot;
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

// Control commands have a separate lane so health and shutdown cannot be
// trapped behind queued invocations.

/// One host panel command delivered through the candidate's sole stdin owner.
enum PanelCommandPayload {
    Activate(super::panel_model::ActivatePanelPayload),
    Deactivate(super::panel_model::DeactivatePanelPayload),
    Event(super::panel_model::PanelEventPayload),
}

struct PanelCommand {
    request_id: super::identifiers::RequestId,
    payload: PanelCommandPayload,
}

/// One asynchronous panel snapshot emitted by a persistent provider.
#[derive(Debug, Clone)]
pub struct PanelDelivery {
    /// Exact provider owner for this session.
    pub plugin_id: Id,
    /// Fixed provider-process generation from the envelope.
    pub process_generation: u64,
    /// Original framed byte count, used as a conservative protocol bound.
    pub payload_byte_count: u64,
    /// Fully validated snapshot.
    pub snapshot: super::panel_model::PanelSnapshot,
}
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
    panel_tx: Option<mpsc::SyncSender<PanelCommand>>,
    panel_rx: mpsc::Receiver<PanelDelivery>,
    control_tx: Option<mpsc::SyncSender<ControlCommand>>,
    available: Arc<Mutex<bool>>,
    thread: Option<JoinHandle<Option<super::persistent::ReapedCandidate>>>,
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

/// A ready provider could not be transferred into its runtime owner thread.
#[derive(Debug)]
pub struct PersistentOwnerStartFailure {
    plugin_id: Id,
    cause: PersistentOwnerStartCause,
    reaped: Vec<super::persistent::ReapedCandidate>,
}

#[derive(Debug)]
enum PersistentOwnerStartCause {
    Thread(std::io::Error),
    CandidateHandoff,
}

impl std::fmt::Display for PersistentOwnerStartFailure {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let cause = match &self.cause {
            PersistentOwnerStartCause::Thread(error) => {
                format!("runtime owner thread creation failed: {error}")
            }
            PersistentOwnerStartCause::CandidateHandoff => {
                "runtime owner thread rejected its ready candidate".to_owned()
            }
        };
        let cleanup_complete = self.reaped.iter().all(|candidate| candidate.reaped);
        write!(
            formatter,
            "required provider {} owner-transfer failed: {cause}; cleanup_complete={cleanup_complete}",
            self.plugin_id
        )
    }
}

impl std::error::Error for PersistentOwnerStartFailure {}

impl PersistentSessionOwner {
    /// Construct from ready candidates and bounds, spawning one owner thread
    /// per candidate.
    pub(super) fn from_candidates(
        candidates: Vec<OwnedCandidate>,
        bounds: SupervisorBounds,
    ) -> Result<Self, PersistentOwnerStartFailure> {
        let mut candidates = candidates.into_iter();
        let mut sessions = Vec::new();
        while let Some(candidate) = candidates.next() {
            match spawn_owner_thread(candidate, bounds) {
                Ok(session) => sessions.push(session),
                Err(mut failure) => {
                    failure
                        .reaped
                        .extend(candidates.map(|candidate| reap_owned(candidate, &bounds)));
                    let mut started = Self { sessions };
                    failure.reaped.extend(started.shutdown_with_evidence());
                    return Err(failure);
                }
            }
        }
        Ok(Self { sessions })
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
        let available = match session.available.lock() {
            Ok(available) => available,
            Err(poisoned) => poisoned.into_inner(),
        };
        if !*available {
            return Err(PersistentInvokeError::SessionGone);
        }
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
        drop(available);
        Ok(PersistentInvocation {
            progress_rx,
            cancel,
            done,
            result_rx: terminal_rx,
        })
    }

    /// Queue an `activate-panel` command on the owning persistent session.
    pub fn activate_panel(
        &self,
        plugin_id: &Id,
        request_id: super::identifiers::RequestId,
        payload: super::panel_model::ActivatePanelPayload,
    ) -> Result<(), PersistentInvokeError> {
        self.send_panel(
            plugin_id,
            request_id,
            PanelCommandPayload::Activate(payload),
        )
    }

    /// Queue a `deactivate-panel` command on the owning persistent session.
    pub fn deactivate_panel(
        &self,
        plugin_id: &Id,
        request_id: super::identifiers::RequestId,
        payload: super::panel_model::DeactivatePanelPayload,
    ) -> Result<(), PersistentInvokeError> {
        self.send_panel(
            plugin_id,
            request_id,
            PanelCommandPayload::Deactivate(payload),
        )
    }

    /// Queue a semantic `panel-event` command on the owning persistent session.
    pub fn panel_event(
        &self,
        plugin_id: &Id,
        request_id: super::identifiers::RequestId,
        payload: super::panel_model::PanelEventPayload,
    ) -> Result<(), PersistentInvokeError> {
        self.send_panel(plugin_id, request_id, PanelCommandPayload::Event(payload))
    }

    fn send_panel(
        &self,
        plugin_id: &Id,
        request_id: super::identifiers::RequestId,
        payload: PanelCommandPayload,
    ) -> Result<(), PersistentInvokeError> {
        let session = self
            .sessions
            .iter()
            .find(|session| &session.plugin_id == plugin_id)
            .ok_or(PersistentInvokeError::NoSession)?;
        let sender = session
            .panel_tx
            .as_ref()
            .ok_or(PersistentInvokeError::SessionGone)?;
        sender
            .try_send(PanelCommand {
                request_id,
                payload,
            })
            .map_err(|error| match error {
                mpsc::TrySendError::Full(_) => PersistentInvokeError::QueueFull,
                mpsc::TrySendError::Disconnected(_) => PersistentInvokeError::SessionGone,
            })
    }

    /// Drain all currently queued asynchronous panel snapshots.
    #[must_use]
    pub fn drain_panel_deliveries(&self) -> Vec<PanelDelivery> {
        let mut deliveries = Vec::new();
        for session in &self.sessions {
            deliveries.extend(session.panel_rx.try_iter());
        }
        deliveries
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
        let _ = self.shutdown_with_evidence();
    }

    fn shutdown_with_evidence(&mut self) -> Vec<super::persistent::ReapedCandidate> {
        let mut reaped = Vec::new();
        for session in &mut self.sessions {
            let _ = session.invocation_tx.take();
            let _ = session.panel_tx.take();
            if let Some(control_tx) = session.control_tx.take() {
                let _ = control_tx.send(ControlCommand::Shutdown);
            }
            if let Some(thread) = session.thread.take()
                && let Ok(Some(candidate)) = thread.join()
            {
                reaped.push(candidate);
            }
        }
        reaped
    }
}

impl Drop for PersistentSessionOwner {
    fn drop(&mut self) {
        for session in &mut self.sessions {
            let _ = session.invocation_tx.take();
            let _ = session.panel_tx.take();
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

include!("persistent_session_owner.rs");

#[cfg(test)]
mod active_input_order_tests {
    use super::*;

    #[test]
    fn panel_traffic_requires_the_negotiated_panels_capability() {
        assert!(!panel_traffic_allowed(&[
            super::super::dto::Capability::Actions
        ]));
        assert!(panel_traffic_allowed(&[
            super::super::dto::Capability::Panels
        ]));
    }

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
    fn deactivate_panel_is_queued_on_the_exact_persistent_owner() {
        let plugin_id =
            Id::parse("vendor.panel").unwrap_or_else(|error| panic!("plugin fixture: {error:?}"));
        let (panel_tx, panel_command_rx) = mpsc::sync_channel(1);
        let (_delivery_tx, panel_rx) = mpsc::sync_channel(1);
        let owner = PersistentSessionOwner {
            sessions: vec![PersistentSession {
                plugin_id: plugin_id.clone(),
                invocation_tx: None,
                panel_tx: Some(panel_tx),
                panel_rx,
                control_tx: None,
                available: Arc::new(Mutex::new(true)),
                thread: None,
            }],
        };
        let request_id = super::super::identifiers::RequestId::new_host(7)
            .unwrap_or_else(|error| panic!("request fixture: {error:?}"));
        let payload = super::super::panel_model::DeactivatePanelPayload {
            panel_instance_id: 9,
            generation: 3,
            reason: super::super::panel_model::DeactivateReason::Dispose,
        };

        owner
            .deactivate_panel(&plugin_id, request_id.clone(), payload.clone())
            .unwrap_or_else(|error| panic!("deactivation queues: {error:?}"));

        let command = panel_command_rx
            .try_recv()
            .unwrap_or_else(|error| panic!("queued command: {error:?}"));
        assert_eq!(command.request_id, request_id);
        let PanelCommandPayload::Deactivate(queued) = command.payload else {
            panic!("deactivate command must retain its closed payload");
        };
        assert_eq!(queued, payload);
    }

    #[test]
    fn health_probe_is_bounded_when_an_owner_does_not_reply() {
        let (control_tx, control_rx) = mpsc::sync_channel(1);
        let plugin_id = Id::parse("vendor.health").unwrap_or_else(|error| panic!("id: {error:?}"));
        let (_delivery_tx, panel_rx) = mpsc::sync_channel(1);
        let owner = PersistentSessionOwner {
            sessions: vec![PersistentSession {
                plugin_id: plugin_id.clone(),
                invocation_tx: None,
                panel_tx: None,
                panel_rx,
                control_tx: Some(control_tx),
                available: Arc::new(Mutex::new(true)),
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

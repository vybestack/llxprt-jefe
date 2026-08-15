fn health_probe_failed(plugin_id: Id, error: &str) -> CandidateHealthSnapshot {
    CandidateHealthSnapshot {
        plugin_id,
        health: CandidateHealth::ProbeFailed {
            error: error.to_owned(),
        },
    }
}

/// Shared admission gate for one persistent owner thread.
struct SessionAvailability(Arc<Mutex<bool>>);

impl SessionAvailability {
    fn mark_unavailable(&self) {
        let mut available = match self.0.lock() {
            Ok(available) => available,
            Err(poisoned) => poisoned.into_inner(),
        };
        *available = false;
    }
}

impl Drop for SessionAvailability {
    fn drop(&mut self) {
        self.mark_unavailable();
    }
}

/// Bounded command and delivery lanes exclusively owned by one provider thread.
struct OwnerChannels {
    invocation_rx: mpsc::Receiver<InvocationCommand>,
    panel_command_rx: mpsc::Receiver<PanelCommand>,
    panel_delivery_tx: mpsc::SyncSender<PanelDelivery>,
    control_rx: mpsc::Receiver<ControlCommand>,
}

/// Spawn one command-owner thread for a candidate.
fn spawn_owner_thread(
    candidate: OwnedCandidate,
    bounds: SupervisorBounds,
) -> Result<PersistentSession, PersistentOwnerStartFailure> {
    let plugin_id = candidate.plugin_id.clone();
    let available = Arc::new(Mutex::new(true));
    let thread_available = Arc::clone(&available);
    let (invocation_tx, invocation_rx) = mpsc::sync_channel(super::MAX_QUEUED_ENVELOPES);
    let (panel_tx, panel_command_rx) = mpsc::sync_channel(super::MAX_QUEUED_ENVELOPES);
    let (panel_delivery_tx, panel_rx) = mpsc::sync_channel(super::MAX_QUEUED_ENVELOPES);
    let (control_tx, control_rx) = mpsc::sync_channel(super::MAX_QUEUED_ENVELOPES);
    let (candidate_tx, candidate_rx) = mpsc::sync_channel(1);
    let thread_result = thread::Builder::new()
        .name(format!("jefe-persistent-{plugin_id}"))
        .spawn(move || {
            let availability = SessionAvailability(thread_available);
            let Ok(candidate) = candidate_rx.recv() else {
                return None;
            };
            let channels = OwnerChannels {
                invocation_rx,
                panel_command_rx,
                panel_delivery_tx,
                control_rx,
            };
            Some(run_owner_thread(candidate, bounds, channels, &availability))
        });

    match thread_result {
        Ok(thread) => match candidate_tx.send(candidate) {
            Ok(()) => Ok(PersistentSession {
                plugin_id,
                invocation_tx: Some(invocation_tx),
                panel_tx: Some(panel_tx),
                panel_rx,
                control_tx: Some(control_tx),
                available,
                thread: Some(thread),
            }),
            Err(error) => {
                let reaped = reap_after_owner_start_failure(error.0, &bounds);
                let _ = thread.join();
                Err(PersistentOwnerStartFailure {
                    plugin_id,
                    cause: PersistentOwnerStartCause::CandidateHandoff,
                    reaped: vec![reaped],
                })
            }
        },
        Err(error) => {
            let reaped = reap_after_owner_start_failure(candidate, &bounds);
            Err(PersistentOwnerStartFailure {
                plugin_id,
                cause: PersistentOwnerStartCause::Thread(error),
                reaped: vec![reaped],
            })
        }
    }
}

fn reap_after_owner_start_failure(
    candidate: OwnedCandidate,
    bounds: &SupervisorBounds,
) -> super::persistent::ReapedCandidate {
    let reaped = reap_owned(candidate, bounds);
    if !reaped.reaped {
        tracing::warn!(
            plugin_id = %reaped.plugin_id,
            cleanup_failure = ?reaped.cleanup_failure,
            "persistent candidate cleanup after owner-start failure was incomplete"
        );
    }
    reaped
}

/// The owner thread's main loop. Invocation traffic is bounded independently
/// from control traffic, so health and shutdown remain serviceable while a
/// provider action is live.
fn run_owner_thread(
    mut candidate: OwnedCandidate,
    bounds: SupervisorBounds,
    channels: OwnerChannels,
    availability: &SessionAvailability,
) -> super::persistent::ReapedCandidate {
    let OwnerChannels {
        invocation_rx,
        panel_command_rx,
        panel_delivery_tx,
        control_rx,
    } = channels;
    loop {
        if service_idle_control(&mut candidate, &control_rx)
            || !service_panel_commands(&mut candidate, &panel_command_rx)
            || !service_idle_stdout(&mut candidate, &panel_delivery_tx)
        {
            availability.mark_unavailable();
            reject_pending_invocations(&invocation_rx);
            return reap_owned(candidate, &bounds);
        }
        match invocation_rx.recv_timeout(CANCEL_POLL) {
            Ok(command) => {
                let signals = InvocationSignals {
                    timeout: command.timeout,
                    progress_tx: &command.progress_tx,
                    cancel: &command.cancel,
                    control_rx: &control_rx,
                    panel_command_rx: &panel_command_rx,
                    panel_delivery_tx: &panel_delivery_tx,
                };
                let (result, shutdown) = drive_invocation(
                    &mut candidate,
                    &command.request_id,
                    &command.payload,
                    &signals,
                );
                let unavailable = shutdown || !candidate.healthy;
                if unavailable {
                    availability.mark_unavailable();
                    reject_pending_invocations(&invocation_rx);
                }
                let _ = command.terminal_tx.send(result);
                command.done.store(true, Ordering::SeqCst);
                if unavailable {
                    return reap_owned(candidate, &bounds);
                }
            }
            Err(mpsc::RecvTimeoutError::Timeout) => {}
            Err(mpsc::RecvTimeoutError::Disconnected) => {
                availability.mark_unavailable();
                return reap_owned(candidate, &bounds);
            }
        }
    }
}

/// Complete commands admitted just before this generation became unavailable.
fn reject_pending_invocations(invocation_rx: &mpsc::Receiver<InvocationCommand>) {
    for command in invocation_rx.try_iter() {
        let result = OneShotResult::without_process(SupervisorFailure::Crashed { exit: None });
        let _ = command.terminal_tx.send(result);
        command.done.store(true, Ordering::SeqCst);
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

fn service_panel_commands(
    candidate: &mut OwnedCandidate,
    commands: &mpsc::Receiver<PanelCommand>,
) -> bool {
    loop {
        let command = match commands.try_recv() {
            Ok(command) => command,
            Err(mpsc::TryRecvError::Empty | mpsc::TryRecvError::Disconnected) => return true,
        };
        let frame = match &command.payload {
            PanelCommandPayload::Activate(payload) => {
                encode::encode_activate_panel(&command.request_id, candidate.generation, payload)
            }
            PanelCommandPayload::Deactivate(payload) => {
                encode::encode_deactivate_panel(&command.request_id, candidate.generation, payload)
            }
            PanelCommandPayload::Event(payload) => {
                encode::encode_panel_event(&command.request_id, candidate.generation, payload)
            }
        };
        if write_frame(candidate.stdin.as_mut(), &frame).is_err() {
            candidate.healthy = false;
            return false;
        }
        tracing::debug!(plugin_id = %candidate.plugin_id, "sent provider panel command");
    }
}

fn service_idle_stdout(
    candidate: &mut OwnedCandidate,
    deliveries: &mpsc::SyncSender<PanelDelivery>,
) -> bool {
    loop {
        let event = match candidate.stdout_drain.receiver.try_recv() {
            Ok(event) => event,
            Err(mpsc::TryRecvError::Empty) => return true,
            Err(mpsc::TryRecvError::Disconnected) => {
                tracing::warn!(plugin_id = %candidate.plugin_id, "persistent provider stdout closed while idle");
                candidate.exited = true;
                return false;
            }
        };
        let StdoutEvent::Frame(frame) = event else {
            candidate.healthy = false;
            return false;
        };
        tracing::debug!(plugin_id = %candidate.plugin_id, "received persistent provider idle stdout frame");
        let parsed = match parse_message(&frame, Direction::ProviderToHost) {
            Ok(parsed) => parsed,
            Err(error) => {
                tracing::warn!(plugin_id = %candidate.plugin_id, %error, "persistent provider emitted an invalid idle frame");
                candidate.healthy = false;
                return false;
            }
        };
        if parsed.generation != candidate.generation {
            tracing::warn!(
                plugin_id = %candidate.plugin_id,
                generation = parsed.generation,
                "persistent provider changed generation while idle"
            );
            candidate.healthy = false;
            return false;
        }
        let parsed_kind = parsed.kind();
        let payload_byte_count = u64::try_from(parsed.payload_byte_count).unwrap_or(u64::MAX);
        let ProviderMessage::PanelSnapshot(snapshot) = parsed.message else {
            tracing::warn!(plugin_id = %candidate.plugin_id, kind = %parsed_kind.as_str(), "persistent provider emitted an unexpected idle message");
            candidate.healthy = false;
            return false;
        };
        let Some(snapshot) = redact_panel_snapshot(snapshot, &candidate.redactor) else {
            candidate.healthy = false;
            return false;
        };
        let delivery = PanelDelivery {
            plugin_id: candidate.plugin_id.clone(),
            process_generation: parsed.generation,
            payload_byte_count,
            snapshot,
        };
        if deliveries.try_send(delivery).is_err() {
            candidate.healthy = false;
            return false;
        }
    }
}

struct InvocationSignals<'a> {
    timeout: Duration,
    progress_tx: &'a mpsc::SyncSender<ProgressPayload>,
    cancel: &'a AtomicBool,
    control_rx: &'a mpsc::Receiver<ControlCommand>,
    panel_command_rx: &'a mpsc::Receiver<PanelCommand>,
    panel_delivery_tx: &'a mpsc::SyncSender<PanelDelivery>,
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
        candidate.healthy = false;
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
        if !service_panel_commands(candidate, signals.panel_command_rx) {
            return (
                OneShotOutcome::Failed(SupervisorFailure::Protocol(
                    super::driver::unexpected_after_invoke(),
                )),
                false,
            );
        }
        match poll_active_input(
            &candidate.stdout_drain.receiver,
            signals.control_rx,
            signals.cancel,
        ) {
            ActiveInput::Stdout(event) => {
                if let Some(outcome) =
                    accept_invocation_event(candidate, &mut progress, signals, event)
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
            candidate.healthy = false;
            return (
                OneShotOutcome::Failed(SupervisorFailure::InvocationTimeout),
                false,
            );
        }
        let read_timeout = deadline.saturating_duration_since(now).min(CANCEL_POLL);
        match candidate.stdout_drain.receiver.recv_timeout(read_timeout) {
            Ok(event) => {
                if let Some(outcome) =
                    accept_invocation_event(candidate, &mut progress, signals, event)
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
    candidate.healthy = false;
    candidate.exited = true;
    (
        OneShotOutcome::Failed(SupervisorFailure::Crashed { exit: None }),
        false,
    )
}

/// Redact and enqueue one snapshot observed during an active invocation.
fn deliver_active_panel_snapshot(
    candidate: &mut OwnedCandidate,
    signals: &InvocationSignals<'_>,
    process_generation: u64,
    payload_byte_count: usize,
    snapshot: super::panel_model::PanelSnapshot,
) -> Option<OneShotOutcome> {
    if process_generation != candidate.generation {
        candidate.healthy = false;
        return Some(OneShotOutcome::Failed(SupervisorFailure::Protocol(
            ProviderError::InvalidGeneration {
                value: process_generation,
            },
        )));
    }
    let Some(snapshot) = redact_panel_snapshot(snapshot, &candidate.redactor) else {
        candidate.healthy = false;
        return Some(OneShotOutcome::Failed(SupervisorFailure::Protocol(
            ProviderError::InvalidValue {
                path: "panel-snapshot".to_owned(),
                reason: "snapshot cannot be represented after secret redaction".to_owned(),
            },
        )));
    };
    let delivery = PanelDelivery {
        plugin_id: candidate.plugin_id.clone(),
        process_generation,
        payload_byte_count: u64::try_from(payload_byte_count).unwrap_or(u64::MAX),
        snapshot,
    };
    if signals.panel_delivery_tx.try_send(delivery).is_err() {
        candidate.healthy = false;
        return Some(OneShotOutcome::Failed(SupervisorFailure::Io(
            "panel delivery queue unavailable".to_owned(),
        )));
    }
    None
}

fn fail_active_generation(
    candidate: &mut OwnedCandidate,
    failure: SupervisorFailure,
) -> OneShotOutcome {
    candidate.healthy = false;
    OneShotOutcome::Failed(failure)
}

/// Apply one provider stream event to the active invocation.
///
/// `None` means a progress or panel-snapshot frame was accepted and the
/// invocation remains live; every terminal or malformed event returns the
/// authoritative result.
fn accept_invocation_event(
    candidate: &mut OwnedCandidate,
    progress: &mut ProgressTracker,
    signals: &InvocationSignals<'_>,
    event: StdoutEvent,
) -> Option<OneShotOutcome> {
    let parsed = match event {
        StdoutEvent::Frame(frame) => match parse_message(&frame, Direction::ProviderToHost) {
            Ok(parsed) => parsed,
            Err(error) => {
                return Some(fail_active_generation(
                    candidate,
                    SupervisorFailure::Protocol(error),
                ));
            }
        },
        StdoutEvent::Oversize(error) => {
            return Some(fail_active_generation(
                candidate,
                SupervisorFailure::Protocol(error),
            ));
        }
        StdoutEvent::ReadError => {
            candidate.exited = true;
            return Some(fail_active_generation(
                candidate,
                SupervisorFailure::Io("stdout read failed".to_owned()),
            ));
        }
    };
    if let ProviderMessage::PanelSnapshot(snapshot) = &parsed.message {
        return deliver_active_panel_snapshot(
            candidate,
            signals,
            parsed.generation,
            parsed.payload_byte_count,
            snapshot.clone(),
        );
    }
    if observe_inbound(
        &mut candidate.lifecycle,
        progress,
        &parsed.message,
        candidate.generation,
    )
    .is_err()
    {
        return Some(fail_active_generation(
            candidate,
            SupervisorFailure::Protocol(super::driver::unexpected_after_invoke()),
        ));
    }
    match parsed.message {
        ProviderMessage::Progress(mut payload) => {
            payload.message = candidate.redactor.redact(&payload.message).into_owned();
            let _ = signals.progress_tx.send(payload);
            None
        }
        ProviderMessage::Outcome(outcome) => Some(OneShotOutcome::Completed(outcome)),
        ProviderMessage::Error(error) => Some(OneShotOutcome::ProviderError(error)),
        _ => Some(fail_active_generation(
            candidate,
            SupervisorFailure::Protocol(super::driver::unexpected_after_invoke()),
        )),
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
    if write_frame(candidate.stdin.as_mut(), &frame).is_err() {
        candidate.healthy = false;
    }
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
        ProviderMessage::ActivatePanel(_) => MessageKind::ActivatePanel,
        ProviderMessage::DeactivatePanel(_) => MessageKind::DeactivatePanel,
        ProviderMessage::PanelEvent(_) => MessageKind::PanelEvent,
        ProviderMessage::PanelSnapshot(_) => MessageKind::PanelSnapshot,
        ProviderMessage::MigrateConfig(_) => MessageKind::MigrateConfig,
        ProviderMessage::MigratedConfig(_) => MessageKind::MigratedConfig,
    }
}

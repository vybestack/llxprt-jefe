//! Provider lifecycle worker extracted from the general app-shell workers.

use std::sync::Arc;
use std::time::Duration;

use tracing::warn;

use crate::AppContext;

/// Poll interval for the provider effect worker drain loop.
const PROVIDER_POLL_MS: u64 = 50;

/// Run the background provider effect worker drain loop
/// (issue #390 CW-10, Slice D, remediation S14–S26).
///
/// Polls [`ProviderEffectHandle`] every [`PROVIDER_POLL_MS`] and runs up to
/// [`MAX_ACTIVE_REQUESTS`](jefe::state::provider_requests::MAX_ACTIVE_REQUESTS)
/// streaming sessions concurrently. Progress and exact-key cancellation remain
/// live for every session; finished sessions deliver terminals in dispatch order,
/// and each free slot is refilled from the FIFO deferred-work queue. Descriptor-
/// selected timeouts are carried exactly into invocation bounds (S15), while
/// cleanup diagnostics remain separate from first-terminal results (S26).
pub async fn run_provider_worker(
    ctx: Option<Arc<std::sync::Mutex<AppContext>>>,
    mut app_state: crate::app_input::AppStateHandle,
) {
    let Some(ctx_arc) = ctx.as_ref() else {
        return;
    };
    let mut active: Vec<ActiveSession> = Vec::new();
    let mut deferred: std::collections::VecDeque<
        jefe::services::provider_effect_worker::ProviderWorkItem,
    > = std::collections::VecDeque::new();
    let mut unavailable_actions = std::collections::BTreeMap::new();
    let mut unavailable_panel_owners = std::collections::BTreeSet::new();
    let mut next_health_probe = std::time::Instant::now();
    let panel_clock_origin = std::time::Instant::now();

    loop {
        smol::Timer::after(Duration::from_millis(PROVIDER_POLL_MS)).await;
        run_pending_migrations(ctx_arc, &mut app_state).await;
        forward_session_signals(&active, ctx_arc, &mut app_state);
        finalize_finished_sessions(&mut active, ctx_arc, &mut app_state);
        dispatch_panel_commands(ctx_arc, &mut app_state);
        let elapsed_ms =
            u64::try_from(panel_clock_origin.elapsed().as_millis()).unwrap_or(u64::MAX);
        accept_panel_deliveries(
            ctx_arc,
            &mut app_state,
            elapsed_ms,
            &unavailable_panel_owners,
        );
        start_available_work(&mut active, &mut deferred, ctx_arc, &mut app_state);
        if std::time::Instant::now() >= next_health_probe {
            publish_persistent_health(
                ctx_arc,
                &mut app_state,
                &mut unavailable_actions,
                &mut unavailable_panel_owners,
            );
            next_health_probe = std::time::Instant::now() + Duration::from_secs(1);
        }
    }
}
async fn run_pending_migrations(
    ctx_arc: &Arc<std::sync::Mutex<AppContext>>,
    app_state: &mut crate::app_input::AppStateHandle,
) {
    let work = {
        let Ok(context) = ctx_arc.try_lock() else {
            return;
        };
        context.provider_effect_handle.drain_migrations()
    };
    for item in work {
        let draft_token = item.draft_token;
        let result = smol::unblock(move || {
            jefe::runtime::provider::run_migration(
                &item.request,
                &jefe::runtime::provider::SupervisorBounds::default(),
                &jefe::runtime::provider::ProcessHostEnv,
            )
        })
        .await;
        let message = match result.outcome {
            jefe::runtime::provider::MigrationOutcome::Migrated(response)
                if result.cleanup_failure.is_none() =>
            {
                jefe::messages::settings::SettingsMessage::MigrationCompleted {
                    draft_token: response.draft_token,
                    target_config: response.target_config,
                    notes: response.notes,
                }
            }
            jefe::runtime::provider::MigrationOutcome::Migrated(_) => {
                let detail = result.cleanup_failure.as_ref().map_or_else(
                    || {
                        "the provisional provider could not complete configuration migration"
                            .to_owned()
                    },
                    migration_cleanup_failure_detail,
                );
                jefe::messages::settings::SettingsMessage::MigrationFailed {
                    draft_token,
                    detail,
                }
            }
            jefe::runtime::provider::MigrationOutcome::Failed(_) => {
                jefe::messages::settings::SettingsMessage::MigrationFailed {
                    draft_token,
                    detail: "the provisional provider could not complete configuration migration"
                        .to_owned(),
                }
            }
        };
        {
            let mut state = app_state.write();
            jefe::state::transition::commit_pure_site(
                &mut state,
                jefe::messages::AppMessage::Settings(Box::new(message)),
            );
        }
        crate::app_input::write_pending(app_state, &Some(Arc::clone(ctx_arc)));
    }
}

fn migration_cleanup_failure_detail(failure: &jefe::runtime::provider::CleanupFailure) -> String {
    let reason = match failure {
        jefe::runtime::provider::CleanupFailure::ShutdownAck(_) => {
            "the provisional provider returned an invalid shutdown acknowledgement"
        }
        jefe::runtime::provider::CleanupFailure::PostTerminal(_) => {
            "the provisional provider sent data after the migration response"
        }
        jefe::runtime::provider::CleanupFailure::DrainTimeout => {
            "the provisional provider output did not close"
        }
        jefe::runtime::provider::CleanupFailure::NotReaped => {
            "the provisional provider process was not reaped"
        }
        jefe::runtime::provider::CleanupFailure::Io(_) => {
            "the provisional provider cleanup operation failed"
        }
    };
    format!(
        "{} migration response was not accepted because {reason}; settings remain unchanged",
        failure.code()
    )
}

fn dispatch_panel_commands(
    ctx_arc: &Arc<std::sync::Mutex<AppContext>>,
    app_state: &mut crate::app_input::AppStateHandle,
) {
    let Ok(ctx_guard) = ctx_arc.try_lock() else {
        return;
    };
    let commands = ctx_guard.provider_effect_handle.drain_panel_commands();
    let Some(coordinator) = ctx_guard.provider_coordinator.as_ref() else {
        return;
    };
    for command in commands {
        let Some(dispatched) = send_panel_effect(coordinator, command.effect) else {
            continue;
        };
        commit_panel_dispatch(app_state, command.correlation, dispatched);
    }
}

struct PanelDispatch {
    owner: jefe::domain::Id,
    instance: u64,
    result: Result<(), String>,
}

fn panel_dispatch(
    owner: jefe::domain::Id,
    instance: u64,
    result: Result<(), impl std::fmt::Display>,
) -> PanelDispatch {
    PanelDispatch {
        owner,
        instance,
        result: result.map_err(|error| error.to_string()),
    }
}

fn send_panel_effect(
    coordinator: &jefe::runtime::provider::ProviderCoordinator,
    effect: jefe::domain::effects::ProviderEffect,
) -> Option<PanelDispatch> {
    use jefe::domain::effects::ProviderEffect;
    use jefe::runtime::provider::protocol::{
        ActivatePanelPayload, DeactivatePanelPayload, PanelEventPayload,
    };

    match effect {
        ProviderEffect::ActivatePanel {
            owner,
            panel_instance_id,
            screen_instance_id,
            panel_type,
            activation,
            prior_host_local,
            panel_generation,
        } => {
            let payload = ActivatePanelPayload {
                panel_instance_id,
                screen_instance_id,
                panel_type,
                activation,
                prior_host_local: prior_host_local.map(runtime_host_local),
                generation: panel_generation,
            };
            let result = coordinator.activate_panel(&owner, payload);
            Some(panel_dispatch(owner, panel_instance_id, result))
        }
        ProviderEffect::DeactivatePanel {
            owner,
            panel_instance_id,
            panel_generation,
            reason,
        } => {
            let reason = runtime_deactivate_reason(reason);
            let payload = DeactivatePanelPayload {
                panel_instance_id,
                reason,
                generation: panel_generation,
            };
            let result = coordinator.deactivate_panel(&owner, payload);
            Some(panel_dispatch(owner, panel_instance_id, result))
        }
        ProviderEffect::PanelEvent {
            owner,
            panel_instance_id,
            panel_generation,
            revision,
            event,
        } => {
            let payload = PanelEventPayload {
                panel_instance_id,
                revision,
                event: runtime_panel_event(event),
                generation: panel_generation,
            };
            let result = coordinator.panel_event(&owner, payload);
            Some(panel_dispatch(owner, panel_instance_id, result))
        }
        _ => None,
    }
}

fn runtime_deactivate_reason(
    reason: jefe::domain::effects::ProviderPanelDeactivateReason,
) -> jefe::runtime::provider::protocol::DeactivateReason {
    use jefe::domain::effects::ProviderPanelDeactivateReason as HostReason;
    use jefe::runtime::provider::protocol::DeactivateReason as WireReason;

    match reason {
        HostReason::Suspend => WireReason::Suspend,
        HostReason::Dispose => WireReason::Dispose,
        HostReason::Replace => WireReason::Replace,
    }
}

fn runtime_host_local(
    local: jefe::domain::effects::ProviderPanelHostLocal,
) -> jefe::runtime::provider::protocol::HostLocal {
    jefe::runtime::provider::protocol::HostLocal {
        focus_target: local.focus_target,
        scroll_offset: local.scroll_offset,
        selected_id: local.selected_id,
        form_draft: local.form_draft,
    }
}

fn commit_panel_dispatch(
    app_state: &mut crate::app_input::AppStateHandle,
    correlation: jefe::domain::effects::Correlation,
    dispatched: PanelDispatch,
) {
    match dispatched.result {
        Ok(()) => {
            let mut state = app_state.write();
            let completion = jefe::domain::effects::EffectCompletion {
                correlation,
                result: Ok(jefe::domain::effects::EffectResponse::Provider(
                    jefe::domain::effects::ProviderResponse::PanelCommandSent {
                        panel_instance_id: dispatched.instance,
                    },
                )),
            };
            jefe::state::transition::commit_pure_site(
                &mut state,
                jefe::messages::AppMessage::EffectCompletion(Box::new(completion)),
            );
        }
        Err(error) => {
            tracing::warn!(owner = %dispatched.owner, %error, "provider panel command failed");
            let mut state = app_state.write();
            if let Err(lifecycle_error) = state.provider_panels.fail_runtime(
                jefe::state::provider_panels::PanelInstanceId::from_u64(dispatched.instance),
            ) {
                tracing::debug!(
                    panel_instance = dispatched.instance,
                    %lifecycle_error,
                    "panel delivery failure arrived after a terminal lifecycle transition"
                );
            }
            state.error_message = Some(error);
            let completion = jefe::domain::effects::EffectCompletion {
                correlation,
                result: Err(jefe::domain::effects::EffectError::new(
                    jefe::domain::effects::EffectErrorKind::Unavailable,
                    false,
                    "provider panel delivery unavailable",
                )),
            };
            jefe::state::transition::commit_pure_site(
                &mut state,
                jefe::messages::AppMessage::EffectCompletion(Box::new(completion)),
            );
            drop(state);
        }
    }
}

fn panel_delivery_owner_available(
    unavailable_panel_owners: &std::collections::BTreeSet<jefe::domain::Id>,
    owner: &jefe::domain::Id,
) -> bool {
    !unavailable_panel_owners.contains(owner)
}

fn accept_panel_deliveries(
    ctx_arc: &Arc<std::sync::Mutex<AppContext>>,
    app_state: &mut crate::app_input::AppStateHandle,
    elapsed_ms: u64,
    unavailable_panel_owners: &std::collections::BTreeSet<jefe::domain::Id>,
) {
    let Ok(ctx_guard) = ctx_arc.try_lock() else {
        return;
    };
    let Some(coordinator) = ctx_guard.provider_coordinator.as_ref() else {
        return;
    };
    let deliveries = coordinator.drain_panel_deliveries();
    drop(ctx_guard);
    for delivery in deliveries {
        if !panel_delivery_owner_available(unavailable_panel_owners, &delivery.plugin_id) {
            tracing::warn!(
                owner = %delivery.plugin_id,
                "late provider panel snapshot ignored after persistent owner failure"
            );
            continue;
        }
        let mut state = app_state.write();
        let accepted =
            state
                .provider_panels
                .accept_snapshot(jefe::state::provider_panels::AcceptSnapshot {
                    owner: &delivery.plugin_id,
                    received_process_generation: delivery.process_generation,
                    payload_byte_count: delivery.payload_byte_count,
                    elapsed_ms,
                    snapshot: &delivery.snapshot,
                });
        if let Err(error) = accepted {
            tracing::warn!(owner = %delivery.plugin_id, %error, "provider panel snapshot rejected");
            state.error_message = Some(error.to_string());
        }
    }
}

fn runtime_panel_event(
    event: jefe::domain::effects::ProviderPanelEvent,
) -> jefe::runtime::provider::protocol::PanelEvent {
    use jefe::domain::effects::ProviderPanelEvent as HostEvent;
    use jefe::runtime::provider::protocol::PanelEvent as WireEvent;

    match event {
        HostEvent::Selected { id } => WireEvent::Selected { id },
        HostEvent::Activated { id } => WireEvent::Activated { id },
        HostEvent::Action { id, arguments } => WireEvent::Action { id, arguments },
        HostEvent::FieldChanged { field_id, value } => WireEvent::FieldChanged { field_id, value },
        HostEvent::Submit { values } => WireEvent::Submit { values },
        HostEvent::PageRequested { token } => WireEvent::PageRequested { token },
        HostEvent::Retry => WireEvent::Retry,
        HostEvent::Cancel => WireEvent::Cancel,
        HostEvent::LinkSelected { link_id } => WireEvent::LinkSelected { link_id },
        HostEvent::ExpansionChanged { id, expanded } => {
            WireEvent::ExpansionChanged { id, expanded }
        }
    }
}

/// Publish post-Ready persistent failures into immutable action availability.
/// Healthy candidates contribute no override and are never restarted here.
fn publish_persistent_health(
    ctx_arc: &Arc<std::sync::Mutex<AppContext>>,
    app_state: &mut crate::app_input::AppStateHandle,
    prior_actions: &mut std::collections::BTreeMap<jefe::domain::action_registry::ActionId, String>,
    prior_panel_owners: &mut std::collections::BTreeSet<jefe::domain::Id>,
) {
    let Some((unavailable, failed_owners)) = persistent_unavailable_actions(ctx_arc) else {
        return;
    };
    if unavailable == *prior_actions && failed_owners == *prior_panel_owners {
        return;
    }
    prior_actions.clone_from(&unavailable);
    prior_panel_owners.clone_from(&failed_owners);
    {
        let mut state = app_state.write();
        for owner in &failed_owners {
            state.provider_panels.fail_runtime_owner(owner);
        }
        jefe::state::transition::commit_pure_site(
            &mut state,
            jefe::messages::AppMessage::Provider(Box::new(
                jefe::messages::ProviderMessage::HealthChanged { unavailable },
            )),
        );
        drop(state);
    }
    crate::app_input::refresh_action_availability(app_state);
}

fn persistent_unavailable_actions(
    ctx_arc: &Arc<std::sync::Mutex<AppContext>>,
) -> Option<(
    std::collections::BTreeMap<jefe::domain::action_registry::ActionId, String>,
    std::collections::BTreeSet<jefe::domain::Id>,
)> {
    use jefe::runtime::provider::persistent::CandidateHealth;

    let Ok(ctx_guard) = ctx_arc.lock() else {
        tracing::error!("provider context lock poisoned during health probe");
        return None;
    };
    let Some(coordinator) = ctx_guard.provider_coordinator.as_ref() else {
        return Some((
            std::collections::BTreeMap::new(),
            std::collections::BTreeSet::new(),
        ));
    };
    let failed_plugins = coordinator
        .health()
        .into_iter()
        .filter_map(|snapshot| {
            let reason = match snapshot.health {
                CandidateHealth::Ready { .. } => return None,
                CandidateHealth::Exited { .. } => "provider stopped after ready",
                CandidateHealth::ProbeFailed { .. } => "provider health probe failed",
                CandidateHealth::ProtocolFault { .. } => "provider protocol fault after ready",
            };
            Some((snapshot.plugin_id, reason.to_owned()))
        })
        .collect::<std::collections::BTreeMap<_, _>>();
    let unavailable = ctx_guard
        .workbench
        .provider_catalog()
        .iter()
        .filter_map(|(action_id, descriptor)| {
            failed_plugins
                .get(&descriptor.plugin_id)
                .map(|reason| (action_id.clone(), reason.clone()))
        })
        .collect();
    let failed_owners = failed_plugins.keys().cloned().collect();
    Some((unavailable, failed_owners))
}

/// Forward host cancels and live progress for every active session.
fn forward_session_signals(
    active: &[ActiveSession],
    ctx_arc: &Arc<std::sync::Mutex<AppContext>>,
    app_state: &mut crate::app_input::AppStateHandle,
) {
    forward_exact_cancels(active, drain_cancels(ctx_arc));
    drain_session_progress(active, |correlation, key, payload| {
        deliver_live_progress(app_state, correlation, key, payload);
    });
}

/// Drain all currently available progress from every live session in dispatch
/// order without letting a quiet session block another session's events.
fn drain_session_progress(
    active: &[ActiveSession],
    mut deliver: impl FnMut(
        &jefe::domain::effects::Correlation,
        &jefe::domain::effects::ProviderRequestKey,
        jefe::runtime::provider::protocol::ProgressPayload,
    ),
) {
    for session in active {
        while let Ok(payload) = session.progress_rx.try_recv() {
            deliver(&session.correlation, &session.key, payload);
        }
    }
}

/// Forward each cancellation to the session with the exact owner/action/generation
/// identity. Request generations are globally unique, but matching all equal
/// identities also makes duplicate internal registration visible as cancellation
/// rather than silently choosing one arbitrary session.
fn forward_exact_cancels(
    active: &[ActiveSession],
    cancel_keys: impl IntoIterator<Item = jefe::domain::effects::ProviderRequestKey>,
) {
    for cancel_key in cancel_keys {
        for session in active.iter().filter(|session| session.key == cancel_key) {
            session
                .cancel
                .store(true, std::sync::atomic::Ordering::SeqCst);
        }
    }
}

/// When the active session's terminal result is ready, consume it and deliver
/// through the reducer (S16/S26). Leaves `active` intact while the invocation
/// is still running, so cancellation stays observable.
fn finalize_finished_sessions(
    active: &mut Vec<ActiveSession>,
    ctx: &Arc<std::sync::Mutex<AppContext>>,
    app_state: &mut crate::app_input::AppStateHandle,
) {
    let mut still_active = Vec::with_capacity(active.len());
    for session in active.drain(..) {
        if session.is_terminal_ready() {
            let key = session.key.clone();
            let correlation = session.correlation.clone();
            let result = session.finish();
            let exec = jefe::services::provider_effect_worker::build_streaming_execution_result(
                correlation,
                &result,
                &key,
            );
            deliver_provider_messages(app_state, ctx, exec);
        } else {
            still_active.push(session);
        }
    }
    *active = still_active;
}

/// Fill every available session slot from the FIFO deferred-work queue.
/// Deferred items are tried first (dispatch order is preserved); a
/// momentarily-unavailable context lock leaves the head item owned locally for
/// the next poll rather than failing or allowing later work to overtake it.
fn start_available_work(
    active: &mut Vec<ActiveSession>,
    deferred: &mut std::collections::VecDeque<
        jefe::services::provider_effect_worker::ProviderWorkItem,
    >,
    ctx_arc: &Arc<std::sync::Mutex<AppContext>>,
    app_state: &mut crate::app_input::AppStateHandle,
) {
    if deferred.is_empty() {
        deferred.extend(drain_work(ctx_arc));
    }
    fill_session_slots(
        active,
        deferred,
        jefe::state::provider_requests::MAX_ACTIVE_REQUESTS,
        |item| start_provider_session(ctx_arc, item),
        |exec_result| deliver_provider_messages(app_state, ctx_arc, exec_result),
    );
}

/// Fill bounded live-session slots from a FIFO queue. A deferred head remains
/// the head and stops admission, while a failed start consumes no slot and lets
/// the next queued item try immediately.
fn fill_session_slots<Item, Session, Failure>(
    active: &mut Vec<Session>,
    deferred: &mut std::collections::VecDeque<Item>,
    limit: usize,
    mut start: impl FnMut(Item) -> SessionStart<Item, Session, Failure>,
    mut deliver_failure: impl FnMut(Failure),
) {
    while active.len() < limit {
        let Some(item) = deferred.pop_front() else {
            break;
        };
        match start(item) {
            SessionStart::Started(session) => active.push(session),
            SessionStart::Deferred(item) => {
                deferred.push_front(item);
                break;
            }
            SessionStart::Failed(failure) => deliver_failure(failure),
        }
    }
}

/// Drain pending cancel keys from the handle.
fn drain_cancels(
    ctx_arc: &Arc<std::sync::Mutex<AppContext>>,
) -> Vec<jefe::domain::effects::ProviderRequestKey> {
    let Ok(ctx_guard) = ctx_arc.try_lock() else {
        return Vec::new();
    };
    ctx_guard.provider_effect_handle.drain_cancels()
}

/// Drain pending work items from the handle.
fn drain_work(
    ctx_arc: &Arc<std::sync::Mutex<AppContext>>,
) -> Vec<jefe::services::provider_effect_worker::ProviderWorkItem> {
    let Ok(ctx_guard) = ctx_arc.try_lock() else {
        return Vec::new();
    };
    if !ctx_guard.provider_effect_handle.is_dirty() {
        return Vec::new();
    }
    let handle = ctx_guard.provider_effect_handle.clone();
    handle.drain()
}

/// Deliver one live progress payload through the reducer before the terminal
/// (S16). Stale-generation protection lives in the reducer: a progress for a
/// superseded or terminal request changes nothing.
fn deliver_live_progress(
    app_state: &mut crate::app_input::AppStateHandle,
    correlation: &jefe::domain::effects::Correlation,
    key: &jefe::domain::effects::ProviderRequestKey,
    payload: jefe::runtime::provider::protocol::ProgressPayload,
) {
    let mut state = app_state.write();
    if !state.pending_effects.is_pending(correlation) {
        return;
    }
    jefe::state::transition::commit_pure_site(
        &mut state,
        jefe::messages::AppMessage::Provider(Box::new(jefe::messages::ProviderMessage::Progress {
            key: key.clone(),
            payload,
        })),
    );
    drop(state);
}

/// One active streaming session (S16/S17).
struct ActiveSession {
    key: jefe::domain::effects::ProviderRequestKey,
    correlation: jefe::domain::effects::Correlation,
    progress_rx: std::sync::mpsc::Receiver<jefe::runtime::provider::protocol::ProgressPayload>,
    cancel: std::sync::Arc<std::sync::atomic::AtomicBool>,
    terminal: TerminalSource,
}

/// How the terminal result arrives for the active session.
enum TerminalSource {
    /// One-shot: the session thread owns the lifecycle and returns the result.
    Thread(std::thread::JoinHandle<jefe::runtime::provider::supervisor::OneShotResult>),
    /// Persistent: the owner thread sends the result through a channel when the
    /// invocation finishes, leaving the candidate alive for the next request.
    Persistent {
        /// `true` once the owner thread has sent the terminal result.
        done: std::sync::Arc<std::sync::atomic::AtomicBool>,
        /// Terminal result sink.
        result_rx: std::sync::mpsc::Receiver<jefe::runtime::provider::supervisor::OneShotResult>,
    },
}

impl ActiveSession {
    /// Whether the terminal result is ready to consume.
    fn is_terminal_ready(&self) -> bool {
        match &self.terminal {
            TerminalSource::Thread(handle) => handle.is_finished(),
            TerminalSource::Persistent { done, .. } => {
                done.load(std::sync::atomic::Ordering::SeqCst)
            }
        }
    }

    /// Consume the terminal result, joining the thread or receiving the
    /// channel. Falls back to a typed failure if the source is gone.
    fn finish(self) -> jefe::runtime::provider::supervisor::OneShotResult {
        use jefe::runtime::provider::supervisor::{OneShotResult, SupervisorFailure};
        match self.terminal {
            TerminalSource::Thread(handle) => match handle.join() {
                Ok(result) => result,
                Err(_) => OneShotResult::without_process(SupervisorFailure::Io(
                    "session thread panicked".to_owned(),
                )),
            },
            TerminalSource::Persistent { result_rx, .. } => match result_rx.recv() {
                Ok(result) => result,
                Err(_) => OneShotResult::without_process(SupervisorFailure::Io(
                    "persistent session channel closed".to_owned(),
                )),
            },
        }
    }
}

/// The outcome of trying to start one queued session.
enum SessionStart<Item, Session, Failure> {
    /// The session started; the thread is running.
    Started(Session),
    /// The context lock was momentarily unavailable. The item must be retried.
    Deferred(Item),
    /// The session could not start. The failure is delivered without consuming
    /// a concurrency slot.
    Failed(Failure),
}

type ProviderSessionStart = SessionStart<
    jefe::services::provider_effect_worker::ProviderWorkItem,
    ActiveSession,
    jefe::services::provider_effect_worker::ProviderExecutionResult,
>;

/// Start a provider session (S14 one-shot or persistent, S15 timeout, S16
/// streaming, S17 cancel).
///
/// Dispatches by descriptor mode through [`resolve_one_shot`]. One-shot spawns
/// a fresh process; persistent invokes on the already-Ready candidate.
fn start_provider_session(
    ctx_arc: &Arc<std::sync::Mutex<AppContext>>,
    item: jefe::services::provider_effect_worker::ProviderWorkItem,
) -> ProviderSessionStart {
    match resolve_one_shot(ctx_arc, &item) {
        ResolveOutcome::Deferred => SessionStart::Deferred(item),
        ResolveOutcome::Failed { reason } => {
            SessionStart::Failed(failed_execution_result(&item, reason))
        }
        ResolveOutcome::OneShot {
            request,
            timeout_seconds,
        } => spawn_session(item, request, timeout_seconds),
        ResolveOutcome::Persistent(invocation) => SessionStart::Started(ActiveSession {
            key: item.invocation.key.clone(),
            correlation: item.correlation.clone(),
            progress_rx: invocation.progress_rx,
            cancel: invocation.cancel,
            terminal: TerminalSource::Persistent {
                done: invocation.done,
                result_rx: invocation.result_rx,
            },
        }),
    }
}

/// The resolution of a work item into a dispatchable session.
enum ResolveOutcome {
    /// The context lock was momentarily unavailable; the item must be retried.
    Deferred,
    /// The session cannot start (no coordinator, bad descriptor). The failure
    /// is delivered directly without spawning a process.
    Failed {
        reason: jefe::state::provider_requests::UnavailableReason,
    },
    /// One-shot: a fresh `OneShotRequest` ready to spawn.
    OneShot {
        request: Box<jefe::runtime::provider::supervisor::OneShotRequest>,
        timeout_seconds: u32,
    },
    /// Persistent: an invocation handle from the already-Ready candidate.
    Persistent(jefe::runtime::provider::PersistentInvocation),
}

/// Look up the descriptor and dispatch by mode: one-shot builds a fresh
/// `OneShotRequest`; persistent invokes on the already-Ready candidate
/// (S14/S15).
fn resolve_one_shot(
    ctx_arc: &Arc<std::sync::Mutex<AppContext>>,
    item: &jefe::services::provider_effect_worker::ProviderWorkItem,
) -> ResolveOutcome {
    use jefe::domain::plugin::provider::ProviderMode;
    use jefe::state::provider_requests::UnavailableReason;

    let Ok(ctx_guard) = ctx_arc.try_lock() else {
        return ResolveOutcome::Deferred;
    };
    let Some(coordinator) = ctx_guard.provider_coordinator.as_ref() else {
        return ResolveOutcome::Failed {
            reason: UnavailableReason::Eof,
        };
    };
    let Ok(action_id) =
        jefe::domain::action_registry::ActionId::parse(item.invocation.key.action_id.as_str())
    else {
        return ResolveOutcome::Failed {
            reason: UnavailableReason::Protocol,
        };
    };
    let Some(descriptor) = ctx_guard
        .workbench
        .provider_catalog()
        .get(&action_id)
        .cloned()
    else {
        return ResolveOutcome::Failed {
            reason: UnavailableReason::Protocol,
        };
    };
    let timeout = descriptor.timeout_seconds;
    match descriptor.mode {
        ProviderMode::OneShot => match coordinator.build_one_shot(&descriptor, &item.invocation) {
            Ok(request) => ResolveOutcome::OneShot {
                request: Box::new(request),
                timeout_seconds: timeout,
            },
            Err(error) => {
                tracing::warn!(?error, "provider worker could not build one-shot request");
                ResolveOutcome::Failed {
                    reason: UnavailableReason::Protocol,
                }
            }
        },
        ProviderMode::Persistent => {
            match coordinator.invoke_persistent(&descriptor, &item.invocation) {
                Ok(invocation) => ResolveOutcome::Persistent(invocation),
                Err(_) => ResolveOutcome::Failed {
                    reason: UnavailableReason::Eof,
                },
            }
        }
        ProviderMode::None => ResolveOutcome::Failed {
            reason: UnavailableReason::Protocol,
        },
    }
}

/// Spawn the session thread running [`run_one_shot_streaming`] for a resolved
/// request, returning the live session handle.
fn spawn_session(
    item: jefe::services::provider_effect_worker::ProviderWorkItem,
    request: Box<jefe::runtime::provider::supervisor::OneShotRequest>,
    timeout_seconds: u32,
) -> ProviderSessionStart {
    use jefe::runtime::provider::environment::ProcessHostEnv;
    use jefe::runtime::provider::supervisor::{SupervisorBounds, run_one_shot_streaming};
    use jefe::state::provider_requests::UnavailableReason;

    let key = item.invocation.key.clone();
    let correlation = item.correlation.clone();
    let (progress_tx, progress_rx) = std::sync::mpsc::channel();
    let cancel = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
    let cancel_for_thread = cancel.clone();
    let bounds = SupervisorBounds::for_invocation(timeout_seconds);
    match std::thread::Builder::new()
        .name("jefe-provider-session".to_owned())
        .spawn(move || {
            run_one_shot_streaming(
                &request,
                &bounds,
                &ProcessHostEnv,
                Some(&progress_tx),
                Some(&cancel_for_thread),
            )
        }) {
        Ok(thread) => SessionStart::Started(ActiveSession {
            key,
            correlation,
            progress_rx,
            cancel,
            terminal: TerminalSource::Thread(thread),
        }),
        Err(error) => {
            tracing::error!("provider session thread spawn failed: {error}");
            SessionStart::Failed(failed_execution_result(&item, UnavailableReason::Eof))
        }
    }
}

/// Build a failed execution result for one work item that could not start.
fn failed_execution_result(
    item: &jefe::services::provider_effect_worker::ProviderWorkItem,
    reason: jefe::state::provider_requests::UnavailableReason,
) -> jefe::services::provider_effect_worker::ProviderExecutionResult {
    jefe::services::provider_effect_worker::ProviderExecutionResult {
        correlation: item.correlation.clone(),
        key: item.invocation.key.clone(),
        messages: vec![jefe::messages::ProviderMessage::GenerationFailed {
            key: item.invocation.key.clone(),
            reason,
        }],
        process_reaped: false,
        terminal: true,
        cleanup_diagnostic: None,
    }
}

/// Deliver typed provider messages and effect completion through the reducer.
///
/// Each message is committed through the post-commit funnel. Any closed host
/// outcome staged by the reducer executes only after the state guard is released.
/// Stale-generation protection lives in both the pending-correlation gate and
/// the request authority checked by the host-outcome adapter.
///
/// A post-terminal cleanup/lifecycle fault (S26) is surfaced as a separate
/// diagnostic and never replaces the first terminal message: the terminal
/// outcome/error/generation-failed is already in `messages` and committed
/// before the diagnostic is logged.
fn deliver_provider_messages(
    app_state: &mut crate::app_input::AppStateHandle,
    ctx: &Arc<std::sync::Mutex<AppContext>>,
    result: jefe::services::provider_effect_worker::ProviderExecutionResult,
) {
    let correlation = result.correlation.clone();
    let correlation_live = app_state.read().pending_effects.is_pending(&correlation);
    if correlation_live {
        for message in result.messages {
            let effects = {
                let mut state = app_state.write();
                jefe::state::transition::commit_in_place(
                    &mut state,
                    jefe::messages::AppMessage::Provider(Box::new(message)),
                )
            };
            let shared_ctx = Some(Arc::clone(ctx));
            crate::app_input::schedule_provider_effects(app_state, &shared_ctx, effects);
        }
    }
    // Surface a post-terminal cleanup fault as a diagnostic without replacing
    // the terminal result already committed above.
    if let Some(diagnostic) = &result.cleanup_diagnostic {
        tracing::warn!(
            action = %result.key.action_id.as_str(),
            code = %diagnostic.code(),
            "provider cleanup fault after terminal result: {:?}",
            diagnostic
        );
    }
    // Deliver the effect completion so correlation tracking closes cleanly.
    // The provider effect family has no response payload — the typed messages
    // already carry the outcome — but the effect ledger must still be closed.
    let mut state = app_state.write();
    let completion = jefe::domain::effects::EffectCompletion {
        correlation,
        result: Ok(jefe::domain::effects::EffectResponse::Provider(
            jefe::domain::effects::ProviderResponse::Invoked {
                key: result.key.clone(),
            },
        )),
    };
    jefe::state::transition::commit_pure_site(
        &mut state,
        jefe::messages::AppMessage::EffectCompletion(Box::new(completion)),
    );
}

/// Synchronously shut down the provider coordinator before host exit.
///
/// Called from the shutdown path so every persistent provider candidate is
/// reaped and no process tree leaks. Best-effort: if the mutex is poisoned,
/// the coordinator is leaked (consistent with the persist/capture shutdown
/// convention).
pub fn shutdown_provider_coordinator(ctx: Option<&Arc<std::sync::Mutex<AppContext>>>) {
    let Some(ctx_arc) = ctx else {
        return;
    };
    let Ok(mut ctx_guard) = ctx_arc.lock() else {
        warn!("shutdown_provider_coordinator: ctx mutex poisoned; skipping provider shutdown");
        return;
    };
    if let Some(coordinator) = ctx_guard.provider_coordinator.as_mut() {
        coordinator.shutdown();
    }
}

#[cfg(test)]
#[path = "app_shell_workers_provider_tests.rs"]
mod provider_worker_concurrency_tests;

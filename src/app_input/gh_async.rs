//! Background-task dispatch for blocking GitHub CLI calls.
//!
//! Issues-mode dispatch runs inside terminal event handling. GitHub CLI calls
//! spawn subprocesses and may perform network I/O, so this helper moves those
//! calls off the UI path and applies result events back through iocraft state.

use std::sync::{Arc, Mutex};

use iocraft::Handler;

use super::{AppStateHandle, SharedContext, issues_list_dispatch::IssueListDelivery};

/// A state-touching continuation queued for the root component.
///
/// iocraft `State` must only be borrowed on the render thread, so background
/// work returns one of these instead of writing state itself (issue #437).
type GhContinuation = Box<dyn FnOnce(&mut AppStateHandle, &SharedContext) + Send>;

/// Typed GitHub-task result delivered to the root component.
pub enum BackgroundGhDelivery {
    /// Completion of an issue-list request.
    IssueList(Box<IssueListDelivery>),
    /// A completed background request whose result is applied by the closure.
    Apply(GhContinuation),
    #[cfg(test)]
    Probe(String),
}

/// Shared slot containing the root component's lifecycle-bound delivery handler.
///
/// iocraft owns and polls the handler's queued futures only while the root
/// component is mounted. A retained clone may enqueue after teardown, but the
/// dropped hook no longer polls that queue; reinstalling on each root render
/// replaces the slot with the current lifecycle owner.
#[derive(Clone, Default)]
pub struct GhDeliveryHandle {
    handler: Arc<Mutex<Option<Handler<'static, BackgroundGhDelivery>>>>,
}

impl GhDeliveryHandle {
    pub(crate) fn install(&self, handler: Handler<'static, BackgroundGhDelivery>) {
        *lock_recover(&self.handler) = Some(handler);
    }

    fn deliver(&self, delivery: BackgroundGhDelivery) {
        if let Some(handler) = lock_recover(&self.handler).as_mut() {
            handler(delivery);
        } else {
            tracing::debug!("discarding background gh delivery without a root handler");
        }
    }
}

fn lock_recover<T>(mutex: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    match mutex.lock() {
        Ok(guard) => guard,
        Err(poisoned) => {
            tracing::warn!("recovering poisoned background gh delivery mutex");
            poisoned.into_inner()
        }
    }
}

pub fn install_gh_delivery_handler(
    ctx: &SharedContext,
    handler: Handler<'static, BackgroundGhDelivery>,
) {
    let Some(ctx) = ctx else {
        tracing::warn!("cannot install background gh delivery handler without app context");
        return;
    };
    let context = match ctx.lock() {
        Ok(context) => context,
        Err(poisoned) => {
            tracing::warn!("recovering poisoned app context while installing gh delivery handler");
            poisoned.into_inner()
        }
    };
    context.gh_deliveries.install(handler);
}

pub(super) fn gh_delivery_handle(ctx: &SharedContext) -> Option<GhDeliveryHandle> {
    let ctx = ctx.as_ref()?;
    let context = match ctx.lock() {
        Ok(context) => context,
        Err(poisoned) => {
            tracing::warn!("recovering poisoned app context while retrieving gh delivery handle");
            poisoned.into_inner()
        }
    };
    Some(context.gh_deliveries.clone())
}

pub(super) fn spawn_gh_request_with_panic<F, R, S, P>(
    deliveries: &GhDeliveryHandle,
    ctx: &SharedContext,
    work: F,
    on_success: S,
    on_panic: P,
) where
    F: FnOnce(SharedContext) -> R + Send + 'static,
    R: Send + 'static,
    S: FnOnce(R) -> BackgroundGhDelivery + Send + 'static,
    P: FnOnce(String) -> BackgroundGhDelivery + Send + 'static,
{
    let deliveries = deliveries.clone();
    let ctx = ctx
        .as_ref()
        .map(|arc| Arc::clone(arc) as Arc<std::sync::Mutex<crate::AppContext>>);
    smol::spawn(async move {
        smol::unblock(move || {
            let delivery = match super::worker_panic::contain(move || work(ctx)) {
                Ok(result) => on_success(result),
                Err(message) => {
                    tracing::error!(error = %message, "background gh request panicked");
                    on_panic(message)
                }
            };
            deliveries.deliver(delivery);
        })
        .await;
    })
    .detach();
}

/// Run blocking GitHub work off the UI thread and apply its outcome on the
/// render thread.
///
/// `work` is handed only the shared context and must not reach iocraft state:
/// borrowing one `State` from both a blocking worker and the render thread
/// races inside `generational-box`'s borrow tracking and panics (issue #437).
/// `apply` and `on_panic` run on the root component's executor through the
/// lifecycle-owned delivery queue, and are dropped without running when the
/// root is gone. They must not panic: they run outside the worker containment
/// boundary, so a panic there is a genuine reducer/state bug and is reported
/// through the normal hook rather than being swallowed.
pub(super) fn spawn_gh_work<F, R, A, P>(
    deliveries: &GhDeliveryHandle,
    ctx: &SharedContext,
    work: F,
    apply: A,
    on_panic: P,
) where
    F: FnOnce(&SharedContext) -> R + Send + 'static,
    R: Send + 'static,
    A: FnOnce(&mut AppStateHandle, &SharedContext, R) + Send + 'static,
    P: FnOnce(&mut AppStateHandle, &SharedContext, String) + Send + 'static,
{
    spawn_gh_request_with_panic(
        deliveries,
        ctx,
        move |ctx| work(&ctx),
        move |result| {
            BackgroundGhDelivery::Apply(Box::new(move |app_state, ctx| {
                apply(app_state, ctx, result);
            }))
        },
        move |message| {
            BackgroundGhDelivery::Apply(Box::new(move |app_state, ctx| {
                record_worker_panic(app_state, &message);
                on_panic(app_state, ctx, message);
            }))
        },
    );
}

/// Record a contained worker panic on the errors screen.
///
/// This runs for every panicking request, including routes that intentionally
/// fail silently, so the report is always retrievable even though nothing
/// interrupts the active screen (issue #437).
fn record_worker_panic(app_state: &mut AppStateHandle, message: &str) {
    let mut state = app_state.write();
    jefe::state::capture_worker_panic(&mut state, message);
}

/// Resolve the root delivery queue, reporting the failure when it is absent.
///
/// A missing queue means the request cannot be applied, so callers surface
/// their own typed failure rather than spawning work whose result is dropped.
pub(super) fn delivery_handle_or_report(
    app_state: &mut AppStateHandle,
    ctx: &SharedContext,
    report: impl FnOnce(&mut AppStateHandle, &SharedContext, String),
) -> Option<GhDeliveryHandle> {
    let deliveries = gh_delivery_handle(ctx);
    if deliveries.is_none() {
        tracing::warn!("dropping a GitHub request: no root delivery queue is installed");
        report(
            app_state,
            ctx,
            "Application delivery context unavailable".to_string(),
        );
    }
    deliveries
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app_input::apply_background_gh_delivery;
    use core::time::Duration;
    use iocraft::prelude::*;
    use jefe::state::{AppState, ScreenMode};
    use smol::stream::StreamExt;
    use std::sync::mpsc;

    #[derive(Default, Props)]
    struct ProbeProps {
        deliveries: Option<GhDeliveryHandle>,
        notify: Option<mpsc::Sender<String>>,
    }

    #[component]
    fn PanicProbe(mut hooks: Hooks, props: &ProbeProps) -> impl Into<AnyElement<'static>> {
        let state = hooks.use_state(|| {
            let mut state = AppState::default();
            state.issues_state.loading.detail = true;
            state
        });
        let notify = props.notify.clone();
        let mut started = hooks.use_state(|| false);

        let mut handler = hooks.use_async_handler(move |delivery| async move {
            let mut app_state = state;
            apply_background_gh_delivery(&mut app_state, &None, delivery);
        });
        if let Some(deliveries) = &props.deliveries {
            deliveries.install(handler.take());
        }

        if !started.get() {
            started.set(true);
            let deliveries = props.deliveries.clone();
            hooks.use_future(async move {
                let Some(deliveries) = deliveries else {
                    return;
                };
                spawn_gh_work(
                    &deliveries,
                    &None,
                    |_ctx| panic!("boom"),
                    |_app_state, _ctx, ()| {},
                    |app_state, _ctx, message| {
                        let mut guard = app_state.write();
                        guard.issues_state.loading.detail = false;
                        guard.issues_state.error = Some(format!("panic handled: {message}"));
                    },
                );
            });
        }

        let snapshot = state.read();
        if !snapshot.issues_state.loading.detail {
            let message = snapshot.issues_state.error.clone().unwrap_or_default();
            drop(snapshot);
            if let Some(sender) = notify {
                let _ = sender.send(message);
            }
            hooks.use_context_mut::<SystemContext>().exit();
        }

        element! { Text(content: String::from("panic-probe")) }
    }

    /// A panicking worker clears its loading flag and surfaces a copyable
    /// message rather than leaving the request stuck in-flight (issue #437).
    #[test]
    fn panic_handler_can_surface_visible_error_and_clear_loading() {
        let deliveries = GhDeliveryHandle::default();
        let (sender, receiver) = mpsc::channel();

        smol::block_on(async move {
            let mut app = element!(PanicProbe(
                deliveries: Some(deliveries),
                notify: Some(sender),
            ));
            let result = smol::future::or(
                async move {
                    let _: Vec<_> = app
                        .mock_terminal_render_loop(MockTerminalConfig::default())
                        .collect()
                        .await;
                    receiver.recv().ok()
                },
                async {
                    smol::Timer::after(Duration::from_secs(10)).await;
                    None
                },
            )
            .await;
            let message = result.unwrap_or_default();
            assert!(
                message.starts_with("panic handled: boom (at "),
                "panic message and location must reach state: {message}"
            );
        });
    }

    #[derive(Default, Props)]
    struct ThreadAffinityProbeProps {
        deliveries: Option<GhDeliveryHandle>,
        observed: Option<mpsc::Sender<(std::thread::ThreadId, std::thread::ThreadId)>>,
    }

    /// Records the thread that renders the component and the thread that
    /// applies a background GitHub result to iocraft state.
    ///
    /// iocraft `State` is backed by one `generational-box` slot guarded by a
    /// borrow-tracking lock. Reading it from a blocking worker while the
    /// render thread holds a borrow is a data race that panics inside the
    /// library's borrow diagnostics (issue #437), so every GitHub result must
    /// be applied on the render thread.
    #[component]
    fn ThreadAffinityProbe(
        mut hooks: Hooks,
        props: &ThreadAffinityProbeProps,
    ) -> impl Into<AnyElement<'static>> {
        let state = hooks.use_state(AppState::default);
        let mut started = hooks.use_state(|| false);
        let mut reported = hooks.use_state(|| false);
        let render_thread = std::thread::current().id();

        let mut handler = hooks.use_async_handler(move |delivery| async move {
            let mut app_state = state;
            apply_background_gh_delivery(&mut app_state, &None, delivery);
        });
        if let Some(deliveries) = &props.deliveries {
            deliveries.install(handler.take());
        }

        if !started.get() {
            started.set(true);
            let deliveries = props.deliveries.clone();
            let observed = props.observed.clone();
            hooks.use_future(async move {
                let Some(deliveries) = deliveries else {
                    return;
                };
                spawn_gh_work(
                    &deliveries,
                    &None,
                    |_ctx| String::from("worker result"),
                    move |app_state, _ctx, _result| {
                        if let Some(sender) = observed {
                            let _ = sender.send((render_thread, std::thread::current().id()));
                        }
                        app_state.write().issues_state.error = Some("applied".to_string());
                    },
                    |_app_state, _ctx, _message| {},
                );
            });
        }

        if state.read().issues_state.error.is_some() && !reported.get() {
            reported.set(true);
            hooks.use_context_mut::<SystemContext>().exit();
        }

        element!(Box)
    }

    /// A GitHub result must be applied on the render thread, never on a
    /// `smol::unblock` worker (issue #437).
    #[test]
    fn background_gh_result_is_applied_on_the_render_thread() {
        let deliveries = GhDeliveryHandle::default();
        let (observed_tx, observed_rx) = mpsc::channel();

        smol::block_on(async {
            let mut app = element!(ThreadAffinityProbe(
                deliveries: Some(deliveries),
                observed: Some(observed_tx),
            ));
            let _: Vec<_> = smol::future::or(
                async {
                    app.mock_terminal_render_loop(MockTerminalConfig::default())
                        .collect()
                        .await
                },
                async {
                    smol::Timer::after(Duration::from_secs(10)).await;
                    Vec::new()
                },
            )
            .await;
        });

        let (render_thread, apply_thread) = observed_rx
            .recv_timeout(Duration::from_secs(5))
            .unwrap_or_else(|_| panic!("the background result must be applied"));
        assert_eq!(
            render_thread, apply_thread,
            "GitHub results must be applied on the render thread, not a blocking worker"
        );
    }

    #[derive(Default, Props)]
    struct SilentPanicProbeProps {
        deliveries: Option<GhDeliveryHandle>,
        observed: Option<mpsc::Sender<(usize, String, String, ScreenMode)>>,
    }

    /// Drives a worker that panics on a route which fails silently, then
    /// reports what the errors screen retained and which screen stayed active.
    #[component]
    fn SilentPanicProbe(
        mut hooks: Hooks,
        props: &SilentPanicProbeProps,
    ) -> impl Into<AnyElement<'static>> {
        let state = hooks.use_state(|| AppState {
            screen_mode: ScreenMode::DashboardIssues,
            ..AppState::default()
        });
        let mut started = hooks.use_state(|| false);
        let mut reported = hooks.use_state(|| false);

        let mut handler = hooks.use_async_handler(move |delivery| async move {
            let mut app_state = state;
            apply_background_gh_delivery(&mut app_state, &None, delivery);
        });
        if let Some(deliveries) = &props.deliveries {
            deliveries.install(handler.take());
        }

        if !started.get() {
            started.set(true);
            let deliveries = props.deliveries.clone();
            hooks.use_future(async move {
                let Some(deliveries) = deliveries else {
                    return;
                };
                spawn_gh_work(
                    &deliveries,
                    &None,
                    |_ctx| panic!("silent route exploded"),
                    |_app_state, _ctx, ()| {},
                    // A silent route deliberately surfaces nothing to the user.
                    |_app_state, _ctx, _message| {},
                );
            });
        }

        let snapshot = state.read();
        if !snapshot.errors_state.is_empty() && !reported.get() {
            let entry = snapshot.errors_state.last_error().map(|entry| {
                (
                    snapshot.errors_state.count(),
                    entry.title.clone(),
                    entry.detail.clone(),
                    snapshot.screen_mode,
                )
            });
            drop(snapshot);
            reported.set(true);
            if let (Some(sender), Some(entry)) = (props.observed.clone(), entry) {
                let _ = sender.send(entry);
            }
            hooks.use_context_mut::<SystemContext>().exit();
        }

        element!(Box)
    }

    /// A panic on a silent route is still retained on the errors screen, and
    /// does not navigate away from the active screen (issue #437).
    #[test]
    fn silent_route_panic_is_recorded_without_leaving_the_active_screen() {
        let deliveries = GhDeliveryHandle::default();
        let (observed_tx, observed_rx) = mpsc::channel();

        smol::block_on(async {
            let mut app = element!(SilentPanicProbe(
                deliveries: Some(deliveries),
                observed: Some(observed_tx),
            ));
            let _: Vec<_> = smol::future::or(
                async {
                    app.mock_terminal_render_loop(MockTerminalConfig::default())
                        .collect()
                        .await
                },
                async {
                    smol::Timer::after(Duration::from_secs(10)).await;
                    Vec::new()
                },
            )
            .await;
        });

        let (count, title, detail, screen_mode) = observed_rx
            .recv_timeout(Duration::from_secs(5))
            .unwrap_or_else(|_| panic!("a silent route panic must reach the errors screen"));
        assert_eq!(count, 1, "exactly one error entry must be retained");
        assert_eq!(title, "Background task panicked");
        assert!(
            detail.starts_with("silent route exploded (at "),
            "the copyable detail must carry the payload and location: {detail}"
        );
        assert_eq!(
            screen_mode,
            ScreenMode::DashboardIssues,
            "recording an error must not navigate away from the active screen"
        );
    }

    #[derive(Default, Props)]
    struct DroppedDeliveryProbeProps {
        deliveries: Option<GhDeliveryHandle>,
        notify: Option<mpsc::Sender<String>>,
    }

    #[component]
    fn DroppedDeliveryProbe(
        mut hooks: Hooks,
        props: &DroppedDeliveryProbeProps,
    ) -> impl Into<AnyElement<'static>> {
        let state = hooks.use_state(AppState::default);
        let notify = props.notify.clone();
        let mut handler = hooks.use_async_handler(move |delivery| {
            let state = state;
            let notify = notify.clone();
            async move {
                let _snapshot = state.read();
                if let BackgroundGhDelivery::Probe(message) = delivery
                    && let Some(sender) = notify
                {
                    let _ = sender.send(message);
                }
            }
        });
        if let Some(deliveries) = &props.deliveries {
            deliveries.install(handler.take());
        }
        element!(Box)
    }

    #[derive(Default, Props)]
    struct LateDeliveryTriggerProps {
        deliveries: Option<GhDeliveryHandle>,
        worker_notify: Option<mpsc::Sender<()>>,
    }

    #[component]
    fn LateDeliveryTrigger(
        mut hooks: Hooks,
        props: &LateDeliveryTriggerProps,
    ) -> impl Into<AnyElement<'static>> {
        let deliveries = props.deliveries.clone();
        let worker_notify = props.worker_notify.clone();
        let mut finished = hooks.use_state(|| false);
        hooks.use_future(async move {
            if let Some(deliveries) = deliveries {
                spawn_gh_request_with_panic(
                    &deliveries,
                    &None,
                    |_ctx| String::from("late result"),
                    move |message| {
                        if let Some(sender) = worker_notify {
                            let _ = sender.send(());
                        }
                        BackgroundGhDelivery::Probe(message)
                    },
                    BackgroundGhDelivery::Probe,
                );
                smol::Timer::after(Duration::from_millis(100)).await;
            }
            finished.set(true);
        });
        if finished.get() {
            hooks.use_context_mut::<SystemContext>().exit();
        }
        element!(Box)
    }

    #[derive(Default, Props)]
    struct DeliveryLifecycleProps {
        deliveries: Option<GhDeliveryHandle>,
        applied_notify: Option<mpsc::Sender<String>>,
        worker_notify: Option<mpsc::Sender<()>>,
    }

    #[component]
    fn DeliveryLifecycle(
        mut hooks: Hooks,
        props: &DeliveryLifecycleProps,
    ) -> impl Into<AnyElement<'static>> {
        let mut show_owner = hooks.use_state(|| true);
        hooks.use_future(async move {
            smol::Timer::after(Duration::from_millis(10)).await;
            show_owner.set(false);
        });
        let child = if show_owner.get() {
            element!(DroppedDeliveryProbe(
                deliveries: props.deliveries.clone(),
                notify: props.applied_notify.clone(),
            ))
            .into_any()
        } else {
            element!(LateDeliveryTrigger(
                deliveries: props.deliveries.clone(),
                worker_notify: props.worker_notify.clone(),
            ))
            .into_any()
        };
        element!(Box { #(vec![child]) })
    }

    #[test]
    fn late_request_result_is_not_applied_after_component_drop() {
        let deliveries = GhDeliveryHandle::default();
        let (applied_tx, applied_rx) = mpsc::channel();
        let (worker_tx, worker_rx) = mpsc::channel();

        smol::block_on(async {
            let mut app = element!(DeliveryLifecycle(
                deliveries: Some(deliveries),
                applied_notify: Some(applied_tx),
                worker_notify: Some(worker_tx),
            ));
            let _: Vec<_> = app
                .mock_terminal_render_loop(MockTerminalConfig::default())
                .collect()
                .await;
        });

        assert!(worker_rx.recv_timeout(Duration::from_secs(2)).is_ok());
        assert!(applied_rx.recv_timeout(Duration::from_millis(100)).is_err());
    }
}

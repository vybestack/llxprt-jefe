//! Background-task dispatch for blocking GitHub CLI calls.
//!
//! Issues-mode dispatch runs inside terminal event handling. GitHub CLI calls
//! spawn subprocesses and may perform network I/O, so this helper moves those
//! calls off the UI path and applies result events back through iocraft state.

use std::sync::{Arc, Mutex};

use iocraft::Handler;

use super::{AppStateHandle, SharedContext, issues_list_dispatch::IssueListDelivery};

/// Typed GitHub-task result delivered to the root component.
pub enum BackgroundGhDelivery {
    /// Completion of an issue-list request.
    IssueList(Box<IssueListDelivery>),
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
            let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| work(ctx)));
            let delivery = match result {
                Ok(result) => on_success(result),
                Err(payload) => {
                    let message = panic_message(&payload);
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

fn panic_message(payload: &Box<dyn std::any::Any + Send>) -> String {
    payload
        .downcast_ref::<String>()
        .map(String::as_str)
        .or_else(|| payload.downcast_ref::<&'static str>().copied())
        .unwrap_or("unknown panic")
        .to_string()
}

pub fn spawn_gh_task_with_panic<F, P>(
    app_state: &AppStateHandle,
    ctx: &SharedContext,
    work: F,
    on_panic: P,
) where
    F: FnOnce(AppStateHandle, SharedContext) + Send + 'static,
    P: FnOnce(AppStateHandle, SharedContext, String) + Send + 'static,
{
    let app_state = *app_state;
    let ctx = ctx
        .as_ref()
        .map(|arc| Arc::clone(arc) as Arc<std::sync::Mutex<crate::AppContext>>);
    smol::spawn(async move {
        smol::unblock(move || {
            let work_ctx = ctx.clone();
            let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                work(app_state, work_ctx);
            }));
            if let Err(panic_payload) = result {
                let message = panic_message(&panic_payload);
                tracing::error!(error = %message, "background gh task panicked");
                on_panic(app_state, ctx, message);
            }
        })
        .await;
    })
    .detach();
}

#[cfg(test)]
mod tests {
    use super::*;
    use core::time::Duration;
    use iocraft::prelude::*;
    use jefe::state::AppState;
    use smol::stream::StreamExt;
    use std::sync::mpsc;

    #[derive(Default, Props)]
    struct ProbeProps {
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

        hooks.use_future(async move {
            spawn_gh_task_with_panic(
                &state,
                &None,
                |_state, _ctx| panic!("boom"),
                |mut state, _ctx, message| {
                    let mut guard = state.write();
                    guard.issues_state.loading.detail = false;
                    guard.issues_state.error = Some(format!("panic handled: {message}"));
                },
            );
        });

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

    #[test]
    fn panic_handler_can_surface_visible_error_and_clear_loading() {
        let (sender, receiver) = mpsc::channel();

        smol::block_on(async move {
            let mut app = element!(PanicProbe(notify: Some(sender)));
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
            assert_eq!(result.as_deref(), Some("panic handled: boom"));
        });
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

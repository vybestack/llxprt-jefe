//! Background-task dispatch for blocking GitHub CLI calls.
//!
//! Issues-mode dispatch runs inside terminal event handling. GitHub CLI calls
//! spawn subprocesses and may perform network I/O, so this helper moves those
//! calls off the UI path and applies result events back through iocraft state.

use std::sync::Arc;

use super::{AppStateHandle, SharedContext};

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
                let message = panic_payload
                    .downcast_ref::<String>()
                    .map(String::as_str)
                    .or_else(|| panic_payload.downcast_ref::<&'static str>().copied())
                    .unwrap_or("unknown panic")
                    .to_string();
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
            state.issues_state.loading.list = true;
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
                    guard.issues_state.loading.list = false;
                    guard.issues_state.error = Some(format!("panic handled: {message}"));
                },
            );
        });

        let snapshot = state.read();
        if !snapshot.issues_state.loading.list {
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
                    smol::Timer::after(Duration::from_secs(2)).await;
                    None
                },
            )
            .await;
            assert_eq!(result.as_deref(), Some("panic handled: boom"));
        });
    }
}

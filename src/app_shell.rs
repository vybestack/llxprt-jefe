use iocraft::prelude::*;
use tracing::{debug, trace, warn};

use crate::AppContext;
use crate::app_input::{
    apply_background_gh_delivery, dispatch_app_event, install_gh_delivery_handler,
    request_pr_background_refresh, resolve_raw_key_mutation, synchronize_actions_geometry,
    try_suppress_synthetic_enter, update_paste_enter_suppression,
};
use crate::app_shell_key_routing::route_registry_key;
use crate::pty_encoding::PasteEnterSuppression;

use jefe::domain::{AgentId, AgentStatus};
use jefe::input::{InputMode, input_mode_for_state};
use jefe::jsp_host::JspHostRuntime;
use jefe::layout::{compute_pty_layout, effective_render_size};
use jefe::messages::AppMessage;
use jefe::runtime::{
    AttachAction, AttachScheduler, DEFAULT_DEBOUNCE, RuntimeManager, TerminalSnapshot,
};
use jefe::state::{AppEvent, AppState, ModalState, PaneFocus, ScreenId};
use jefe::theme::{ThemeColors, ThemeManager};
use jefe::ui::orchestration::{
    ModalViewport, TerminalRenderData, build_modal_element, build_screen_element,
    derive_confirm_modal_data,
};

use crate::app_input::{durable_save_request, schedule_durable_save};
use std::sync::Arc;
use std::time::Instant;
fn drain_jsp_messages(
    app_state: &mut crate::app_input::AppStateHandle,
    ctx: &crate::app_input::SharedContext,
) -> bool {
    let Some(ctx_arc) = ctx else {
        return false;
    };
    let messages = match ctx_arc.try_lock() {
        Ok(context) => match context
            .jsp_host
            .as_ref()
            .map(JspHostRuntime::drain_messages)
        {
            Some(Ok(messages)) => messages,
            Some(Err(error)) => {
                warn!(error = %error, "JSP observation delivery poisoned; draining aborted");
                return false;
            }
            None => Vec::new(),
        },
        Err(_) => return false,
    };
    if messages.is_empty() {
        return false;
    }
    let mut state = app_state.write();
    for message in messages {
        jefe::state::transition::commit_pure_site(&mut state, AppMessage::Runtime(message));
    }
    true
}

#[derive(Default, Props)]
pub struct AppProps {
    pub context: Option<Arc<std::sync::Mutex<AppContext>>>,
}

#[component]
pub fn App(mut hooks: Hooks, props: &AppProps) -> impl Into<AnyElement<'static>> {
    let should_quit = hooks.use_state(|| false);
    let mut app_state = hooks.use_state(AppState::default);
    let render_tick = hooks.use_state(|| 0u64);
    let mut initialized = hooks.use_state(|| false);
    let mut startup_sessions_restored = hooks.use_state(|| false);
    let mut attach_scheduler = hooks.use_state(|| AttachScheduler::new(DEFAULT_DEBOUNCE));
    let mut suppress_next_enter = hooks.use_state(PasteEnterSuppression::new);
    let mut mouse_click = hooks.use_state(crate::mouse_routing::MouseClickState::default);
    let last_activity = hooks.use_state(Instant::now);

    let ctx = props.context.clone();

    let startup_probe_effects = if initialized.get() {
        Vec::new()
    } else {
        initialized.set(true);
        crate::app_init::init_app_state(&mut app_state, &ctx)
    };

    hooks.use_future({
        let app_state = app_state;
        async move {
            crate::app_shell_workers::run_agent_availability_probes(
                startup_probe_effects,
                app_state,
            )
            .await;
        }
    });

    let mut gh_delivery_handler = hooks.use_async_handler({
        let app_state = app_state;
        let ctx = ctx.clone();
        move |delivery| {
            let mut app_state = app_state;
            let ctx = ctx.clone();
            async move {
                apply_background_gh_delivery(&mut app_state, &ctx, delivery);
            }
        }
    });
    install_gh_delivery_handler(&ctx, gh_delivery_handler.take());

    // Restore runtime session map from persisted agent statuses exactly once.
    if !startup_sessions_restored.get() {
        startup_sessions_restored.set(true);
        crate::app_init::restore_runtime_sessions(&mut app_state, &ctx);
    }

    hooks.use_future({
        let ctx = ctx.clone();
        let mut app_state = app_state;
        let mut render_tick = render_tick;
        async move {
            const POLL_MS: u64 = 16;
            const SAFETY_NET_MS: u64 = 1000;
            const PREVIEW_THROTTLE_MS: u64 = 250;
            let mut elapsed_ms: u64 = 0;
            loop {
                smol::Timer::after(std::time::Duration::from_millis(POLL_MS)).await;
                elapsed_ms = elapsed_ms.saturating_add(POLL_MS);

                let terminal_focused = {
                    let state = app_state.read();
                    state.pane_focus == PaneFocus::Terminal
                };
                let running_preview = !terminal_focused && {
                    let state = app_state.read();
                    state
                        .selected_agent()
                        .is_some_and(|agent| agent.status == AgentStatus::Running)
                };

                let dirty = crate::app_shell_workers::is_pty_dirty(ctx.as_ref());
                let jsp_dirty = drain_jsp_messages(&mut app_state, &ctx);
                let should_render = elapsed_ms >= SAFETY_NET_MS
                    || jsp_dirty
                    || (terminal_focused && dirty)
                    || (running_preview && elapsed_ms >= PREVIEW_THROTTLE_MS && dirty);

                if crate::app_shell_panic::drain_into_errors(&mut app_state) || should_render {
                    elapsed_ms = 0;
                    let tick = render_tick.get();
                    render_tick.set(tick.wrapping_add(1));
                }
            }
        }
    });
    hooks.use_future({
        let app_state = app_state;
        let ctx = ctx.clone();
        async move { crate::app_input::shell_overlay::observe_shell_exit(app_state, ctx).await }
    });
    // Batched shell-window inventory observer (issue #361 PR A): reconciles
    // hidden shells against the multiplexer off the input/render path.
    hooks.use_future({
        let app_state = app_state;
        async move { crate::app_input::shell_overlay::observe_shell_inventory(app_state).await }
    });
    hooks.use_future({
        let app_state = app_state;
        let ctx = ctx.clone();
        async move {
            crate::app_input::terminal_manager::observe_terminal_manager_preview(app_state, ctx)
                .await;
        }
    });
    hooks.use_future({
        let app_state = app_state;
        let ctx = ctx.clone();
        async move {
            crate::app_input::terminal_manager::observe_pending_shell_focus(app_state, ctx).await;
        }
    });

    // Slow-poll LOCAL agent liveness (~every 2s). On Windows it first probes
    // the shared psmux server identity; a `Gone`/`Replaced` server transitions
    // affected running agents to `ServerLost` (binding preserved) instead of
    // `Dead`. On other platforms the batched check is unchanged (issue #493).
    // The body lives in [`crate::app_shell_liveness::run_local_liveness`].
    hooks.use_future({
        let ctx = ctx.clone();
        let app_state = app_state;
        async move {
            crate::app_shell_liveness::run_local_liveness(app_state, ctx).await;
        }
    });

    // Issue #301: background persistence worker drain. The actual loop body
    // lives in [`crate::app_shell_workers::run_persist_worker`].
    hooks.use_future({
        let ctx = ctx.clone();
        async move {
            crate::app_shell_workers::run_persist_worker(ctx, app_state).await;
        }
    });

    // Issue #301 Phase 2: background capture worker drain. The actual loop
    // body lives in [`crate::app_shell_workers::run_capture_worker`].
    hooks.use_future({
        let ctx = ctx.clone();
        async move {
            crate::app_shell_workers::run_capture_worker(ctx).await;
        }
    });

    // Issue #662: refresh the run marker while the interface is alive. A run
    // that is killed without warning leaves behind the last moment it was
    // known to be running, which is what makes the death attributable.
    hooks.use_future(async move {
        const HEARTBEAT_SECONDS: u64 = 5;
        loop {
            smol::Timer::after(std::time::Duration::from_secs(HEARTBEAT_SECONDS)).await;
            smol::unblock(jefe::run_diagnostics::heartbeat).await;
        }
    });
    // Background attach/detach future. Polls the AttachScheduler every 50ms
    // and performs the actual runtime.attach()/detach() on a background OS
    // thread (via smol::unblock) so the render/input path is never blocked.
    hooks.use_future({
        let ctx = ctx.clone();
        let mut attach_scheduler = attach_scheduler;
        let mut app_state = app_state;
        async move {
            loop {
                smol::Timer::after(std::time::Duration::from_millis(50)).await;

                let action = {
                    let mut scheduler = attach_scheduler.write();
                    scheduler.poll(std::time::Instant::now())
                };

                let target = match action {
                    AttachAction::Stable | AttachAction::Waiting => continue,
                    AttachAction::Perform(target) => target,
                };

                // Drop last-good scrollback before switching agents so a contended mid-handoff frame returns empty (issue #489).
                crate::app_shell_workers::clear_last_history_lines();

                let Some(ctx_arc) = ctx.as_ref() else {
                    attach_scheduler.write().mark_attached(target);
                    continue;
                };
                let ctx_clone = std::sync::Arc::clone(ctx_arc);
                let outcome = smol::unblock(move || {
                    crate::app_shell_attach::perform_async_attach(ctx_clone, target)
                })
                .await;

                match outcome {
                    crate::app_shell_attach::AsyncAttachOutcome::Attached(agent_id) => {
                        {
                            let mut scheduler = attach_scheduler.write();
                            scheduler.mark_attached(Some(agent_id.clone()));
                        }
                        mark_agent_attached(&mut app_state, &agent_id);
                    }
                    crate::app_shell_attach::AsyncAttachOutcome::Detached => {
                        {
                            let mut scheduler = attach_scheduler.write();
                            scheduler.mark_attached(None);
                        }
                        clear_all_attachments(&mut app_state);
                    }
                    crate::app_shell_attach::AsyncAttachOutcome::Failed(agent_id) => {
                        {
                            let mut scheduler = attach_scheduler.write();
                            // Explicitly clear desired so the scheduler does not
                            // immediately retry the failed agent. The render body
                            // will update desired on the next frame (the agent is
                            // now Dead, so desired becomes None).
                            scheduler.set_desired(None);
                            scheduler.mark_attached(None);
                        }
                        crate::app_input::terminal_manager::on_shell_attach_failed(
                            &mut app_state,
                            &agent_id,
                        );
                        apply_attach_failure(&mut app_state, &agent_id);
                        let persisted = {
                            let mut state = app_state.write();
                            durable_save_request(&mut state)
                        };
                        // Offload file I/O to a background thread so the
                        // smol executor is not blocked during attach failure.
                        let ctx_for_persist = ctx.clone();
                        smol::unblock(move || schedule_durable_save(&ctx_for_persist, persisted))
                            .await;
                    }
                }
            }
        }
    });

    // Periodic PR-mode background refresh (~every 60s). Mirrors the liveness
    // poll pattern: a background loop that fires while the PR view is open and
    // silently refreshes the PR list + detail without flashing the loading
    // spinner or disrupting the user's selection/scroll position.
    //
    // Issue #411: the refresh is suppressed when the user has been idle (no
    // keyboard/mouse/paste input) for longer than the idle threshold, so
    // leaving jefe open on the PR screen cannot drain the GraphQL budget.
    hooks.use_future({
        let ctx = ctx.clone();
        let mut app_state = app_state;
        async move {
            const REFRESH_INTERVAL_SECONDS: u64 = 60;
            const IDLE_THRESHOLD_SECONDS: u64 = 5 * 60;
            loop {
                smol::Timer::after(std::time::Duration::from_secs(REFRESH_INTERVAL_SECONDS)).await;
                let is_idle = last_activity.get().elapsed().as_secs() >= IDLE_THRESHOLD_SECONDS;
                request_pr_background_refresh(&mut app_state, &ctx, is_idle);
            }
        }
    });

    hooks.use_terminal_events({
        let ctx = ctx.clone();
        let mut app_state = app_state;
        let mut should_quit = should_quit;
        let mut last_activity = last_activity;

        move |event| {
            // Issue #411: any terminal event (key, mouse, paste, resize)
            // resets the idle timer so the background refresh resumes.
            last_activity.set(Instant::now());
            handle_terminal_event(
                event,
                ctx.as_ref(),
                &mut app_state,
                &mut should_quit,
                &mut suppress_next_enter,
                &mut mouse_click,
            );
        }
    });

    if should_quit.get() {
        // Clean up transient agent work directories before exit (issue #213).
        {
            let state = app_state.read();
            crate::app_input::transient_cleanup::cleanup_transient_agent_dirs(&state);
        }
        // Graceful shutdown: close every tracked `jefe-shell` window (visible
        // and hidden) exactly once, best-effort, without killing agent
        // sessions (issue #361 PR A). Replaces the prior separate
        // cleanup_active_shell call so the visible shell is not closed twice.
        crate::app_input::shell_overlay::shutdown_all_shells(&mut app_state);
        // Issue #301: flush the coalescing persistence worker so the final
        // state is durable before exit.
        crate::app_shell_workers::shutdown_flush_persist(ctx.as_ref());
        // Issue #301: drain any pending capture request so the capture
        // worker does not leave a request mid-flight on exit.
        crate::app_shell_workers::shutdown_flush_capture(ctx.as_ref());

        hooks.use_context_mut::<SystemContext>().exit();

        // Return minimal element during exit.
        return element! {
            Box(width: 1, height: 1)
        };
    }

    // Agent liveness is checked by the slow-poll future (every ~2s), not here.
    // This keeps expensive tmux subprocess calls off the render hot path.

    let state = app_state.read();
    let modal = state.modal.clone();
    let snapshot: AppState = (*state).clone();
    drop(state);

    trace!(
        modal = ?std::mem::discriminant(&modal),
        screen = ?snapshot.screen(),
        pane_focus = ?snapshot.pane_focus,
        terminal_focused = snapshot.terminal_focused,
        repos = snapshot.repositories.len(),
        agents = snapshot.agents.len(),
        "render cycle"
    );

    // Get theme colors. Use try_lock so the render body never blocks waiting
    // for the ctx mutex — if a background attach holds it, we fall through to
    // the default theme for this frame and pick up the real theme next frame.
    let (theme_name, colors) = if let Some(ctx_arc) = &ctx {
        if let Ok(ctx_guard) = ctx_arc.try_lock() {
            (
                ctx_guard.theme_manager.active_theme().name.clone(),
                ctx_guard.theme_manager.active_theme().colors.clone(),
            )
        } else {
            ("green-screen".to_owned(), ThemeColors::default())
        }
    } else {
        ("green-screen".to_owned(), ThemeColors::default())
    };

    // Track selected agent separately from selected-running agent.
    let selected_agent_id = snapshot.selected_agent().map(|agent| agent.id.clone());
    let selected_running_agent_id = snapshot
        .selected_agent()
        .filter(|agent| agent.status == AgentStatus::Running)
        .map(|agent| agent.id.clone());

    // Record desired attach target non-blocking. The background future
    // performs the actual attach after the debounce window elapses.
    let desired_changed = {
        let scheduler = attach_scheduler.read();
        scheduler.desired() != selected_running_agent_id.as_ref()
    };
    if desired_changed {
        let mut scheduler = attach_scheduler.write();
        scheduler.set_desired(selected_running_agent_id.clone());
    }

    // Render snapshot rules:
    //  - Running selected agent: live viewer snapshot (guarded by attachment match).
    //  - Dead selected agent: captured dead pane output for same agent only.
    //  - Other states: no terminal content.
    let terminal_snapshot: Option<TerminalSnapshot> = capture_terminal_snapshot(
        ctx.as_ref(),
        &snapshot,
        selected_agent_id.as_ref(),
        selected_running_agent_id.as_ref(),
    );

    // Consume render tick.
    let _ = render_tick.get();

    // Calculate render dimensions.
    let (term_cols, term_rows) = crossterm::terminal::size().unwrap_or((120, 40));
    let (render_cols, render_rows) = effective_render_size(term_cols, term_rows);

    // Resolve this frame's geometry exactly once. Every consumer downstream —
    // renderer, mouse routing, selection, wrapping, PTY resize — reads this one
    // snapshot, so a band opening or a resize cannot leave two of them
    // disagreeing about where a panel is. A resize produces a new snapshot on
    // the next frame because the size read above is the only input.
    let mut snapshot = snapshot;
    snapshot.resolved_layout = jefe::screen_layout::resolve_screen(&snapshot, term_cols, term_rows);
    let snapshot = snapshot;

    // Capture scrollback history lines for the terminal pane (issue #198).
    // Only Dashboard mode renders the embedded terminal, so gate the (cloning)
    // cache capture to that mode — other modes waste the clone every frame.
    //
    // Issue #301 Phase 2: the render path no longer calls `capture_history`
    // (which shells out to `tmux capture-pane`) synchronously. Instead it
    // requests a background capture via the `CaptureHandle` and reads the
    // runtime's `HistoryCache` directly (non-blocking `get`). The background
    // worker drains the request and stores the result in the cache.
    let history_lines: Vec<String> = if snapshot.screen() == ScreenId::Dashboard
        || (snapshot.screen() == ScreenId::Terminals && snapshot.shell_overlay_active())
    {
        crate::app_shell_workers::capture_history_from_cache(ctx.as_ref())
    } else {
        Vec::new()
    };

    // NOTE: scroll-geometry (terminal_viewport_rows / terminal_total_lines) is
    // NOT written here. Mutating AppState during render causes an infinite
    // re-render loop (iocraft sees a state change and re-renders, which writes
    // again), starving the input loop (qqq never processed). The geometry is
    // refreshed at dispatch time instead — see refresh_terminal_scroll_geometry
    // (mirrors the detail-pane viewport-refresh pattern).
    // The embedded shell overlay replaces the workspace wholesale and is not
    // modelled by a descriptor, so it keeps its own geometry. Everything else
    // reads the frame's snapshot: the terminal pane is sized by the resolver,
    // which guarantees a nonzero content rectangle or hides the pane, so there
    // is no `.max(1)` to apply here.
    let terminal_rect = snapshot.resolved_layout.as_ref().and_then(|layout| {
        let descriptor = jefe::workbench::screen_descriptor(snapshot.screen()).ok()?;
        jefe::workbench::pty_content_rect(
            descriptor,
            layout,
            &jefe::workbench::PanelId::from_static("terminal"),
        )
    });
    let pty_layout = if snapshot.shell_overlay_active() && snapshot.screen() == ScreenId::Terminals
    {
        jefe::layout::compute_terminal_manager_pty_layout(term_cols, term_rows)
    } else if snapshot.shell_overlay_active() {
        jefe::layout::compute_shell_overlay_pty_layout(term_cols, term_rows)
    } else {
        compute_pty_layout(term_cols, term_rows)
    };
    let (terminal_pane_rows, terminal_pane_cols) = terminal_rect.map_or_else(
        || {
            (
                usize::from(pty_layout.pty_rows).max(1),
                usize::from(pty_layout.pty_cols).max(1),
            )
        },
        |rect| (usize::from(rect.height), usize::from(rect.width)),
    );
    let screen_el = build_screen_element(
        &snapshot,
        &colors,
        &theme_name,
        TerminalRenderData {
            snapshot: terminal_snapshot,
            history_lines,
            pane_rows: terminal_pane_rows,
            pane_cols: terminal_pane_cols,
        },
    );
    let confirm_data = derive_confirm_modal_data(&snapshot, &modal);
    let modal_el = build_modal_element(
        &snapshot,
        &modal,
        &colors,
        confirm_data,
        snapshot.help_scroll_offset,
        ModalViewport {
            cols: render_cols,
            rows: render_rows,
        },
    );

    // Root element with proper dimensions.
    // Search is an in-band mode used by SplitScreen's filter bar, not a blocking
    // overlay modal. Keep rendering the underlying screen in search mode.
    let content_el: AnyElement<'static> = if matches!(modal, ModalState::Search { .. }) {
        screen_el
    } else {
        modal_el.unwrap_or(screen_el)
    };

    element! {
        Box(
            flex_direction: FlexDirection::Column,
            background_color: Color::Rgb { r: 0, g: 0, b: 0 },
            width: u32::from(render_cols),
            height: u32::from(render_rows),
        ) {
            #(content_el)
        }
    }
}

pub type HookState<T> = iocraft::hooks::State<T>;
pub type CtxArc = Arc<std::sync::Mutex<AppContext>>;

/// Dispatch a terminal event to the appropriate input/runtime handler.
fn handle_terminal_event(
    event: TerminalEvent,
    ctx: Option<&CtxArc>,
    app_state: &mut HookState<AppState>,
    should_quit: &mut HookState<bool>,
    suppress_next_enter: &mut HookState<PasteEnterSuppression>,
    mouse_click: &mut HookState<crate::mouse_routing::MouseClickState>,
) {
    match event {
        TerminalEvent::Resize(cols, rows) => {
            mouse_click.write().clear();
            crate::mouse_routing::clear_selection(app_state);
            synchronize_actions_geometry(app_state, cols, rows);
            let state = app_state.read();
            crate::app_input::shell_overlay::resize_terminal(&ctx.cloned(), cols, rows, &state);
        }
        TerminalEvent::FullscreenMouse(mouse_event) => {
            crate::mouse_routing::handle_fullscreen_mouse(
                ctx,
                app_state,
                should_quit,
                suppress_next_enter,
                mouse_click,
                mouse_event,
            );
        }
        TerminalEvent::Paste(pasted_text) => {
            mouse_click.write().clear();
            crate::mouse_routing::clear_selection(app_state);
            handle_paste(ctx, app_state, suppress_next_enter, pasted_text);
        }
        TerminalEvent::Key(key_event) => {
            mouse_click.write().clear();
            // Clear selection on any keypress except Esc. Esc always clears;
            // other keys also clear so the selection doesn't linger after the
            // user transitions to keyboard interaction. (If keyboard copy of
            // the selection is added later, that key would be excluded here.)
            if crate::app_input::handle_dirty_guard_key(app_state, &ctx.cloned(), &key_event) {
                return;
            }
            // A waiting chord capture and an open layout tree each own the
            // keyboard while they are up, so the key the user is aiming at them
            // cannot also do whatever it is bound to underneath.
            if crate::app_input::handle_capture_key(app_state, &key_event) {
                return;
            }
            if crate::app_input::handle_layout_key(app_state, &key_event) {
                return;
            }
            if key_event.kind != iocraft::KeyEventKind::Release {
                crate::mouse_routing::clear_selection(app_state);
            }
            handle_key_event(ctx, app_state, should_quit, suppress_next_enter, key_event);
        }
        _ => {}
    }
}

fn handle_paste(
    ctx: Option<&CtxArc>,
    app_state: &mut HookState<AppState>,
    suppress_next_enter: &mut HookState<PasteEnterSuppression>,
    pasted_text: String,
) {
    let input_mode = {
        let state = app_state.read();
        input_mode_for_state(&state)
    };

    match input_mode {
        InputMode::TerminalCapture => paste_to_terminal(ctx, suppress_next_enter, pasted_text),
        InputMode::Form | InputMode::Search => {
            paste_to_form(ctx, app_state, suppress_next_enter, pasted_text);
        }
        InputMode::IssuesInline => {
            paste_to_issues_inline(ctx, app_state, suppress_next_enter, pasted_text);
        }
        InputMode::IssuesSearch => {
            paste_to_issues_search(app_state, suppress_next_enter, pasted_text);
        }
        _ => {
            suppress_next_enter.set(PasteEnterSuppression::new());
        }
    }
}

fn paste_to_terminal(
    ctx: Option<&CtxArc>,
    suppress_next_enter: &mut HookState<PasteEnterSuppression>,
    pasted_text: String,
) {
    let Some(ctx_arc) = ctx else {
        return;
    };
    let Ok(mut ctx_guard) = ctx_arc.lock() else {
        return;
    };

    let bytes = if ctx_guard.runtime.bracketed_paste_active() {
        let mut payload = Vec::with_capacity(pasted_text.len() + 12);
        payload.extend_from_slice(b"\x1b[200~");
        payload.extend_from_slice(pasted_text.as_bytes());
        payload.extend_from_slice(b"\x1b[201~");
        payload
    } else {
        pasted_text.into_bytes()
    };

    if let Err(e) = ctx_guard.runtime.write_input(&bytes) {
        warn!(error = %e, "runtime.write_input failed for paste");
    }
    suppress_next_enter.set(PasteEnterSuppression::new());
}

fn paste_to_form(
    ctx: Option<&CtxArc>,
    app_state: &mut HookState<AppState>,
    suppress_next_enter: &mut HookState<PasteEnterSuppression>,
    pasted_text: String,
) {
    let mut state = app_state.write();
    for ch in pasted_text.chars().filter(|ch| *ch != '\r' && *ch != '\n') {
        jefe::state::transition::commit_pure_site(&mut state, (AppEvent::FormChar(ch)).into());
    }
    let persisted = durable_save_request(&mut state);
    drop(state);
    schedule_durable_save(&ctx.cloned(), persisted);
    suppress_next_enter.set(PasteEnterSuppression::new());
}

fn paste_to_issues_inline(
    ctx: Option<&CtxArc>,
    app_state: &mut HookState<AppState>,
    suppress_next_enter: &mut HookState<PasteEnterSuppression>,
    pasted_text: String,
) {
    let mut state = app_state.write();
    for ch in pasted_text.chars().filter(|ch| *ch != '\r') {
        if ch == '\n' {
            jefe::state::transition::commit_pure_site(&mut state, (AppEvent::InlineNewline).into());
        } else {
            jefe::state::transition::commit_pure_site(
                &mut state,
                (AppEvent::InlineChar(ch)).into(),
            );
        }
    }
    let persisted = durable_save_request(&mut state);
    drop(state);
    schedule_durable_save(&ctx.cloned(), persisted);
    suppress_next_enter.set(PasteEnterSuppression::new());
}

fn paste_to_issues_search(
    app_state: &mut HookState<AppState>,
    suppress_next_enter: &mut HookState<PasteEnterSuppression>,
    pasted_text: String,
) {
    let mut state = app_state.write();
    let filtered: String = pasted_text
        .chars()
        .filter(|ch| *ch != '\r' && *ch != '\n')
        .collect();
    if !filtered.is_empty() {
        let mut query = state.issues_state.search_query.clone();
        query.push_str(&filtered);
        jefe::state::transition::commit_pure_site(
            &mut state,
            (AppEvent::SetSearchQuery { query }).into(),
        );
    }
    drop(state);
    suppress_next_enter.set(PasteEnterSuppression::new());
}

fn normalize_terminal_focus(app_state: &mut HookState<AppState>, ctx: Option<&CtxArc>) {
    let needs_normalization = {
        let state = app_state.read();
        state.terminal_focused && state.pane_focus != PaneFocus::Terminal
    };
    if !needs_normalization {
        return;
    }
    debug!("clearing stale terminal_focused (pane not Terminal)");
    let mut state = app_state.write();
    state.terminal_focused = false;
    let persisted = durable_save_request(&mut state);
    drop(state);
    schedule_durable_save(&ctx.cloned(), persisted);
}

fn should_ignore_key_event(key_event: &KeyEvent) -> bool {
    key_event.kind == KeyEventKind::Release
        || (key_event.kind == KeyEventKind::Repeat && key_event.code == KeyCode::Enter)
}

fn handle_key_event(
    ctx: Option<&CtxArc>,
    app_state: &mut HookState<AppState>,
    should_quit: &mut HookState<bool>,
    suppress_next_enter: &mut HookState<PasteEnterSuppression>,
    key_event: KeyEvent,
) {
    if should_ignore_key_event(&key_event) {
        return;
    }
    normalize_terminal_focus(app_state, ctx);

    let state_ro = app_state.read();
    let term_focused = state_ro.terminal_focused;
    let pane_focus = state_ro.pane_focus;
    let screen = state_ro.screen();
    let modal = state_ro.modal.clone();
    let input_mode = input_mode_for_state(&state_ro);
    drop(state_ro);

    trace!(
        code = ?key_event.code,
        modifiers = ?key_event.modifiers,
        kind = ?key_event.kind,
        term_focused,
        pane_focus = ?pane_focus,
        screen = ?screen,
        modal = ?std::mem::discriminant(&modal),
        "key event received"
    );

    let now = Instant::now();
    if try_suppress_synthetic_enter(suppress_next_enter, &key_event, now) {
        return;
    }
    update_paste_enter_suppression(input_mode, suppress_next_enter, &key_event, now);

    let raw_event = {
        let state = app_state.read();
        resolve_raw_key_mutation(&state, &key_event)
    };
    if let Some(event) = raw_event {
        dispatch_app_event(app_state, &ctx.cloned(), event);
        return;
    }

    let _ = route_registry_key(ctx, app_state, should_quit, suppress_next_enter, &key_event);
}

fn mark_agent_attached(app_state: &mut HookState<AppState>, selected_agent_id: &AgentId) {
    let mut state = app_state.write();
    for agent in &mut state.agents {
        if let Some(binding) = agent.runtime_binding.as_mut() {
            binding.attached = agent.id == *selected_agent_id;
        }
    }
}

fn apply_attach_failure(app_state: &mut HookState<AppState>, agent_id: &AgentId) {
    let mut state = app_state.write();
    state.terminal_focused = false;
    state.pane_focus = PaneFocus::Agents;
    for agent in &mut state.agents {
        if agent.id == *agent_id
            && let Some(binding) = agent.runtime_binding.as_mut()
        {
            binding.attached = false;
        }
    }
}

fn clear_all_attachments(app_state: &mut HookState<AppState>) {
    let mut state = app_state.write();
    for agent in &mut state.agents {
        if let Some(binding) = agent.runtime_binding.as_mut() {
            binding.attached = false;
        }
    }
}

#[must_use]
fn wants_live_snapshot(status: AgentStatus) -> bool {
    matches!(status, AgentStatus::Running | AgentStatus::Dead)
}

#[cfg(test)]
#[must_use]
pub fn wants_live_snapshot_pub(status: AgentStatus) -> bool {
    wants_live_snapshot(status)
}

/// Capture terminal output without shelling out during render.
pub fn capture_terminal_snapshot(
    ctx: Option<&CtxArc>,
    snapshot: &AppState,
    selected_agent_id: Option<&AgentId>,
    selected_running_agent_id: Option<&AgentId>,
) -> Option<TerminalSnapshot> {
    let selected_agent = snapshot.selected_agent()?;

    // The live PTY snapshot is attempted for any Running agent whose viewer is
    // attached, regardless of pane focus. Decoupling the snapshot from
    // `pane_focus` lets the terminal render as a read-only *preview* while the
    // user navigates the agents/repos lists (and after restart), so a healthy
    // live session is never mistaken for a lost one (issue #160). Keystroke
    // forwarding is controlled separately by `terminal_focused`.
    //
    // Early-return for statuses that never produce a snapshot, before locking
    // the ctx mutex.
    if !wants_live_snapshot(selected_agent.status) {
        return None;
    }

    match selected_agent.status {
        AgentStatus::Running => {
            // `try_lock` keeps the render cycle non-blocking: when a background
            // attach holds the ctx mutex, this frame simply returns None and
            // the next frame picks up the snapshot.
            let ctx_arc = ctx?;
            let ctx_guard = ctx_arc.try_lock().ok()?;
            selected_running_agent_id
                .as_ref()
                .filter(|id| ctx_guard.runtime.attached_agent() == Some(*id))
                .and_then(|_| ctx_guard.runtime.snapshot())
        }
        AgentStatus::Dead => selected_agent_id.as_ref().and_then(|agent_id| {
            snapshot
                .repository_for_agent(agent_id)
                .filter(|repository| !repository.remote.enabled)?;
            snapshot
                .dead_preview(agent_id)
                .map(jefe::runtime::snapshot_from_lines)
        }),
        _ => None,
    }
}

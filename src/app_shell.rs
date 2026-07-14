//! Root application component (App) for the Jefe TUI.
//!
//! Houses the iocraft component lifecycle: state hooks, futures,
//! terminal event handling, PTY attachment, and render composition.

use iocraft::prelude::*;
use tracing::{debug, trace, warn};

use crate::AppContext;
use crate::app_input::{
    dispatch_app_event, forward_key_to_pty, handle_f12_toggle, handle_global_shortcut_key,
    handle_mode_auth_key, handle_mode_confirm_key, handle_mode_form_key, handle_mode_help_key,
    handle_mode_search_key, handle_mode_theme_picker_key, handle_normal_key_event, persist_state,
    request_pr_background_refresh, synchronize_actions_geometry, to_persisted_state,
    try_ctrl_c_interrupt_passthrough, try_intercept_terminal_scrollback,
    try_suppress_synthetic_enter, update_paste_enter_suppression,
};
use crate::pty_encoding::PasteEnterSuppression;

use jefe::domain::{AgentId, AgentStatus};
use jefe::input::{InputMode, input_mode_for_state};
use jefe::layout::{compute_pty_layout, effective_render_size};
use jefe::runtime::{
    AttachAction, AttachScheduler, DEFAULT_DEBOUNCE, RuntimeManager, TerminalSnapshot,
};
use jefe::state::{AppEvent, AppState, ModalState, PaneFocus, ScreenMode};
use jefe::theme::{ThemeColors, ThemeManager};
use jefe::ui::orchestration::{
    TerminalRenderData, build_modal_element, build_screen_element, derive_confirm_modal_data,
};

use std::sync::Arc;
use std::time::Instant;

/// Props for the root app component.
#[derive(Default, Props)]
pub struct AppProps {
    pub context: Option<Arc<std::sync::Mutex<AppContext>>>,
}

/// Root application component that manages state and renders the UI.
#[component]
pub fn App(mut hooks: Hooks, props: &AppProps) -> impl Into<AnyElement<'static>> {
    let should_quit = hooks.use_state(|| false);
    let mut app_state = hooks.use_state(AppState::default);
    let render_tick = hooks.use_state(|| 0u64);
    let help_scroll = hooks.use_state(|| 0u32);
    let mut initialized = hooks.use_state(|| false);
    let mut startup_sessions_restored = hooks.use_state(|| false);
    // Debounce state machine for background attach/detach. The render body
    // records the *desired* target here; a background future polls and
    // performs the actual attach off the render/input hot path.
    let mut attach_scheduler = hooks.use_state(|| AttachScheduler::new(DEFAULT_DEBOUNCE));
    // Some terminals emit a synthetic Enter key before/after a Paste event for
    // Cmd/Ctrl+V. Suppress only the synthetic Enter (bounded by a short time
    // window) to avoid swallowing a real submit Enter under load (issue #286).
    let mut suppress_next_enter = hooks.use_state(PasteEnterSuppression::new);

    let ctx = props.context.clone();

    // One-time initialization: load persisted state.
    if !initialized.get() {
        initialized.set(true);
        crate::app_init::init_app_state(&mut app_state, &ctx);
    }

    // Restore runtime session map from persisted agent statuses exactly once.
    if !startup_sessions_restored.get() {
        startup_sessions_restored.set(true);
        crate::app_init::restore_runtime_sessions(&mut app_state, &ctx);
    }

    // Event-driven render: only trigger a re-render when the PTY has new data
    // (dirty flag) or a safety-net interval (~1s) has elapsed. This avoids
    // wasteful ~30fps renders that block keyboard input on smol's single
    // executor thread. PTY dirtiness triggers fast renders when the terminal
    // pane has input focus; for a read-only preview (Running agent selected but
    // terminal not focused), dirty renders are throttled to avoid starving the
    // input loop while navigating lists (issue #160).
    hooks.use_future({
        let ctx = ctx.clone();
        let app_state = app_state;
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

                let dirty = is_pty_dirty(ctx.as_ref());
                let should_render = elapsed_ms >= SAFETY_NET_MS
                    || (terminal_focused && dirty)
                    || (running_preview && elapsed_ms >= PREVIEW_THROTTLE_MS && dirty);

                if should_render {
                    elapsed_ms = 0;
                    let tick = render_tick.get();
                    render_tick.set(tick.wrapping_add(1));
                }
            }
        }
    });

    // Slow-poll LOCAL agent liveness (~every 2s). The batched check uses
    // exactly two tmux subprocess invocations (list-sessions + list-panes -a)
    // regardless of agent count, offloaded to a background OS thread via
    // `smol::unblock` so the executor stays free for input events (issue #287).
    // Remote agents are excluded — their SSH round-trips would starve the
    // executor; remote death is detected lazily on select/attach.
    hooks.use_future({
        let ctx = ctx.clone();
        let mut app_state = app_state;
        async move {
            loop {
                smol::Timer::after(std::time::Duration::from_secs(2)).await;

                let Some(ctx_arc) = &ctx else {
                    continue;
                };

                // Collect local-only check targets under the lock, then release it.
                let targets: Vec<_> = {
                    let Ok(ctx_guard) = ctx_arc.lock() else {
                        continue;
                    };
                    let state = app_state.read();
                    let running_ids: Vec<AgentId> = state
                        .agents
                        .iter()
                        .filter(|a| a.is_running())
                        .map(|a| a.id.clone())
                        .collect();
                    drop(state);
                    let all_targets = ctx_guard.runtime.liveness_targets();
                    drop(ctx_guard);

                    all_targets
                        .into_iter()
                        .filter(|t| t.remote.is_none() && running_ids.contains(&t.agent_id))
                        .collect::<Vec<_>>()
                };

                if targets.is_empty() {
                    continue;
                }

                // Offload the batched tmux subprocess calls to a background OS
                // thread so the smol executor can continue processing input.
                let dead_identities = smol::unblock(move || {
                    jefe::runtime::batch_liveness_check_with_identity(&targets)
                })
                .await;

                if !dead_identities.is_empty() {
                    debug!(
                        count = dead_identities.len(),
                        "liveness poll found dead agents"
                    );
                    // Issue #301 Phase 4: stale-result protection. Before
                    // marking an agent dead, verify the agent's current
                    // binding session name and lifecycle generation still
                    // match the liveness snapshot. A mismatch means the
                    // agent was rebound/restarted after the check was
                    // dispatched; skip it.
                    let mut state = app_state.write();
                    // Build a lookup map from agent_id → (session_name, gen)
                    // from the current state to avoid O(n*m) scan for each
                    // identity (issue #301 review feedback).
                    let current_bindings: std::collections::HashMap<
                        &AgentId,
                        (Option<&String>, u64),
                    > = state
                        .agents
                        .iter()
                        .map(|a| {
                            let session = a.runtime_binding.as_ref().map(|b| &b.session_name);
                            let lifecycle_gen = a
                                .runtime_binding
                                .as_ref()
                                .map_or(0, |b| b.lifecycle_generation);
                            (&a.id, (session, lifecycle_gen))
                        })
                        .collect();
                    let mut to_apply: Vec<AgentId> = Vec::new();
                    for identity in &dead_identities {
                        let Some(&(current_session, current_gen)) =
                            current_bindings.get(&identity.agent_id)
                        else {
                            // Agent removed from state since the check — skip.
                            continue;
                        };
                        let session_matches =
                            current_session == identity.binding_session_name.as_ref();
                        let gen_matches = current_gen == identity.lifecycle_generation;
                        if session_matches && gen_matches {
                            to_apply.push(identity.agent_id.clone());
                        } else {
                            debug!(
                                agent_id = %identity.agent_id.0,
                                checked_session = ?identity.binding_session_name,
                                current_session = ?current_session,
                                checked_gen = identity.lifecycle_generation,
                                current_gen,
                                "liveness: stale result after rebind/restart; skipping"
                            );
                        }
                    }
                    for agent_id in &to_apply {
                        *state = std::mem::take(&mut *state).apply(AppEvent::AgentStatusChanged(
                            agent_id.clone(),
                            AgentStatus::Dead,
                        ));
                        if let Some(agent) =
                            state.agents.iter_mut().find(|agent| &agent.id == agent_id)
                        {
                            agent.runtime_binding = None;
                        }
                    }
                    if to_apply.is_empty() {
                        drop(state);
                    } else {
                        let persisted = to_persisted_state(&state);
                        drop(state);
                        persist_state(&ctx, &persisted);
                    }
                }
            }
        }
    });

    // Issue #301: background persistence worker drain. The actual loop body
    // lives in [`crate::app_shell_workers::run_persist_worker`].
    hooks.use_future({
        let ctx = ctx.clone();
        async move {
            crate::app_shell_workers::run_persist_worker(ctx).await;
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
                        apply_attach_failure(&mut app_state, &agent_id);
                        let persisted = {
                            let state = app_state.read();
                            to_persisted_state(&state)
                        };
                        // Offload file I/O to a background thread so the
                        // smol executor is not blocked during attach failure.
                        let ctx_for_persist = ctx.clone();
                        smol::unblock(move || persist_state(&ctx_for_persist, &persisted)).await;
                    }
                }
            }
        }
    });

    // Periodic PR-mode background refresh (~every 60s). Mirrors the liveness
    // poll pattern: a background loop that fires while the PR view is open and
    // silently refreshes the PR list + detail without flashing the loading
    // spinner or disrupting the user's selection/scroll position.
    hooks.use_future({
        let ctx = ctx.clone();
        let mut app_state = app_state;
        async move {
            loop {
                const REFRESH_INTERVAL_SECONDS: u64 = 60;
                smol::Timer::after(std::time::Duration::from_secs(REFRESH_INTERVAL_SECONDS)).await;
                request_pr_background_refresh(&mut app_state, &ctx);
            }
        }
    });

    // Handle terminal events.
    hooks.use_terminal_events({
        let ctx = ctx.clone();
        let mut app_state = app_state;
        let mut should_quit = should_quit;
        let mut help_scroll = help_scroll;

        move |event| {
            handle_terminal_event(
                event,
                ctx.as_ref(),
                &mut app_state,
                &mut should_quit,
                &mut help_scroll,
                &mut suppress_next_enter,
            );
        }
    });

    // Handle quit.
    if should_quit.get() {
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

    // Read state for rendering.
    let state = app_state.read();
    let modal = state.modal.clone();
    let snapshot: AppState = (*state).clone();
    drop(state);

    trace!(
        modal = ?std::mem::discriminant(&modal),
        screen_mode = ?snapshot.screen_mode,
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

    // Capture scrollback history lines for the terminal pane (issue #198).
    // Only Dashboard mode renders the embedded terminal, so gate the (cloning)
    // cache capture to that mode — other modes waste the clone every frame.
    //
    // Issue #301 Phase 2: the render path no longer calls `capture_history`
    // (which shells out to `tmux capture-pane`) synchronously. Instead it
    // requests a background capture via the `CaptureHandle` and reads the
    // runtime's `HistoryCache` directly (non-blocking `get`). The background
    // worker drains the request and stores the result in the cache.
    let history_lines: Vec<String> = if snapshot.screen_mode == ScreenMode::Dashboard {
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
    let pty_layout = compute_pty_layout(term_cols, term_rows);
    let screen_el = build_screen_element(
        &snapshot,
        &colors,
        &theme_name,
        TerminalRenderData {
            snapshot: terminal_snapshot,
            history_lines,
            pane_rows: usize::from(pty_layout.pty_rows).max(1),
            pane_cols: usize::from(pty_layout.pty_cols).max(1),
        },
    );
    let confirm_data = derive_confirm_modal_data(&snapshot, &modal);
    let modal_el = build_modal_element(
        &snapshot,
        &modal,
        &colors,
        confirm_data,
        usize::try_from(help_scroll.get()).unwrap_or(0),
        render_rows,
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
///
/// Extracted from the `App` component so the iocraft hook closures stay
/// within clippy's cognitive complexity budget.
fn handle_terminal_event(
    event: TerminalEvent,
    ctx: Option<&CtxArc>,
    app_state: &mut HookState<AppState>,
    should_quit: &mut HookState<bool>,
    help_scroll: &mut HookState<u32>,
    suppress_next_enter: &mut HookState<PasteEnterSuppression>,
) {
    match event {
        TerminalEvent::Resize(cols, rows) => {
            crate::mouse_routing::clear_selection(app_state);
            synchronize_actions_geometry(app_state, cols, rows);
            handle_resize(ctx, cols, rows);
        }
        TerminalEvent::FullscreenMouse(mouse_event) => {
            crate::mouse_routing::handle_fullscreen_mouse(ctx, app_state, mouse_event);
        }
        TerminalEvent::Paste(pasted_text) => {
            crate::mouse_routing::clear_selection(app_state);
            handle_paste(ctx, app_state, suppress_next_enter, pasted_text);
        }
        TerminalEvent::Key(key_event) => {
            // Clear selection on any keypress except Esc. Esc always clears;
            // other keys also clear so the selection doesn't linger after the
            // user transitions to keyboard interaction. (If keyboard copy of
            // the selection is added later, that key would be excluded here.)
            if key_event.kind != iocraft::KeyEventKind::Release {
                crate::mouse_routing::clear_selection(app_state);
            }
            handle_key_event(
                ctx,
                app_state,
                should_quit,
                help_scroll,
                suppress_next_enter,
                key_event,
            );
        }
        _ => {}
    }
}

fn handle_resize(ctx: Option<&CtxArc>, cols: u16, rows: u16) {
    if let Some(ctx_arc) = ctx
        && let Ok(mut ctx_guard) = ctx_arc.lock()
    {
        let layout = compute_pty_layout(cols, rows);
        let _ = ctx_guard.runtime.resize(layout.pty_rows, layout.pty_cols);
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
        *state = std::mem::take(&mut *state).apply(AppEvent::FormChar(ch));
    }
    let persisted = to_persisted_state(&state);
    drop(state);
    persist_state(&ctx.cloned(), &persisted);
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
            *state = std::mem::take(&mut *state).apply(AppEvent::InlineNewline);
        } else {
            *state = std::mem::take(&mut *state).apply(AppEvent::InlineChar(ch));
        }
    }
    let persisted = to_persisted_state(&state);
    drop(state);
    persist_state(&ctx.cloned(), &persisted);
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
        *state = std::mem::take(&mut *state).apply(AppEvent::SetSearchQuery { query });
    }
    drop(state);
    suppress_next_enter.set(PasteEnterSuppression::new());
}

/// Whether a key event should be ignored entirely (not forwarded or processed).
///
/// Release events are always ignored. Enter repeats are ignored so that holding
/// Enter under OS key-repeat does not send duplicate submits into the PTY
/// (issue #286). All other keys (Backspace, Delete, arrows, character input)
/// keep their normal repeat behavior.
fn should_ignore_key_event(key_event: &KeyEvent) -> bool {
    key_event.kind == KeyEventKind::Release
        || (key_event.kind == KeyEventKind::Repeat && key_event.code == KeyCode::Enter)
}

fn handle_key_event(
    ctx: Option<&CtxArc>,
    app_state: &mut HookState<AppState>,
    should_quit: &mut HookState<bool>,
    help_scroll: &mut HookState<u32>,
    suppress_next_enter: &mut HookState<PasteEnterSuppression>,
    key_event: KeyEvent,
) {
    if should_ignore_key_event(&key_event) {
        return;
    }

    let state_ro = app_state.read();
    let term_focused = state_ro.terminal_focused;
    let pane_focus = state_ro.pane_focus;
    let screen_mode = state_ro.screen_mode;
    let modal = state_ro.modal.clone();
    let early_input_mode = input_mode_for_state(&state_ro);
    drop(state_ro);

    trace!(
        code = ?key_event.code,
        modifiers = ?key_event.modifiers,
        kind = ?key_event.kind,
        term_focused,
        pane_focus = ?pane_focus,
        screen_mode = ?screen_mode,
        modal = ?std::mem::discriminant(&modal),
        "key event received"
    );

    let now = Instant::now();
    if try_suppress_synthetic_enter(suppress_next_enter, &key_event, now) {
        return;
    }
    update_paste_enter_suppression(early_input_mode, suppress_next_enter, &key_event, now);

    // F12 toggles terminal focus in Dashboard/Split/Actions. In Issues/PR
    // mode F12 is mode-aware (defocus / return to list) and is handled by
    // the mode-specific resolvers below, so it must NOT be intercepted here.
    if key_event.code == KeyCode::F(12)
        && matches!(
            screen_mode,
            ScreenMode::Dashboard | ScreenMode::Split | ScreenMode::DashboardActions
        )
    {
        handle_f12_toggle(app_state, &ctx.cloned());
        return;
    }

    if handle_global_shortcut_key(app_state, &ctx.cloned(), &key_event) {
        return;
    }

    let input_mode = resolve_input_mode(app_state, ctx, term_focused, pane_focus);

    // Ctrl-C interrupt passthrough (#200): forward Ctrl-C to the attached
    // agent terminal regardless of pane focus when the plain dashboard is
    // showing (no modal/form/search owns the key). See
    // [`try_ctrl_c_interrupt_passthrough`] for why focus-independence matters.
    if try_ctrl_c_interrupt_passthrough(ctx, suppress_next_enter, input_mode, &key_event) {
        return;
    }

    if input_mode == InputMode::TerminalCapture {
        // Check scrollback key interception BEFORE forwarding to PTY (issue #198).
        if try_intercept_terminal_scrollback(app_state, &ctx.cloned(), &key_event) {
            return;
        }
        forward_key_to_pty(ctx, suppress_next_enter, &key_event);
        return;
    }

    if dispatch_mode_specific_key(app_state, ctx, help_scroll, &key_event, input_mode) {
        return;
    }

    if let Some(evt) = handle_normal_key_event(
        app_state,
        should_quit,
        &ctx.cloned(),
        &key_event,
        screen_mode,
    ) {
        dispatch_app_event(app_state, &ctx.cloned(), evt);
    }
}

fn resolve_input_mode(
    app_state: &mut HookState<AppState>,
    ctx: Option<&CtxArc>,
    term_focused: bool,
    pane_focus: PaneFocus,
) -> InputMode {
    if term_focused && pane_focus != PaneFocus::Terminal {
        // Defensive guard: terminal input is only valid when terminal pane is active.
        debug!(
            pane_focus = ?pane_focus,
            "clearing stale terminal_focused (pane not Terminal)"
        );
        let mut state = app_state.write();
        state.terminal_focused = false;
        let persisted = to_persisted_state(&state);
        drop(state);
        persist_state(&ctx.cloned(), &persisted);
        InputMode::Normal
    } else {
        let state = app_state.read();
        input_mode_for_state(&state)
    }
}

/// Returns true when the event was fully handled (caller should return).
fn dispatch_mode_specific_key(
    app_state: &mut HookState<AppState>,
    ctx: Option<&CtxArc>,
    help_scroll: &mut HookState<u32>,
    key_event: &KeyEvent,
    input_mode: InputMode,
) -> bool {
    match input_mode {
        InputMode::Help => {
            handle_mode_help_key(app_state, &ctx.cloned(), help_scroll, key_event);
            true
        }
        InputMode::Confirm => handle_mode_confirm_key(app_state, &ctx.cloned(), key_event),
        InputMode::ThemePicker => {
            handle_mode_theme_picker_key(app_state, &ctx.cloned(), key_event);
            true
        }
        InputMode::Auth => handle_mode_auth_key(app_state, &ctx.cloned(), key_event),
        InputMode::Search => handle_mode_search_key(app_state, &ctx.cloned(), key_event),
        InputMode::Form => handle_mode_form_key(app_state, &ctx.cloned(), key_event),
        // @plan PLAN-20260329-ISSUES-MODE.P03
        InputMode::TerminalCapture
        | InputMode::Normal
        | InputMode::IssuesNormal
        | InputMode::IssuesInline
        | InputMode::IssuesSearch
        | InputMode::IssuesFilter
        | InputMode::IssuesChooser
        // @plan PLAN-20260624-PR-MODE.P03
        // @requirement REQ-PR-002
        // P03: PR input modes not yet routed (P09 handles key routing).
        | InputMode::PrsNormal
        | InputMode::PrsInline
        | InputMode::PrsSearch
        | InputMode::PrsFilter
        | InputMode::PrsChooser
        | InputMode::ActionsNormal
        | InputMode::ActionsFilter
        | InputMode::ActionsSearch => false,
    }
}

fn mark_agent_attached(app_state: &mut HookState<AppState>, selected_agent_id: &AgentId) {
    let mut state = app_state.write();
    for agent in &mut state.agents {
        if let Some(binding) = agent.runtime_binding.as_mut() {
            binding.attached = agent.id == *selected_agent_id;
        }
    }
}

/// Apply an attach failure to app state only (no runtime side effects).
///
/// `mark_session_dead` is called in [`perform_async_attach`] while the mutex
/// is held; this function only updates the UI/state layer.
fn apply_attach_failure(app_state: &mut HookState<AppState>, agent_id: &AgentId) {
    let mut state = app_state.write();
    state.terminal_focused = false;
    state.pane_focus = PaneFocus::Agents;
    if let Some(agent) = state.agents.iter_mut().find(|agent| agent.id == *agent_id) {
        agent.status = AgentStatus::Dead;
        agent.runtime_binding = None;
    }
    for agent in &mut state.agents {
        if let Some(binding) = agent.runtime_binding.as_mut() {
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

/// Whether the live PTY snapshot should be attempted for the selected agent.
///
/// Previously gated on `pane_focus == Terminal` for Running agents via
/// `should_skip_live_snapshot(status, pane_focus)` (issue #160); now always
/// attempts for Running agents so the terminal renders as a read-only preview
/// regardless of which pane has focus. Dead agents also get a one-shot capture.
#[must_use]
fn wants_live_snapshot(status: AgentStatus) -> bool {
    matches!(status, AgentStatus::Running | AgentStatus::Dead)
}

/// Capture terminal output for the currently selected agent if available.
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

    // `try_lock` keeps the render cycle non-blocking: when a background attach
    // holds the ctx mutex, this frame simply returns None and the next frame
    // picks up the snapshot.
    let ctx_arc = ctx?;
    let ctx_guard = ctx_arc.try_lock().ok()?;
    match selected_agent.status {
        AgentStatus::Running => selected_running_agent_id
            .as_ref()
            .filter(|id| ctx_guard.runtime.attached_agent() == Some(*id))
            .and_then(|_| ctx_guard.runtime.snapshot()),
        AgentStatus::Dead => selected_agent_id.as_ref().and_then(|agent_id| {
            snapshot
                .repository_for_agent(agent_id)
                .filter(|repository| !repository.remote.enabled)
                .and_then(|_| ctx_guard.runtime.capture_session_output(agent_id))
        }),
        _ => None,
    }
}

/// Check whether the attached PTY has new data since the last render.
///
/// Uses `try_lock` so the timer future never blocks on the `ctx` mutex. When
/// the lock is contended (e.g. a background attach is running), this returns
/// `false` — the next poll iteration will try again.
fn is_pty_dirty(ctx: Option<&CtxArc>) -> bool {
    let Some(ctx_arc) = ctx else {
        return false;
    };
    let Ok(ctx_guard) = ctx_arc.try_lock() else {
        return false;
    };
    ctx_guard.runtime.take_dirty()
}

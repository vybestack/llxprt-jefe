//! Root application component (App) for the Jefe TUI.
//!
//! Houses the iocraft component lifecycle: state hooks, futures,
//! terminal event handling, PTY attachment, and render composition.

use iocraft::prelude::*;
use tracing::{debug, trace, warn};

use crate::AppContext;
use crate::app_input::{
    dispatch_app_event, handle_f12_toggle, handle_global_shortcut_key, handle_mode_confirm_key,
    handle_mode_form_key, handle_mode_help_key, handle_mode_search_key, handle_normal_key_event,
    persist_state, request_pr_background_refresh, to_persisted_state,
};
use crate::pty_encoding::{
    key_to_bytes, should_arm_paste_enter_suppression, should_disarm_paste_enter_suppression,
    should_suppress_synthetic_enter,
};

use jefe::domain::{AgentId, AgentStatus};
use jefe::input::{InputMode, input_mode_for_state};
use jefe::layout::{compute_pty_layout, effective_render_size};
use jefe::runtime::{
    AttachAction, AttachScheduler, DEFAULT_DEBOUNCE, RuntimeError, RuntimeManager, TerminalSnapshot,
};
use jefe::state::{AppEvent, AppState, ModalState, PaneFocus};
use jefe::theme::{ThemeColors, ThemeManager};
use jefe::ui::orchestration::{
    build_modal_element, build_screen_element, derive_confirm_modal_data,
};

use std::sync::Arc;

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
    // Some terminals emit a synthetic Enter key before a Paste event for Cmd/Ctrl+V.
    // Suppress just that one Enter to avoid accidental submits while pasting.
    let mut suppress_next_enter = hooks.use_state(|| false);

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
                let running_preview = !terminal_focused
                    && {
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

    // Slow-poll LOCAL agent liveness via tmux subprocess (~every 2s).
    // This keeps the expensive `tmux has-session` calls off the render hot path.
    //
    // Remote agents are deliberately excluded: SSH liveness round-trips are
    // blocking I/O that starves the smol executor, causing dropped keystrokes
    // and sluggish UI for *all* agents. Remote agent death is detected lazily
    // when the user selects/attaches to one.
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
                let targets = {
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

                let dead_agents: Vec<AgentId> = targets
                    .into_iter()
                    .filter(|t| !jefe::runtime::check_session_alive(&t.session_name))
                    .map(|t| t.agent_id)
                    .collect();

                if !dead_agents.is_empty() {
                    debug!(count = dead_agents.len(), "liveness poll found dead agents");
                    let mut state = app_state.write();
                    for agent_id in &dead_agents {
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
                    let persisted = to_persisted_state(&state);
                    drop(state);
                    persist_state(&ctx, &persisted);
                }
            }
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
                let outcome = smol::unblock(move || perform_async_attach(ctx_clone, target)).await;

                match outcome {
                    AsyncAttachOutcome::Attached(agent_id) => {
                        {
                            let mut scheduler = attach_scheduler.write();
                            scheduler.mark_attached(Some(agent_id.clone()));
                        }
                        mark_agent_attached(&mut app_state, &agent_id);
                    }
                    AsyncAttachOutcome::Detached => {
                        {
                            let mut scheduler = attach_scheduler.write();
                            scheduler.mark_attached(None);
                        }
                        clear_all_attachments(&mut app_state);
                    }
                    AsyncAttachOutcome::Failed(agent_id) => {
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
                smol::Timer::after(std::time::Duration::from_secs(60)).await;
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
        // Save state before exiting.
        let state = app_state.read();
        let persisted = to_persisted_state(&state);
        drop(state);
        persist_state(&ctx, &persisted);

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

    // Build screen and modal elements using orchestration helpers.
    let screen_el = build_screen_element(&snapshot, &colors, &theme_name, terminal_snapshot);
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
    suppress_next_enter: &mut HookState<bool>,
) {
    match event {
        TerminalEvent::Resize(cols, rows) => {
            crate::mouse_routing::clear_selection(app_state);
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
    suppress_next_enter: &mut HookState<bool>,
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
            suppress_next_enter.set(false);
        }
    }
}

fn paste_to_terminal(
    ctx: Option<&CtxArc>,
    suppress_next_enter: &mut HookState<bool>,
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
    suppress_next_enter.set(false);
}

fn paste_to_form(
    ctx: Option<&CtxArc>,
    app_state: &mut HookState<AppState>,
    suppress_next_enter: &mut HookState<bool>,
    pasted_text: String,
) {
    let mut state = app_state.write();
    for ch in pasted_text.chars().filter(|ch| *ch != '\r' && *ch != '\n') {
        *state = std::mem::take(&mut *state).apply(AppEvent::FormChar(ch));
    }
    let persisted = to_persisted_state(&state);
    drop(state);
    persist_state(&ctx.cloned(), &persisted);
    suppress_next_enter.set(false);
}

fn paste_to_issues_inline(
    ctx: Option<&CtxArc>,
    app_state: &mut HookState<AppState>,
    suppress_next_enter: &mut HookState<bool>,
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
    suppress_next_enter.set(false);
}

fn paste_to_issues_search(
    app_state: &mut HookState<AppState>,
    suppress_next_enter: &mut HookState<bool>,
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
    suppress_next_enter.set(false);
}

fn handle_key_event(
    ctx: Option<&CtxArc>,
    app_state: &mut HookState<AppState>,
    should_quit: &mut HookState<bool>,
    help_scroll: &mut HookState<u32>,
    suppress_next_enter: &mut HookState<bool>,
    key_event: KeyEvent,
) {
    // Ignore release events if we've seen press/repeat.
    if key_event.kind == KeyEventKind::Release {
        return;
    }

    let state_ro = app_state.read();
    let term_focused = state_ro.terminal_focused;
    let pane_focus = state_ro.pane_focus;
    let screen_mode = state_ro.screen_mode;
    let modal = state_ro.modal.clone();
    drop(state_ro);

    trace!(
        code = ?key_event.code,
        modifiers = ?key_event.modifiers,
        term_focused,
        pane_focus = ?pane_focus,
        screen_mode = ?screen_mode,
        modal = ?std::mem::discriminant(&modal),
        "key event received"
    );

    if should_suppress_synthetic_enter(suppress_next_enter.get(), &key_event) {
        debug!("suppressing synthetic Enter preceding paste");
        suppress_next_enter.set(false);
        return;
    }

    let current_input_mode = {
        let state = app_state.read();
        input_mode_for_state(&state)
    };
    if should_arm_paste_enter_suppression(&key_event, current_input_mode) {
        suppress_next_enter.set(true);
    } else if should_disarm_paste_enter_suppression(suppress_next_enter.get(), &key_event) {
        suppress_next_enter.set(false);
    }

    if key_event.code == KeyCode::F(12) {
        handle_f12_toggle(app_state, &ctx.cloned());
        return;
    }

    if handle_global_shortcut_key(app_state, &ctx.cloned(), &key_event) {
        return;
    }

    let input_mode = resolve_input_mode(app_state, ctx, term_focused, pane_focus);
    if input_mode == InputMode::TerminalCapture {
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

fn forward_key_to_pty(
    ctx: Option<&CtxArc>,
    suppress_next_enter: &mut HookState<bool>,
    key_event: &KeyEvent,
) {
    let encoded = key_to_bytes(key_event, false);

    trace!(
        code = ?key_event.code,
        modifiers = ?key_event.modifiers,
        encoded_len = encoded.as_ref().map_or(0, std::vec::Vec::len),
        "forwarding key to PTY"
    );

    let unmapped = encoded.is_none();
    if let Some(bytes) = encoded
        && let Some(ctx_arc) = ctx
        && let Ok(mut ctx_guard) = ctx_arc.lock()
    {
        if let Err(e) = ctx_guard.runtime.write_input(&bytes)
            && !matches!(e, RuntimeError::WriteFailed(_))
        {
            warn!(error = %e, "runtime.write_input failed");
        }
    } else if unmapped {
        // Unmapped key: ignore immediately and clear suppression arm.
        suppress_next_enter.set(false);
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
        InputMode::Confirm => {
            handle_mode_confirm_key(app_state, &ctx.cloned(), key_event);
            true
        }
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
        | InputMode::PrsChooser => false,
    }
}

/// Outcome of a background attach/detach operation.
enum AsyncAttachOutcome {
    Attached(AgentId),
    Detached,
    Failed(AgentId),
}

/// Perform attach/detach on a background thread (via `smol::unblock`).
///
/// This locks the `AppContext` mutex and calls the runtime — but on a separate
/// OS thread, so the executor's input/render path is not blocked.
fn perform_async_attach(
    ctx: Arc<std::sync::Mutex<AppContext>>,
    target: Option<AgentId>,
) -> AsyncAttachOutcome {
    let Ok(mut ctx_guard) = ctx.lock() else {
        return match target {
            Some(id) => AsyncAttachOutcome::Failed(id),
            None => AsyncAttachOutcome::Detached,
        };
    };

    if let Some(agent_id) = target.as_ref() {
        debug!(agent_id = %agent_id.0, "background: attaching to running selection");
        match ctx_guard.runtime.attach(agent_id) {
            Ok(()) => AsyncAttachOutcome::Attached(agent_id.clone()),
            Err(error) => {
                warn!(
                    agent_id = %agent_id.0,
                    error = %error,
                    "background: attach failed for running selection"
                );
                let _ = ctx_guard.runtime.mark_session_dead(agent_id);
                AsyncAttachOutcome::Failed(agent_id.clone())
            }
        }
    } else {
        debug!("background: detaching (no running agent selected)");
        let _ = ctx_guard.runtime.detach();
        AsyncAttachOutcome::Detached
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

#[cfg(test)]
mod tests {
    use super::*;
    use jefe::domain::AgentStatus;
    use jefe::state::PaneFocus;

    // --- wants_live_snapshot (issue #160 gate removal) ---
    //
    // The old `should_skip_live_snapshot(status, pane_focus)` gate suppressed
    // Running-agent snapshots when `pane_focus != Terminal`. That gate was
    // removed; `wants_live_snapshot` is the replacement decision function that
    // depends ONLY on agent status, never on pane focus. These tests pin that
    // contract: a Running agent's snapshot is always eligible, so the terminal
    // renders as a read-only preview after restart and during list navigation.

    #[test]
    fn wants_live_snapshot_for_running_regardless_of_pane() {
        // Running agents always get a snapshot attempt. `wants_live_snapshot`
        // does NOT take pane_focus — proving the old focus gate is gone.
        for focus in [
            PaneFocus::Agents,
            PaneFocus::Repositories,
            PaneFocus::Terminal,
        ] {
            let _ = focus; // pane focus is intentionally irrelevant
            assert!(
                wants_live_snapshot(AgentStatus::Running),
                "Running status must always want a live snapshot (pane_focus={focus:?})"
            );
        }
    }

    #[test]
    fn wants_live_snapshot_for_dead() {
        // Dead agents get a one-shot pane capture.
        assert!(
            wants_live_snapshot(AgentStatus::Dead),
            "Dead status must want a snapshot capture"
        );
    }

    #[test]
    fn does_not_want_live_snapshot_for_idle_statuses() {
        for status in [AgentStatus::Queued, AgentStatus::Completed] {
            assert!(
                !wants_live_snapshot(status),
                "{status:?} should not produce a terminal snapshot"
            );
        }
    }

    // --- is_pty_dirty ---

    #[test]
    fn is_pty_dirty_returns_false_without_ctx() {
        assert!(
            !is_pty_dirty(None),
            "is_pty_dirty should be false with no ctx"
        );
    }
}

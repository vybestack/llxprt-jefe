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
    persist_state, to_persisted_state,
};
use crate::pty_encoding::{
    key_to_bytes, mouse_event_to_bytes, should_arm_paste_enter_suppression,
    should_disarm_paste_enter_suppression, should_suppress_synthetic_enter,
};

use jefe::domain::{AgentId, AgentStatus};
use jefe::input::{InputMode, input_mode_for_state};
use jefe::layout::{compute_pty_layout, effective_render_size};
use jefe::runtime::{RuntimeError, RuntimeManager, TerminalSnapshot};
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
    // Track which agent the render loop last attached to, so we only call
    // runtime.attach() when the selection actually changes — not every frame.
    let mut last_attached_key = hooks.use_state(|| Option::<String>::None);
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

    // Poll for PTY output updates (~30fps).
    hooks.use_future({
        let mut render_tick = render_tick;
        async move {
            loop {
                smol::Timer::after(std::time::Duration::from_millis(33)).await;
                let tick = render_tick.get();
                render_tick.set(tick.wrapping_add(1));
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

    // Get theme colors.
    let (theme_name, colors) = if let Some(ctx_arc) = &ctx {
        if let Ok(ctx_guard) = ctx_arc.lock() {
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

    // Attach viewer only for running agents. Keep a running-agent key to avoid
    // stale cross-agent snapshots when selection changes to a dead/non-running agent.
    let selected_running_key = selected_running_agent_id.as_ref().map(|id| id.0.clone());
    let prev_key = last_attached_key.read().clone();
    if prev_key != selected_running_key {
        reattach_running_agent(
            ctx.as_ref(),
            &mut app_state,
            selected_running_agent_id.as_ref(),
        );
        last_attached_key.set(selected_running_key);
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
    let modal_el = build_modal_element(&snapshot, &modal, &colors, confirm_data);

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

type HookState<T> = iocraft::hooks::State<T>;
type CtxArc = Arc<std::sync::Mutex<AppContext>>;

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
        TerminalEvent::Resize(cols, rows) => handle_resize(ctx, cols, rows),
        TerminalEvent::FullscreenMouse(mouse_event) => {
            handle_fullscreen_mouse(ctx, app_state, mouse_event);
        }
        TerminalEvent::Paste(pasted_text) => {
            handle_paste(ctx, app_state, suppress_next_enter, pasted_text);
        }
        TerminalEvent::Key(key_event) => handle_key_event(
            ctx,
            app_state,
            should_quit,
            help_scroll,
            suppress_next_enter,
            key_event,
        ),
        _ => {}
    }
}

fn handle_resize(ctx: Option<&CtxArc>, cols: u16, rows: u16) {
    if let Some(ctx_arc) = ctx
        && let Ok(mut ctx_guard) = ctx_arc.lock()
    {
        let (pty_rows, pty_cols, _, _) = compute_pty_layout(cols, rows);
        let _ = ctx_guard.runtime.resize(pty_rows, pty_cols);
    }
}

fn handle_fullscreen_mouse(
    ctx: Option<&CtxArc>,
    app_state: &HookState<AppState>,
    mouse_event: iocraft::FullscreenMouseEvent,
) {
    let terminal_input_enabled = {
        let state = app_state.read();
        state.terminal_focused && state.pane_focus == PaneFocus::Terminal
    };
    if !terminal_input_enabled {
        return;
    }

    let Some(ctx_arc) = ctx else {
        return;
    };
    let Ok(mut ctx_guard) = ctx_arc.lock() else {
        return;
    };

    if !ctx_guard.runtime.mouse_reporting_active() {
        return;
    }

    let (cols, rows) = crossterm::terminal::size().unwrap_or((120, 40));
    let (pty_rows, pty_cols, pane_col0, pane_row0) = compute_pty_layout(cols, rows);

    let row_end = pane_row0.saturating_add(pty_rows.saturating_sub(1));
    let col_end = pane_col0.saturating_add(pty_cols.saturating_sub(1));

    let screen_row0 = mouse_event.row;
    let screen_col0 = mouse_event.column;

    let in_terminal_bounds = screen_col0 >= pane_col0
        && screen_col0 <= col_end
        && screen_row0 >= pane_row0
        && screen_row0 <= row_end;

    if !in_terminal_bounds {
        return;
    }

    let local_row = screen_row0.saturating_sub(pane_row0);
    let local_col = screen_col0.saturating_sub(pane_col0);

    let mut local_event =
        iocraft::FullscreenMouseEvent::new(mouse_event.kind, local_col, local_row);
    local_event.modifiers = mouse_event.modifiers;

    if let Some(bytes) = mouse_event_to_bytes(&local_event) {
        let _ = ctx_guard.runtime.write_input(&bytes);
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
        | InputMode::IssuesChooser => false,
    }
}

/// Attach the viewer to the selected running agent, or detach when none is selected.
///
/// On attach failure the session is marked dead and focus returns to the agents pane.
fn reattach_running_agent(
    ctx: Option<&CtxArc>,
    app_state: &mut HookState<AppState>,
    selected_running_agent_id: Option<&AgentId>,
) {
    let Some(ctx_arc) = ctx else {
        return;
    };
    let Ok(mut ctx_guard) = ctx_arc.lock() else {
        return;
    };

    if let Some(selected_agent_id) = selected_running_agent_id {
        debug!(agent_id = %selected_agent_id.0, "render-path: attaching to new running selection");
        match ctx_guard.runtime.attach(selected_agent_id) {
            Ok(()) => mark_agent_attached(app_state, selected_agent_id),
            Err(error) => {
                warn!(
                    agent_id = %selected_agent_id.0,
                    error = %error,
                    "render-path: attach failed for running selection"
                );
                handle_attach_failure(&mut ctx_guard, app_state, selected_agent_id);
            }
        }
    } else {
        debug!("render-path: detaching (no running agent selected)");
        let _ = ctx_guard.runtime.detach();
        clear_all_attachments(app_state);
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

fn handle_attach_failure(
    ctx_guard: &mut std::sync::MutexGuard<'_, AppContext>,
    app_state: &mut HookState<AppState>,
    selected_agent_id: &AgentId,
) {
    let _ = ctx_guard.runtime.mark_session_dead(selected_agent_id);
    let mut state = app_state.write();
    state.terminal_focused = false;
    state.pane_focus = PaneFocus::Agents;
    if let Some(agent) = state
        .agents
        .iter_mut()
        .find(|agent| agent.id == *selected_agent_id)
    {
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

/// Capture terminal output for the currently selected agent if available.
fn capture_terminal_snapshot(
    ctx: Option<&CtxArc>,
    snapshot: &AppState,
    selected_agent_id: Option<&AgentId>,
    selected_running_agent_id: Option<&AgentId>,
) -> Option<TerminalSnapshot> {
    let ctx_arc = ctx.as_ref()?;
    let ctx_guard = ctx_arc.lock().ok()?;
    let selected_agent = snapshot.selected_agent()?;
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

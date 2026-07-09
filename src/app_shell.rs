//! Root application component (App) for the Jefe TUI.
//!
//! Houses the iocraft component lifecycle: state hooks, futures,
//! terminal event handling, PTY attachment, and render composition.

use iocraft::prelude::*;
use tracing::{debug, trace, warn};

use crate::AppContext;
use crate::app_input::{
    dispatch_app_event, handle_f12_toggle, handle_global_shortcut_key, handle_mode_confirm_key,
    handle_mode_form_key, handle_mode_help_key, handle_mode_search_key,
    handle_mode_theme_picker_key, handle_normal_key_event, persist_state_snapshot,
    to_persisted_state,
};
use crate::pty_encoding::{
    key_to_bytes, mouse_event_to_bytes, should_arm_paste_enter_suppression,
    should_disarm_paste_enter_suppression, should_suppress_synthetic_enter,
};

use jefe::domain::{AgentId, AgentStatus};
use jefe::input::{InputMode, input_mode_for_state};
use jefe::layout::{compute_pty_layout, effective_render_size};
use jefe::persistence::PersistenceManager;
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
#[allow(clippy::cognitive_complexity)]
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
        let mut render_tick = render_tick.clone();
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
        let mut app_state = app_state.clone();
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
                    // Persist after liveness updates.
                    if let Ok(ctx_guard) = ctx_arc.lock() {
                        if let Err(e) = ctx_guard
                            .persistence
                            .save_state(&to_persisted_state(&state))
                        {
                            warn!(error = %e, "could not save state after liveness update");
                        }
                    }
                }
            }
        }
    });

    // Handle terminal events.
    hooks.use_terminal_events({
        let ctx = ctx.clone();
        let mut app_state = app_state.clone();
        let mut should_quit = should_quit;
        let mut help_scroll = help_scroll;

        #[allow(clippy::cognitive_complexity)]
        move |event| {
            match event {
                TerminalEvent::Resize(cols, rows) => {
                    if let Some(ctx_arc) = &ctx {
                        if let Ok(mut ctx_guard) = ctx_arc.lock() {
                            let (pty_rows, pty_cols, _, _) = compute_pty_layout(cols, rows);
                            let _ = ctx_guard.runtime.resize(pty_rows, pty_cols);
                        }
                    }
                }
                TerminalEvent::FullscreenMouse(mouse_event) => {
                    let state = app_state.read();
                    let terminal_input_enabled =
                        state.terminal_focused && state.pane_focus == PaneFocus::Terminal;
                    drop(state);

                    if !terminal_input_enabled {
                        return;
                    }

                    let Some(ctx_arc) = &ctx else {
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
                TerminalEvent::Paste(pasted_text) => {
                    let input_mode = {
                        let state = app_state.read();
                        input_mode_for_state(&state)
                    };

                    match input_mode {
                        InputMode::TerminalCapture => {
                            let Some(ctx_arc) = &ctx else {
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
                        InputMode::Form | InputMode::Search => {
                            let mut state = app_state.write();
                            for ch in pasted_text.chars().filter(|ch| *ch != '\r' && *ch != '\n') {
                                *state = std::mem::take(&mut *state).apply(AppEvent::FormChar(ch));
                            }
                            persist_state_snapshot(&ctx, &state);
                            suppress_next_enter.set(false);
                        }
                        InputMode::IssuesInline => {
                            let mut state = app_state.write();
                            for ch in pasted_text.chars().filter(|ch| *ch != '\r') {
                                if ch == '\n' {
                                    *state =
                                        std::mem::take(&mut *state).apply(AppEvent::InlineNewline);
                                } else {
                                    *state =
                                        std::mem::take(&mut *state).apply(AppEvent::InlineChar(ch));
                                }
                            }
                            persist_state_snapshot(&ctx, &state);
                            suppress_next_enter.set(false);
                        }
                        InputMode::IssuesSearch => {
                            let mut state = app_state.write();
                            let filtered: String = pasted_text
                                .chars()
                                .filter(|ch| *ch != '\r' && *ch != '\n')
                                .collect();
                            if !filtered.is_empty() {
                                let mut query = state.issues_state.search_query.clone();
                                query.push_str(&filtered);
                                *state = std::mem::take(&mut *state)
                                    .apply(AppEvent::SetSearchQuery { query });
                            }
                            suppress_next_enter.set(false);
                        }
                        _ => {
                            suppress_next_enter.set(false);
                        }
                    }
                }
                TerminalEvent::Key(key_event) => {
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

                    // On some terminals, Cmd/Ctrl+V emits an Enter key event first,
                    // then emits a Paste event. Drop that synthetic Enter.
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
                    } else if should_disarm_paste_enter_suppression(
                        suppress_next_enter.get(),
                        &key_event,
                    ) {
                        suppress_next_enter.set(false);
                    }

                    if key_event.code == KeyCode::F(12) {
                        handle_f12_toggle(&mut app_state, &ctx);
                        return;
                    }

                    // Global per-agent shortcut jump (works even in terminal capture mode).
                    if handle_global_shortcut_key(&mut app_state, &ctx, &key_event) {
                        return;
                    }

                    // Determine active input mode from current state.
                    let input_mode = if term_focused && pane_focus != PaneFocus::Terminal {
                        // Defensive guard: terminal input is only valid when terminal pane is active.
                        // If focus state is stale, clear it so navigation keys never leak into llxprt.
                        debug!(
                            pane_focus = ?pane_focus,
                            "clearing stale terminal_focused (pane not Terminal)"
                        );
                        let mut state = app_state.write();
                        state.terminal_focused = false;
                        persist_state_snapshot(&ctx, &state);
                        InputMode::Normal
                    } else {
                        let state = app_state.read();
                        input_mode_for_state(&state)
                    };

                    // When terminal input is focused, forward keys to PTY.
                    if input_mode == InputMode::TerminalCapture {
                        let encoded = key_to_bytes(&key_event, false);

                        trace!(
                            code = ?key_event.code,
                            modifiers = ?key_event.modifiers,
                            encoded_len = encoded.as_ref().map_or(0, std::vec::Vec::len),
                            "forwarding key to PTY"
                        );

                        if let Some(bytes) = encoded {
                            if let Some(ctx_arc) = &ctx {
                                if let Ok(mut ctx_guard) = ctx_arc.lock() {
                                    if let Err(e) = ctx_guard.runtime.write_input(&bytes) {
                                        if !matches!(e, RuntimeError::WriteFailed(_)) {
                                            warn!(error = %e, "runtime.write_input failed");
                                        }
                                    }
                                }
                            }
                        } else {
                            // Unmapped key: ignore immediately and clear suppression arm.
                            suppress_next_enter.set(false);
                        }
                        return;
                    }

                    // Handle mode-specific keys first.
                    match input_mode {
                        InputMode::Help => {
                            handle_mode_help_key(
                                &mut app_state,
                                &ctx,
                                &mut help_scroll,
                                &key_event,
                            );
                            return;
                        }
                        InputMode::Confirm => {
                            handle_mode_confirm_key(&mut app_state, &ctx, &key_event);
                            return;
                        }
                        InputMode::ThemePicker => {
                            handle_mode_theme_picker_key(&mut app_state, &ctx, &key_event);
                            return;
                        }
                        InputMode::Search => {
                            if handle_mode_search_key(&mut app_state, &ctx, &key_event) {
                                return;
                            }
                        }
                        InputMode::Form => {
                            if handle_mode_form_key(&mut app_state, &ctx, &key_event) {
                                return;
                            }
                        }
                        // @plan PLAN-20260329-ISSUES-MODE.P03
                        InputMode::TerminalCapture
                        | InputMode::Normal
                        | InputMode::IssuesNormal
                        | InputMode::IssuesInline
                        | InputMode::IssuesSearch
                        | InputMode::IssuesFilter
                        | InputMode::IssuesChooser => {}
                    }

                    if let Some(evt) = handle_normal_key_event(
                        &mut app_state,
                        &mut should_quit,
                        &ctx,
                        &key_event,
                        screen_mode,
                    ) {
                        dispatch_app_event(&mut app_state, &ctx, evt);
                    }
                }
                _ => {}
            }
        }
    });

    // Handle quit.
    if should_quit.get() {
        // Save state before exiting.
        let state = app_state.read();
        persist_state_snapshot(&ctx, &state);

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
        if let Some(ctx_arc) = &ctx
            && let Ok(mut ctx_guard) = ctx_arc.lock()
        {
            if let Some(ref selected_agent_id) = selected_running_agent_id {
                debug!(agent_id = %selected_agent_id.0, "render-path: attaching to new running selection");
                match ctx_guard.runtime.attach(selected_agent_id) {
                    Ok(()) => {
                        let mut state = app_state.write();
                        for agent in &mut state.agents {
                            if let Some(binding) = agent.runtime_binding.as_mut() {
                                binding.attached = agent.id == *selected_agent_id;
                            }
                        }
                    }
                    Err(error) => {
                        warn!(
                            agent_id = %selected_agent_id.0,
                            error = %error,
                            "render-path: attach failed for running selection"
                        );
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
                }
            } else {
                debug!("render-path: detaching (no running agent selected)");
                let _ = ctx_guard.runtime.detach();
                let mut state = app_state.write();
                for agent in &mut state.agents {
                    if let Some(binding) = agent.runtime_binding.as_mut() {
                        binding.attached = false;
                    }
                }
            }
        }
        last_attached_key.set(selected_running_key);
    }

    // Render snapshot rules:
    //  - Running selected agent: live viewer snapshot (guarded by attachment match).
    //  - Dead selected agent: captured dead pane output for same agent only.
    //  - Other states: no terminal content.
    let terminal_snapshot: Option<TerminalSnapshot> = if let Some(ctx_arc) = &ctx {
        if let Ok(ctx_guard) = ctx_arc.lock() {
            if let Some(selected_agent) = snapshot.selected_agent() {
                match selected_agent.status {
                    AgentStatus::Running => {
                        if let Some(selected_agent_id) = selected_running_agent_id.as_ref() {
                            if ctx_guard.runtime.attached_agent() == Some(selected_agent_id) {
                                ctx_guard.runtime.snapshot()
                            } else {
                                None
                            }
                        } else {
                            None
                        }
                    }
                    AgentStatus::Dead => selected_agent_id.as_ref().and_then(|agent_id| {
                        snapshot
                            .repository_for_agent(agent_id)
                            .filter(|repository| !repository.remote.enabled)
                            .and_then(|_| ctx_guard.runtime.capture_session_output(agent_id))
                    }),
                    _ => None,
                }
            } else {
                None
            }
        } else {
            None
        }
    } else {
        None
    };

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

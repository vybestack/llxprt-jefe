//! Jefe - Terminal application for managing multiple llxprt coding agents.
//!
//! @plan PLAN-20260216-FIRSTVERSION-V1
//! @requirement REQ-TECH-001

#![allow(clippy::print_stderr)]
#![allow(clippy::collapsible_if)]
#![allow(clippy::clone_on_copy)]
#![allow(clippy::significant_drop_tightening)]

use std::sync::Arc;

use iocraft::prelude::*;

use jefe::domain::{AgentId, AgentStatus, LaunchSignature, RepositoryId};
use jefe::persistence::{FilePersistenceManager, PersistenceManager, Settings, State};
use jefe::runtime::{RuntimeError, RuntimeManager, TerminalSnapshot, TmuxRuntimeManager};
use jefe::state::{AppEvent, AppState, ModalState, PaneFocus, ScreenMode};
use jefe::theme::{FileThemeManager, ThemeColors, ThemeManager};
use jefe::ui::{ConfirmModal, Dashboard, HelpModal, NewAgentForm, NewRepositoryForm, SplitScreen};

/// Check if fullscreen mode is enabled.
fn is_fullscreen_enabled() -> bool {
    std::env::var("JEFE_WINDOWED").ok().as_deref() != Some("1")
}

/// Layout constants (mirrors dashboard component proportions).
const LEFT_COL_WIDTH: u16 = 22;
const RIGHT_COL_WIDTH: u16 = 36;
const OUTER_BARS_HEIGHT: u16 = 2; // status + keybind
const TERMINAL_WIDGET_CHROME_ROWS: u16 = 3; // top border + header row + bottom border
const TERMINAL_WIDGET_CHROME_COLS: u16 = 2; // left + right border

/// Calculate effective render dimensions.
fn effective_render_size(cols: u16, rows: u16) -> (u16, u16) {
    if is_fullscreen_enabled() {
        (cols, rows)
    } else {
        (cols.saturating_sub(2).max(1), rows.saturating_sub(2).max(1))
    }
}

/// Compute PTY viewport size and its origin within the fullscreen render grid.
///
/// Layout mirrors dashboard proportions:
/// - top status bar (1 row)
/// - bottom keybind bar (1 row)
/// - middle column split: agent list 25%, terminal 75%
/// - terminal widget chrome: border + header + border
fn compute_pty_layout(term_cols: u16, term_rows: u16) -> (u16, u16, u16, u16) {
    let (render_cols, render_rows) = effective_render_size(term_cols, term_rows);

    let content_rows = render_rows.saturating_sub(OUTER_BARS_HEIGHT);
    let middle_cols = render_cols.saturating_sub(LEFT_COL_WIDTH + RIGHT_COL_WIDTH);

    let agent_rows = content_rows.saturating_mul(25).saturating_div(100);
    let terminal_slot_rows = content_rows.saturating_sub(agent_rows);

    let pty_rows = terminal_slot_rows
        .saturating_sub(TERMINAL_WIDGET_CHROME_ROWS)
        .max(2);
    let pty_cols = middle_cols
        .saturating_sub(TERMINAL_WIDGET_CHROME_COLS)
        .max(2);

    let pane_col0 = LEFT_COL_WIDTH.saturating_add(1);
    let pane_row0 = 1u16.saturating_add(agent_rows).saturating_add(2);

    (pty_rows, pty_cols, pane_col0, pane_row0)
}

/// Shared application context passed to the root component.
struct AppContext {
    persistence: FilePersistenceManager,
    theme_manager: FileThemeManager,
    runtime: TmuxRuntimeManager,
}

fn to_persisted_state(state: &AppState) -> State {
    State {
        schema_version: jefe::persistence::STATE_SCHEMA_VERSION,
        repositories: state.repositories.clone(),
        agents: state.agents.clone(),
        selected_repository_index: state.selected_repository_index,
        selected_agent_index: state.selected_agent_index,
    }
}

fn persist_state_snapshot(ctx: &Option<Arc<std::sync::Mutex<AppContext>>>, state: &AppState) {
    if let Some(ctx_arc) = ctx
        && let Ok(ctx_guard) = ctx_arc.lock()
        && let Err(e) = ctx_guard.persistence.save_state(&to_persisted_state(state))
    {
        eprintln!("Warning: Could not save state: {e}");
    }
}

/// Delete the currently selected repository from state.
fn delete_selected_repository(state: &mut AppState, repository_id: &RepositoryId) {
    if let Some(repo_idx) = state
        .repositories
        .iter()
        .position(|r| &r.id == repository_id)
    {
        state.repositories.remove(repo_idx);

        // Remove all agents belonging to the deleted repository.
        state
            .agents
            .retain(|agent| &agent.repository_id != repository_id);

        if state.repositories.is_empty() {
            state.selected_repository_index = None;
            state.selected_agent_index = None;
            state.pane_focus = PaneFocus::Repositories;
            state.rebuild_repository_agent_ids();
            state.normalize_selection_indices();
            return;
        }

        let next_repo_idx = repo_idx.min(state.repositories.len().saturating_sub(1));
        state.selected_repository_index = Some(next_repo_idx);

        let selected_repo_id = state.repositories[next_repo_idx].id.clone();
        state.selected_agent_index = state
            .agents
            .iter()
            .enumerate()
            .find_map(|(idx, agent)| (agent.repository_id == selected_repo_id).then_some(idx));

        if state.selected_agent_index.is_none() {
            state.pane_focus = PaneFocus::Repositories;
            state.terminal_focused = false;
        }

        state.rebuild_repository_agent_ids();
        state.normalize_selection_indices();
    }
}

/// Delete a selected agent from state and optionally remove its working directory.
fn delete_selected_agent(
    state: &mut AppState,
    agent_id: &AgentId,
    delete_work_dir: bool,
) -> Option<AgentId> {
    let agent_idx = state.agents.iter().position(|a| &a.id == agent_id)?;

    let removed_agent = state.agents.remove(agent_idx);
    if delete_work_dir {
        if removed_agent.work_dir.exists()
            && let Err(e) = std::fs::remove_dir_all(&removed_agent.work_dir)
        {
            eprintln!(
                "Warning: Could not remove work directory {}: {e}",
                removed_agent.work_dir.display()
            );
        }
    }

    let selected_repo_id = state
        .selected_repository_index
        .and_then(|idx| state.repositories.get(idx).map(|r| r.id.clone()));

    state.selected_agent_index = selected_repo_id.as_ref().and_then(|repo_id| {
        state
            .agents
            .iter()
            .enumerate()
            .find_map(|(idx, agent)| (&agent.repository_id == repo_id).then_some(idx))
    });

    if state.selected_agent_index.is_none() {
        state.pane_focus = PaneFocus::Repositories;
        state.terminal_focused = false;
    }

    state.rebuild_repository_agent_ids();
    state.normalize_selection_indices();

    Some(removed_agent.id)
}

/// Props for the root app component.
#[derive(Default, Props)]
struct AppProps {
    context: Option<Arc<std::sync::Mutex<AppContext>>>,
}

/// Root application component that manages state and renders the UI.
#[component]
fn App(mut hooks: Hooks, props: &AppProps) -> impl Into<AnyElement<'static>> {
    let should_quit = hooks.use_state(|| false);
    let mut app_state = hooks.use_state(AppState::default);
    let render_tick = hooks.use_state(|| 0u64);
    let help_scroll = hooks.use_state(|| 0u32);
    let mut initialized = hooks.use_state(|| false);

    let ctx = props.context.clone();

    // One-time initialization: load persisted state.
    if !initialized.get() {
        initialized.set(true);
        if let Some(ref ctx_arc) = ctx {
            if let Ok(ctx_guard) = ctx_arc.lock() {
                // Load settings
                let settings = ctx_guard.persistence.load_settings().unwrap_or_else(|e| {
                    eprintln!("Warning: Could not load settings: {e}");
                    Settings::default_with_version()
                });

                // Load state
                let persisted = ctx_guard.persistence.load_state().unwrap_or_else(|e| {
                    eprintln!("Warning: Could not load state: {e}");
                    State::default_with_version()
                });

                // Apply to app state
                let mut state = app_state.write();
                state.repositories = persisted.repositories;
                state.agents = persisted.agents;
                state.selected_repository_index = persisted.selected_repository_index;
                state.selected_agent_index = persisted.selected_agent_index;
                state.terminal_focused = false;
                state.rebuild_repository_agent_ids();
                state.normalize_selection_indices();

                // Set theme
                drop(ctx_guard);
                if let Ok(mut ctx_mut) = ctx_arc.lock() {
                    let _ = ctx_mut.theme_manager.set_active(&settings.theme);
                }
            }
        }
    }

    // Spawn tmux sessions for agents on first render.
    hooks.use_future({
        let ctx = ctx.clone();
        let app_state = app_state.clone();
        async move {
            if let Some(ref ctx_arc) = ctx {
                let agents: Vec<_> = {
                    let state = app_state.read();
                    state.agents.clone()
                };

                for agent in agents {
                    let signature = LaunchSignature {
                        work_dir: agent.work_dir.clone(),
                        profile: agent.profile.clone(),
                        mode_flags: agent.mode_flags.clone(),
                        pass_continue: agent.pass_continue,
                    };
                    if let Ok(mut ctx_guard) = ctx_arc.lock() {
                        match ctx_guard.runtime.spawn_session(
                            &agent.id,
                            &agent.work_dir,
                            &signature,
                        ) {
                            Ok(()) | Err(RuntimeError::AlreadyRunning(_)) => {
                                // Existing sessions from a previous run are fine.
                            }
                            Err(e) => {
                                eprintln!(
                                    "Warning: Could not spawn session for {}: {e}",
                                    agent.id.0
                                );
                            }
                        }
                    }
                }
            }
        }
    });

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

    // Handle terminal events.
    hooks.use_terminal_events({
        let ctx = ctx.clone();
        let mut app_state = app_state.clone();
        let mut should_quit = should_quit;
        let mut help_scroll = help_scroll;

        move |event| {
            match event {
            TerminalEvent::Resize(cols, rows) => {
                if let Some(ref ctx_arc) = ctx {
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

                let Some(ref ctx_arc) = ctx else {
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

                // F12 toggles terminal input focus.
                // When enabling, force pane focus to terminal and require attach success.
                if key_event.code == KeyCode::F(12) {
                    let (enabling_focus, selected_agent_id) = {
                        let mut state = app_state.write();

                        if state.terminal_focused {
                            *state = std::mem::take(&mut *state).apply(AppEvent::ToggleTerminalFocus);
                            (false, None)
                        } else {
                            state.pane_focus = PaneFocus::Terminal;
                            *state = std::mem::take(&mut *state).apply(AppEvent::ToggleTerminalFocus);
                            (true, state.selected_agent().map(|agent| agent.id.clone()))
                        }
                    };

                    if enabling_focus {
                        let attached = selected_agent_id.as_ref().is_some_and(|agent_id| {
                            if let Some(ref ctx_arc) = ctx {
                                if let Ok(mut ctx_guard) = ctx_arc.lock() {
                                    match ctx_guard.runtime.attach(agent_id) {
                                        Ok(()) => true,
                                        Err(e) => {
                                            eprintln!(
                                                "Warning: Could not attach session for {} on F12 focus: {e}",
                                                agent_id.0
                                            );
                                            false
                                        }
                                    }
                                } else {
                                    false
                                }
                            } else {
                                false
                            }
                        });

                        if !attached {
                            let mut state = app_state.write();
                            state.terminal_focused = false;
                        }
                    }

                    let state = app_state.read();
                    persist_state_snapshot(&ctx, &state);
                    return;
                }

                // Defensive guard: terminal input is only valid when the terminal pane is active.
                // If focus state is stale, clear it so navigation keys never leak into llxprt.
                let terminal_input_enabled = term_focused && pane_focus == PaneFocus::Terminal;
                if term_focused && pane_focus != PaneFocus::Terminal {
                    let mut state = app_state.write();
                    state.terminal_focused = false;
                    persist_state_snapshot(&ctx, &state);
                }


                // When terminal input is focused, forward keys to PTY.
                if terminal_input_enabled {
                    if let Some(bytes) = key_to_bytes(&key_event) {
                        if let Some(ref ctx_arc) = ctx {
                            if let Ok(mut ctx_guard) = ctx_arc.lock() {
                                let _ = ctx_guard.runtime.write_input(&bytes);
                            }
                        }
                    }
                    return;
                }

                // Handle modal keys.
                match &modal {
                    ModalState::Help => {
                        match key_event.code {
                            KeyCode::Esc | KeyCode::Char('?') => {
                                let mut state = app_state.write();
                                *state = std::mem::take(&mut *state).apply(AppEvent::CloseModal);
                                persist_state_snapshot(&ctx, &state);
                            }
                            KeyCode::Up => {
                                let offset = help_scroll.get();
                                if offset > 0 {
                                    help_scroll.set(offset - 1);
                                }
                            }
                            KeyCode::Down => {
                                help_scroll.set(help_scroll.get() + 1);
                            }
                            _ => {}
                        }
                        return;
                    }
                    ModalState::ConfirmDeleteRepository { .. }
                    | ModalState::ConfirmDeleteAgent { .. } => {
                        match key_event.code {
                            KeyCode::Esc => {
                                let mut state = app_state.write();
                                *state = std::mem::take(&mut *state).apply(AppEvent::CloseModal);
                                persist_state_snapshot(&ctx, &state);
                            }
                            KeyCode::Enter => {
                                let modal_snapshot = {
                                    let state = app_state.read();
                                    state.modal.clone()
                                };

                                match modal_snapshot {
                                    ModalState::ConfirmDeleteAgent {
                                        id,
                                        delete_work_dir,
                                    } => {
                                        if let Some(ref ctx_arc) = ctx {
                                            if let Ok(mut ctx_guard) = ctx_arc.lock() {
                                                if let Err(e) = ctx_guard.runtime.kill(&id) {
                                                    match e {
                                                        RuntimeError::SessionNotFound(_) => {}
                                                        _ => {
                                                            eprintln!(
                                                                "Warning: Could not kill runtime session for {} before delete: {e}",
                                                                id.0
                                                            );
                                                        }
                                                    }
                                                }
                                            }
                                        }

                                        let mut state = app_state.write();
                                        let _ = delete_selected_agent(&mut state, &id, delete_work_dir);
                                        state.modal = ModalState::None;
                                        persist_state_snapshot(&ctx, &state);
                                    }
                                    ModalState::ConfirmDeleteRepository { id } => {
                                        if let Some(ref ctx_arc) = ctx {
                                            if let Ok(mut ctx_guard) = ctx_arc.lock() {
                                                let agent_ids: Vec<AgentId> = {
                                                    let state = app_state.read();
                                                    state
                                                        .agents
                                                        .iter()
                                                        .filter(|agent| agent.repository_id == id)
                                                        .map(|agent| agent.id.clone())
                                                        .collect()
                                                };

                                                for agent_id in &agent_ids {
                                                    if let Err(e) = ctx_guard.runtime.kill(agent_id) {
                                                        match e {
                                                            RuntimeError::SessionNotFound(_) => {}
                                                            _ => {
                                                                eprintln!(
                                                                    "Warning: Could not kill runtime session for {} before repository delete: {e}",
                                                                    agent_id.0
                                                                );
                                                            }
                                                        }
                                                    }
                                                }
                                            }
                                        }

                                        let mut state = app_state.write();
                                        delete_selected_repository(&mut state, &id);
                                        state.modal = ModalState::None;
                                        persist_state_snapshot(&ctx, &state);
                                    }
                                    _ => {}
                                }
                            }
                            KeyCode::Char(' ') | KeyCode::Char('d') | KeyCode::Char('D')
                            | KeyCode::Up
                            | KeyCode::Down => {
                                let mut state = app_state.write();
                                *state = std::mem::take(&mut *state)
                                    .apply(AppEvent::ToggleDeleteWorkDir);
                                persist_state_snapshot(&ctx, &state);
                            }
                            _ => {}
                        }
                        return;
                    }

                    ModalState::Search { .. } => {
                        if key_event.code == KeyCode::Esc {
                            let mut state = app_state.write();
                            *state = std::mem::take(&mut *state).apply(AppEvent::CloseModal);
                            persist_state_snapshot(&ctx, &state);
                        }
                        return;
                    }
                    ModalState::NewRepository { .. }
                    | ModalState::EditRepository { .. }
                    | ModalState::NewAgent { .. }
                    | ModalState::EditAgent { .. } => {
                        // Form field navigation and input
                        let app_event = match key_event.code {
                            KeyCode::Esc => Some(AppEvent::CloseModal),
                            KeyCode::Enter => {
                                // Submit form and spawn PTY if new agent
                                let state_ro = app_state.read();
                                let is_new_agent = matches!(state_ro.modal, ModalState::NewAgent { .. });
                                drop(state_ro);

                                let mut state = app_state.write();
                                *state = std::mem::take(&mut *state).apply(AppEvent::SubmitForm);
                                persist_state_snapshot(&ctx, &state);

                                // If new agent was created, spawn session and attach viewer.
                                if is_new_agent && state.modal == ModalState::None {
                                    if let Some(agent) = state.selected_agent().cloned() {
                                        let agent_id = agent.id.clone();
                                        let work_dir = agent.work_dir.clone();
                                        let signature = LaunchSignature {
                                            work_dir: agent.work_dir.clone(),
                                            profile: agent.profile.clone(),
                                            mode_flags: agent.mode_flags.clone(),
                                            pass_continue: agent.pass_continue,
                                        };
                                        // Match toy1 behavior: new agent opens attached and focused.
                                        state.terminal_focused = true;
                                        persist_state_snapshot(&ctx, &state);
                                        drop(state);

                                        if let Some(ref ctx_arc) = ctx {
                                            if let Ok(mut ctx_guard) = ctx_arc.lock() {
                                                if let Err(e) = ctx_guard.runtime.spawn_session(
                                                    &agent_id,
                                                    &work_dir,
                                                    &signature,
                                                ) {
                                                    eprintln!("Warning: Could not spawn session: {e}");
                                                } else if let Err(e) = ctx_guard.runtime.attach(&agent_id) {
                                                    eprintln!("Warning: Could not attach session for {}: {e}", agent_id.0);
                                                }
                                            }
                                        }
                                    }
                                }
                                return;
                            }
                            KeyCode::Tab | KeyCode::Down => Some(AppEvent::FormNextField),
                            KeyCode::BackTab | KeyCode::Up => Some(AppEvent::FormPrevField),
                            KeyCode::Backspace => Some(AppEvent::FormBackspace),
                            // Space: toggle checkbox only on checkbox fields, otherwise type space
                            KeyCode::Char(' ') => Some(AppEvent::FormChar(' ')),
                            KeyCode::Char(c) => Some(AppEvent::FormChar(c)),
                            _ => None,
                        };
                        if let Some(evt) = app_event {
                            let mut state = app_state.write();
                            *state = std::mem::take(&mut *state).apply(evt);
                            persist_state_snapshot(&ctx, &state);
                        }
                        return;
                    }
                    _ => {}
                }

                // Get additional state for keybinding decisions.
                let state_ro = app_state.read();
                let pane_focus = state_ro.pane_focus;
                let selected_repo_id = state_ro
                    .selected_repository_index
                    .and_then(|i| state_ro.repositories.get(i).map(|r| r.id.clone()));
                let selected_agent_id = state_ro.selected_agent().map(|agent| agent.id.clone());
                drop(state_ro);

                // Normal keybindings.
                let app_event = match key_event.code {
                    // Quit
                    KeyCode::Char('q' | 'Q') => {
                        should_quit.set(true);
                        return;
                    }

                    // Navigation
                    KeyCode::Up => Some(AppEvent::NavigateUp),
                    KeyCode::Down => Some(AppEvent::NavigateDown),
                    KeyCode::Left => Some(AppEvent::NavigateLeft),
                    KeyCode::Right => Some(AppEvent::NavigateRight),
                    KeyCode::Tab => Some(AppEvent::CyclePaneFocus),

                    // New (n = new agent, N = new repository)
                    KeyCode::Char('n') => {
                        // If no repo is selected but repos exist, auto-select the first one
                        let repo_id = selected_repo_id.clone().or_else(|| {
                            let state = app_state.read();
                            if state.repositories.is_empty() {
                                None
                            } else {
                                let first_id = state.repositories[0].id.clone();
                                drop(state);
                                let mut state_mut = app_state.write();
                                state_mut.selected_repository_index = Some(0);
                                state_mut.normalize_selection_indices();
                                persist_state_snapshot(&ctx, &state_mut);
                                Some(first_id)
                            }
                        });
                        if repo_id.is_none() {
                            Some(AppEvent::OpenNewRepository)
                        } else {
                            repo_id.map(AppEvent::OpenNewAgent)
                        }
                    }
                    KeyCode::Char('N') => Some(AppEvent::OpenNewRepository),

                    // Delete
                    KeyCode::Char('d' | 'D')
                        if key_event.modifiers.contains(KeyModifiers::CONTROL) =>
                    {
                        if pane_focus == PaneFocus::Agents || pane_focus == PaneFocus::Terminal {
                            selected_agent_id.clone().map(AppEvent::OpenDeleteAgent)
                        } else if pane_focus == PaneFocus::Repositories {
                            selected_repo_id.clone().map(AppEvent::OpenDeleteRepository)
                        } else {
                            None
                        }
                    }

                    // Kill agent
                    KeyCode::Char('k' | 'K')
                        if key_event.modifiers.contains(KeyModifiers::CONTROL) =>
                    {
                        selected_agent_id.clone().map(AppEvent::KillAgent)
                    }

                    // Relaunch agent
                    KeyCode::Char('l' | 'L') => {
                        selected_agent_id.clone().map(AppEvent::RelaunchAgent)
                    }

                    // Split mode
                    KeyCode::Char('s' | 'S') if screen_mode == ScreenMode::Dashboard => {
                        Some(AppEvent::EnterSplitMode)
                    }
                    KeyCode::Esc if screen_mode == ScreenMode::Split => {
                        Some(AppEvent::ExitSplitMode)
                    }

                    // Grab mode (in split screen)
                    KeyCode::Char('g' | 'G') if screen_mode == ScreenMode::Split => {
                        Some(AppEvent::EnterGrabMode)
                    }

                    // Help and search
                    KeyCode::Char('?' | 'h' | 'H') | KeyCode::F(1) => Some(AppEvent::OpenHelp),
                    KeyCode::Char('/') => Some(AppEvent::OpenSearch),

                    // Direct pane focus
                    KeyCode::Char('r' | 'R') => {
                        let mut state = app_state.write();
                        state.pane_focus = PaneFocus::Repositories;
                        persist_state_snapshot(&ctx, &state);
                        None
                    }
                    KeyCode::Char('a' | 'A') => {
                        let mut state = app_state.write();
                        state.pane_focus = PaneFocus::Agents;
                        persist_state_snapshot(&ctx, &state);
                        None
                    }
                    KeyCode::Char('t' | 'T') => {
                        let selected_agent_id = {
                            let mut state = app_state.write();
                            state.pane_focus = PaneFocus::Terminal;
                            if !state.terminal_focused {
                                *state =
                                    std::mem::take(&mut *state).apply(AppEvent::ToggleTerminalFocus);
                            }
                            state.selected_agent().map(|agent| agent.id.clone())
                        };

                        if let Some(agent_id) = selected_agent_id {
                            if let Some(ref ctx_arc) = ctx {
                                if let Ok(mut ctx_guard) = ctx_arc.lock() {
                                    if let Err(e) = ctx_guard.runtime.attach(&agent_id) {
                                        eprintln!(
                                            "Warning: Could not attach session for {}: {e}",
                                            agent_id.0
                                        );
                                        let mut state = app_state.write();
                                        state.terminal_focused = false;
                                        persist_state_snapshot(&ctx, &state);
                                    }
                                }
                            }
                        } else {
                            let mut state = app_state.write();
                            state.terminal_focused = false;
                            persist_state_snapshot(&ctx, &state);
                        }

                        None
                    }

                    // Enter selects current item (edit agent/repo)
                    KeyCode::Enter => {
                        match pane_focus {
                            PaneFocus::Agents => selected_agent_id.clone().map(AppEvent::OpenEditAgent),
                            PaneFocus::Repositories => selected_repo_id.clone().map(AppEvent::OpenEditRepository),
                            PaneFocus::Terminal => {
                                // Toggle terminal focus on Enter when in terminal pane
                                Some(AppEvent::ToggleTerminalFocus)
                            }
                        }
                    }

                    // Theme switching (1/2/3)
                    KeyCode::Char('1') => {
                        if let Some(ref ctx_arc) = ctx {
                            if let Ok(mut ctx_guard) = ctx_arc.lock() {
                                let _ = ctx_guard.theme_manager.set_active("green-screen");
                            }
                        }
                        None
                    }
                    KeyCode::Char('2') => {
                        if let Some(ref ctx_arc) = ctx {
                            if let Ok(mut ctx_guard) = ctx_arc.lock() {
                                let _ = ctx_guard.theme_manager.set_active("dracula");
                            }
                        }
                        None
                    }
                    KeyCode::Char('3') => {
                        if let Some(ref ctx_arc) = ctx {
                            if let Ok(mut ctx_guard) = ctx_arc.lock() {
                                let _ = ctx_guard.theme_manager.set_active("default-dark");
                            }
                        }
                        None
                    }

                    _ => None,
                };

                if let Some(evt) = app_event {
                    match evt {
                        AppEvent::ToggleTerminalFocus => {
                            // Keep Enter-in-terminal-pane as a UI focus toggle only.
                            // Runtime attach/detach remains bound to F12.
                            let mut state = app_state.write();
                            *state = std::mem::take(&mut *state).apply(AppEvent::ToggleTerminalFocus);
                            persist_state_snapshot(&ctx, &state);
                        }
                        AppEvent::KillAgent(ref agent_id) => {
                            if let Some(ref ctx_arc) = ctx {
                                if let Ok(mut ctx_guard) = ctx_arc.lock() {
                                    if let Err(e) = ctx_guard.runtime.kill(agent_id) {
                                        eprintln!("Warning: Could not kill runtime session for {}: {e}", agent_id.0);
                                    }
                                }
                            }
                            let mut state = app_state.write();
                            *state = std::mem::take(&mut *state).apply(evt);
                            state.terminal_focused = false;
                            persist_state_snapshot(&ctx, &state);
                        }
                        AppEvent::RelaunchAgent(ref agent_id) => {
                            let mut relaunched = false;
                            if let Some(ref ctx_arc) = ctx {
                                if let Ok(mut ctx_guard) = ctx_arc.lock() {
                                    match ctx_guard.runtime.relaunch(agent_id) {
                                        Ok(()) => {
                                            relaunched = true;
                                        }
                                        Err(RuntimeError::NotRunning(_)) => {
                                            // If not in dead-signatures map (e.g. app restart), fallback to spawn.
                                            let state_ro = app_state.read();
                                            if let Some(agent) = state_ro.agents.iter().find(|a| a.id == *agent_id) {
                                                let signature = LaunchSignature {
                                                    work_dir: agent.work_dir.clone(),
                                                    profile: agent.profile.clone(),
                                                    mode_flags: agent.mode_flags.clone(),
                                                    pass_continue: agent.pass_continue,
                                                };
                                                match ctx_guard.runtime.spawn_session(agent_id, &agent.work_dir, &signature) {
                                                    Ok(()) => {
                                                        relaunched = true;
                                                    }
                                                    Err(e2) => {
                                                        eprintln!("Warning: Could not relaunch runtime session for {}: {e2}", agent_id.0);
                                                    }
                                                }
                                            }
                                        }
                                        Err(e) => {
                                            eprintln!("Warning: Could not relaunch runtime session for {}: {e}", agent_id.0);
                                        }
                                    }

                                    if relaunched {
                                        // Relaunch should make output visible immediately; focus remains separate.
                                        if let Err(e) = ctx_guard.runtime.attach(agent_id) {
                                            eprintln!("Warning: Could not attach relaunched session for {}: {e}", agent_id.0);
                                        }
                                    }
                                }
                            }

                            let mut state = app_state.write();
                            *state = std::mem::take(&mut *state).apply(evt);
                            if relaunched {
                                state.terminal_focused = false;
                            }
                            persist_state_snapshot(&ctx, &state);
                        }
                        _ => {
                            let mut state = app_state.write();
                            *state = std::mem::take(&mut *state).apply(evt);
                            persist_state_snapshot(&ctx, &state);
                        }
                    }
                }
            }
            _ => {}
        }}
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

    // Update agent liveness from runtime.
    if let Some(ref ctx_arc) = ctx {
        if let Ok(ctx_guard) = ctx_arc.lock() {
            let dead_agents: Vec<AgentId> = {
                let state = app_state.read();
                state
                    .agents
                    .iter()
                    .filter(|a| a.is_running() && !ctx_guard.runtime.is_alive(&a.id))
                    .map(|a| a.id.clone())
                    .collect()
            };
            drop(ctx_guard);

            if !dead_agents.is_empty() {
                let mut state = app_state.write();
                for agent_id in dead_agents {
                    *state = std::mem::take(&mut *state)
                        .apply(AppEvent::AgentStatusChanged(agent_id, AgentStatus::Dead));
                }
                persist_state_snapshot(&ctx, &state);
            }
        }
    }

    // Read state for rendering.
    let state = app_state.read();
    let screen_mode = state.screen_mode;
    let modal = state.modal.clone();
    let snapshot: AppState = (*state).clone();
    drop(state);

    // Get theme colors.
    let (theme_name, colors) = if let Some(ref ctx_arc) = ctx {
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

    // Ensure selected agent is attached for rendering (separate from input focus).
    if let Some(selected_agent_id) = snapshot.selected_agent().map(|agent| agent.id.clone())
        && let Some(ref ctx_arc) = ctx
        && let Ok(mut ctx_guard) = ctx_arc.lock()
    {
        let need_attach = ctx_guard
            .runtime
            .attached_agent()
            .is_none_or(|attached| attached != &selected_agent_id);
        if need_attach {
            let _ = ctx_guard.runtime.attach(&selected_agent_id);
        }
    }

    // Get terminal snapshot from currently attached viewer.
    let terminal_snapshot: Option<TerminalSnapshot> = if let Some(ref ctx_arc) = ctx {
        if let Ok(ctx_guard) = ctx_arc.lock() {
            ctx_guard.runtime.snapshot()
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

    // Build screen element.
    let screen_el: AnyElement<'static> = match screen_mode {
        ScreenMode::Dashboard => element! {
            Dashboard(
                state: Some(snapshot.clone()),
                colors: Some(colors.clone()),
                theme_name: theme_name.clone(),
                terminal_snapshot: terminal_snapshot,
            )
        }
        .into_any(),
        ScreenMode::Split => element! {
            SplitScreen(
                state: Some(snapshot.clone()),
                colors: Some(colors.clone()),
                theme_name: theme_name.clone(),
            )
        }
        .into_any(),
    };

    let confirm_modal_data = match &modal {
        ModalState::ConfirmDeleteAgent {
            id,
            delete_work_dir,
        } => {
            let agent_name = snapshot
                .agents
                .iter()
                .find(|agent| &agent.id == id)
                .map(|agent| agent.name.clone())
                .unwrap_or_else(|| String::from("selected agent"));
            Some((
                String::from("Delete Agent"),
                format!("Delete {agent_name}?"),
                true,
                *delete_work_dir,
            ))
        }
        ModalState::ConfirmDeleteRepository { id } => {
            let repo_name = snapshot
                .repositories
                .iter()
                .find(|repo| &repo.id == id)
                .map(|repo| repo.name.clone())
                .unwrap_or_else(|| String::from("selected repository"));
            Some((
                String::from("Delete Repository"),
                format!("Delete {repo_name} and all its agents?"),
                false,
                false,
            ))
        }
        _ => None,
    };

    // Build modal element if needed.
    let modal_el: Option<AnyElement<'static>> = match &modal {
        ModalState::Help => Some(
            element! {
                HelpModal(colors: colors.clone())
            }
            .into_any(),
        ),
        ModalState::NewRepository { .. } | ModalState::EditRepository { .. } => Some(
            element! {
                NewRepositoryForm(
                    state: Some(snapshot.clone()),
                    colors: Some(colors.clone()),
                )
            }
            .into_any(),
        ),
        ModalState::NewAgent { .. } | ModalState::EditAgent { .. } => Some(
            element! {
                NewAgentForm(
                    state: Some(snapshot.clone()),
                    colors: Some(colors.clone()),
                )
            }
            .into_any(),
        ),
        ModalState::ConfirmDeleteRepository { .. } | ModalState::ConfirmDeleteAgent { .. } => {
            confirm_modal_data.map(|(title, message, show_delete_work_dir, delete_work_dir)| {
                element! {
                    ConfirmModal(
                        title: title,
                        message: message,
                        show_delete_work_dir: show_delete_work_dir,
                        delete_work_dir: delete_work_dir,
                        colors: colors.clone(),
                    )
                }
                .into_any()
            })
        }
        _ => None,
    };

    // Root element with proper dimensions.
    // When a modal is active, show only the modal; otherwise show the screen.
    let content_el: AnyElement<'static> = modal_el.unwrap_or(screen_el);

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

/// Convert a key event to raw bytes for PTY input.
fn key_to_bytes(key: &KeyEvent) -> Option<Vec<u8>> {
    match key.code {
        KeyCode::Char(c) => {
            if key.modifiers.contains(KeyModifiers::CONTROL) {
                let ctrl_char = (c as u8) & 0x1f;
                Some(vec![ctrl_char])
            } else {
                let mut buf = [0u8; 4];
                let s = c.encode_utf8(&mut buf);
                Some(s.as_bytes().to_vec())
            }
        }
        KeyCode::Enter => Some(vec![b'\r']),
        KeyCode::Backspace => Some(vec![0x7f]),
        KeyCode::Tab => Some(vec![b'\t']),
        KeyCode::Esc => Some(vec![0x1b]),
        KeyCode::Up => Some(b"\x1b[A".to_vec()),
        KeyCode::Down => Some(b"\x1b[B".to_vec()),
        KeyCode::Right => Some(b"\x1b[C".to_vec()),
        KeyCode::Left => Some(b"\x1b[D".to_vec()),
        KeyCode::Home => Some(b"\x1b[H".to_vec()),
        KeyCode::End => Some(b"\x1b[F".to_vec()),
        KeyCode::PageUp => Some(b"\x1b[5~".to_vec()),
        KeyCode::PageDown => Some(b"\x1b[6~".to_vec()),
        KeyCode::Delete => Some(b"\x1b[3~".to_vec()),
        KeyCode::Insert => Some(b"\x1b[2~".to_vec()),
        KeyCode::F(n) => Some(format!("\x1b[{n}~").into_bytes()),
        _ => None,
    }
}

/// Convert a fullscreen mouse event into xterm SGR mouse reporting bytes.
fn mouse_event_to_bytes(event: &iocraft::FullscreenMouseEvent) -> Option<Vec<u8>> {
    use iocraft::MouseEventKind;

    let (cb, release) = match event.kind {
        MouseEventKind::Down(button) => {
            let code = match button {
                crossterm::event::MouseButton::Left => 0,
                _ => return None,
            };
            (code, false)
        }
        MouseEventKind::Up(button) => {
            let code = match button {
                crossterm::event::MouseButton::Left => 0,
                _ => return None,
            };
            (code, true)
        }
        MouseEventKind::Drag(button) => {
            let base = match button {
                crossterm::event::MouseButton::Left => 0,
                _ => return None,
            };
            (base + 32, false)
        }
        MouseEventKind::Moved => return None,
        MouseEventKind::ScrollDown => (65, false),
        MouseEventKind::ScrollUp => (64, false),
        MouseEventKind::ScrollLeft => (66, false),
        MouseEventKind::ScrollRight => (67, false),
    };

    let mut cb_with_mods = cb;
    if event.modifiers.contains(iocraft::KeyModifiers::SHIFT) {
        cb_with_mods += 4;
    }
    if event.modifiers.contains(iocraft::KeyModifiers::ALT) {
        cb_with_mods += 8;
    }
    if event.modifiers.contains(iocraft::KeyModifiers::CONTROL) {
        cb_with_mods += 16;
    }

    let cx = event.column.saturating_add(1);
    let cy = event.row.saturating_add(1);
    let suffix = if release { 'm' } else { 'M' };
    let seq = format!("\x1b[<{};{};{}{}", cb_with_mods, cx, cy, suffix);
    Some(seq.into_bytes())
}

fn handle_cli_version_flag() -> bool {
    let mut args = std::env::args().skip(1);
    match (args.next().as_deref(), args.next()) {
        (Some("--version" | "-V"), None) => {
            let version = jefe::VERSION;
            println!("jefe {version}");
            true
        }
        _ => false,
    }
}

fn main() {
    if handle_cli_version_flag() {
        return;
    }

    // Get terminal size and derive PTY viewport size from dashboard geometry.
    let (cols, rows) = crossterm::terminal::size().unwrap_or((120, 40));
    let (pty_rows, pty_cols, _, _) = compute_pty_layout(cols, rows);

    // Initialize managers.
    let persistence = FilePersistenceManager::new();
    let theme_manager = FileThemeManager::new();
    let runtime = TmuxRuntimeManager::new(pty_rows, pty_cols);

    let context = Arc::new(std::sync::Mutex::new(AppContext {
        persistence,
        theme_manager,
        runtime,
    }));

    smol::block_on(async {
        let mut app = element!(App(context: Some(context)));

        if is_fullscreen_enabled() {
            if let Err(e) = app.fullscreen().await {
                eprintln!("Error: {e}");
            }
        } else if let Err(e) = app.render_loop().await {
            eprintln!("Error: {e}");
        }
    });
}

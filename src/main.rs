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

use jefe::domain::{AgentId, AgentStatus, LaunchSignature};
use jefe::persistence::{FilePersistenceManager, PersistenceManager, Settings, State};
use jefe::runtime::{RuntimeError, RuntimeManager, TerminalSnapshot, TmuxRuntimeManager};
use jefe::state::{AppEvent, AppState, ModalState, PaneFocus, ScreenMode};
use jefe::theme::{FileThemeManager, ThemeColors, ThemeManager};
use jefe::ui::{Dashboard, HelpModal, NewAgentForm, NewRepositoryForm, SplitScreen};

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

/// Compute the PTY viewport size so llxprt fits the visible terminal pane.
///
/// This follows the same geometry used by the dashboard screen:
/// - top status bar (1 row)
/// - bottom keybind bar (1 row)
/// - middle column split: agent list 25%, terminal 75%
/// - terminal widget chrome: border + header
fn compute_pty_size(term_cols: u16, term_rows: u16) -> (u16, u16) {
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

    (pty_rows, pty_cols)
}

/// Shared application context passed to the root component.
struct AppContext {
    persistence: FilePersistenceManager,
    theme_manager: FileThemeManager,
    runtime: TmuxRuntimeManager,
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
                let settings = ctx_guard
                    .persistence
                    .load_settings()
                    .unwrap_or_else(|e| {
                        eprintln!("Warning: Could not load settings: {e}");
                        Settings::default_with_version()
                    });

                // Load state
                let persisted = ctx_guard
                    .persistence
                    .load_state()
                    .unwrap_or_else(|e| {
                        eprintln!("Warning: Could not load state: {e}");
                        State::default_with_version()
                    });

                // Apply to app state
                let mut state = app_state.write();
                state.repositories = persisted.repositories;
                state.agents = persisted.agents;
                state.selected_repository_index = persisted.selected_repository_index;
                state.selected_agent_index = persisted.selected_agent_index;

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
                        match ctx_guard.runtime.spawn_session(&agent.id, &agent.work_dir, &signature) {
                            Ok(()) => {}
                            Err(RuntimeError::AlreadyRunning(_)) => {
                                // Existing sessions from a previous run are fine.
                            }
                            Err(e) => {
                                eprintln!("Warning: Could not spawn session for {}: {e}", agent.id.0);
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
                        let (pty_rows, pty_cols) = compute_pty_size(cols, rows);
                        let _ = ctx_guard.runtime.resize(pty_rows, pty_cols);
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

                // F12 toggles terminal input focus.
                // Session attach/detach is independent and handled by runtime/view logic.
                if key_event.code == KeyCode::F(12) {
                    let mut state = app_state.write();
                    *state = std::mem::take(&mut *state).apply(AppEvent::ToggleTerminalFocus);
                    return;
                }

                // Defensive guard: terminal input is only valid when the terminal pane is active.
                // If focus state is stale, clear it so navigation keys never leak into llxprt.
                let terminal_input_enabled = term_focused && pane_focus == PaneFocus::Terminal;
                if term_focused && pane_focus != PaneFocus::Terminal {
                    let mut state = app_state.write();
                    state.terminal_focused = false;
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
                    ModalState::Search { .. } => {
                        if key_event.code == KeyCode::Esc {
                            let mut state = app_state.write();
                            *state = std::mem::take(&mut *state).apply(AppEvent::CloseModal);
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

                                // If new agent was created, spawn session and attach viewer.
                                if is_new_agent && state.modal == ModalState::None {
                                    if let Some(agent) = state.selected_agent_index.and_then(|i| state.agents.get(i)) {
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
                            KeyCode::Tab => Some(AppEvent::FormNextField),
                            KeyCode::BackTab => Some(AppEvent::FormPrevField),
                            KeyCode::Down => Some(AppEvent::FormNextField),
                            KeyCode::Up => Some(AppEvent::FormPrevField),
                            KeyCode::Backspace => Some(AppEvent::FormBackspace),
                            // Space: toggle checkbox only on checkbox fields, otherwise type space
                            KeyCode::Char(' ') => Some(AppEvent::FormChar(' ')),
                            KeyCode::Char(c) => Some(AppEvent::FormChar(c)),
                            _ => None,
                        };
                        if let Some(evt) = app_event {
                            let mut state = app_state.write();
                            *state = std::mem::take(&mut *state).apply(evt);
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
                let selected_agent_id = state_ro
                    .selected_agent_index
                    .and_then(|i| state_ro.agents.get(i).map(|a| a.id.clone()));
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
                            if !state.repositories.is_empty() {
                                let first_id = state.repositories[0].id.clone();
                                drop(state);
                                let mut state_mut = app_state.write();
                                state_mut.selected_repository_index = Some(0);
                                Some(first_id)
                            } else {
                                None
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
                    KeyCode::Char('d') => {
                        if pane_focus == PaneFocus::Agents {
                            selected_agent_id.clone().map(AppEvent::OpenDeleteAgent)
                        } else if pane_focus == PaneFocus::Repositories {
                            selected_repo_id.clone().map(AppEvent::OpenDeleteRepository)
                        } else {
                            None
                        }
                    }

                    // Kill agent
                    KeyCode::Char('k' | 'K') => {
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
                    KeyCode::Char('?') => Some(AppEvent::OpenHelp),
                    KeyCode::Char('h' | 'H') => Some(AppEvent::OpenHelp),
                    KeyCode::F(1) => Some(AppEvent::OpenHelp),
                    KeyCode::Char('/') => Some(AppEvent::OpenSearch),

                    // Direct pane focus
                    KeyCode::Char('r' | 'R') => {
                        let mut state = app_state.write();
                        state.pane_focus = PaneFocus::Repositories;
                        None
                    }
                    KeyCode::Char('a' | 'A') => {
                        let mut state = app_state.write();
                        state.pane_focus = PaneFocus::Agents;
                        None
                    }
                    KeyCode::Char('t' | 'T') => {
                        let mut state = app_state.write();
                        state.pane_focus = PaneFocus::Terminal;
                        if !state.terminal_focused {
                            *state = std::mem::take(&mut *state).apply(AppEvent::ToggleTerminalFocus);
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
                        }
                        _ => {
                            let mut state = app_state.write();
                            *state = std::mem::take(&mut *state).apply(evt);
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
        if let Some(ref ctx_arc) = ctx {
            if let Ok(ctx_guard) = ctx_arc.lock() {
                let state = app_state.read();
                let final_state = State {
                    schema_version: jefe::persistence::STATE_SCHEMA_VERSION,
                    repositories: state.repositories.clone(),
                    agents: state.agents.clone(),
                    selected_repository_index: state.selected_repository_index,
                    selected_agent_index: state.selected_agent_index,
                };
                if let Err(e) = ctx_guard.persistence.save_state(&final_state) {
                    eprintln!("Warning: Could not save state: {e}");
                }
            }
        }

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
    if let Some(selected_agent_id) = snapshot
        .selected_agent_index
        .and_then(|i| snapshot.agents.get(i).map(|a| a.id.clone()))
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

fn main() {
    // Get terminal size and derive PTY viewport size from dashboard geometry.
    let (cols, rows) = crossterm::terminal::size().unwrap_or((120, 40));
    let (pty_rows, pty_cols) = compute_pty_size(cols, rows);

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

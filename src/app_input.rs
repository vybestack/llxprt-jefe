use std::sync::Arc;

use iocraft::hooks::State as HookState;
use iocraft::prelude::*;
use tracing::{debug, warn};

use jefe::domain::{AgentId, LaunchSignature};
use jefe::input::{SearchKeyRoute, route_search_key};
use jefe::persistence::{PersistenceManager, State as PersistedState};
use jefe::runtime::{RuntimeError, RuntimeManager};
use jefe::state::{AppEvent, AppState, ModalState, PaneFocus, ScreenMode};
use jefe::theme::ThemeManager;

pub type SharedContext = Option<Arc<std::sync::Mutex<super::AppContext>>>;
pub type AppStateHandle = HookState<AppState>;
pub type QuitHandle = HookState<bool>;
pub type HelpScrollHandle = HookState<u32>;

pub fn to_persisted_state(state: &AppState) -> PersistedState {
    PersistedState {
        schema_version: jefe::persistence::STATE_SCHEMA_VERSION,
        repositories: state.repositories.clone(),
        agents: state.agents.clone(),
        selected_repository_index: state.selected_repository_index,
        selected_agent_index: state.selected_agent_index,
    }
}

pub fn persist_state_snapshot(ctx: &SharedContext, state: &AppState) {
    if let Some(ctx_arc) = &ctx
        && let Ok(ctx_guard) = ctx_arc.lock()
        && let Err(e) = ctx_guard.persistence.save_state(&to_persisted_state(state))
    {
        warn!(error = %e, "could not save state");
    }
}

fn apply_and_persist(app_state: &mut AppStateHandle, ctx: &SharedContext, evt: AppEvent) {
    let mut state = app_state.write();
    *state = std::mem::take(&mut *state).apply(evt);
    persist_state_snapshot(ctx, &state);
}

fn close_modal_and_persist(app_state: &mut AppStateHandle, ctx: &SharedContext) {
    apply_and_persist(app_state, ctx, AppEvent::CloseModal);
}

pub fn handle_mode_help_key(
    app_state: &mut AppStateHandle,
    ctx: &SharedContext,
    help_scroll: &mut HelpScrollHandle,
    key_event: &KeyEvent,
) {
    match key_event.code {
        KeyCode::Esc | KeyCode::Char('?') => {
            close_modal_and_persist(app_state, ctx);
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
}

pub fn handle_mode_search_key(
    app_state: &mut AppStateHandle,
    ctx: &SharedContext,
    key_event: &KeyEvent,
) -> bool {
    match route_search_key(key_event) {
        SearchKeyRoute::CloseAndConsume => {
            close_modal_and_persist(app_state, ctx);
            true
        }
        SearchKeyRoute::Backspace => {
            apply_and_persist(app_state, ctx, AppEvent::FormBackspace);
            true
        }
        SearchKeyRoute::EditQueryChar(c) => {
            apply_and_persist(app_state, ctx, AppEvent::FormChar(c));
            true
        }
        SearchKeyRoute::CloseAndReroute => {
            debug!(
                code = ?key_event.code,
                modifiers = ?key_event.modifiers,
                "closing search mode on non-search key"
            );
            close_modal_and_persist(app_state, ctx);
            false
        }
        SearchKeyRoute::Ignore => true,
    }
}

pub fn dispatch_app_event(app_state: &mut AppStateHandle, ctx: &SharedContext, evt: AppEvent) {
    debug!(event = ?evt, "dispatching app event");

    match evt {
        AppEvent::ToggleTerminalFocus => {
            // Keep Enter-in-terminal-pane as a UI focus toggle only.
            // Runtime attach/detach remains bound to F12.
            apply_and_persist(app_state, ctx, AppEvent::ToggleTerminalFocus);
        }
        AppEvent::KillAgent(ref agent_id) => {
            if let Some(ctx_arc) = &ctx
                && let Ok(mut ctx_guard) = ctx_arc.lock()
                && let Err(e) = ctx_guard.runtime.kill(agent_id)
            {
                warn!(agent_id = %agent_id.0, error = %e, "could not kill runtime session");
            }

            let mut state = app_state.write();
            *state = std::mem::take(&mut *state).apply(evt);
            state.terminal_focused = false;
            persist_state_snapshot(ctx, &state);
        }
        AppEvent::RelaunchAgent(ref agent_id) => {
            let mut relaunched = false;
            if let Some(ctx_arc) = &ctx
                && let Ok(mut ctx_guard) = ctx_arc.lock()
            {
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
                            match ctx_guard.runtime.spawn_session(
                                agent_id,
                                &agent.work_dir,
                                &signature,
                            ) {
                                Ok(()) => {
                                    relaunched = true;
                                }
                                Err(e2) => {
                                    warn!(
                                        agent_id = %agent_id.0,
                                        error = %e2,
                                        "could not relaunch via spawn_session"
                                    );
                                }
                            }
                        }
                    }
                    Err(e) => {
                        warn!(
                            agent_id = %agent_id.0,
                            error = %e,
                            "could not relaunch runtime session"
                        );
                    }
                }

                if relaunched {
                    // Relaunch should make output visible immediately; focus remains separate.
                    if let Err(e) = ctx_guard.runtime.attach(agent_id) {
                        warn!(
                            agent_id = %agent_id.0,
                            error = %e,
                            "could not attach relaunched session"
                        );
                    }
                }
            }

            let mut state = app_state.write();
            *state = std::mem::take(&mut *state).apply(evt);
            if relaunched {
                state.terminal_focused = false;
            }
            persist_state_snapshot(ctx, &state);
        }
        _ => {
            apply_and_persist(app_state, ctx, evt);
        }
    }
}

pub fn handle_f12_toggle(app_state: &mut AppStateHandle, ctx: &SharedContext) {
    // F12 toggles terminal input focus.
    // When enabling, force pane focus to terminal and require attach success.
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
            if let Some(ctx_arc) = &ctx
                && let Ok(mut ctx_guard) = ctx_arc.lock()
            {
                match ctx_guard.runtime.attach(agent_id) {
                    Ok(()) => true,
                    Err(e) => {
                        warn!(
                            agent_id = %agent_id.0,
                            error = %e,
                            "could not attach session on F12 focus"
                        );
                        false
                    }
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
    persist_state_snapshot(ctx, &state);
}

pub fn handle_mode_confirm_key(
    app_state: &mut AppStateHandle,
    ctx: &SharedContext,
    key_event: &KeyEvent,
) {
    match key_event.code {
        KeyCode::Esc => {
            close_modal_and_persist(app_state, ctx);
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
                    if let Some(ctx_arc) = &ctx
                        && let Ok(mut ctx_guard) = ctx_arc.lock()
                        && let Err(e) = ctx_guard.runtime.kill(&id)
                    {
                        match e {
                            RuntimeError::SessionNotFound(_) => {}
                            _ => {
                                warn!(
                                    agent_id = %id.0,
                                    error = %e,
                                    "could not kill runtime session before delete"
                                );
                            }
                        }
                    }

                    let mut state = app_state.write();
                    let _ = super::delete_selected_agent(&mut state, &id, delete_work_dir);
                    state.modal = ModalState::None;
                    persist_state_snapshot(ctx, &state);
                }
                ModalState::ConfirmDeleteRepository { id } => {
                    if let Some(ctx_arc) = &ctx
                        && let Ok(mut ctx_guard) = ctx_arc.lock()
                    {
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
                                        warn!(
                                            agent_id = %agent_id.0,
                                            error = %e,
                                            "could not kill runtime session before repository delete"
                                        );
                                    }
                                }
                            }
                        }
                    }

                    let mut state = app_state.write();
                    super::delete_selected_repository(&mut state, &id);
                    state.modal = ModalState::None;
                    persist_state_snapshot(ctx, &state);
                }
                _ => {}
            }
        }
        KeyCode::Char(' ' | 'd' | 'D') | KeyCode::Up | KeyCode::Down => {
            apply_and_persist(app_state, ctx, AppEvent::ToggleDeleteWorkDir);
        }
        _ => {}
    }
}

pub fn handle_mode_form_key(
    app_state: &mut AppStateHandle,
    ctx: &SharedContext,
    key_event: &KeyEvent,
) -> bool {
    let app_event = match key_event.code {
        KeyCode::Esc => Some(AppEvent::CloseModal),
        KeyCode::Enter => {
            // Submit form and spawn PTY if new agent.
            let state_ro = app_state.read();
            let is_new_agent = matches!(state_ro.modal, ModalState::NewAgent { .. });
            drop(state_ro);

            let mut state = app_state.write();
            *state = std::mem::take(&mut *state).apply(AppEvent::SubmitForm);
            persist_state_snapshot(ctx, &state);

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
                    persist_state_snapshot(ctx, &state);
                    drop(state);

                    if let Some(ctx_arc) = &ctx
                        && let Ok(mut ctx_guard) = ctx_arc.lock()
                    {
                        if let Err(e) = ctx_guard
                            .runtime
                            .spawn_session(&agent_id, &work_dir, &signature)
                        {
                            warn!(error = %e, "could not spawn session for new agent");
                        } else if let Err(e) = ctx_guard.runtime.attach(&agent_id) {
                            warn!(
                                agent_id = %agent_id.0,
                                error = %e,
                                "could not attach session for new agent"
                            );
                        }
                    }
                }
            }

            return true;
        }
        KeyCode::Tab | KeyCode::Down => Some(AppEvent::FormNextField),
        KeyCode::BackTab | KeyCode::Up => Some(AppEvent::FormPrevField),
        KeyCode::Backspace => Some(AppEvent::FormBackspace),
        // Space: toggle checkbox only on checkbox fields, otherwise type space.
        KeyCode::Char(' ') => Some(AppEvent::FormChar(' ')),
        KeyCode::Char(c) => Some(AppEvent::FormChar(c)),
        _ => None,
    };

    if let Some(evt) = app_event {
        apply_and_persist(app_state, ctx, evt);
    }

    true
}

pub fn handle_normal_key_event(
    app_state: &mut AppStateHandle,
    should_quit: &mut QuitHandle,
    ctx: &SharedContext,
    key_event: &KeyEvent,
    screen_mode: ScreenMode,
) -> Option<AppEvent> {
    let state_ro = app_state.read();
    let pane_focus = state_ro.pane_focus;
    let selected_repo_id = state_ro
        .selected_repository_index
        .and_then(|i| state_ro.repositories.get(i).map(|r| r.id.clone()));
    let selected_agent_id = state_ro.selected_agent().map(|agent| agent.id.clone());
    drop(state_ro);

    match key_event.code {
        // Quit
        KeyCode::Char('q' | 'Q') => {
            should_quit.set(true);
            None
        }

        // Navigation
        KeyCode::Up => Some(AppEvent::NavigateUp),
        KeyCode::Down => Some(AppEvent::NavigateDown),
        KeyCode::Left => Some(AppEvent::NavigateLeft),
        KeyCode::Right => Some(AppEvent::NavigateRight),
        KeyCode::Tab => Some(AppEvent::CyclePaneFocus),

        // New (n = new agent, N = new repository)
        KeyCode::Char('n') => {
            debug!(
                selected_repo_id = ?selected_repo_id,
                "n pressed: deriving new agent/repo action"
            );
            // If no repo is selected but repos exist, auto-select the first one.
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
                    persist_state_snapshot(ctx, &state_mut);
                    Some(first_id)
                }
            });
            if repo_id.is_none() {
                debug!("n: no repos → OpenNewRepository");
                Some(AppEvent::OpenNewRepository)
            } else {
                debug!(repo_id = ?repo_id, "n: repo exists → OpenNewAgent");
                repo_id.map(AppEvent::OpenNewAgent)
            }
        }
        KeyCode::Char('N') => {
            debug!("N pressed: OpenNewRepository");
            Some(AppEvent::OpenNewRepository)
        }

        // Delete
        KeyCode::Char('d' | 'D') if key_event.modifiers.contains(KeyModifiers::CONTROL) => {
            if pane_focus == PaneFocus::Agents || pane_focus == PaneFocus::Terminal {
                selected_agent_id.clone().map(AppEvent::OpenDeleteAgent)
            } else if pane_focus == PaneFocus::Repositories {
                selected_repo_id.clone().map(AppEvent::OpenDeleteRepository)
            } else {
                None
            }
        }

        // Kill agent
        KeyCode::Char('k' | 'K') if key_event.modifiers.contains(KeyModifiers::CONTROL) => {
            selected_agent_id.clone().map(AppEvent::KillAgent)
        }

        // Relaunch agent
        KeyCode::Char('l' | 'L') => selected_agent_id.clone().map(AppEvent::RelaunchAgent),

        // Split mode
        KeyCode::Char('s' | 'S') if screen_mode == ScreenMode::Dashboard => {
            Some(AppEvent::EnterSplitMode)
        }
        KeyCode::Esc if screen_mode == ScreenMode::Split => Some(AppEvent::ExitSplitMode),

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
            persist_state_snapshot(ctx, &state);
            None
        }
        KeyCode::Char('a' | 'A') => {
            let mut state = app_state.write();
            state.pane_focus = PaneFocus::Agents;
            persist_state_snapshot(ctx, &state);
            None
        }
        KeyCode::Char('t' | 'T') => {
            let selected_agent_id = {
                let mut state = app_state.write();
                state.pane_focus = PaneFocus::Terminal;
                if !state.terminal_focused {
                    *state = std::mem::take(&mut *state).apply(AppEvent::ToggleTerminalFocus);
                }
                state.selected_agent().map(|agent| agent.id.clone())
            };

            if let Some(agent_id) = selected_agent_id {
                if let Some(ctx_arc) = &ctx
                    && let Ok(mut ctx_guard) = ctx_arc.lock()
                    && let Err(e) = ctx_guard.runtime.attach(&agent_id)
                {
                    warn!(
                        agent_id = %agent_id.0,
                        error = %e,
                        "could not attach session on 't' focus"
                    );
                    let mut state = app_state.write();
                    state.terminal_focused = false;
                    persist_state_snapshot(ctx, &state);
                }
            } else {
                let mut state = app_state.write();
                state.terminal_focused = false;
                persist_state_snapshot(ctx, &state);
            }

            None
        }

        // Enter selects current item (edit agent/repo)
        KeyCode::Enter => match pane_focus {
            PaneFocus::Agents => selected_agent_id.clone().map(AppEvent::OpenEditAgent),
            PaneFocus::Repositories => selected_repo_id.clone().map(AppEvent::OpenEditRepository),
            PaneFocus::Terminal => {
                // Toggle terminal focus on Enter when in terminal pane.
                Some(AppEvent::ToggleTerminalFocus)
            }
        },

        // Theme switching (1/2/3)
        KeyCode::Char('1') => {
            if let Some(ctx_arc) = &ctx
                && let Ok(mut ctx_guard) = ctx_arc.lock()
            {
                let _ = ctx_guard.theme_manager.set_active("green-screen");
            }
            None
        }
        KeyCode::Char('2') => {
            if let Some(ctx_arc) = &ctx
                && let Ok(mut ctx_guard) = ctx_arc.lock()
            {
                let _ = ctx_guard.theme_manager.set_active("dracula");
            }
            None
        }
        KeyCode::Char('3') => {
            if let Some(ctx_arc) = &ctx
                && let Ok(mut ctx_guard) = ctx_arc.lock()
            {
                let _ = ctx_guard.theme_manager.set_active("default-dark");
            }
            None
        }

        _ => None,
    }
}

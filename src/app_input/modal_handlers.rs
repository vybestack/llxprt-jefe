//! Modal/confirm/form key handlers.
//!
//! Extracted from mod.rs to keep file sizes manageable.

use iocraft::prelude::*;
use tracing::warn;

use jefe::domain::{AgentId, SandboxEngine};
use jefe::runtime::{RuntimeError, RuntimeManager};
use jefe::state::{AgentFormFocus, AppEvent, ModalState, PaneFocus, RepositoryFormFocus};

use super::{
    AppStateHandle, SharedContext, apply_and_persist, clear_agent_runtime_attachment,
    close_modal_and_persist, execute_agent_launch, launch_signature_for_agent,
    mark_agent_runtime_attached, persist_state_snapshot, preflight_or_prompt,
    repository_focus_toggles_checkbox,
};

pub fn handle_f12_toggle(app_state: &mut AppStateHandle, ctx: &SharedContext) {
    // F12 toggles terminal input focus.
    // When enabling, force pane focus to terminal and require attach success.
    let (enabling_focus, selected_agent_id) = {
        let mut state = app_state.write();

        if state.terminal_focused {
            // Leaving terminal capture should always return keyboard focus to agents.
            state.pane_focus = PaneFocus::Agents;
            *state = std::mem::take(&mut *state).apply(AppEvent::ToggleTerminalFocus);
            (false, None)
        } else {
            let selected_running_agent_id = state
                .selected_agent()
                .filter(|agent| agent.is_running())
                .map(|agent| agent.id.clone());

            if selected_running_agent_id.is_some() {
                state.pane_focus = PaneFocus::Terminal;
                *state = std::mem::take(&mut *state).apply(AppEvent::ToggleTerminalFocus);
                (true, selected_running_agent_id)
            } else {
                // Dead/non-running agents are not attachable.
                state.pane_focus = PaneFocus::Agents;
                state.terminal_focused = false;
                (false, None)
            }
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

        let mut state = app_state.write();
        if !attached {
            state.terminal_focused = false;
            state.pane_focus = PaneFocus::Agents;
            if let Some(agent_id) = selected_agent_id.as_ref() {
                mark_agent_runtime_attached(&mut state, agent_id, false);
            }
        } else if let Some(agent_id) = selected_agent_id.as_ref() {
            clear_agent_runtime_attachment(&mut state);
            mark_agent_runtime_attached(&mut state, agent_id, true);
        }
    }

    let state = app_state.read();
    persist_state_snapshot(ctx, &state);
}

#[allow(clippy::too_many_lines)]
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
                    let _ = super::super::delete_selected_agent(&mut state, &id, delete_work_dir);
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
                    super::super::delete_selected_repository(&mut state, &id);
                    state.modal = ModalState::None;
                    persist_state_snapshot(ctx, &state);
                }
                ModalState::PreflightPrompt {
                    agent_id,
                    signature,
                    issue,
                    ..
                } => {
                    super::preflight::handle_preflight_prompt_enter(
                        app_state, ctx, agent_id, signature, issue,
                    );
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

#[allow(clippy::too_many_lines)]
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
                    let repository = state.repository_by_id(&agent.repository_id).cloned();
                    let Some(repository) = repository else {
                        state.terminal_focused = false;
                        state.error_message =
                            Some("selected agent repository not found".to_owned());
                        persist_state_snapshot(ctx, &state);
                        return true;
                    };
                    let signature = launch_signature_for_agent(&agent, &repository);

                    // Drop write guard before preflight (it may take the lock).
                    drop(state);

                    // Run preflight checks before spawning.
                    if !preflight_or_prompt(app_state, ctx, &agent_id, &signature) {
                        return true;
                    }

                    // Match toy1 behavior: new agent opens attached and focused.
                    {
                        let mut state = app_state.write();
                        state.terminal_focused = true;
                        persist_state_snapshot(ctx, &state);
                    }

                    execute_agent_launch(app_state, ctx, &agent_id, &work_dir, &signature, false);
                }
            }

            return true;
        }
        KeyCode::Tab | KeyCode::Down => Some(AppEvent::FormNextField),
        KeyCode::BackTab | KeyCode::Up => Some(AppEvent::FormPrevField),
        KeyCode::Left => Some(AppEvent::FormMoveCursorLeft),
        KeyCode::Right => Some(AppEvent::FormMoveCursorRight),
        KeyCode::Backspace => Some(AppEvent::FormBackspace),
        KeyCode::Delete => Some(AppEvent::FormDelete),
        // Space toggles checkbox or cycles sandbox engine on the dedicated controls.
        KeyCode::Char(' ') => {
            enum FocusedFormField {
                Repository(RepositoryFormFocus),
                Agent(AgentFormFocus),
                None,
            }

            let focused = {
                let state = app_state.read();
                match &state.modal {
                    ModalState::NewRepository { focus, .. }
                    | ModalState::EditRepository { focus, .. } => {
                        FocusedFormField::Repository(*focus)
                    }
                    ModalState::NewAgent { focus, .. } | ModalState::EditAgent { focus, .. } => {
                        FocusedFormField::Agent(*focus)
                    }
                    _ => FocusedFormField::None,
                }
            };

            match focused {
                FocusedFormField::Repository(focus) if repository_focus_toggles_checkbox(focus) => {
                    Some(AppEvent::FormToggleCheckbox)
                }
                FocusedFormField::Agent(
                    AgentFormFocus::PassContinue
                    | AgentFormFocus::Sandbox
                    | AgentFormFocus::Shortcut,
                ) => Some(AppEvent::FormToggleCheckbox),
                FocusedFormField::Agent(AgentFormFocus::SandboxEngine) => {
                    let mut state = app_state.write();
                    if let ModalState::NewAgent { fields, .. }
                    | ModalState::EditAgent { fields, .. } = &mut state.modal
                    {
                        SandboxEngine::next_from_form_value(&fields.sandbox_engine)
                            .label()
                            .clone_into(&mut fields.sandbox_engine);
                    }
                    persist_state_snapshot(ctx, &state);
                    return true;
                }
                _ => Some(AppEvent::FormChar(' ')),
            }
        }
        KeyCode::Char(c) => Some(AppEvent::FormChar(c)),
        _ => None,
    };

    if let Some(evt) = app_event {
        apply_and_persist(app_state, ctx, evt);
    }

    true
}

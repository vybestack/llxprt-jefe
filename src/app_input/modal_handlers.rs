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
    mark_agent_runtime_attached, persist_state, preflight_or_prompt,
    repository_focus_toggles_checkbox, to_persisted_state,
};

pub fn handle_f12_toggle(app_state: &mut AppStateHandle, ctx: &SharedContext) {
    let (enabling_focus, selected_agent_id) = prepare_f12_toggle(app_state);

    if enabling_focus {
        let attached = selected_agent_id
            .as_ref()
            .is_some_and(|agent_id| attach_for_f12(ctx, agent_id));
        update_f12_attachment_state(app_state, selected_agent_id.as_ref(), attached);
    }

    persist_current_state(app_state, ctx);
}

fn prepare_f12_toggle(app_state: &mut AppStateHandle) -> (bool, Option<AgentId>) {
    let mut state = app_state.write();

    let toggle_result = if state.terminal_focused {
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
            state.pane_focus = PaneFocus::Agents;
            state.terminal_focused = false;
            (false, None)
        }
    };

    drop(state);
    toggle_result
}

fn attach_for_f12(ctx: &SharedContext, agent_id: &AgentId) -> bool {
    if let Some(ctx_arc) = ctx
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
}

fn update_f12_attachment_state(
    app_state: &mut AppStateHandle,
    selected_agent_id: Option<&AgentId>,
    attached: bool,
) {
    let mut state = app_state.write();
    if !attached {
        state.terminal_focused = false;
        state.pane_focus = PaneFocus::Agents;
        if let Some(agent_id) = selected_agent_id {
            mark_agent_runtime_attached(&mut state, agent_id, false);
        }
    } else if let Some(agent_id) = selected_agent_id {
        clear_agent_runtime_attachment(&mut state);
        mark_agent_runtime_attached(&mut state, agent_id, true);
    }
    drop(state);
}

fn persist_current_state(app_state: &AppStateHandle, ctx: &SharedContext) {
    let state = app_state.read();
    let persisted = to_persisted_state(&state);
    drop(state);
    persist_state(ctx, &persisted);
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
                    let _ = jefe::state::delete_selected_agent(&mut state, &id, delete_work_dir);
                    state.modal = ModalState::None;
                    let persisted = to_persisted_state(&state);
                    drop(state);
                    persist_state(ctx, &persisted);
                }
                ModalState::ConfirmDeleteRepository { id } => {
                    // Read app_state first, then lock context (consistent ordering)
                    let agent_ids: Vec<AgentId> = {
                        let state = app_state.read();
                        state
                            .agents
                            .iter()
                            .filter(|agent| agent.repository_id == id)
                            .map(|agent| agent.id.clone())
                            .collect()
                    };

                    if let Some(ctx_arc) = &ctx
                        && let Ok(mut ctx_guard) = ctx_arc.lock()
                    {
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
                    jefe::state::delete_selected_repository(&mut state, &id);
                    state.modal = ModalState::None;
                    let persisted = to_persisted_state(&state);
                    drop(state);
                    persist_state(ctx, &persisted);
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

            let launch_after_submit = {
                let mut state = app_state.write();
                *state = std::mem::take(&mut *state).apply(AppEvent::SubmitForm);

                let launch_after_submit = if is_new_agent && state.modal == ModalState::None {
                    state.selected_agent().cloned().and_then(|agent| {
                        state
                            .repository_by_id(&agent.repository_id)
                            .map(|repository| {
                                let signature = launch_signature_for_agent(&agent, repository);
                                (agent.id.clone(), agent.work_dir.clone(), signature)
                            })
                    })
                } else {
                    None
                };

                if is_new_agent
                    && state.modal == ModalState::None
                    && launch_after_submit.is_none()
                    && state.selected_agent().is_some()
                {
                    state.terminal_focused = false;
                    state.error_message = Some("selected agent repository not found".to_owned());
                }

                let persisted = to_persisted_state(&state);
                drop(state);
                persist_state(ctx, &persisted);
                launch_after_submit
            };

            if let Some((agent_id, work_dir, signature)) = launch_after_submit {
                // Run preflight checks before spawning.
                if !preflight_or_prompt(app_state, ctx, &agent_id, &signature) {
                    return true;
                }

                // Match toy1 behavior: new agent opens attached and focused.
                {
                    let mut state = app_state.write();
                    state.terminal_focused = true;
                    let persisted = to_persisted_state(&state);
                    drop(state);
                    persist_state(ctx, &persisted);
                }

                execute_agent_launch(app_state, ctx, &agent_id, &work_dir, &signature, false);
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
                    let persisted = to_persisted_state(&state);
                    drop(state);
                    persist_state(ctx, &persisted);
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

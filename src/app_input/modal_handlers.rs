//! Modal/confirm/form key handlers.
//!
//! Extracted from mod.rs to keep file sizes manageable.

use iocraft::prelude::*;
use tracing::warn;

use jefe::domain::{AgentId, LaunchSignature, SandboxEngine};
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
    } else {
        update_f12_attachment_state(app_state, None, false);
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
        clear_agent_runtime_attachment(&mut state);
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

pub fn handle_mode_confirm_key(
    app_state: &mut AppStateHandle,
    ctx: &SharedContext,
    key_event: &KeyEvent,
) {
    // The dirty-copy confirm only accepts Enter (discard + proceed) or
    // Esc/n (halt). It must NOT toggle the delete-work-dir checkbox used by
    // the agent/repository delete confirms.
    if matches!(
        app_state.read().modal,
        ModalState::ConfirmIssueDirtyCopy { .. }
    ) {
        match key_event.code {
            KeyCode::Enter => handle_confirm_enter(app_state, ctx),
            KeyCode::Esc | KeyCode::Char('n' | 'N') => {
                close_modal_and_persist(app_state, ctx);
            }
            _ => {}
        }
        return;
    }
    match key_event.code {
        KeyCode::Esc => close_modal_and_persist(app_state, ctx),
        KeyCode::Enter => handle_confirm_enter(app_state, ctx),
        KeyCode::Char(' ' | 'd' | 'D') | KeyCode::Up | KeyCode::Down => {
            apply_and_persist(app_state, ctx, AppEvent::ToggleDeleteWorkDir);
        }
        _ => {}
    }
}

fn handle_confirm_enter(app_state: &mut AppStateHandle, ctx: &SharedContext) {
    let modal_snapshot = {
        let state = app_state.read();
        state.modal.clone()
    };

    match modal_snapshot {
        ModalState::ConfirmDeleteAgent {
            id,
            delete_work_dir,
        } => confirm_delete_agent(app_state, ctx, id, delete_work_dir),
        ModalState::ConfirmDeleteRepository { id } => confirm_delete_repository(app_state, ctx, id),
        ModalState::PreflightPrompt {
            agent_id,
            signature,
            issue,
            ..
        } => super::preflight::handle_preflight_prompt_enter(
            app_state, ctx, agent_id, signature, issue,
        ),
        ModalState::ConfirmIssueDirtyCopy {
            agent_id,
            work_dir,
            signature,
            payload,
        } => super::issues_send::confirm_issue_dirty_copy_enter(
            app_state, ctx, agent_id, work_dir, signature, payload,
        ),
        _ => {}
    }
}

fn confirm_delete_agent(
    app_state: &mut AppStateHandle,
    ctx: &SharedContext,
    id: AgentId,
    delete_work_dir: bool,
) {
    kill_agent_before_delete(ctx, &id);

    let mut state = app_state.write();
    let _ = jefe::state::delete_selected_agent(&mut state, &id, delete_work_dir);
    state.modal = ModalState::None;
    let persisted = to_persisted_state(&state);
    drop(state);
    persist_state(ctx, &persisted);
}

fn confirm_delete_repository(
    app_state: &mut AppStateHandle,
    ctx: &SharedContext,
    id: jefe::domain::RepositoryId,
) {
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
        kill_agent_before_delete(ctx, agent_id);
    }

    let mut state = app_state.write();
    jefe::state::delete_selected_repository(&mut state, &id);
    state.modal = ModalState::None;
    let persisted = to_persisted_state(&state);
    drop(state);
    persist_state(ctx, &persisted);
}

fn kill_agent_before_delete(ctx: &SharedContext, agent_id: &AgentId) {
    if let Some(ctx_arc) = ctx
        && let Ok(mut ctx_guard) = ctx_arc.lock()
        && let Err(error) = ctx_guard.runtime.kill(agent_id)
        && !matches!(error, RuntimeError::SessionNotFound(_))
    {
        warn!(
            agent_id = %agent_id.0,
            error = %error,
            "could not kill runtime session before delete"
        );
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
            handle_form_submit(app_state, ctx);
            return true;
        }
        KeyCode::Tab | KeyCode::Down => Some(AppEvent::FormNextField),
        KeyCode::BackTab | KeyCode::Up => Some(AppEvent::FormPrevField),
        KeyCode::Left => Some(AppEvent::FormMoveCursorLeft),
        KeyCode::Right => Some(AppEvent::FormMoveCursorRight),
        KeyCode::Backspace => Some(AppEvent::FormBackspace),
        KeyCode::Delete => Some(AppEvent::FormDelete),
        KeyCode::Char(' ') => handle_form_space(app_state, ctx),
        KeyCode::Char(c) => Some(AppEvent::FormChar(c)),
        _ => None,
    };

    if let Some(evt) = app_event {
        apply_and_persist(app_state, ctx, evt);
    }

    true
}

fn handle_form_submit(app_state: &mut AppStateHandle, ctx: &SharedContext) {
    let is_new_agent = {
        let state_ro = app_state.read();
        matches!(state_ro.modal, ModalState::NewAgent { .. })
    };

    let launch_after_submit = submit_form_and_snapshot_launch(app_state, ctx, is_new_agent);
    if let Some((agent_id, work_dir, signature)) = launch_after_submit {
        if !preflight_or_prompt(app_state, ctx, &agent_id, &signature) {
            return;
        }
        focus_terminal_after_submit(app_state, ctx);
        execute_agent_launch(app_state, ctx, &agent_id, &work_dir, &signature, false);
    }
}

fn submit_form_and_snapshot_launch(
    app_state: &mut AppStateHandle,
    ctx: &SharedContext,
    is_new_agent: bool,
) -> Option<(AgentId, std::path::PathBuf, LaunchSignature)> {
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
}

fn focus_terminal_after_submit(app_state: &mut AppStateHandle, ctx: &SharedContext) {
    let mut state = app_state.write();
    state.terminal_focused = true;
    let persisted = to_persisted_state(&state);
    drop(state);
    persist_state(ctx, &persisted);
}

fn handle_form_space(app_state: &mut AppStateHandle, ctx: &SharedContext) -> Option<AppEvent> {
    match focused_form_field(app_state) {
        FocusedFormField::Repository(focus) if repository_focus_toggles_checkbox(focus) => {
            Some(AppEvent::FormToggleCheckbox)
        }
        FocusedFormField::Agent(
            AgentFormFocus::PassContinue | AgentFormFocus::Sandbox | AgentFormFocus::Shortcut,
        ) => Some(AppEvent::FormToggleCheckbox),
        FocusedFormField::Agent(AgentFormFocus::SandboxEngine) => {
            cycle_sandbox_engine(app_state, ctx);
            None
        }
        _ => Some(AppEvent::FormChar(' ')),
    }
}

enum FocusedFormField {
    Repository(RepositoryFormFocus),
    Agent(AgentFormFocus),
    None,
}

fn focused_form_field(app_state: &AppStateHandle) -> FocusedFormField {
    let state = app_state.read();
    match &state.modal {
        ModalState::NewRepository { focus, .. } | ModalState::EditRepository { focus, .. } => {
            FocusedFormField::Repository(*focus)
        }
        ModalState::NewAgent { focus, .. } | ModalState::EditAgent { focus, .. } => {
            FocusedFormField::Agent(*focus)
        }
        _ => FocusedFormField::None,
    }
}

fn cycle_sandbox_engine(app_state: &mut AppStateHandle, ctx: &SharedContext) {
    let mut state = app_state.write();
    if let ModalState::NewAgent { fields, .. } | ModalState::EditAgent { fields, .. } =
        &mut state.modal
    {
        SandboxEngine::next_from_form_value(&fields.sandbox_engine)
            .label()
            .clone_into(&mut fields.sandbox_engine);
    }
    let persisted = to_persisted_state(&state);
    drop(state);
    persist_state(ctx, &persisted);
}

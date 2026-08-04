//! Modal/confirm/form key handlers.
//!
//! Extracted from mod.rs to keep file sizes manageable.

use tracing::warn;

use jefe::domain::{AgentId, AgentLaunchRequest, SandboxEngine};
use jefe::runtime::{RuntimeError, RuntimeManager};
use jefe::state::{
    AgentFormFocus, AppEvent, AppState, ConfirmFocus, ModalState, PaneFocus, RepositoryFormFocus,
};

use super::{
    AppStateHandle, SharedContext, close_modal_and_persist, durable_save_request,
    execute_agent_launch, launch_signature_for_new_agent, preflight_or_prompt,
    repository_focus_toggles_checkbox, schedule_durable_save,
};

pub fn handle_f12_toggle(app_state: &mut AppStateHandle, ctx: &SharedContext) {
    // Issue #301 Phase 5: F12 is now pure intent. It updates pane_focus /
    // terminal_focused deterministically and persists the state change. The
    // actual runtime attach is performed asynchronously by the background
    // attach future (Phase 3) driven by the AttachScheduler's desired target.
    // The render body sets desired from `selected_running_agent_id`, so F12
    // just flips the focus intent — no synchronous `runtime.attach()` call.
    //
    // When F12 toggles terminal focus OFF, the viewer stays attached: the
    // scheduler's desired target is driven by `selected_running_agent_id`
    // (which is still `Some` because the agent is still Running), not by
    // `terminal_focused`. This is intentional — the terminal pane continues
    // to render as a read-only preview (issue #160). F12 controls keystroke
    // routing only; the viewer detaches only when the selected agent changes
    // or the agent transitions out of Running.
    //
    // If the background attach later fails (session gone, tmux error), the
    // attach worker calls `apply_attach_failure`, which resets
    // `terminal_focused` to false and `pane_focus` to Agents, restoring the
    // pre-F12 dashboard view. The user can press F12 again to retry.
    prepare_f12_toggle(app_state);
    persist_current_state(app_state, ctx);
}

fn prepare_f12_toggle(app_state: &mut AppStateHandle) {
    let mut state = app_state.write();

    if state.terminal_focused {
        state.pane_focus = PaneFocus::Agents;
        jefe::state::transition::commit_pure_site(
            &mut state,
            (AppEvent::ToggleTerminalFocus).into(),
        );
    } else {
        let selected_running_agent_id = state
            .selected_agent()
            .filter(|agent| agent.is_running())
            .map(|agent| agent.id.clone());

        if selected_running_agent_id.is_some() {
            state.pane_focus = PaneFocus::Terminal;
            jefe::state::transition::commit_pure_site(
                &mut state,
                (AppEvent::ToggleTerminalFocus).into(),
            );
        } else {
            state.pane_focus = PaneFocus::Agents;
            state.terminal_focused = false;
        }
    }
}

fn persist_current_state(app_state: &mut AppStateHandle, ctx: &SharedContext) {
    let mut state = app_state.write();
    let persisted = durable_save_request(&mut state);
    drop(state);
    schedule_durable_save(ctx, persisted);
}

pub(super) fn handle_confirm_enter(app_state: &mut AppStateHandle, ctx: &SharedContext) {
    let modal_snapshot = {
        let state = app_state.read();
        state.modal.clone()
    };

    // If Cancel is focused, Enter dismisses without performing the action (issue #228).
    if confirm_focus_is_cancel(&modal_snapshot) {
        close_modal_and_persist(app_state, ctx);
        return;
    }

    match modal_snapshot {
        ModalState::ConfirmDeleteAgent {
            id,
            delete_work_dir,
            ..
        } => confirm_delete_agent(app_state, ctx, id, delete_work_dir),
        ModalState::ConfirmDeleteRepository { id, .. } => {
            confirm_delete_repository(app_state, ctx, id);
        }
        ModalState::ConfirmServerLostRecovery { agent_ids, .. } => {
            super::relaunch::dispatch_server_lost_recovery(app_state, ctx, agent_ids);
        }
        ModalState::PreflightPrompt {
            agent_id,
            signature,
            issue,
            issue_self_assignment,
            ..
        } => super::preflight::handle_preflight_prompt_enter(
            app_state,
            ctx,
            agent_id,
            signature,
            issue,
            issue_self_assignment,
        ),
        ModalState::ConfirmIssueDirtyCopy {
            agent_id,
            work_dir,
            signature,
            payload,
            ..
        } => super::issues_send::confirm_issue_dirty_copy_enter(
            app_state, ctx, agent_id, work_dir, signature, payload,
        ),
        ModalState::ConfirmIssueOriginMismatch {
            agent_id,
            work_dir,
            signature,
            payload,
            ..
        } => super::issues_send::confirm_issue_origin_mismatch_enter(
            app_state, ctx, agent_id, work_dir, signature, payload,
        ),
        _ => {}
    }
}

/// Returns true when the confirm modal's focused button is Cancel (issue #228).
pub(super) fn confirm_focus_is_cancel(modal: &ModalState) -> bool {
    match modal {
        ModalState::ConfirmDeleteAgent { confirm_focus, .. }
        | ModalState::ConfirmDeleteRepository { confirm_focus, .. }
        | ModalState::ConfirmKillAgent { confirm_focus, .. }
        | ModalState::ConfirmServerLostRecovery { confirm_focus, .. }
        | ModalState::PreflightPrompt { confirm_focus, .. }
        | ModalState::ConfirmIssueDirtyCopy { confirm_focus, .. }
        | ModalState::ConfirmIssueOriginMismatch { confirm_focus, .. } => {
            *confirm_focus == ConfirmFocus::Cancel
        }
        _ => false,
    }
}

fn confirm_delete_agent(
    app_state: &mut AppStateHandle,
    ctx: &SharedContext,
    id: AgentId,
    delete_work_dir: bool,
) {
    reap_orphan_before_delete(app_state, &id);
    kill_agent_before_delete(ctx, &id);

    let mut state = app_state.write();
    let _ = jefe::state::delete_selected_agent(&mut state, &id, delete_work_dir);
    state.modal = ModalState::None;
    let persisted = durable_save_request(&mut state);
    drop(state);
    schedule_durable_save(ctx, persisted);
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
        reap_orphan_before_delete(app_state, agent_id);
        kill_agent_before_delete(ctx, agent_id);
    }

    let mut state = app_state.write();
    jefe::state::delete_selected_repository(&mut state, &id);
    state.modal = ModalState::None;
    let persisted = durable_save_request(&mut state);
    drop(state);
    schedule_durable_save(ctx, persisted);
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

/// Best-effort reap of any validated orphan worker before deletion (issue #332,
/// AC16). Reads the agent's recorded worker identities and reaps them alongside
/// the stale session, all non-fatal — cleanup failures never block record
/// removal, which `delete_selected_agent` performs regardless.
fn reap_orphan_before_delete(app_state: &AppStateHandle, agent_id: &AgentId) {
    let (identities, session_name) = {
        let state = app_state.read();
        let agent = state.agents.iter().find(|a| &a.id == agent_id);
        let Some(binding) = agent.and_then(|a| a.runtime_binding.as_ref()) else {
            return;
        };
        let pair = (
            binding.worker_identities.clone(),
            binding.session_name.clone(),
        );
        drop(state);
        pair
    };
    jefe::runtime::reap_orphan_session(&identities, &session_name);
}

pub(super) fn handle_form_submit(app_state: &mut AppStateHandle, ctx: &SharedContext) {
    // Check if this is a WorkflowDispatch modal submit — route it through
    // the Actions orchestration so the dispatch actually happens.
    let dispatch_info = extract_workflow_dispatch_info(app_state);
    if let Some(info) = dispatch_info {
        handle_workflow_dispatch_submit(app_state, ctx, info);
        return;
    }

    // Validate local installed-kind availability BEFORE applying SubmitForm
    // (which closes the modal). This keeps the modal open with a visible
    // error when the selected agent kind is not installed for a local
    // repository. Remote repositories bypass the check.
    if !validate_form_kind_available(app_state) {
        return;
    }

    // Capture the selected operation from the generated agent form BEFORE
    // SubmitForm consumes the result and closes the modal. The canonical
    // launch signature defaults to Resume; the generated form lets the user
    // choose among the definition-supported operations.
    let generated_operation = {
        let state_ro = app_state.read();
        match &state_ro.modal {
            ModalState::GeneratedAgent { form, .. } => Some(form.selected_operation()),
            _ => None,
        }
    };

    let is_new_agent = {
        let state_ro = app_state.read();
        matches!(
            state_ro.modal,
            ModalState::NewAgent { .. } | ModalState::GeneratedAgent { .. }
        )
    };

    let launch_after_submit = submit_form_and_snapshot_launch(app_state, ctx, is_new_agent);
    let Some((agent_id, work_dir, mut signature)) = launch_after_submit else {
        return;
    };

    // Honor the operation the user selected in the generated form, overriding
    // the canonical Resume default.
    if let Some(operation) = generated_operation {
        signature.operation = operation;
    }

    // Enforce local installed-kind availability before any launch attempt.
    // Remote repositories skip this because remote PATH resolution is
    // authoritative.
    if !super::availability::launch_available_or_error(app_state, &signature) {
        return;
    }

    if !preflight_or_prompt(app_state, ctx, &agent_id, &signature, None) {
        return;
    }
    focus_terminal_after_submit(app_state, ctx);
    let _ = execute_agent_launch(app_state, ctx, &agent_id, &work_dir, &signature, false);
}

fn validate_form_kind_available(app_state: &mut AppStateHandle) -> bool {
    let state = app_state.read();
    let (type_id, remote) = match &state.modal {
        ModalState::NewRepository { fields, .. } | ModalState::EditRepository { fields, .. } => {
            let type_id =
                match jefe::domain::agent_definition::AgentTypeId::parse(&fields.default_type_id) {
                    Ok(value) => value,
                    Err(error) => {
                        drop(state);
                        app_state.write().error_message = Some(error.to_string());
                        return false;
                    }
                };
            let remote = match jefe::state::AppState::remote_settings_from_fields(fields) {
                Ok(value) => value,
                Err(error) => {
                    drop(state);
                    app_state.write().error_message = Some(error);
                    return false;
                }
            };
            (type_id, remote)
        }
        ModalState::NewAgent {
            repository_id,
            fields,
            ..
        } => {
            let Some(repository) = state.repository_by_id(repository_id) else {
                return false;
            };
            let Ok(type_id) =
                jefe::domain::agent_definition::AgentTypeId::parse(&fields.agent_type_id)
            else {
                return false;
            };
            (type_id, repository.remote.clone())
        }
        ModalState::EditAgent { id, fields, .. } => {
            let Some(repository) = state.repository_for_agent(id) else {
                return false;
            };
            let Ok(type_id) =
                jefe::domain::agent_definition::AgentTypeId::parse(&fields.agent_type_id)
            else {
                return false;
            };
            (type_id, repository.remote.clone())
        }
        _ => return true,
    };
    drop(state);
    let request = jefe::domain::AgentLaunchRequest {
        type_id,
        values: jefe::domain::TypedMap::new(),
        work_dir: std::path::PathBuf::new(),
        remote,
        operation: jefe::domain::agent_definition::Operation::Normal,
    };
    super::availability::launch_available_or_error(app_state, &request)
}

/// Extract workflow dispatch form data if the modal is a WorkflowDispatch
/// with focus on Submit or Cancel.
struct WorkflowDispatchInfo {
    workflow_id: String,
    ref_name: String,
    inputs_raw: String,
    is_cancel: bool,
}

fn extract_workflow_dispatch_info(app_state: &AppStateHandle) -> Option<WorkflowDispatchInfo> {
    let (workflow_id, ref_name, inputs_raw, is_cancel, is_submit) = {
        let state = app_state.read();
        let ModalState::WorkflowDispatch {
            workflow,
            fields,
            focus,
            ..
        } = &state.modal
        else {
            return None;
        };
        let is_cancel = matches!(focus, jefe::state::WorkflowDispatchFormFocus::Cancel);
        let is_submit = matches!(focus, jefe::state::WorkflowDispatchFormFocus::Submit);
        let info = (
            workflow.id.to_string(),
            fields.ref_name.clone(),
            fields.inputs.clone(),
            is_cancel,
            is_submit,
        );
        drop(state);
        info
    };
    if !is_submit && !is_cancel {
        return None;
    }
    Some(WorkflowDispatchInfo {
        workflow_id,
        ref_name,
        inputs_raw,
        is_cancel,
    })
}

/// Handle a WorkflowDispatch submit: close the modal and dispatch the workflow
/// (or just close if Cancel).
fn handle_workflow_dispatch_submit(
    app_state: &mut AppStateHandle,
    ctx: &SharedContext,
    info: WorkflowDispatchInfo,
) {
    if info.is_cancel {
        close_modal_and_persist(app_state, ctx);
        return;
    }
    // Validate ref_name
    let trimmed_ref = info.ref_name.trim();
    if trimmed_ref.is_empty() {
        let mut state = app_state.write();
        state.actions_state.error = Some("Ref name is required".to_string());
        let persisted = durable_save_request(&mut state);
        drop(state);
        schedule_durable_save(ctx, persisted);
        return;
    }
    // Parse inputs (cheap, no state access).
    let inputs = jefe::state::AppState::parse_workflow_dispatch_inputs(&info.inputs_raw);
    // Validate the repository BEFORE closing the modal: if there is no
    // selected repository, surface an error and keep the modal open so the
    // user sees the failure instead of a silent no-op dispatch.
    let scope_repo_id = {
        let state = app_state.read();
        state.selected_repository().map(|r| r.id.clone())
    };
    // Validate the repository BEFORE closing the modal: if there is no
    // selected repository, surface an error and keep the modal open so the
    // user sees the failure instead of a silent no-op dispatch.
    let Some(scope_repo_id) = scope_repo_id else {
        let mut state = app_state.write();
        state.actions_state.error = Some("No repository selected".to_string());
        let persisted = durable_save_request(&mut state);
        drop(state);
        schedule_durable_save(ctx, persisted);
        return;
    };
    // All validation passed — close the modal now so the dispatch proceeds.
    close_modal_and_persist(app_state, ctx);
    let message = jefe::messages::ActionsMessage::WorkflowDispatchSubmitted {
        scope_repo_id,
        workflow_id: info.workflow_id,
        ref_name: trimmed_ref.to_string(),
        inputs,
    };
    super::actions_orchestration::dispatch_actions_message(app_state, ctx, message);
}

fn submit_form_and_snapshot_launch(
    app_state: &mut AppStateHandle,
    ctx: &SharedContext,
    is_new_agent: bool,
) -> Option<(AgentId, std::path::PathBuf, AgentLaunchRequest)> {
    let package_probe_plan = {
        let state = app_state.read();
        super::new_agent_submit::new_agent_package_probe_plan(&state)
    };
    let package_probe_result = super::new_agent_submit::execute_new_agent_package_probe(
        &package_probe_plan,
        jefe::runtime::require_launch_package_available,
    );

    let mut state = app_state.write();
    if !super::new_agent_submit::apply_form_submit_after_package_probe(
        &mut state,
        package_probe_result,
    ) {
        return None;
    }

    let launch_after_submit = if is_new_agent && state.modal == ModalState::None {
        state.selected_agent().cloned().and_then(|agent| {
            state
                .repository_by_id(&agent.repository_id)
                .map(|repository| {
                    let signature = launch_signature_for_new_agent(&agent, repository);
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

    let persisted = durable_save_request(&mut state);
    drop(state);
    schedule_durable_save(ctx, persisted);
    launch_after_submit
}

fn focus_terminal_after_submit(app_state: &mut AppStateHandle, ctx: &SharedContext) {
    let mut state = app_state.write();
    focus_terminal_state(&mut state);
    let persisted = durable_save_request(&mut state);
    drop(state);
    schedule_durable_save(ctx, persisted);
}

pub(super) fn focus_terminal_state(state: &mut AppState) {
    state.pane_focus = PaneFocus::Terminal;
    state.terminal_focused = true;
}

pub(super) fn handle_form_space(
    app_state: &mut AppStateHandle,
    ctx: &SharedContext,
) -> Option<AppEvent> {
    match focused_form_field(app_state) {
        FocusedFormField::Repository(focus) if repository_focus_toggles_checkbox(focus) => {
            Some(AppEvent::FormToggleCheckbox)
        }
        FocusedFormField::Agent(
            AgentFormFocus::AgentType
            | AgentFormFocus::PassContinue
            | AgentFormFocus::Sandbox
            | AgentFormFocus::Shortcut,
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
            .as_llxprt_arg()
            .clone_into(&mut fields.sandbox_engine);
    }
    let persisted = durable_save_request(&mut state);
    drop(state);
    schedule_durable_save(ctx, persisted);
}

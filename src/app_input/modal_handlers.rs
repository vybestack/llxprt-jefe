//! Modal/confirm/form key handlers.
//!
//! Extracted from mod.rs to keep file sizes manageable.

use iocraft::prelude::*;
use tracing::warn;

use jefe::domain::{AgentId, LaunchSignature, SandboxEngine};
use jefe::persistence::PersistenceManager;
use jefe::runtime::{RuntimeError, RuntimeManager};
use jefe::state::{
    AgentFormFocus, AppEvent, AuthDialogPhase, ConfirmFocus, ModalState, PaneFocus,
    RepositoryFormFocus,
};
use jefe::theme::ThemeManager;

use super::{
    AppStateHandle, SharedContext, apply_and_persist, auth_remediation, close_modal_and_persist,
    execute_agent_launch, launch_signature_for_agent, persist_state, preflight_or_prompt,
    repository_focus_toggles_checkbox, to_persisted_state,
};

pub fn handle_f12_toggle(app_state: &mut AppStateHandle, ctx: &SharedContext) {
    // Issue #301 Phase 5: F12 is now pure intent. It updates pane_focus /
    // terminal_focused deterministically and persists the state change. The
    // actual runtime attach is performed asynchronously by the background
    // attach future (Phase 3) driven by the AttachScheduler's desired target.
    // The render body sets desired from `selected_running_agent_id`, so F12
    // just flips the focus intent — no synchronous `runtime.attach()` call.
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
        *state = std::mem::take(&mut *state).apply(AppEvent::ToggleTerminalFocus);
    } else {
        let selected_running_agent_id = state
            .selected_agent()
            .filter(|agent| agent.is_running())
            .map(|agent| agent.id.clone());

        if selected_running_agent_id.is_some() {
            state.pane_focus = PaneFocus::Terminal;
            *state = std::mem::take(&mut *state).apply(AppEvent::ToggleTerminalFocus);
        } else {
            state.pane_focus = PaneFocus::Agents;
            state.terminal_focused = false;
        }
    }
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
) -> bool {
    match key_event.code {
        KeyCode::Esc | KeyCode::Char('n' | 'N') => close_modal_and_persist(app_state, ctx),
        KeyCode::Left | KeyCode::Right | KeyCode::Tab | KeyCode::BackTab => {
            apply_and_persist(app_state, ctx, AppEvent::ConfirmCycleFocus);
        }
        KeyCode::Enter => handle_confirm_enter(app_state, ctx),
        KeyCode::Char(' ' | 'd' | 'D') | KeyCode::Up | KeyCode::Down => {
            apply_and_persist(app_state, ctx, AppEvent::ToggleDeleteWorkDir);
        }
        _ => {}
    }
    true
}

/// Handle keys while the in-app device-code auth dialog is open (issue #244).
///
/// - Esc: cancel the flow (dismiss; sets an actionable error_message).
/// - `r` / Enter when `Failed`: retry the device-code flow.
/// - All other keys are ignored — the dialog is not text-editable; the code +
///   URL are displayed for the user to act on in a browser.
///
/// # Orphaned `gh` on cancel
/// Esc closes the modal but does NOT kill the background `gh auth login`
/// subprocess (its `Child` handle is not retained across the dispatch seam).
/// This is accepted for v1: `gh`'s device-code flow has a server-side expiry
/// (~15 min), and with stdin null + `GH_BROWSER=/bin/true` it exits on its own
/// once the code expires or the user authorizes elsewhere. The leak is bounded
/// and inert (issue #244).
///
/// Returns `true` so the caller short-circuits (the auth modal consumes the
/// key), mirroring the form/search handlers.
pub fn handle_mode_auth_key(
    app_state: &mut AppStateHandle,
    ctx: &SharedContext,
    key_event: &KeyEvent,
) -> bool {
    let in_failed_phase = {
        let state = app_state.read();
        matches!(
            &state.modal,
            ModalState::Auth {
                state: dialog
            } if matches!(dialog.phase, AuthDialogPhase::Failed { .. })
        )
    };

    match key_event.code {
        KeyCode::Esc => apply_and_persist(app_state, ctx, AppEvent::AuthCancelled),
        KeyCode::Char('r' | 'R') | KeyCode::Enter if in_failed_phase => {
            apply_and_persist(app_state, ctx, AppEvent::AuthRetry);
            auth_remediation::spawn_device_auth_flow(app_state, ctx);
        }
        _ => {}
    }
    true
}

fn handle_confirm_enter(app_state: &mut AppStateHandle, ctx: &SharedContext) {
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

/// Handle keys while the theme picker modal is open.
///
/// - Up/Down: move the selection cursor (via reducer) and **live-preview** the
///   newly-selected theme by applying it to the `ThemeManager` in memory (no
///   persistence). The render loop reads the manager each frame, so colors
///   update instantly as the user navigates.
/// - Enter: persist the previewed theme to `settings.toml` and close the
///   picker. Falls back to Green Screen if the slug is invalid.
/// - Esc: revert the manager back to the theme that was active when the
///   picker opened (`active_slug`), then close without persisting.
pub fn handle_mode_theme_picker_key(
    app_state: &mut AppStateHandle,
    ctx: &SharedContext,
    key_event: &KeyEvent,
) {
    match key_event.code {
        KeyCode::Up => {
            apply_and_persist(app_state, ctx, AppEvent::ThemePickerNavigateUp);
            preview_theme_selection(app_state, ctx);
        }
        KeyCode::Down => {
            apply_and_persist(app_state, ctx, AppEvent::ThemePickerNavigateDown);
            preview_theme_selection(app_state, ctx);
        }
        KeyCode::Tab => {
            // Pure modal-flag toggle (issue #179). The runtime mirror is only
            // committed on Enter via the reducer, so no preview/restore needed.
            apply_and_persist(app_state, ctx, AppEvent::ThemePickerToggleOverride);
        }
        KeyCode::Enter => {
            apply_theme_picker_selection(app_state, ctx);
        }
        KeyCode::Esc => {
            revert_theme_to_active(app_state, ctx);
            apply_and_persist(app_state, ctx, AppEvent::CloseThemePicker);
        }
        _ => {}
    }
}

/// Apply the currently-selected theme to the `ThemeManager` **in memory only**
/// (no persistence), so the user can live-preview themes as they navigate.
///
/// Called after each Up/Down navigation moves `selected_index`. The render
/// loop reads `theme_manager.active_theme()` each frame, so the new colors
/// take effect on the next render. Persistence only happens on Enter.
fn preview_theme_selection(app_state: &AppStateHandle, ctx: &SharedContext) {
    let selected_slug = {
        let state = app_state.read();
        match &state.modal {
            ModalState::ThemePicker {
                available_themes,
                selected_index,
                ..
            } => available_themes
                .get(*selected_index)
                .map(|(slug, _)| slug.clone()),
            _ => None,
        }
    };

    if let Some(slug) = selected_slug
        && let Some(ctx_arc) = ctx
        && let Ok(mut ctx_guard) = ctx_arc.lock()
        && let Err(e) = ctx_guard.theme_manager.set_active(&slug)
    {
        warn!(error = %e, theme = %slug, "theme picker: preview fell back to Green Screen");
    }
}

/// Restore the `ThemeManager` to the theme that was active when the picker
/// opened (`active_slug`), discarding any live-preview changes. Called on Esc
/// so cancelling reverts the visible colors to what the user had before.
fn revert_theme_to_active(app_state: &AppStateHandle, ctx: &SharedContext) {
    let active_slug = {
        let state = app_state.read();
        match &state.modal {
            ModalState::ThemePicker { active_slug, .. } => Some(active_slug.clone()),
            _ => None,
        }
    };

    // Only the theme selection was live-previewed (into the ThemeManager), so
    // reverting it is sufficient. The override toggle lives entirely in the
    // modal and is discarded on cancel (issue #179) — no restore needed.
    if let Some(slug) = active_slug
        && let Some(ctx_arc) = ctx
        && let Ok(mut ctx_guard) = ctx_arc.lock()
        && let Err(e) = ctx_guard.theme_manager.set_active(&slug)
    {
        warn!(error = %e, theme = %slug, "theme picker: could not revert preview on cancel");
    }
}

/// Apply the selected theme from the picker, persist to settings.toml, then close.
fn apply_theme_picker_selection(app_state: &mut AppStateHandle, ctx: &SharedContext) {
    // Read both the selected slug and the in-dialog override toggle in a single
    // short read lock (issue #179).
    let (selected_slug, override_theme) = {
        let state = app_state.read();
        match &state.modal {
            ModalState::ThemePicker {
                available_themes,
                selected_index,
                override_theme,
                ..
            } => (
                available_themes
                    .get(*selected_index)
                    .map(|(slug, _)| slug.clone()),
                *override_theme,
            ),
            _ => (None, state.override_agent_theme),
        }
    };

    // Apply the theme to the ThemeManager and read back the active slug
    // + settings, all under a single short lock.
    if let Some(slug) = selected_slug
        && let Some(ctx_arc) = &ctx
    {
        let save_action = match ctx_arc.lock() {
            Ok(mut ctx_guard) => {
                if let Err(e) = ctx_guard.theme_manager.set_active(&slug) {
                    warn!(error = %e, theme = %slug, "theme picker: invalid selection, fell back to Green Screen");
                }
                let active_slug = ctx_guard.theme_manager.active_theme().slug.clone();
                let path = ctx_guard.persistence.settings_path();
                match ctx_guard.persistence.load_settings() {
                    Ok(mut settings) => {
                        settings.theme = active_slug;
                        settings.override_agent_theme = override_theme;
                        Some((settings, path))
                    }
                    Err(e) => {
                        // Don't save — writing defaults would destroy existing settings.
                        warn!(error = %e, "could not load settings; skipping theme persistence");
                        None
                    }
                }
            }
            Err(_) => {
                // Lock failed — skip persistence but still close the picker below.
                None
            }
        };
        // File I/O outside the mutex lock.
        if let Some((settings, path)) = save_action
            && let Err(e) =
                jefe::persistence::FilePersistenceManager::save_settings_to(&settings, &path)
        {
            warn!(error = %e, "could not persist theme selection");
        }
    }

    // Close the picker regardless of persistence outcome.
    apply_and_persist(app_state, ctx, AppEvent::ThemePickerConfirm);
}
fn handle_form_submit(app_state: &mut AppStateHandle, ctx: &SharedContext) {
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

    let is_new_agent = {
        let state_ro = app_state.read();
        matches!(state_ro.modal, ModalState::NewAgent { .. })
    };

    let launch_after_submit = submit_form_and_snapshot_launch(app_state, ctx, is_new_agent);
    let Some((agent_id, work_dir, signature)) = launch_after_submit else {
        return;
    };

    // Enforce local installed-kind availability before any launch attempt.
    // Remote repositories skip this because remote PATH resolution is
    // authoritative.
    if !super::availability::local_kind_available_or_error(
        app_state,
        signature.agent_kind,
        &signature.remote,
    ) {
        return;
    }

    if !preflight_or_prompt(app_state, ctx, &agent_id, &signature, None) {
        return;
    }
    focus_terminal_after_submit(app_state, ctx);
    let _ = execute_agent_launch(app_state, ctx, &agent_id, &work_dir, &signature, false);
}

/// Pre-submit validation: check that the selected agent kind is locally
/// installed for local repositories. For repository forms, validates the
/// `default_agent_kind` field. For agent forms, validates the `agent_kind`
/// field. Sets a visible error and returns `false` (modal stays open) when
/// the kind is not installed and the repository is not remote-enabled.
fn validate_form_kind_available(app_state: &mut AppStateHandle) -> bool {
    use jefe::domain::{AgentKind, RemoteRepositorySettings};

    let state = app_state.read();
    let selection = match &state.modal {
        ModalState::NewRepository { fields, .. } | ModalState::EditRepository { fields, .. } => {
            let kind = AgentKind::from_form_value(&fields.default_agent_kind).unwrap_or_default();
            jefe::state::AppState::remote_settings_from_fields(fields).map(|remote| (kind, remote))
        }
        ModalState::NewAgent {
            repository_id,
            fields,
            ..
        } => {
            let kind = AgentKind::from_form_value(&fields.agent_kind).unwrap_or_default();
            let remote = state
                .repository_by_id(repository_id)
                .map_or_else(RemoteRepositorySettings::default, |repo| {
                    repo.remote.clone()
                });
            Ok((kind, remote))
        }
        ModalState::EditAgent { id, fields, .. } => {
            let kind = AgentKind::from_form_value(&fields.agent_kind).unwrap_or_default();
            let remote = state
                .repository_for_agent(id)
                .map_or_else(RemoteRepositorySettings::default, |repo| {
                    repo.remote.clone()
                });
            Ok((kind, remote))
        }
        _ => return true,
    };
    drop(state);
    let (kind, remote) = match selection {
        Ok(selection) => selection,
        Err(error) => {
            app_state.write().error_message = Some(error);
            return false;
        }
    };

    super::availability::local_kind_available_or_error(app_state, kind, &remote)
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
        let persisted = to_persisted_state(&state);
        drop(state);
        persist_state(ctx, &persisted);
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
        let persisted = to_persisted_state(&state);
        drop(state);
        persist_state(ctx, &persisted);
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
            AgentFormFocus::AgentKind
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
            .label()
            .clone_into(&mut fields.sandbox_engine);
    }
    let persisted = to_persisted_state(&state);
    drop(state);
    persist_state(ctx, &persisted);
}

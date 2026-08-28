use jefe::domain::agent_definition::AgentDefinition;
use jefe::domain::canonical_values::typed_field;
use jefe::domain::{AgentId, AgentLaunchRequest, SandboxEngine, TypedValue};
use jefe::runtime::{
    PreflightAction, PreflightIssue, execute_preflight_action, sandbox_preflight,
    validate_launch_request,
};
use jefe::state::ModalState;

use super::{
    AppStateHandle, SharedContext, durable_save_request, execute_agent_launch,
    schedule_durable_save,
};

/// Run launch preflight checks and either show a prompt or proceed with launch.
///
/// Returns `true` if the launch can proceed immediately (no issues, or the
/// launch is not gated by host sandbox state). Returns `false` if a
/// `PreflightPrompt` modal was opened and the caller should abort the
/// immediate launch path.
pub fn preflight_or_prompt(
    app_state: &mut AppStateHandle,
    ctx: &SharedContext,
    agent_id: &AgentId,
    signature: &AgentLaunchRequest,
    issue_self_assignment: Option<&jefe::state::IssueSelfAssignmentFollowUp>,
) -> bool {
    let Some(issue) = launch_preflight_issue(signature, sandbox_preflight) else {
        return true;
    };
    open_preflight_prompt(
        app_state,
        ctx,
        agent_id,
        signature,
        issue,
        issue_self_assignment,
    );
    false
}

/// Decide which issue, if any, must be prompted before this launch runs.
///
/// Runtime-option validation comes first: an unlaunchable request is rejected
/// before jefe inspects the host at all. A launch that is gated by host sandbox
/// state (see [`sandbox_preflight_engine`]) then consults `host_check`, which
/// production supplies as [`jefe::runtime::sandbox_preflight`].
pub(super) fn launch_preflight_issue(
    signature: &AgentLaunchRequest,
    host_check: impl Fn(SandboxEngine) -> Option<PreflightIssue>,
) -> Option<PreflightIssue> {
    if let Err(diagnostic) = validate_launch_request(signature) {
        return Some(PreflightIssue::UnsupportedRuntimeOption {
            diagnostic: diagnostic.to_string(),
        });
    }

    sandbox_preflight_engine(signature).and_then(host_check)
}

/// The sandbox engine whose host state gates this launch, if any.
///
/// Returns `None`, meaning the launch is not gated on host sandbox state,
/// when any of the following holds:
///
/// - the target is remote, because the local container daemon and the local
///   SSH agent describe the wrong machine;
/// - the active definition declares no `sandbox_enabled` field, so a stale
///   value persisted from a sandbox-capable agent cannot gate it;
/// - the request has the sandbox switched off.
pub(super) fn sandbox_preflight_engine(signature: &AgentLaunchRequest) -> Option<SandboxEngine> {
    if signature.remote.enabled {
        return None;
    }

    let definition = AgentDefinition::shipped()
        .into_iter()
        .find(|definition| definition.id == signature.type_id)?;
    let declares_sandbox = definition
        .agent_fields
        .iter()
        .chain(definition.repository_fields.iter())
        .any(|field| field.id == "sandbox_enabled");
    if !declares_sandbox {
        return None;
    }

    if !matches!(
        typed_field(&signature.values, "sandbox_enabled"),
        Some(TypedValue::Bool(true))
    ) {
        return None;
    }

    let engine = match typed_field(&signature.values, "sandbox_engine") {
        Some(TypedValue::String(value)) => SandboxEngine::from_form_value(value),
        _ => None,
    };
    Some(engine.unwrap_or_default())
}

fn open_preflight_prompt(
    app_state: &mut AppStateHandle,
    ctx: &SharedContext,
    agent_id: &AgentId,
    signature: &AgentLaunchRequest,
    issue: PreflightIssue,
    issue_self_assignment: Option<&jefe::state::IssueSelfAssignmentFollowUp>,
) {
    let mut state = app_state.write();
    state.modal = ModalState::PreflightPrompt {
        agent_id: agent_id.clone(),
        signature: signature.clone(),
        issue,
        remaining_issues: Vec::new(),
        issue_self_assignment: issue_self_assignment.cloned(),
        confirm_focus: jefe::state::ConfirmFocus::Cancel,
    };
    let persisted = durable_save_request(&mut state);
    drop(state);
    schedule_durable_save(ctx, persisted);
}

/// Handle preflight prompt confirmation: execute remediation, re-check, then launch.
///
/// The re-check uses the same gate as the launch paths, so a host that is still
/// not ready after remediation prompts again instead of launching an agent that
/// would fail inside the sandbox.
pub(super) fn handle_preflight_prompt_enter(
    app_state: &mut AppStateHandle,
    ctx: &SharedContext,
    agent_id: AgentId,
    mut signature: AgentLaunchRequest,
    issue: PreflightIssue,
    issue_self_assignment: Option<jefe::state::IssueSelfAssignmentFollowUp>,
) {
    if !apply_preflight_action(app_state, ctx, &agent_id, &mut signature, issue.action()) {
        return;
    }

    let next = sandbox_preflight_engine(&signature).and_then(sandbox_preflight);
    if let Some(next) = next {
        persist_next_preflight(
            app_state,
            ctx,
            agent_id,
            signature,
            next,
            issue_self_assignment,
        );
    } else {
        persist_launch_resume(app_state, ctx);
        let launch_ok = execute_agent_launch(
            app_state,
            ctx,
            &agent_id,
            &signature.work_dir,
            &signature,
            false,
        )
        .is_ok();
        // Fire the non-blocking issue self-assignment carried from the
        // issue-driven launch path ONLY on a successful launch (issue #186).
        // No-op for non-issue launches; no assignment when the resumed launch
        // failed. The decision is pure (post_preflight_assignment_action).
        super::issues_send::spawn_post_preflight_issue_self_assignment(
            app_state,
            ctx,
            launch_ok,
            issue_self_assignment.as_ref(),
        );
    }
}

fn apply_preflight_action(
    app_state: &mut AppStateHandle,
    ctx: &SharedContext,
    agent_id: &AgentId,
    signature: &mut AgentLaunchRequest,
    action: PreflightAction,
) -> bool {
    match action {
        PreflightAction::SwitchEngine(target_engine) => {
            apply_engine_switch(app_state, ctx, agent_id, signature, target_engine)
        }
        PreflightAction::NoRemediation => {
            persist_modal_close(app_state, ctx, None);
            false
        }
        _ => match execute_preflight_action(&action) {
            Ok(()) => true,
            Err(error) => {
                persist_modal_close(app_state, ctx, Some(error));
                false
            }
        },
    }
}

fn apply_engine_switch(
    app_state: &mut AppStateHandle,
    ctx: &SharedContext,
    _agent_id: &AgentId,
    _signature: &mut AgentLaunchRequest,
    _target_engine: jefe::domain::SandboxEngine,
) -> bool {
    persist_modal_close(
        app_state,
        ctx,
        Some("sandbox remediation must be declared by the active agent definition".to_owned()),
    );
    false
}

fn persist_next_preflight(
    app_state: &mut AppStateHandle,
    ctx: &SharedContext,
    agent_id: AgentId,
    signature: AgentLaunchRequest,
    issue: PreflightIssue,
    issue_self_assignment: Option<jefe::state::IssueSelfAssignmentFollowUp>,
) {
    let mut state = app_state.write();
    state.modal = ModalState::PreflightPrompt {
        agent_id,
        signature,
        issue,
        remaining_issues: Vec::new(),
        issue_self_assignment,
        confirm_focus: jefe::state::ConfirmFocus::Cancel,
    };
    persist_state_guard(ctx, state);
}

fn persist_launch_resume(app_state: &mut AppStateHandle, ctx: &SharedContext) {
    let mut state = app_state.write();
    state.modal = ModalState::None;
    state.terminal_focused = true;
    persist_state_guard(ctx, state);
}

fn persist_modal_close(
    app_state: &mut AppStateHandle,
    ctx: &SharedContext,
    error_message: Option<String>,
) {
    let mut state = app_state.write();
    state.modal = ModalState::None;
    state.error_message = error_message;
    persist_state_guard(ctx, state);
}

fn persist_state_guard(
    ctx: &SharedContext,
    mut state: iocraft::hooks::StateMutRef<'_, jefe::state::AppState>,
) {
    let persisted = durable_save_request(&mut state);
    drop(state);
    schedule_durable_save(ctx, persisted);
}

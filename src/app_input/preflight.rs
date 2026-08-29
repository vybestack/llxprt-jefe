use jefe::domain::{AgentId, AgentLaunchRequest};
use jefe::runtime::{
    PreflightAction, PreflightIssue, execute_preflight_action, validate_launch_request,
};
use jefe::state::screen_overlays::ConfirmationRequest;

use super::{
    AppStateHandle, SharedContext, durable_save_request, execute_agent_launch,
    schedule_durable_save,
};

/// Run sandbox preflight checks and either show a prompt or proceed with launch.
///
/// Returns `true` if the launch can proceed immediately (no issues or sandbox
/// not enabled). Returns `false` if a `PreflightPrompt` modal was opened and
/// the caller should abort the immediate launch path.
///
/// Preflight is gated to [`jefe::domain::shipped_agent_type(3)`] only: CodePuppy does not use
/// the LLxprt sandbox flags/engine, and stale `sandbox_enabled`/`sandbox_engine`
/// fields persisted from a prior LLxprt configuration must not trigger LLxprt
/// preflight for a CodePuppy agent.
pub fn preflight_or_prompt(
    app_state: &mut AppStateHandle,
    ctx: &SharedContext,
    agent_id: &AgentId,
    signature: &AgentLaunchRequest,
    issue_self_assignment: Option<&jefe::state::IssueSelfAssignmentFollowUp>,
) -> bool {
    if let Err(diagnostic) = validate_launch_request(signature) {
        open_preflight_prompt(
            app_state,
            ctx,
            agent_id,
            signature,
            PreflightIssue::UnsupportedRuntimeOption {
                diagnostic: diagnostic.to_string(),
            },
            issue_self_assignment,
        );
        return false;
    }

    true
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
    let opened = state.open_confirmation_payload(ConfirmationRequest::Preflight {
        agent_id: agent_id.clone(),
        signature: signature.clone(),
        issue,
        remaining_issues: Vec::new(),
        issue_self_assignment: issue_self_assignment.cloned(),
    });
    if !opened {
        // Refusal means an occupied/undeclared overlay already owns the
        // presentation; persist nothing so the active overlay survives and the
        // refusal is not rewritten as a no-op save.
        return;
    }
    let persisted = durable_save_request(&mut state);
    drop(state);
    schedule_durable_save(ctx, persisted);
}

/// Handle preflight prompt confirmation: execute remediation, re-check, then launch.
///
/// Preflight is LLxprt-only: CodePuppy does not have a sandbox subsystem and
/// must not run LLxprt preflight even when stale `sandbox_enabled` is true.
pub(super) fn handle_preflight_prompt_enter(
    app_state: &mut AppStateHandle,
    ctx: &SharedContext,
    expected: &ConfirmationRequest,
) {
    let ConfirmationRequest::Preflight {
        agent_id,
        signature,
        issue,
        issue_self_assignment,
        ..
    } = expected
    else {
        return;
    };
    let agent_id = agent_id.clone();
    let mut signature = signature.clone();
    let issue = issue.clone();
    let issue_self_assignment = issue_self_assignment.clone();
    if !super::close_expected_generic_confirmation_and_persist(app_state, ctx, expected) {
        return;
    }
    if !apply_preflight_action(app_state, ctx, &agent_id, &mut signature, issue.action()) {
        return;
    }

    let next = None;
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
    state.open_confirmation_payload(ConfirmationRequest::Preflight {
        agent_id,
        signature,
        issue,
        remaining_issues: Vec::new(),
        issue_self_assignment,
    });
    persist_state_guard(ctx, state);
}

fn persist_launch_resume(app_state: &mut AppStateHandle, ctx: &SharedContext) {
    let mut state = app_state.write();
    state.terminal_focused = true;
    persist_state_guard(ctx, state);
}

fn persist_modal_close(
    app_state: &mut AppStateHandle,
    ctx: &SharedContext,
    error_message: Option<String>,
) {
    let mut state = app_state.write();
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

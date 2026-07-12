use jefe::domain::{AgentId, AgentKind, LaunchSignature, PlatformCapabilities, SandboxEngine};
use jefe::runtime::{PreflightAction, PreflightIssue, execute_preflight_action, sandbox_preflight};
use jefe::state::ModalState;

use super::{
    AppStateHandle, SharedContext, execute_agent_launch, persist_state, to_persisted_state,
};

/// Run sandbox preflight checks and either show a prompt or proceed with launch.
///
/// Returns `true` if the launch can proceed immediately (no issues or sandbox
/// not enabled). Returns `false` if a `PreflightPrompt` modal was opened and
/// the caller should abort the immediate launch path.
///
/// Preflight is gated to [`AgentKind::Llxprt`] only: CodePuppy does not use
/// the LLxprt sandbox flags/engine, and stale `sandbox_enabled`/`sandbox_engine`
/// fields persisted from a prior LLxprt configuration must not trigger LLxprt
/// preflight for a CodePuppy agent.
pub fn preflight_or_prompt(
    app_state: &mut AppStateHandle,
    ctx: &SharedContext,
    agent_id: &AgentId,
    signature: &LaunchSignature,
    issue_self_assignment: Option<&jefe::state::IssueSelfAssignmentFollowUp>,
) -> bool {
    if !should_run_sandbox_preflight(signature) {
        return true;
    }

    if let Some(issue) = sandbox_preflight(signature.sandbox_engine) {
        let mut state = app_state.write();
        state.modal = ModalState::PreflightPrompt {
            agent_id: agent_id.clone(),
            signature: signature.clone(),
            issue,
            remaining_issues: Vec::new(),
            issue_self_assignment: issue_self_assignment.cloned(),
        };
        let persisted = to_persisted_state(&state);
        drop(state);
        persist_state(ctx, &persisted);
        return false;
    }

    true
}

/// Pure predicate: should sandbox preflight run for this signature?
///
/// Preflight runs only when **both** conditions hold:
/// 1. `sandbox_enabled` is true, AND
/// 2. `agent_kind == Llxprt` (CodePuppy has no LLxprt sandbox subsystem).
///
/// This gates out CodePuppy agents that carry stale `sandbox_enabled = true`
/// from persisted edit data — they must not run LLxprt preflight.
#[must_use]
pub(super) fn should_run_sandbox_preflight(signature: &LaunchSignature) -> bool {
    signature.sandbox_enabled && signature.agent_kind == AgentKind::Llxprt
}

/// Handle preflight prompt confirmation: execute remediation, re-check, then launch.
///
/// Preflight is LLxprt-only: CodePuppy does not have a sandbox subsystem and
/// must not run LLxprt preflight even when stale `sandbox_enabled` is true.
pub(super) fn handle_preflight_prompt_enter(
    app_state: &mut AppStateHandle,
    ctx: &SharedContext,
    agent_id: AgentId,
    mut signature: LaunchSignature,
    issue: PreflightIssue,
    issue_self_assignment: Option<jefe::state::IssueSelfAssignmentFollowUp>,
) {
    if !apply_preflight_action(app_state, ctx, &agent_id, &mut signature, issue.action()) {
        return;
    }

    // Gate preflight re-check to LLxprt — CodePuppy should never have reached
    // this modal, but if it did, skip further sandbox preflight.
    let next = if signature.agent_kind == AgentKind::Llxprt {
        sandbox_preflight(signature.sandbox_engine)
    } else {
        None
    };
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
        // failed.
        if launch_ok {
            super::issues_send::spawn_post_preflight_issue_self_assignment(
                app_state,
                ctx,
                issue_self_assignment.as_ref(),
            );
        }
    }
}

fn apply_preflight_action(
    app_state: &mut AppStateHandle,
    ctx: &SharedContext,
    agent_id: &AgentId,
    signature: &mut LaunchSignature,
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
    agent_id: &AgentId,
    signature: &mut LaunchSignature,
    target_engine: SandboxEngine,
) -> bool {
    let caps = PlatformCapabilities::current();
    let Some(normalized_engine) = caps.normalize_engine(target_engine) else {
        persist_modal_close(
            app_state,
            ctx,
            Some(format!(
                "No supported sandbox engines are available on {}. Disable sandbox to continue.",
                caps.platform_label()
            )),
        );
        return false;
    };

    signature.sandbox_engine = normalized_engine;
    let mut state = app_state.write();
    if let Some(agent) = state.agents.iter_mut().find(|agent| agent.id == *agent_id) {
        agent.sandbox_engine = normalized_engine;
    }
    drop(state);
    true
}

fn persist_next_preflight(
    app_state: &mut AppStateHandle,
    ctx: &SharedContext,
    agent_id: AgentId,
    signature: LaunchSignature,
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
    state: iocraft::hooks::StateMutRef<'_, jefe::state::AppState>,
) {
    let persisted = to_persisted_state(&state);
    drop(state);
    persist_state(ctx, &persisted);
}

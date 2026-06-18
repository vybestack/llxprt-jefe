use jefe::domain::{AgentId, LaunchSignature, PlatformCapabilities, SandboxEngine};
use jefe::runtime::{PreflightAction, PreflightIssue, execute_preflight_action, sandbox_preflight};
use jefe::state::ModalState;

use super::{
    AppStateHandle, SharedContext, execute_agent_launch, persist_state, to_persisted_state,
};

/// Handle preflight prompt confirmation: execute remediation, re-check, then launch.
pub(super) fn handle_preflight_prompt_enter(
    app_state: &mut AppStateHandle,
    ctx: &SharedContext,
    agent_id: AgentId,
    mut signature: LaunchSignature,
    issue: PreflightIssue,
) {
    if !apply_preflight_action(app_state, ctx, &agent_id, &mut signature, issue.action()) {
        return;
    }

    if let Some(next) = sandbox_preflight(signature.sandbox_engine) {
        persist_next_preflight(app_state, ctx, agent_id, signature, next);
    } else {
        persist_launch_resume(app_state, ctx);
        execute_agent_launch(
            app_state,
            ctx,
            &agent_id,
            &signature.work_dir,
            &signature,
            false,
        );
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
) {
    let mut state = app_state.write();
    state.modal = ModalState::PreflightPrompt {
        agent_id,
        signature,
        issue,
        remaining_issues: Vec::new(),
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

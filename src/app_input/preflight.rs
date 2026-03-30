use jefe::domain::{AgentId, LaunchSignature, PlatformCapabilities, SandboxEngine};
use jefe::runtime::{PreflightAction, PreflightIssue, execute_preflight_action, sandbox_preflight};
use jefe::state::ModalState;

use super::{AppStateHandle, SharedContext, execute_agent_launch, persist_state_snapshot};

/// Handle preflight prompt confirmation: execute remediation, re-check, then launch.
pub(super) fn handle_preflight_prompt_enter(
    app_state: &mut AppStateHandle,
    ctx: &SharedContext,
    agent_id: AgentId,
    mut signature: LaunchSignature,
    issue: PreflightIssue,
) {
    let action = issue.action();
    if matches!(action, PreflightAction::SwitchToPodman) {
        let caps = PlatformCapabilities::current();
        if let Some(normalized_engine) = caps.normalize_engine(SandboxEngine::Podman) {
            signature.sandbox_engine = normalized_engine;
            let mut state = app_state.write();
            if let Some(agent) = state.agents.iter_mut().find(|a| a.id == agent_id) {
                agent.sandbox_engine = normalized_engine;
            }
            persist_state_snapshot(ctx, &state);
        } else {
            let mut state = app_state.write();
            state.modal = ModalState::None;
            state.error_message = Some(format!(
                "No supported sandbox engines are available on {}. Disable sandbox to continue.",
                caps.platform_label()
            ));
            persist_state_snapshot(ctx, &state);
            return;
        }
    } else if let Err(e) = execute_preflight_action(&action) {
        let mut state = app_state.write();
        state.modal = ModalState::None;
        state.error_message = Some(e);
        persist_state_snapshot(ctx, &state);
        return;
    }

    if let Some(next) = sandbox_preflight(signature.sandbox_engine) {
        let mut state = app_state.write();
        state.modal = ModalState::PreflightPrompt {
            agent_id,
            signature,
            issue: next,
            remaining_issues: Vec::new(),
        };
        persist_state_snapshot(ctx, &state);
    } else {
        let work_dir = signature.work_dir.clone();
        {
            let mut state = app_state.write();
            state.modal = ModalState::None;
            state.terminal_focused = true;
            persist_state_snapshot(ctx, &state);
        }
        execute_agent_launch(app_state, ctx, &agent_id, &work_dir, &signature, false);
    }
}

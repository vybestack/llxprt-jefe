//! Issue send-to-agent orchestration (extracted from mod.rs).
//!
//! Resolves issue send context, writes the `.jefe/issue-prompt.md`, prepares
//! the agent working copy (default-branch checkout + pull, dirty-copy guard),
//! and spawns/attaches the issue-driven agent session. The issue-driven path
//! never passes `--continue` (issue #166).

use jefe::domain::{AgentId, LaunchSignature};
use jefe::runtime::RuntimeManager;
use jefe::state::{AppEvent, AppState, ModalState};

use super::agent_runtime::{clear_agent_runtime_attachment, mark_agent_runtime_attached};
use super::issues_dispatch;
use super::{
    AppStateHandle, REMOTE_ATTACH_SETTLE_DELAY, SharedContext, apply_and_persist,
    close_modal_and_persist, issue_git_prep, launch_signature_for_agent, persist_state,
    pid_on_success, preflight_or_prompt, to_persisted_state,
};
use tracing::warn;

pub(super) fn dispatch_agent_chooser_confirm(app_state: &mut AppStateHandle, ctx: &SharedContext) {
    let send_info = issue_send_info(app_state);
    apply_and_persist(app_state, ctx, AppEvent::AgentChooserConfirm);

    let Some(send_info) = send_info else {
        return;
    };
    if let Err(error) = write_issue_prompt(&send_info.work_dir, &send_info.payload) {
        apply_send_to_agent_failed(app_state, error);
        return;
    }

    // Issue-driven launches are always fresh instructions, so never resume a
    // prior session regardless of the agent's configured `pass_continue`.
    let mut launch_sig = send_info.signature;
    launch_sig.pass_continue = false;
    launch_sig.mode_flags.push("-i".to_owned());
    launch_sig
        .mode_flags
        .push("Read and work on the GitHub issue described in .jefe/issue-prompt.md".to_owned());

    // Ensure the agent starts from a clean, up-to-date checkout of the repo's
    // default branch (issue #166). The prompt file under `.jefe/` is already
    // written and is excluded from the dirty-copy guard and any cleanup.
    if let Err(error) = issue_git_prep::prepare_issue_workdir(&send_info.work_dir) {
        apply_send_to_agent_failed(app_state, error);
        return;
    }
    match issue_git_prep::is_workdir_dirty(&send_info.work_dir) {
        Ok(false) => proceed_issue_launch(
            app_state,
            ctx,
            &send_info.agent_id,
            send_info.work_dir.clone(),
            launch_sig,
        ),
        Ok(true) => prompt_dirty_copy_confirm(
            app_state,
            ctx,
            &send_info.agent_id,
            &send_info.work_dir,
            launch_sig,
        ),
        Err(error) => apply_send_to_agent_failed(app_state, error),
    }
}

/// Run preflight; if it passes (or sandbox is disabled), launch the issue agent.
fn proceed_issue_launch(
    app_state: &mut AppStateHandle,
    ctx: &SharedContext,
    agent_id: &AgentId,
    work_dir: std::path::PathBuf,
    launch_sig: LaunchSignature,
) {
    if preflight_or_prompt(app_state, ctx, agent_id, &launch_sig) {
        launch_issue_agent(app_state, ctx, agent_id.clone(), work_dir, launch_sig);
    }
}

/// Open the dirty-copy confirm modal. The default is no/halt — the user must
/// explicitly press Enter to discard changes and proceed.
fn prompt_dirty_copy_confirm(
    app_state: &mut AppStateHandle,
    ctx: &SharedContext,
    agent_id: &AgentId,
    work_dir: &std::path::Path,
    launch_sig: LaunchSignature,
) {
    let mut state = app_state.write();
    state.modal = ModalState::ConfirmIssueDirtyCopy {
        agent_id: agent_id.clone(),
        work_dir: work_dir.to_path_buf(),
        signature: launch_sig,
    };
    let persisted = to_persisted_state(&state);
    drop(state);
    persist_state(ctx, &persisted);
}

/// Dirty-copy confirm: user pressed Enter to discard uncommitted changes and
/// proceed with the issue-driven launch. Runs `git reset --hard` + `git clean`
/// (preserving `.jefe/` and `.llxprt/`), then closes the modal and launches.
pub(super) fn confirm_issue_dirty_copy_enter(
    app_state: &mut AppStateHandle,
    ctx: &SharedContext,
    agent_id: AgentId,
    work_dir: std::path::PathBuf,
    launch_sig: LaunchSignature,
) {
    if let Err(error) = issue_git_prep::discard_workdir_changes(&work_dir) {
        close_modal_and_persist(app_state, ctx);
        apply_send_to_agent_failed(app_state, error);
        return;
    }
    close_modal_and_persist(app_state, ctx);
    proceed_issue_launch(app_state, ctx, &agent_id, work_dir, launch_sig);
}

pub(super) struct IssueSendInfo {
    pub(super) agent_id: AgentId,
    pub(super) work_dir: std::path::PathBuf,
    pub(super) signature: LaunchSignature,
    pub(super) payload: jefe::github::SendPayload,
}

fn issue_send_info(app_state: &AppStateHandle) -> Option<IssueSendInfo> {
    let state = app_state.read();
    let result = issue_send_info_from_state(&state);
    drop(state);
    result
}

/// Resolve the issue send info from a raw `AppState` (testable without
/// `AppStateHandle`). Mirrors `pr_send_info_from_state`.
pub(super) fn issue_send_info_from_state(state: &AppState) -> Option<IssueSendInfo> {
    let chooser = state.issues_state.agent_chooser.as_ref()?;
    let detail = state.issues_state.issue_detail.as_ref()?;
    let (agent_id, _) = chooser.agents.get(chooser.selected_index)?.clone();
    let agent = state
        .agents
        .iter()
        .find(|agent| agent.id == agent_id)?
        .clone();
    let repo = state.repository_by_id(&agent.repository_id)?;
    let focused_comment = focused_issue_comment(state, detail);
    let work_dir = agent.work_dir.clone();
    let signature = launch_signature_for_agent(&agent, repo);
    let payload = jefe::github::GhClient::build_send_payload(
        &repo.slug,
        detail,
        focused_comment.as_ref(),
        &repo.issue_base_prompt,
    );

    Some(IssueSendInfo {
        agent_id,
        work_dir,
        signature,
        payload,
    })
}

fn focused_issue_comment(
    state: &AppState,
    detail: &jefe::domain::IssueDetail,
) -> Option<jefe::domain::IssueComment> {
    match state.issues_state.detail_subfocus {
        jefe::state::DetailSubfocus::Comment(idx) => detail.comments.get(idx).cloned(),
        _ => None,
    }
}

fn write_issue_prompt(
    work_dir: &std::path::Path,
    payload: &jefe::github::SendPayload,
) -> Result<(), String> {
    let prompt_dir = work_dir.join(".jefe");
    std::fs::create_dir_all(&prompt_dir)
        .map_err(|error| format!("Failed to create .jefe dir: {error}"))?;
    let prompt_path = prompt_dir.join("issue-prompt.md");
    let prompt_content = issues_dispatch::format_issue_prompt(payload);
    std::fs::write(&prompt_path, &prompt_content)
        .map_err(|error| format!("Failed to write issue prompt: {error}"))
}

fn launch_issue_agent(
    app_state: &mut AppStateHandle,
    ctx: &SharedContext,
    agent_id: AgentId,
    work_dir: std::path::PathBuf,
    launch_sig: LaunchSignature,
) {
    let launched = spawn_and_attach_fresh_for_issue(ctx, &agent_id, &work_dir, &launch_sig);
    // Resolve the worker PID for the persisted binding's PID-liveness
    // fallback, before taking the app-state write lock (lock-ordering
    // constraint). Skipped on the failure path (no binding persisted).
    let pid = pid_on_success(ctx, &agent_id, launched);
    let mut state = app_state.write();
    if launched {
        persist_issue_agent_launch_success(&mut state, &agent_id, launch_sig, pid);
    } else {
        *state = std::mem::take(&mut *state).apply(AppEvent::SendToAgentFailed {
            error: "Failed to launch agent".to_string(),
        });
    }
    let persisted = to_persisted_state(&state);
    drop(state);
    persist_state(ctx, &persisted);
}

fn spawn_and_attach_fresh_for_issue(
    ctx: &SharedContext,
    agent_id: &AgentId,
    work_dir: &std::path::Path,
    launch_sig: &LaunchSignature,
) -> bool {
    let Some(ctx_arc) = ctx else {
        return false;
    };
    let Ok(mut ctx_guard) = ctx_arc.lock() else {
        return false;
    };
    match ctx_guard
        .runtime
        .spawn_session_fresh(agent_id, work_dir, launch_sig)
    {
        Ok(()) => attach_issue_agent(&mut ctx_guard.runtime, agent_id),
        Err(error) => {
            warn!(agent_id = %agent_id.0, error = %error, "could not spawn agent for issue send");
            false
        }
    }
}

fn attach_issue_agent(runtime: &mut jefe::runtime::TmuxRuntimeManager, agent_id: &AgentId) -> bool {
    std::thread::sleep(REMOTE_ATTACH_SETTLE_DELAY);
    match runtime.attach(agent_id) {
        Ok(()) => true,
        Err(error) => {
            warn!(agent_id = %agent_id.0, error = %error, "could not attach agent after issue send");
            let _ = runtime.mark_session_dead(agent_id);
            false
        }
    }
}

fn persist_issue_agent_launch_success(
    state: &mut AppState,
    agent_id: &AgentId,
    launch_sig: LaunchSignature,
    pid: Option<u32>,
) {
    if let Some(agent) = state.agents.iter_mut().find(|agent| &agent.id == agent_id) {
        agent.status = jefe::domain::AgentStatus::Running;
        let session_name = jefe::runtime::RuntimeSession::session_name_for(agent_id);
        agent.runtime_binding = Some(jefe::domain::RuntimeBinding {
            session_name,
            launch_signature: launch_sig,
            attached: false,
            last_seen: None,
            pid,
        });
    }
    clear_agent_runtime_attachment(state);
    mark_agent_runtime_attached(state, agent_id, true);
}

fn apply_send_to_agent_failed(app_state: &mut AppStateHandle, error: String) {
    let mut state = app_state.write();
    *state = std::mem::take(&mut *state).apply(AppEvent::SendToAgentFailed { error });
}

//! Transient-agent queue draining (extracted from `mod.rs` to keep it under
//! the source-file-size hard limit).
//!
//! `drain_transient_queue` is called when a transient agent reaches a
//! terminal state. It finds a queued send for the same repository, builds a
//! fresh transient agent, and dispatches the send.

use jefe::domain::{Agent, AgentId, RepositoryId};
use jefe::state::{AppEvent, QueuedTransientSend};

use super::{
    AppStateHandle, SharedContext, apply_and_persist, clone_identity, issues_send,
    transient_issue_send, transient_pr_send,
};

/// Drain the transient agent queue when a transient agent completes
/// (issue #213). If there are queued sends for the repo whose transient agent
/// just reached a terminal state, pop the oldest and launch it.
pub(super) fn drain_transient_queue(app_state: &mut AppStateHandle, ctx: &SharedContext) {
    let repo_id = find_terminal_transient_repo(app_state);
    let Some(repo_id) = repo_id else {
        return;
    };

    // Verify the repo still exists BEFORE popping the queue item, so a
    // deleted repo does not permanently lose the queued send (issue #213
    // OCR fix).
    let repo = {
        let state = app_state.read();
        state.repository_by_id(&repo_id).cloned()
    };
    let Some(repo) = repo else {
        return;
    };

    let dequeued = {
        let mut state = app_state.write();
        let item = state.pop_transient_queue_for_repo(&repo_id);
        if item.is_some() {
            state.clear_transient_notice();
        }
        item
    };
    let Some(item) = dequeued else {
        return;
    };

    apply_and_persist(app_state, ctx, AppEvent::TransientAgentDequeued);

    let agent_id = AgentId(jefe::services::generate_id("transient"));
    let clone_identity = clone_identity::CloneIdentity::from_repository(&repo);
    let repo_id: RepositoryId = item.repository_id.clone();
    let agent =
        agent_from_queued_signature(agent_id.clone(), repo_id, &repo, &item.launch_signature);
    transient_issue_send::push_transient_agent_pub(app_state, ctx, agent);

    dispatch_dequeued_payload(app_state, ctx, item, &agent_id, clone_identity);
}

pub(super) fn agent_from_queued_signature(
    agent_id: AgentId,
    repository_id: RepositoryId,
    repo: &jefe::domain::Repository,
    signature: &jefe::domain::LaunchSignature,
) -> Agent {
    Agent::new_transient_from_signature(agent_id, repository_id, repo, signature)
}

/// Find a repo that has a terminal transient agent and queued items.
fn find_terminal_transient_repo(app_state: &AppStateHandle) -> Option<RepositoryId> {
    let state = app_state.read();
    state
        .agents
        .iter()
        .filter(|a| a.is_transient())
        .filter(|a| {
            matches!(
                a.status,
                jefe::domain::AgentStatus::Completed
                    | jefe::domain::AgentStatus::Errored
                    | jefe::domain::AgentStatus::Dead
            )
        })
        .find(|a| {
            state
                .transient_queue
                .pending
                .iter()
                .any(|q| q.repository_id == a.repository_id)
        })
        .map(|a| a.repository_id.clone())
}

/// Dispatch the dequeued payload (issue or PR) to the appropriate send path.
fn dispatch_dequeued_payload(
    app_state: &mut AppStateHandle,
    ctx: &SharedContext,
    item: QueuedTransientSend,
    agent_id: &AgentId,
    clone_identity: Option<clone_identity::CloneIdentity>,
) {
    let work_dir = item.work_dir.clone();
    let launch_sig = item.launch_signature.clone();
    match item.payload {
        jefe::state::TransientPayload::Issue { payload } => {
            let launch_sig = issues_send::prepare_issue_launch_signature(launch_sig);
            transient_issue_send::dispatch_transient_dequeued_issue(
                app_state,
                ctx,
                transient_issue_send::TransientDequeuedIssue {
                    agent_id: agent_id.clone(),
                    work_dir,
                    launch_sig,
                    clone_identity,
                    payload,
                },
            );
        }
        jefe::state::TransientPayload::PullRequest { payload } => {
            transient_pr_send::dispatch_transient_dequeued_pr(
                app_state,
                ctx,
                transient_pr_send::TransientDequeuedPr {
                    agent_id: agent_id.clone(),
                    work_dir,
                    launch_sig,
                    clone_identity,
                    payload,
                },
            );
        }
    }
}

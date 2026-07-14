//! Transient agent send-to-agent orchestration for Issues mode (issue #213).
//!
//! Extracted from `issues_send.rs` to keep that file under the source-file
//! hard limit. Mirrors the persistent-agent send flow but creates a temporary
//! agent on-the-fly using repository defaults, generates a temp directory,
//! clones the repo, and launches. Queueing is supported via
//! `transient_max_concurrent`.

use std::path::PathBuf;

use jefe::domain::{AgentId, LaunchSignature, Repository};
use jefe::state::AppEvent;

use super::clone_identity::CloneIdentity;
use super::issue_prep::{DirtyPolicy, PrepOutcome, prepare_issue_target};
use super::issue_self_assignment::IssueAssignment;
use super::issues_dispatch;
use super::{AppStateHandle, SharedContext, apply_and_persist, persist_state, to_persisted_state};

pub(super) use super::issues_send::{
    apply_assignment_action, apply_send_to_agent_failed, focused_issue_comment,
    persist_issue_agent_launch_success, prepare_issue_launch_signature,
    spawn_and_attach_fresh_for_issue,
};

/// Whether the issues agent-chooser has the transient slot selected.
pub(super) fn is_transient_slot_selected_issues(app_state: &AppStateHandle) -> bool {
    let state = app_state.read();
    is_transient_slot_selected(state.issues_state.agent_chooser.as_ref())
}

/// Whether a given agent chooser has the transient slot selected.
fn is_transient_slot_selected(chooser: Option<&jefe::state::AgentChooserState>) -> bool {
    let Some(chooser) = chooser else {
        return false;
    };
    chooser.transient_available && chooser.selected_index == chooser.agents.len()
}

/// Dispatch a transient issue send: close the chooser, check queue capacity,
/// generate a temp dir, create the transient agent, clone + launch.
pub(super) fn dispatch_transient_issue_send(app_state: &mut AppStateHandle, ctx: &SharedContext) {
    apply_and_persist(app_state, ctx, AppEvent::AgentChooserConfirm);

    let Some(payload_and_repo) = transient_issue_payload_and_repo(app_state) else {
        apply_send_to_agent_failed(
            app_state,
            ctx,
            "Could not resolve issue context for transient send".to_string(),
        );
        return;
    };
    let (payload, repo, repo_id) = payload_and_repo;

    if let Some(_queue_pos) =
        check_transient_queue_capacity(app_state, &repo_id, repo.transient_max_concurrent)
    {
        let work_dir = generate_transient_work_dir(&repo);
        let launch_sig = super::launch_signature_for_transient(&repo, &work_dir);
        let _clone_identity = CloneIdentity::from_repository(&repo);
        let queue_item = jefe::state::QueuedTransientSend {
            repository_id: repo_id,
            work_dir,
            launch_signature: launch_sig,
            payload: jefe::state::TransientPayload::Issue { payload },
        };
        let mut state = app_state.write();
        let pos = state.push_transient_queue_item(queue_item);
        let persisted = to_persisted_state(&state);
        drop(state);
        persist_state(ctx, &persisted);
        apply_and_persist(
            app_state,
            ctx,
            AppEvent::TransientAgentQueued {
                queue_position: pos,
            },
        );
        return;
    }

    let work_dir = generate_transient_work_dir(&repo);
    let agent = jefe::domain::Agent::new_transient(
        jefe::domain::AgentId(jefe::services::generate_id("transient")),
        repo_id.clone(),
        work_dir.clone(),
        &repo,
    );
    let launch_sig = super::launch_signature_for_transient(&repo, &work_dir);
    let clone_identity = CloneIdentity::from_repository(&repo);
    let agent_id = agent.id.clone();

    // Push agent to state as Queued — not Running — so it doesn't count
    // against capacity or appear as running if prep/launch fails.
    push_transient_agent_queued(app_state, ctx, agent);

    prepare_and_launch_transient_issue(
        app_state,
        ctx,
        TransientPrepContext {
            work_dir,
            launch_sig,
            clone_identity,
            payload,
            agent_id,
        },
    );
}

/// Bundled context for `prepare_and_launch_transient_issue` (keeps the
/// argument count under the clippy limit).
struct TransientPrepContext {
    work_dir: PathBuf,
    launch_sig: LaunchSignature,
    clone_identity: Option<CloneIdentity>,
    payload: jefe::github::SendPayload,
    agent_id: AgentId,
}

/// Availability check and remote probe for a transient issue send (issue
/// #213). Returns the resolved target on success, or `None` after surfacing
/// the appropriate failure and cleaning up the transient agent.
fn transient_issue_availability_and_target(
    app_state: &mut AppStateHandle,
    ctx: &SharedContext,
    prep: &TransientPrepContext,
) -> Option<super::target_resolution::WorkTarget> {
    if !super::availability::local_kind_available_or_error(
        app_state,
        prep.launch_sig.agent_kind,
        &prep.launch_sig.remote,
    ) {
        fail_transient_agent(app_state, ctx, &prep.agent_id);
        return None;
    }
    let target = match super::target_resolution::resolve_target(&prep.launch_sig.remote) {
        Ok(target) => target,
        Err(error) => {
            apply_send_to_agent_failed(app_state, ctx, error);
            fail_transient_agent(app_state, ctx, &prep.agent_id);
            return None;
        }
    };
    if !super::remote_probe::pre_side_effect_runtime_available_or_error(
        app_state,
        &target,
        &prep.work_dir,
        prep.launch_sig.agent_kind,
    ) {
        fail_transient_agent(app_state, ctx, &prep.agent_id);
        return None;
    }
    Some(target)
}

/// Handle a transient prep outcome: launch on Ready, fail on everything else.
fn handle_transient_prep_outcome(
    app_state: &mut AppStateHandle,
    ctx: &SharedContext,
    prep: &TransientPrepContext,
    outcome: Result<PrepOutcome, String>,
) {
    match outcome {
        Ok(PrepOutcome::Ready) => {
            let launch_sig = prepare_issue_launch_signature(prep.launch_sig.clone());
            launch_transient_issue_agent(
                app_state,
                ctx,
                prep.agent_id.clone(),
                prep.work_dir.clone(),
                launch_sig,
                issue_assignment_from_payload(&prep.payload),
            );
        }
        Ok(PrepOutcome::Dirty) => {
            apply_send_to_agent_failed(
                app_state,
                ctx,
                "Transient work directory is unexpectedly dirty".to_owned(),
            );
            fail_transient_agent(app_state, ctx, &prep.agent_id);
        }
        Ok(PrepOutcome::OriginMismatch { .. }) => {
            apply_send_to_agent_failed(
                app_state,
                ctx,
                "Transient work directory has unexpected origin after fresh clone".to_owned(),
            );
            fail_transient_agent(app_state, ctx, &prep.agent_id);
        }
        Err(error) => {
            apply_send_to_agent_failed(app_state, ctx, error);
            fail_transient_agent(app_state, ctx, &prep.agent_id);
        }
    }
}

/// Availability check, target resolution, prep, and launch for a transient
/// issue send. Extracted to keep `dispatch_transient_issue_send` under the
/// line limit.
fn prepare_and_launch_transient_issue(
    app_state: &mut AppStateHandle,
    ctx: &SharedContext,
    prep: TransientPrepContext,
) {
    let Some(target) = transient_issue_availability_and_target(app_state, ctx, &prep) else {
        return;
    };

    let prompt = issues_dispatch::format_issue_prompt(&prep.payload);
    let outcome = prepare_issue_target(
        &target,
        &prep.work_dir,
        prep.clone_identity.as_ref(),
        DirtyPolicy::Stop,
        &prompt,
    );
    handle_transient_prep_outcome(app_state, ctx, &prep, outcome);
}

/// Resolve the issue payload and repository for a transient send.
fn transient_issue_payload_and_repo(
    app_state: &AppStateHandle,
) -> Option<(
    jefe::github::SendPayload,
    Repository,
    jefe::domain::RepositoryId,
)> {
    let state = app_state.read();
    let result = transient_issue_payload_and_repo_from_state(&state);
    drop(state);
    result
}

/// Pure state-reading variant (testable without `AppStateHandle`).
fn transient_issue_payload_and_repo_from_state(
    state: &jefe::state::AppState,
) -> Option<(
    jefe::github::SendPayload,
    Repository,
    jefe::domain::RepositoryId,
)> {
    let detail = state.issues_state.issue_detail.as_ref()?;
    let repo = state.selected_repository()?;
    let repo_id = repo.id.clone();
    let focused_comment = state
        .issues_state
        .issue_detail
        .as_ref()
        .and_then(|d| focused_issue_comment(state, d));
    let payload = jefe::github::GhClient::build_send_payload(
        &detail.repo_owner_name,
        detail,
        focused_comment.as_ref(),
        &repo.issue_base_prompt,
    );
    Some((payload, repo.clone(), repo_id))
}

/// Check whether the transient queue is at capacity. Returns `Some(position)`
/// if the send should be queued (max_concurrent reached), or `None` if it can
/// proceed immediately.
pub(super) fn check_transient_queue_capacity(
    app_state: &AppStateHandle,
    repo_id: &jefe::domain::RepositoryId,
    max_concurrent: u32,
) -> Option<usize> {
    if max_concurrent == 0 {
        return None;
    }
    let state = app_state.read();
    let running = state.running_transient_count(repo_id);
    let queued = state
        .transient_queue
        .pending
        .iter()
        .filter(|q| &q.repository_id == repo_id)
        .count();
    drop(state);
    let active = u32::try_from(running + queued).unwrap_or(u32::MAX);
    if active >= max_concurrent {
        Some(queued)
    } else {
        None
    }
}

/// Public re-export for PRs orchestration (issue #213).
pub(super) fn check_transient_queue_capacity_pub(
    app_state: &AppStateHandle,
    repo_id: &jefe::domain::RepositoryId,
    max_concurrent: u32,
) -> Option<usize> {
    check_transient_queue_capacity(app_state, repo_id, max_concurrent)
}

/// Generate a unique work directory under the repo's effective transient dir.
pub(super) fn generate_transient_work_dir(repo: &Repository) -> PathBuf {
    let id = jefe::services::generate_id("transient");
    repo.effective_transient_dir().join(format!("jefe-{id}"))
}

/// Public re-export for PRs orchestration (issue #213).
pub(super) fn generate_transient_work_dir_pub(repo: &Repository) -> PathBuf {
    generate_transient_work_dir(repo)
}

/// Push a transient agent to `state.agents` in Queued status (runtime-only,
/// not added to any repo's `agent_ids`). Used for the initial send path
/// where prep/launch has not yet succeeded.
fn push_transient_agent_queued(
    app_state: &mut AppStateHandle,
    ctx: &SharedContext,
    agent: jefe::domain::Agent,
) {
    let mut state = app_state.write();
    state.agents.push(agent);
    let persisted = to_persisted_state(&state);
    drop(state);
    persist_state(ctx, &persisted);
}

/// Public re-export for PRs orchestration (issue #213). Pushes a transient
/// agent in Queued status.
pub(super) fn push_transient_agent_pub(
    app_state: &mut AppStateHandle,
    ctx: &SharedContext,
    agent: jefe::domain::Agent,
) {
    push_transient_agent_queued(app_state, ctx, agent);
}

/// Launch a transient issue agent (mirrors `launch_issue_agent` but for the
/// transient agent).
fn launch_transient_issue_agent(
    app_state: &mut AppStateHandle,
    ctx: &SharedContext,
    agent_id: AgentId,
    work_dir: PathBuf,
    launch_sig: LaunchSignature,
    assignment: IssueAssignment,
) {
    let launched = spawn_and_attach_fresh_for_issue(ctx, &agent_id, &work_dir, &launch_sig);
    let (pid, process_identity) = super::process_on_success(ctx, &agent_id, launched);
    let mut state = app_state.write();
    if launched {
        persist_issue_agent_launch_success(
            &mut state,
            &agent_id,
            launch_sig,
            pid,
            process_identity,
        );
    } else {
        // Mark the transient agent as errored instead of leaving it Running.
        if let Some(agent) = state.agents.iter_mut().find(|a| a.id == agent_id) {
            agent.status = jefe::domain::AgentStatus::Errored;
        }
        *state = std::mem::take(&mut *state).apply(AppEvent::SendToAgentFailed {
            error: "Failed to launch transient agent".to_string(),
        });
    }
    let persisted = to_persisted_state(&state);
    drop(state);
    persist_state(ctx, &persisted);
    apply_assignment_action(
        app_state,
        ctx,
        super::issue_self_assignment::direct_assignment_action(launched, assignment),
    );
}

pub(super) struct TransientDequeuedIssue {
    pub agent_id: AgentId,
    pub work_dir: PathBuf,
    pub launch_sig: LaunchSignature,
    pub clone_identity: Option<CloneIdentity>,
    pub payload: jefe::github::SendPayload,
}

/// Dispatch a dequeued transient issue send: availability check, target
/// resolution, clone/checkout, and launch (issue #213). Called by
/// `drain_transient_queue` when a transient agent completes and a queued
/// issue send is next in line.
pub(super) fn dispatch_transient_dequeued_issue(
    app_state: &mut AppStateHandle,
    ctx: &SharedContext,
    params: TransientDequeuedIssue,
) {
    let prep = TransientPrepContext {
        work_dir: params.work_dir,
        launch_sig: params.launch_sig,
        clone_identity: params.clone_identity,
        payload: params.payload,
        agent_id: params.agent_id,
    };
    prepare_and_launch_transient_issue(app_state, ctx, prep);
}

fn issue_assignment_from_payload(payload: &jefe::github::SendPayload) -> IssueAssignment {
    let tracker = jefe::domain::GitHubRepoRef::parse(&payload.repository)
        .ok()
        .flatten();
    IssueAssignment::from_send_context(tracker.as_ref(), payload.issue_number)
}

/// Mark a transient agent as errored and remove it from state on a failure
/// path (issue #213). This prevents phantom Running agents from consuming
/// capacity indefinitely.
pub(super) fn fail_transient_agent(
    app_state: &mut AppStateHandle,
    ctx: &SharedContext,
    agent_id: &AgentId,
) {
    let mut state = app_state.write();
    state.agents.retain(|a| a.id != *agent_id);
    let persisted = to_persisted_state(&state);
    drop(state);
    persist_state(ctx, &persisted);
}

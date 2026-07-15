//! Transient-agent PR send logic (extracted from `prs_orchestration.rs` to
//! keep it under the source-file-size hard limit).
//!
//! Contains: `is_transient_slot_selected_prs`, `dispatch_transient_pr_send`,
//! `prepare_and_launch_transient_pr`, `transient_pr_payload_and_repo`, and
//! the `TransientPrPrepContext` / `TransientDequeuedPr` structs.

use std::path::PathBuf;

use jefe::domain::{LaunchSignature, Repository};
use jefe::state::AppState;

use super::clone_identity;
use super::fresh_prompt::{FreshPromptKind, prepare_fresh_prompt_signature};
use super::issue_prep::{DirtyPolicy, PrepOutcome};
use super::preflight::preflight_or_prompt;
use super::prs_dispatch;
use super::{AppEvent, apply_and_persist};
use crate::app_input::{AppStateHandle, SharedContext};

use super::prs_orchestration::{
    apply_pr_send_to_agent_failed, focused_pr_comment, launch_pr_agent, pr_base_prompt,
};

const PR_PROMPT_RELATIVE_PATH: &str = ".jefe/pr-prompt.md";

/// Whether the PRs agent-chooser has the transient slot selected.
pub(super) fn is_transient_slot_selected_prs(app_state: &AppStateHandle) -> bool {
    let state = app_state.read();
    let selected = state
        .prs_state
        .agent_chooser
        .as_ref()
        .is_some_and(|c| c.transient_available && c.selected_index == c.agents.len());
    drop(state);
    selected
}

/// Bundled context for transient PR prep (keeps arg count under the clippy
/// limit).
pub(super) struct TransientPrPrepContext {
    pub work_dir: PathBuf,
    pub launch_sig: LaunchSignature,
    pub clone_identity: Option<clone_identity::CloneIdentity>,
    pub payload: jefe::github::PrSendPayload,
    pub agent_id: jefe::domain::AgentId,
}

/// Dispatch a transient PR send: close chooser, check queue, create agent,
/// clone + launch.
pub(super) fn dispatch_transient_pr_send(app_state: &mut AppStateHandle, ctx: &SharedContext) {
    apply_and_persist(app_state, ctx, AppEvent::PrAgentChooserConfirm);

    let Some(payload_and_repo) = transient_pr_payload_and_repo(app_state) else {
        apply_pr_send_to_agent_failed(
            app_state,
            ctx,
            "Could not resolve PR context for transient send".to_string(),
        );
        return;
    };
    let (payload, repo, repo_id) = payload_and_repo;

    if let Some(_queue_pos) = super::transient_issue_send::check_transient_queue_capacity_pub(
        app_state,
        &repo_id,
        repo.transient_max_concurrent,
    ) {
        let work_dir = super::transient_issue_send::generate_transient_work_dir_pub(&repo);
        let launch_sig = super::launch_signature_for_transient(&repo, &work_dir);
        let queue_item = jefe::state::QueuedTransientSend {
            repository_id: repo_id,
            work_dir,
            launch_signature: launch_sig,
            payload: jefe::state::TransientPayload::PullRequest { payload },
        };
        let mut state = app_state.write();
        let pos = state.push_transient_queue_item(queue_item);
        drop(state);
        // apply_and_persist will persist the queue item along with the event
        // state — no separate persist needed (issue #213 OCR fix).
        apply_and_persist(
            app_state,
            ctx,
            AppEvent::TransientAgentQueued {
                queue_position: pos,
            },
        );
        return;
    }

    let work_dir = super::transient_issue_send::generate_transient_work_dir_pub(&repo);
    let agent = jefe::domain::Agent::new_transient(
        jefe::domain::AgentId(jefe::services::generate_id("transient")),
        repo_id.clone(),
        work_dir.clone(),
        &repo,
    );
    let agent_id = agent.id.clone();
    let launch_sig = super::launch_signature_for_transient(&repo, &work_dir);
    let clone_identity = clone_identity::CloneIdentity::from_repository(&repo);
    super::transient_issue_send::push_transient_agent_pub(app_state, ctx, agent);

    prepare_and_launch_transient_pr(
        app_state,
        ctx,
        TransientPrPrepContext {
            work_dir,
            launch_sig,
            clone_identity,
            payload,
            agent_id,
        },
    );
}

/// Availability check and remote probe for a transient PR send (issue #213).
fn transient_pr_availability_and_target(
    app_state: &mut AppStateHandle,
    ctx: &SharedContext,
    prep: &TransientPrPrepContext,
) -> Option<super::target_resolution::WorkTarget> {
    if !super::availability::launch_available_or_error(
        app_state,
        prep.launch_sig.agent_kind,
        prep.launch_sig.llxprt_version.as_ref(),
        &prep.launch_sig.remote,
    ) {
        super::transient_issue_send::fail_transient_agent(app_state, ctx, &prep.agent_id);
        return None;
    }
    let target = match super::target_resolution::resolve_target(&prep.launch_sig.remote) {
        Ok(target) => target,
        Err(error) => {
            apply_pr_send_to_agent_failed(app_state, ctx, error);
            super::transient_issue_send::fail_transient_agent(app_state, ctx, &prep.agent_id);
            return None;
        }
    };
    if !super::remote_probe::pre_side_effect_runtime_available_or_error(
        app_state,
        &target,
        &prep.launch_sig,
    ) {
        super::transient_issue_send::fail_transient_agent(app_state, ctx, &prep.agent_id);
        return None;
    }
    Some(target)
}

/// Handle a transient PR prep outcome: launch on Ready, fail on everything else.
fn handle_transient_pr_outcome(
    app_state: &mut AppStateHandle,
    ctx: &SharedContext,
    prep: &TransientPrPrepContext,
    outcome: Result<PrepOutcome, String>,
) {
    match outcome {
        Ok(PrepOutcome::Ready) => {
            if preflight_or_prompt(app_state, ctx, &prep.agent_id, &prep.launch_sig, None) {
                launch_pr_agent(
                    app_state,
                    ctx,
                    prep.agent_id.clone(),
                    prep.work_dir.clone(),
                    prep.launch_sig.clone(),
                );
            }
        }
        Ok(PrepOutcome::Dirty) => {
            apply_pr_send_to_agent_failed(
                app_state,
                ctx,
                "Transient PR work directory is unexpectedly dirty".to_owned(),
            );
            super::transient_issue_send::fail_transient_agent(app_state, ctx, &prep.agent_id);
        }
        Ok(PrepOutcome::OriginMismatch { .. }) => {
            apply_pr_send_to_agent_failed(
                app_state,
                ctx,
                "Transient PR work directory has unexpected origin after fresh clone".to_owned(),
            );
            super::transient_issue_send::fail_transient_agent(app_state, ctx, &prep.agent_id);
        }
        Err(error) => {
            apply_pr_send_to_agent_failed(app_state, ctx, error);
            super::transient_issue_send::fail_transient_agent(app_state, ctx, &prep.agent_id);
        }
    }
}

/// Availability check, target resolution, clone + prompt write, and launch
/// for a transient PR send. Reuses the shared clone/checkout prep so the PR
/// gets a real repository checkout (not just an empty dir).
fn prepare_and_launch_transient_pr(
    app_state: &mut AppStateHandle,
    ctx: &SharedContext,
    prep: TransientPrPrepContext,
) {
    let launch_sig = prepare_fresh_prompt_signature(
        prep.launch_sig,
        FreshPromptKind::PullRequest,
        PR_PROMPT_RELATIVE_PATH,
    );
    let prep = TransientPrPrepContext { launch_sig, ..prep };
    let Some(target) = transient_pr_availability_and_target(app_state, ctx, &prep) else {
        return;
    };
    let prompt_content = prs_dispatch::format_pr_prompt(&prep.payload);
    let outcome = super::issue_prep::prepare_issue_target(
        &target,
        &prep.work_dir,
        prep.clone_identity.as_ref(),
        DirtyPolicy::Stop,
        &prompt_content,
    );
    handle_transient_pr_outcome(app_state, ctx, &prep, outcome);
}

pub(super) struct TransientDequeuedPr {
    pub agent_id: jefe::domain::AgentId,
    pub work_dir: PathBuf,
    pub launch_sig: LaunchSignature,
    pub clone_identity: Option<clone_identity::CloneIdentity>,
    pub payload: jefe::github::PrSendPayload,
}

/// Dispatch a dequeued transient PR send: availability check, target
/// resolution, clone/checkout, and launch (issue #213). Called by
/// `drain_transient_queue` when a transient agent completes and a queued
/// PR send is next in line.
pub(super) fn dispatch_transient_dequeued_pr(
    app_state: &mut AppStateHandle,
    ctx: &SharedContext,
    params: TransientDequeuedPr,
) {
    let prep = TransientPrPrepContext {
        work_dir: params.work_dir,
        launch_sig: params.launch_sig,
        clone_identity: params.clone_identity,
        payload: params.payload,
        agent_id: params.agent_id,
    };
    prepare_and_launch_transient_pr(app_state, ctx, prep);
}

/// Resolve the PR payload and repository for a transient send.
fn transient_pr_payload_and_repo(
    app_state: &AppStateHandle,
) -> Option<(
    jefe::github::PrSendPayload,
    Repository,
    jefe::domain::RepositoryId,
)> {
    let state = app_state.read();
    let result = transient_pr_payload_and_repo_from_state(&state);
    drop(state);
    result
}

/// Pure state-reading variant (testable without `AppStateHandle`).
fn transient_pr_payload_and_repo_from_state(
    state: &AppState,
) -> Option<(
    jefe::github::PrSendPayload,
    Repository,
    jefe::domain::RepositoryId,
)> {
    let detail = state.prs_state.pr_detail.as_ref()?;
    let repo = state.selected_repository()?;
    let repo_id = repo.id.clone();
    let focused_comment = focused_pr_comment(state, detail);
    let payload = jefe::github::GhClient::build_pr_send_payload(
        &detail.repo_owner_name,
        detail,
        focused_comment.as_ref(),
        pr_base_prompt(repo),
    );
    Some((payload, repo.clone(), repo_id))
}

//! Issue send-to-agent orchestration (extracted from mod.rs).
//!
//! Resolves issue send context, prepares the agent working copy via the
//! target-aware prep in [`super::issue_prep`] (clone/checkout/dirty-guard/
//! prompt-write, on the same target where the `LaunchSignature` runs), and
//! spawns/attaches the issue-driven agent session. The issue-driven path
//! never passes `--continue` (issue #166).
//!
//! Clone identity derives only from a valid `Repository.github_repo`
//! `owner/repo` value (validated in [`super::clone_identity`]) and always uses
//! the canonical HTTPS clone URL, regardless of local/remote execution
//! (issue #184).

use std::path::{Path, PathBuf};

use jefe::domain::{AgentId, LaunchSignature};
use jefe::runtime::RuntimeManager;
use jefe::state::{AppEvent, AppState, ModalState};

use tracing::warn;

use super::agent_runtime::{clear_agent_runtime_attachment, mark_agent_runtime_attached};
use super::clone_identity::CloneIdentity;
use super::fresh_prompt::{FreshPromptKind, prepare_fresh_prompt_signature};
use super::issue_prep::{
    DirtyPolicy, ISSUE_PROMPT_RELATIVE_PATH, PrepOutcome, prepare_issue_target,
    prepare_issue_target_force_reclone,
};
use super::issue_self_assignment::{
    IssueAssignment, IssueAssignmentAction, SelfAssignment, direct_assignment_action,
    post_preflight_assignment_action,
};
use super::issues_dispatch;
use super::{
    AppStateHandle, REMOTE_ATTACH_SETTLE_DELAY, SharedContext, apply_and_persist,
    close_modal_and_persist, gh_async, github_client, launch_signature_for_agent, persist_state,
    pid_on_success, preflight_or_prompt, to_persisted_state,
};

pub(super) fn dispatch_agent_chooser_confirm(app_state: &mut AppStateHandle, ctx: &SharedContext) {
    let send_info = issue_send_info(app_state);
    apply_and_persist(app_state, ctx, AppEvent::AgentChooserConfirm);

    let Some(send_info) = send_info else {
        return;
    };

    // Issue-driven launches are always fresh instructions, so never resume a
    // prior session regardless of the agent's configured `pass_continue`.
    let launch_sig = prepare_issue_launch_signature(send_info.signature);

    // Availability guard BEFORE any prep side effects: a missing agent
    // runtime must not trigger a remote clone/checkout. Prep (clone/reset/
    // clean/prompt-write) only runs when the agent kind is available.
    if !super::availability::local_kind_available_or_error(
        app_state,
        launch_sig.agent_kind,
        &launch_sig.remote,
    ) {
        return;
    }

    let target = match super::target_resolution::resolve_target(&launch_sig.remote) {
        Ok(target) => target,
        Err(error) => {
            apply_send_to_agent_failed(app_state, ctx, error);
            return;
        }
    };

    // Centralized pre-side-effect availability probe (defect 2): BEFORE any
    // git prep/cleanup/prompt side effect, probe the selected runtime on the
    // resolved target. For local targets this reuses the session snapshot;
    // for remote targets this is a no-install/no-setup/side-effect-free
    // ssh -T probe for the exact binary executed as the effective run_as_user.
    // Unavailable remote means no prep/prompt operation.
    if !super::remote_probe::pre_side_effect_runtime_available_or_error(
        app_state,
        &target,
        &send_info.work_dir,
        launch_sig.agent_kind,
    ) {
        return;
    }

    let prompt = issues_dispatch::format_issue_prompt(&send_info.payload);

    // Initial send uses the Stop policy: a dirty working copy returns Dirty
    // without altering it, so the user is prompted before any destructive
    // cleanup. One orchestration drives local/remote and Stop/Discard.
    let outcome = prepare_issue_target(
        &target,
        &send_info.work_dir,
        send_info.clone_identity.as_ref(),
        DirtyPolicy::Stop,
        &prompt,
    );
    handle_initial_prep_outcome(
        app_state,
        ctx,
        outcome,
        PrepOutcomeContext {
            agent_id: send_info.agent_id,
            work_dir: send_info.work_dir,
            launch_sig,
            payload: send_info.payload.clone(),
        },
    );
}

/// Context for handling an initial prep outcome, bundling the fields that
/// the launch/confirm paths need so the handler stays under the argument
/// count limit.
struct PrepOutcomeContext {
    agent_id: AgentId,
    work_dir: PathBuf,
    launch_sig: LaunchSignature,
    payload: jefe::github::SendPayload,
}

/// Bundled origin-mismatch info (actual/expected shortforms) to stay under
/// the argument-count limit.
struct OriginMismatchInfo {
    actual: String,
    expected: String,
}

/// Handle the outcome of the initial (Stop-policy) prep for the agent chooser
/// confirm path. Dispatches to launch, dirty-confirm, or origin-mismatch
/// confirm depending on the result.
fn issue_assignment_from_payload(payload: &jefe::github::SendPayload) -> IssueAssignment {
    let tracker = jefe::domain::GitHubRepoRef::parse(&payload.repository)
        .ok()
        .flatten();
    IssueAssignment::from_send_context(tracker.as_ref(), payload.issue_number)
}

fn handle_initial_prep_outcome(
    app_state: &mut AppStateHandle,
    ctx: &SharedContext,
    outcome: Result<PrepOutcome, String>,
    prep_ctx: PrepOutcomeContext,
) {
    match outcome {
        Ok(PrepOutcome::Ready) => preflight_and_launch_issue(
            app_state,
            ctx,
            &prep_ctx.agent_id,
            prep_ctx.work_dir,
            prep_ctx.launch_sig,
            issue_assignment_from_payload(&prep_ctx.payload),
        ),
        Ok(PrepOutcome::Dirty) => prompt_dirty_copy_confirm(
            app_state,
            ctx,
            &prep_ctx.agent_id,
            &prep_ctx.work_dir,
            prep_ctx.launch_sig,
            prep_ctx.payload,
        ),
        Ok(PrepOutcome::OriginMismatch { actual, expected }) => prompt_origin_mismatch_confirm(
            app_state,
            ctx,
            &prep_ctx,
            OriginMismatchInfo { actual, expected },
        ),
        Err(error) => apply_send_to_agent_failed(app_state, ctx, error),
    }
}

/// Build the launch signature for an issue-driven launch from the agent's
/// base signature. Issue-driven launches are always fresh instructions, so
/// `pass_continue` is forced to `false` regardless of the agent's configured
/// value, and the issue prompt instruction is appended with the correct
/// per-kind arg shape.
///
/// Delegates to [`prepare_fresh_prompt_signature`] so the issue and PR send
/// paths share the same kind-specific arg construction. CodePuppy and LLxprt
/// share identical prep; only the launch signature/runtime args differ.
///
/// Extracted as a pure function so the `pass_continue = false` override is
/// unit-testable without a runtime/git context.
pub(super) fn prepare_issue_launch_signature(sig: LaunchSignature) -> LaunchSignature {
    prepare_fresh_prompt_signature(sig, FreshPromptKind::Issue, ISSUE_PROMPT_RELATIVE_PATH)
}

/// Run preflight; if it passes (or sandbox is disabled), launch the issue
/// agent. Availability was already verified before prep side effects by the
/// caller. On a successful launch, `assignment` (when present) triggers the
/// non-blocking self-assignment of the issue to the viewer (issue #186).
fn preflight_and_launch_issue(
    app_state: &mut AppStateHandle,
    ctx: &SharedContext,
    agent_id: &AgentId,
    work_dir: PathBuf,
    launch_sig: LaunchSignature,
    assignment: IssueAssignment,
) {
    let carried = assignment.carried();
    if preflight_or_prompt(app_state, ctx, agent_id, &launch_sig, Some(&carried)) {
        launch_issue_agent(
            app_state,
            ctx,
            agent_id.clone(),
            work_dir,
            launch_sig,
            assignment,
        );
    }
}

/// Open the dirty-copy confirm modal. The default is no/halt — the user must
/// explicitly press Enter to discard changes and proceed.
fn prompt_dirty_copy_confirm(
    app_state: &mut AppStateHandle,
    ctx: &SharedContext,
    agent_id: &AgentId,
    work_dir: &Path,
    launch_sig: LaunchSignature,
    payload: jefe::github::SendPayload,
) {
    let mut state = app_state.write();
    state.modal = ModalState::ConfirmIssueDirtyCopy {
        agent_id: agent_id.clone(),
        work_dir: work_dir.to_path_buf(),
        signature: launch_sig,
        payload,
        confirm_focus: jefe::state::ConfirmFocus::Cancel,
    };
    let persisted = to_persisted_state(&state);
    drop(state);
    persist_state(ctx, &persisted);
}

/// Open the origin-mismatch confirm modal. The default is no/halt — the user
/// must explicitly press Enter to replace the mismatched repo and proceed.
fn prompt_origin_mismatch_confirm(
    app_state: &mut AppStateHandle,
    ctx: &SharedContext,
    prep_ctx: &PrepOutcomeContext,
    origins: OriginMismatchInfo,
) {
    let mut state = app_state.write();
    state.modal = ModalState::ConfirmIssueOriginMismatch {
        agent_id: prep_ctx.agent_id.clone(),
        work_dir: prep_ctx.work_dir.clone(),
        signature: prep_ctx.launch_sig.clone(),
        payload: prep_ctx.payload.clone(),
        actual: origins.actual,
        expected: origins.expected,
        confirm_focus: jefe::state::ConfirmFocus::Cancel,
    };
    let persisted = to_persisted_state(&state);
    drop(state);
    persist_state(ctx, &persisted);
}

/// Shared prefix for the dirty-copy and origin-mismatch confirm paths: close
/// the modal, re-check local kind availability, resolve the run target, and
/// run the centralized pre-side-effect remote availability probe. Returns the
/// resolved target on success, or `None` (after surfacing the appropriate
/// failure) when any guard fails.
fn prepare_confirm_send_target(
    app_state: &mut AppStateHandle,
    ctx: &SharedContext,
    work_dir: &Path,
    launch_sig: &LaunchSignature,
) -> Option<super::issue_prep::WorkTarget> {
    // Close the confirm modal first so the UI reflects the user's decision
    // before the (potentially slow) remote prep runs.
    close_modal_and_persist(app_state, ctx);

    // Re-check availability BEFORE prep side effects: the runtime may have
    // been removed while the confirm modal was open.
    if !super::availability::local_kind_available_or_error(
        app_state,
        launch_sig.agent_kind,
        &launch_sig.remote,
    ) {
        return None;
    }

    let target = match super::target_resolution::resolve_target(&launch_sig.remote) {
        Ok(target) => target,
        Err(error) => {
            apply_send_to_agent_failed(app_state, ctx, error);
            return None;
        }
    };

    // Centralized pre-side-effect availability probe (defect 2): BEFORE any
    // destructive prep, re-probe the selected runtime on the resolved target.
    if !super::remote_probe::pre_side_effect_runtime_available_or_error(
        app_state,
        &target,
        work_dir,
        launch_sig.agent_kind,
    ) {
        return None;
    }

    Some(target)
}

/// Dirty-copy confirm: user pressed Enter to discard uncommitted changes and
/// proceed with the issue-driven launch. Uses the **same** target-aware
/// orchestration as the initial send, but with the `Discard` policy: the
/// prep cleans the working copy (reset --hard + clean -fd, preserving
/// `.jefe/`/`.llxprt/`), checks out + pulls the default branch, writes the
/// prompt last, and then launches.
pub(super) fn confirm_issue_dirty_copy_enter(
    app_state: &mut AppStateHandle,
    ctx: &SharedContext,
    agent_id: AgentId,
    work_dir: PathBuf,
    launch_sig: LaunchSignature,
    payload: jefe::github::SendPayload,
) {
    let Some(target) = prepare_confirm_send_target(app_state, ctx, &work_dir, &launch_sig) else {
        return;
    };

    let prompt = issues_dispatch::format_issue_prompt(&payload);

    // Resolve the clone identity from the agent's repository (validates
    // github_repo owner/repo; never falls back to slug).
    let clone_identity = clone_identity_for_agent(app_state, &agent_id);

    match prepare_issue_target(
        &target,
        &work_dir,
        clone_identity.as_ref(),
        DirtyPolicy::Discard,
        &prompt,
    ) {
        Ok(PrepOutcome::Ready) => {
            preflight_and_launch_issue(
                app_state,
                ctx,
                &agent_id,
                work_dir,
                launch_sig,
                issue_assignment_from_payload(&payload),
            );
        }
        // Discard policy cleans first, so Dirty should not occur — but treat
        // it defensively as a launch failure rather than silently dropping.
        Ok(PrepOutcome::Dirty) => apply_send_to_agent_failed(
            app_state,
            ctx,
            "Working copy is still dirty after discard".to_owned(),
        ),
        Ok(PrepOutcome::OriginMismatch { actual, expected }) => {
            prompt_origin_mismatch_confirm(
                app_state,
                ctx,
                &PrepOutcomeContext {
                    agent_id,
                    work_dir,
                    launch_sig,
                    payload,
                },
                OriginMismatchInfo { actual, expected },
            );
        }
        Err(error) => apply_send_to_agent_failed(app_state, ctx, error),
    }
}

/// Origin-mismatch confirm: user pressed Enter to replace the mismatched
/// repo with a fresh clone and proceed with the issue-driven launch. This
/// removes the existing workdir, re-clones from the configured identity,
/// runs post-clone prep (checkout+pull, prompt write), then launches.
pub(super) fn confirm_issue_origin_mismatch_enter(
    app_state: &mut AppStateHandle,
    ctx: &SharedContext,
    agent_id: AgentId,
    work_dir: PathBuf,
    launch_sig: LaunchSignature,
    payload: jefe::github::SendPayload,
) {
    let Some(target) = prepare_confirm_send_target(app_state, ctx, &work_dir, &launch_sig) else {
        return;
    };

    let prompt = issues_dispatch::format_issue_prompt(&payload);
    let clone_identity = clone_identity_for_agent(app_state, &agent_id);

    // MUST-FIX #2: Validate the clone identity BEFORE calling force-reclone.
    // If the agent/repository was deleted or github_repo became invalid while
    // the modal was open, we must NOT destroy the existing repo with no
    // replacement. Fail with a clear error instead.
    let Some(clone_identity) = clone_identity else {
        apply_send_to_agent_failed(
            app_state,
            ctx,
            "Cannot force-reclone: no valid github_repo (owner/repo) configured for this agent's \
             repository."
                .to_owned(),
        );
        return;
    };

    match prepare_issue_target_force_reclone(&target, &work_dir, &clone_identity, &prompt) {
        Ok(PrepOutcome::Ready) => {
            preflight_and_launch_issue(
                app_state,
                ctx,
                &agent_id,
                work_dir,
                launch_sig,
                issue_assignment_from_payload(&payload),
            );
        }
        Ok(PrepOutcome::Dirty) => apply_send_to_agent_failed(
            app_state,
            ctx,
            "Working copy is dirty after force-reclone".to_owned(),
        ),
        Ok(PrepOutcome::OriginMismatch { actual, expected }) => {
            // A force-reclone clones from the validated configured identity,
            // so an OriginMismatch here is an unexpected error (the clone did
            // not land on the configured origin), NOT a re-prompt. Re-opening
            // the modal could loop indefinitely, so fail hard with a clear
            // message instead.
            apply_send_to_agent_failed(
                app_state,
                ctx,
                format!(
                    "Force-reclone completed but the working copy origin is {actual}, expected \
                     {expected}. This should not happen after a fresh clone; please verify the \
                     configured github_repo and retry."
                ),
            );
        }
        Err(error) => apply_send_to_agent_failed(app_state, ctx, error),
    }
}

pub(super) struct IssueSendInfo {
    pub(super) agent_id: AgentId,
    pub(super) work_dir: PathBuf,
    pub(super) signature: LaunchSignature,
    pub(super) payload: jefe::github::SendPayload,
    pub(super) clone_identity: Option<CloneIdentity>,
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
    let agent = state.agents.iter().find(|a| a.id == agent_id)?;
    let repo = state.repository_by_id(&agent.repository_id)?;
    let focused_comment = focused_issue_comment(state, detail);
    let work_dir = agent.work_dir.clone();
    let signature = launch_signature_for_agent(agent, repo);
    // Detail carries the immutable source repository for the loaded issue.
    // Worktree preparation still uses the configured fork's clone identity.
    let payload = jefe::github::GhClient::build_send_payload(
        &detail.repo_owner_name,
        detail,
        focused_comment.as_ref(),
        &repo.issue_base_prompt,
    );

    // Clone identity derives ONLY from a valid github_repo (owner/repo),
    // never from slug. HTTPS clone URL regardless of local/remote (issue #184).
    let clone_identity = CloneIdentity::from_repository(repo);
    Some(IssueSendInfo {
        agent_id,
        work_dir,
        signature,
        payload,
        clone_identity,
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

/// Resolve the validated clone identity for an agent's repository from
/// `AppState`. Reads `github_repo` (never `slug`).
fn clone_identity_for_agent(
    app_state: &AppStateHandle,
    agent_id: &AgentId,
) -> Option<CloneIdentity> {
    let state = app_state.read();
    let agent = state.agents.iter().find(|a| &a.id == agent_id)?;
    let repo = state.repository_by_id(&agent.repository_id)?;
    let identity = CloneIdentity::from_repository(repo);
    drop(state);
    identity
}

fn launch_issue_agent(
    app_state: &mut AppStateHandle,
    ctx: &SharedContext,
    agent_id: AgentId,
    work_dir: PathBuf,
    launch_sig: LaunchSignature,
    assignment: IssueAssignment,
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

    // Self-assign the issue to the authenticated viewer only on a successful
    // launch (issue #186). Non-blocking: failures surface a warning, not a
    // send failure. The decision is pure (`direct_assignment_action`) so the
    // success/failure × resolved/unavailable matrix is unit-tested.
    apply_assignment_action(
        app_state,
        ctx,
        direct_assignment_action(launched, assignment),
    );
}

/// Apply a deterministic [`IssueAssignmentAction`] at the orchestration
/// boundary: spawn the background assignment, surface a non-blocking warning,
/// or do nothing. Shared by the direct and post-preflight paths (issue #186).
fn apply_assignment_action(
    app_state: &mut AppStateHandle,
    ctx: &SharedContext,
    action: IssueAssignmentAction,
) {
    match action {
        IssueAssignmentAction::Spawn(resolved) => {
            spawn_issue_self_assignment(app_state, ctx, resolved);
        }
        IssueAssignmentAction::Warn {
            owner_repo,
            issue_number,
            reason,
        } => fail_assignment(app_state, ctx, &owner_repo, issue_number, &reason),
        IssueAssignmentAction::None => {}
    }
}

fn spawn_and_attach_fresh_for_issue(
    ctx: &SharedContext,
    agent_id: &AgentId,
    work_dir: &Path,
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

fn apply_send_to_agent_failed(app_state: &mut AppStateHandle, ctx: &SharedContext, error: String) {
    let mut state = app_state.write();
    *state = std::mem::take(&mut *state).apply(AppEvent::SendToAgentFailed { error });
    let persisted = to_persisted_state(&state);
    drop(state);
    persist_state(ctx, &persisted);
}

/// After a successful issue-driven launch, self-assign the issue to the
/// authenticated viewer (issue #186). Runs as a non-blocking background task:
/// it resolves the viewer login, then POSTs to the assignees endpoint. Any
/// failure applies an `IssueSelfAssignmentFailed` event through the reducer,
/// which surfaces a `warning_message` (the send itself already succeeded).
fn spawn_issue_self_assignment(
    app_state: &AppStateHandle,
    ctx: &SharedContext,
    assignment: SelfAssignment,
) {
    let owner = assignment.owner;
    let repo = assignment.repo;
    let owner_repo = assignment.owner_repo;
    let owner_repo_panic = owner_repo.clone();
    let issue_number = assignment.issue_number;
    gh_async::spawn_gh_task_with_panic(
        app_state,
        ctx,
        move |mut app_state, ctx| {
            let Some(client) = github_client(&ctx) else {
                fail_assignment(
                    &mut app_state,
                    &ctx,
                    &owner_repo,
                    issue_number,
                    "GitHub client is unavailable",
                );
                return;
            };
            let viewer = match client.viewer_login() {
                Ok(login) => login,
                Err(error) => {
                    warn!(error = %error, "could not resolve viewer login for self-assignment");
                    fail_assignment(
                        &mut app_state,
                        &ctx,
                        &owner_repo,
                        issue_number,
                        &error.to_string(),
                    );
                    return;
                }
            };
            if let Err(error) = client.assign_issue(&owner, &repo, issue_number, &viewer) {
                warn!(
                    viewer = %viewer,
                    error = %error,
                    "could not self-assign issue on send"
                );
                fail_assignment(
                    &mut app_state,
                    &ctx,
                    &owner_repo,
                    issue_number,
                    &error.to_string(),
                );
            }
        },
        move |mut app_state, ctx, message| {
            fail_assignment(
                &mut app_state,
                &ctx,
                &owner_repo_panic,
                issue_number,
                &format!("Issue self-assignment panicked: {message}"),
            );
        },
    );
}

/// Apply the non-blocking self-assignment-failed event through the reducer so
/// the warning transition is deterministic and unit-testable. The issue send
/// itself already succeeded, so this must not flip the launch into a failure.
fn fail_assignment(
    app_state: &mut AppStateHandle,
    ctx: &SharedContext,
    owner_repo: &str,
    issue_number: u64,
    error: &str,
) {
    apply_and_persist(
        app_state,
        ctx,
        AppEvent::IssueSelfAssignmentFailed {
            owner_repo: owner_repo.to_string(),
            issue_number,
            error: error.to_string(),
        },
    );
}

/// Reconstruct the assignment follow-up from the state-level intent carried
/// through the preflight modal and fire it after a successful post-preflight
/// issue-driven launch (issue #186). Called by the preflight confirm path.
///
/// The decision is pure (`post_preflight_assignment_action`); this function
/// only applies its result at the boundary. `None` (non-issue launch) or a
/// failed resumed launch both yield no action.
pub(super) fn spawn_post_preflight_issue_self_assignment(
    app_state: &mut AppStateHandle,
    ctx: &SharedContext,
    launch_ok: bool,
    carried: Option<&jefe::state::IssueSelfAssignmentFollowUp>,
) {
    let action = post_preflight_assignment_action(launch_ok, carried);
    apply_assignment_action(app_state, ctx, action);
}

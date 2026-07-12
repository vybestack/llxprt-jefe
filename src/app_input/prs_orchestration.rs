//! PR-mode dispatch routing + orchestration helpers (extracted from mod.rs).
//!
//! @plan PLAN-20260624-PR-MODE.P11
//! @requirement REQ-PR-001
//! @requirement REQ-PR-003
//! @requirement REQ-PR-009
//! @requirement REQ-PR-010
//! @requirement REQ-PR-011
//! @requirement REQ-PR-012
//! @pseudocode component-004 lines 97-175

use jefe::domain::{AgentId, LaunchSignature, Repository};
use jefe::messages::{AppMessage, PullRequestsMessage};
use jefe::runtime::RuntimeManager;
use jefe::state::{AppEvent, AppState};
use tracing::warn;

use super::fresh_prompt::{FreshPromptKind, prepare_fresh_prompt_signature};
use super::{
    AppStateHandle, REMOTE_ATTACH_SETTLE_DELAY, SharedContext, apply_and_persist,
    clear_agent_runtime_attachment, dispatch_app_event, gh_async, github_client,
    launch_signature_for_agent, mark_agent_runtime_attached, persist_state, pid_on_success,
    preflight_or_prompt, prs_comments_dispatch, prs_dispatch, prs_list_dispatch, prs_mutation,
    to_persisted_state,
};

// ── PR-mode dispatch routing + loader helpers ──────────────────────────────
//
// @plan PLAN-20260624-PR-MODE.P11
// @requirement REQ-PR-001
// @requirement REQ-PR-003
// @requirement REQ-PR-009
// @requirement REQ-PR-010
// @requirement REQ-PR-011
// @requirement REQ-PR-012
// @pseudocode component-004 lines 97-175

/// Route a `PullRequestsMessage` to the appropriate dispatch helper.
///
/// Mirrors the `AppMessage::Issues` arm structure. Side-effecting arms route
/// to the PR dispatch/loader helpers; all other variants fall through to
/// `apply_and_persist` via the catch-all.
///
/// @plan PLAN-20260624-PR-MODE.P11
/// @requirement REQ-PR-001
/// @requirement REQ-PR-003
/// @requirement REQ-PR-010
/// @requirement REQ-PR-011
/// @requirement REQ-PR-012
/// @pseudocode component-004 lines 97-118
pub(super) fn dispatch_prs_message(
    app_state: &mut AppStateHandle,
    ctx: &SharedContext,
    message: PullRequestsMessage,
) {
    update_pr_detail_viewport_rows(app_state);
    route_prs_message(app_state, ctx, message);
}

/// Route a `PullRequestsMessage` to the appropriate dispatch helper.
/// Extracted from `dispatch_prs_message` to stay under the per-function line
/// limit (issue #128).
fn route_prs_message(
    app_state: &mut AppStateHandle,
    ctx: &SharedContext,
    message: PullRequestsMessage,
) {
    use jefe::messages::{PrInlineMsg, ScrollDir};

    match message {
        m @ (PullRequestsMessage::Navigate(_)
        | PullRequestsMessage::CycleFocus
        | PullRequestsMessage::CycleFocusReverse) => {
            dispatch_prs_navigation(app_state, ctx, m);
        }
        m @ (PullRequestsMessage::EnterMode
        | PullRequestsMessage::RefocusList
        | PullRequestsMessage::ApplyFilter
        | PullRequestsMessage::ClearFilter
        | PullRequestsMessage::ApplySearch) => {
            prs_list_dispatch::dispatch_pr_list_reload(app_state, ctx, m);
        }
        PullRequestsMessage::Enter => {
            apply_and_persist(app_state, ctx, AppEvent::PrListEnter);
            prs_dispatch::load_pr_detail_for_selection(app_state, ctx);
        }
        m @ PullRequestsMessage::ScrollDetail(ScrollDir::Down | ScrollDir::PageDown) => {
            apply_and_persist(app_state, ctx, AppEvent::from(m));
            prs_comments_dispatch::load_more_pr_comments(app_state, ctx);
        }
        PullRequestsMessage::AgentChooserConfirm => {
            dispatch_pr_agent_chooser_confirm(app_state, ctx);
        }
        m @ PullRequestsMessage::Inline(PrInlineMsg::Submit) => {
            apply_and_persist(app_state, ctx, AppEvent::from(m));
            prs_mutation::handle_pr_inline_submit(app_state, ctx);
        }
        PullRequestsMessage::OpenInBrowser => {
            apply_and_persist(
                app_state,
                ctx,
                AppEvent::from(AppMessage::PullRequests(PullRequestsMessage::OpenInBrowser)),
            );
            prs_dispatch::dispatch_pr_open_in_browser(app_state, ctx);
        }
        PullRequestsMessage::OpenMergeChooser => {
            apply_and_persist(app_state, ctx, AppEvent::PrOpenMergeChooser);
            let chooser_open = { app_state.read().prs_state.merge_chooser.is_some() };
            if chooser_open {
                prs_dispatch::dispatch_pr_merge_methods_load(app_state, ctx);
            }
        }
        PullRequestsMessage::MergeConfirm => {
            apply_and_persist(app_state, ctx, AppEvent::PrMergeConfirm);
            prs_dispatch::dispatch_pr_merge(app_state, ctx);
        }
        PullRequestsMessage::ToggleThreadResolve { .. } => {
            apply_and_persist(app_state, ctx, AppEvent::from(message));
            prs_mutation::handle_pr_thread_resolve(app_state, ctx);
        }
        PullRequestsMessage::Merged { .. } | PullRequestsMessage::CommentCreated { .. } => {
            let is_merged = matches!(message, PullRequestsMessage::Merged { .. });
            dispatch_prs_post_mutation(app_state, ctx, message, is_merged);
        }
        // All other PullRequests variants (data-load results, notices, etc.)
        // route through the reducer only.
        message => apply_and_persist(app_state, ctx, AppEvent::from(message)),
    }
}

/// Post-mutation refresh: after a merge or comment, reload the list/detail to
/// reflect server state (issue #128).
fn dispatch_prs_post_mutation(
    app_state: &mut AppStateHandle,
    ctx: &SharedContext,
    message: PullRequestsMessage,
    is_merged: bool,
) {
    apply_and_persist(app_state, ctx, AppEvent::from(message));
    // Merged reloads BOTH list + detail; CommentCreated reloads detail only.
    if is_merged {
        prs_list_dispatch::request_pr_list_reload(app_state, ctx);
    }
    prs_dispatch::load_pr_detail_for_selection(app_state, ctx);
}

/// Request a silent background refresh of the PR list + detail (issue #128).
///
/// Fires ONLY when the PR view is open (`DashboardPullRequests`) and no list
/// or detail load is already in flight. The refresh is silent: it preserves
/// selection, scroll offset, filter, and search query, and does NOT flash the
/// loading spinner.
///
/// @requirement issue #128
pub fn request_pr_background_refresh(app_state: &mut AppStateHandle, ctx: &SharedContext) {
    let should_refresh = {
        let state = app_state.read();
        should_background_refresh(
            state.screen_mode,
            state.prs_state.list_reload_pending.is_some(),
            state.prs_state.list_page_pending.is_some(),
            state.prs_state.detail_pending.is_some(),
        )
    };
    if should_refresh {
        prs_list_dispatch::request_pr_list_silent_refresh(app_state, ctx);
        prs_dispatch::load_pr_detail_silent_refresh(app_state, ctx);
    }
}

/// Pure guard predicate for `request_pr_background_refresh` (issue #128).
/// Returns `true` when the PR view is open AND no list/detail load is in
/// flight. Extracted so the guard logic is unit-testable without an
/// `AppStateHandle`.
///
/// @requirement issue #128
pub(super) fn should_background_refresh(
    screen_mode: jefe::state::ScreenMode,
    list_reload_pending: bool,
    list_page_pending: bool,
    detail_pending: bool,
) -> bool {
    screen_mode == jefe::state::ScreenMode::DashboardPullRequests
        && !list_reload_pending
        && !list_page_pending
        && !detail_pending
}

/// Silently refresh PR detail for the currently selected PR (issue #128).
/// Mirrors `prs_dispatch::load_pr_detail_for_selection` but does NOT set
/// `loading.detail` (no spinner flash), preserves `detail_subfocus` and
/// `detail_scroll_offset` on success, and does NOT surface errors visibly on
/// failure. Lives here (not in `prs_dispatch.rs`) to keep that file under the
/// architecture boundary line limit.
///
/// @requirement issue #128
pub(super) fn load_pr_detail_silent_refresh(app_state: &mut AppStateHandle, ctx: &SharedContext) {
    let Some(mut params) = prs_dispatch::pr_detail_load_params(app_state) else {
        return;
    };
    mark_pr_detail_silent_loading(app_state, &mut params);
    if params.owner.is_empty() || params.repo.is_empty() {
        // Missing repo: silently clear the pending marker (no visible error).
        apply_and_persist(app_state, ctx, silent_refresh_failed_event(&params));
        return;
    }

    let panic_params = params.clone();
    gh_async::spawn_gh_task_with_panic(
        app_state,
        ctx,
        move |mut app_state, ctx| {
            let event = silent_refresh_event(&ctx, &params);
            apply_and_persist(&mut app_state, &ctx, event);
        },
        move |mut app_state, ctx, _message| {
            // On panic: silently clear the pending marker (no visible error).
            apply_and_persist(
                &mut app_state,
                &ctx,
                silent_refresh_failed_event(&panic_params),
            );
        },
    );
}

/// Mark the PR detail as silently loading (does NOT set `loading.detail`).
/// @requirement issue #128
fn mark_pr_detail_silent_loading(
    app_state: &mut AppStateHandle,
    params: &mut prs_dispatch::PrDetailLoadParams,
) {
    let mut state = app_state.write();
    let request_id = state.next_pr_detail_request_id();
    state.mark_pr_detail_silent_loading(params.scope_repo_id.clone(), params.pr_number, request_id);
    drop(state);
    params.request_id = request_id;
}

/// Build the silent-refresh detail-loaded/failed event from the gh result.
/// Unlike the loud variant, failures are delivered as
/// `PrDetailSilentRefreshFailed` (no visible error) and success as
/// `PrDetailSilentRefreshed` (no `loading.detail` flag).
/// @requirement issue #128
fn silent_refresh_event(
    ctx: &SharedContext,
    params: &prs_dispatch::PrDetailLoadParams,
) -> AppEvent {
    let result = github_client(ctx).map(|client| {
        client.get_pull_request_detail(&params.owner, &params.repo, params.pr_number)
    });
    match result {
        Some(Ok(detail)) => AppEvent::PrDetailSilentRefreshed {
            scope_repo_id: params.scope_repo_id.clone(),
            pr_number: params.pr_number,
            request_id: params.request_id,
            detail: std::boxed::Box::new(detail),
        },
        _ => silent_refresh_failed_event(params),
    }
}

/// Build the silent-refresh failure event (clears pending, no visible error).
/// @requirement issue #128
fn silent_refresh_failed_event(params: &prs_dispatch::PrDetailLoadParams) -> AppEvent {
    AppEvent::PrDetailSilentRefreshFailed {
        scope_repo_id: params.scope_repo_id.clone(),
        pr_number: params.pr_number,
        request_id: params.request_id,
    }
}

/// PR navigation dispatch: reducer moves selection/repo scope, then detail
/// preview + repo-scope refresh.
///
/// @plan PLAN-20260624-PR-MODE.P11
/// @requirement REQ-PR-003
/// @pseudocode component-004 lines 119-126
fn dispatch_prs_navigation(
    app_state: &mut AppStateHandle,
    ctx: &SharedContext,
    message: PullRequestsMessage,
) {
    let (focus, prev_repo_idx, prev_pr_idx) = {
        let state = app_state.read();
        (
            state.prs_state.pr_focus,
            state.selected_repository_index,
            state.prs_state.selected_pr_index,
        )
    };
    apply_and_persist(app_state, ctx, AppEvent::from(message));
    refresh_prs_navigation(app_state, ctx, focus, prev_repo_idx, prev_pr_idx);
}

/// Refresh PR detail preview + repo scope after a navigation event.
///
/// @plan PLAN-20260624-PR-MODE.P11
/// @requirement REQ-PR-003
/// @pseudocode component-004 lines 123-126
fn refresh_prs_navigation(
    app_state: &mut AppStateHandle,
    ctx: &SharedContext,
    focus: jefe::state::PrFocus,
    prev_repo_idx: Option<usize>,
    prev_pr_idx: Option<usize>,
) {
    match focus {
        jefe::state::PrFocus::RepoList => {
            refresh_repo_scope_if_changed_prs(app_state, ctx, prev_repo_idx);
        }
        jefe::state::PrFocus::PrList => {
            refresh_pr_preview_if_changed(app_state, prev_pr_idx);
            prs_list_dispatch::load_more_prs_if_at_end(app_state, ctx);
        }
        jefe::state::PrFocus::PrDetail => {}
    }
}

/// Reset + reload the PR list when the selected repository changes.
///
/// @plan PLAN-20260624-PR-MODE.P11
/// @requirement REQ-PR-007
/// @pseudocode component-004 lines 123-125
fn refresh_repo_scope_if_changed_prs(
    app_state: &mut AppStateHandle,
    ctx: &SharedContext,
    prev_repo_idx: Option<usize>,
) {
    let new_repo_idx = app_state.read().selected_repository_index;
    if new_repo_idx == prev_repo_idx {
        return;
    }
    reset_pr_list_for_repo_change(app_state);
    dispatch_app_event(app_state, ctx, AppEvent::RefocusPrList);
    app_state.write().prs_state.pr_focus = jefe::state::PrFocus::RepoList;
    prs_list_dispatch::request_pr_list_reload(app_state, ctx);
}

/// Reset the PR list state for a repository change (mirrors the issues reset).
///
/// @plan PLAN-20260624-PR-MODE.P11
/// @requirement REQ-PR-007
/// @pseudocode component-004 lines 123-125
fn reset_pr_list_for_repo_change(app_state: &mut AppStateHandle) {
    let mut state = app_state.write();
    state.prs_state.pull_requests.clear();
    state.prs_state.selected_pr_index = None;
    state.prs_state.pr_detail = None;
    state.prs_state.list_cursor = None;
    state.prs_state.has_more_prs = false;
    state.prs_state.error = None;
    if state.prs_state.inline_state != jefe::state::InlineState::None {
        state.prs_state.draft_notice = Some("Unsent draft discarded".to_string());
    }
    state.prs_state.inline_state = jefe::state::InlineState::None;
    state.prs_state.mutation_pending = None;
    state.prs_state.loading.detail = false;
    state.prs_state.loading.comments = false;
    state.prs_state.detail_pending = None;
    state.prs_state.comments_page_pending = None;
    state.prs_state.list_reload_pending = None;
    state.prs_state.list_page_pending = None;
    state.prs_state.agent_chooser = None;
    state.prs_state.merge_chooser = None;
    state.prs_state.merge_mutation_pending = None;
    state.prs_state.loading.list = true;
}

/// Refresh the PR preview from list data when the selected PR changes.
///
/// @plan PLAN-20260624-PR-MODE.P11
/// @requirement REQ-PR-003
/// @pseudocode component-004 lines 119-126
fn refresh_pr_preview_if_changed(app_state: &mut AppStateHandle, prev_pr_idx: Option<usize>) {
    let new_pr_idx = app_state.read().prs_state.selected_pr_index;
    if new_pr_idx != prev_pr_idx {
        prs_dispatch::preview_pr_from_list(app_state);
    }
}

/// Update the PR detail viewport row count from the layout module.
///
/// Reads `crossterm::size()` ONCE at the dispatch boundary and writes the
/// computed viewport rows into `prs_state.detail_viewport_rows` so the
/// reducers never touch crossterm (#37/#39/#55). The content width for
/// truncation is computed independently by the screen renderer (it does not
/// live in reducer state — the reducer never wraps).
///
/// @plan PLAN-20260624-PR-MODE.P11
/// @requirement REQ-PR-009
/// @pseudocode component-004 lines 156-159
fn update_pr_detail_viewport_rows(app_state: &mut AppStateHandle) {
    let (term_rows, _term_cols) = crossterm::terminal::size().map_or((40, 120), |(c, r)| (r, c));
    let mut state = app_state.write();
    state.prs_state.detail_viewport_rows = jefe::layout::prs_detail_viewport_rows(
        term_rows as usize,
        state.prs_state.error.is_some(),
        state.prs_state.filter_ui.controls_open,
    );
}

/// The PR prompt file written into the agent work dir and referenced in
/// the launch instruction. Both the fresh-prompt signature construction
/// (the agent instruction string) and `write_pr_prompt` (the on-disk path)
/// must use exactly this relative path.
const PR_PROMPT_RELATIVE_PATH: &str = ".jefe/pr-prompt.md";

/// Dispatch the PR agent-chooser confirm (send-to-agent) side effects.
///
/// Mirrors `dispatch_agent_chooser_confirm` exactly: resolve send info, apply
/// the chooser-confirm reducer (closes chooser + records send), write the PR
/// prompt, then launch the agent. The ordering is reducer-before-spawn so the
/// chooser is closed and the send recorded BEFORE any side effect.
///
/// @plan PLAN-20260624-PR-MODE.P11
/// @requirement REQ-PR-011
/// @pseudocode component-003 lines 147-156
fn dispatch_pr_agent_chooser_confirm(app_state: &mut AppStateHandle, ctx: &SharedContext) {
    let send_info = pr_send_info(app_state);
    apply_and_persist(app_state, ctx, AppEvent::PrAgentChooserConfirm);

    let Some(send_info) = send_info else {
        return;
    };

    // Use the shared kind-specific prompt construction so CodePuppy PR sends
    // do not get a duplicate -i (the runtime layer prepends it) and the
    // issue/PR send paths agree on the exact arg shape.
    let launch_sig = prepare_fresh_prompt_signature(
        send_info.signature,
        FreshPromptKind::PullRequest,
        PR_PROMPT_RELATIVE_PATH,
    );

    // Availability + target validation BEFORE any prompt side effect: a
    // missing agent runtime or an invalid/incomplete remote config must not
    // trigger a local or remote prompt write.
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
            apply_pr_send_to_agent_failed(app_state, ctx, error);
            return;
        }
    };

    // Centralized pre-side-effect availability probe (defect 2): BEFORE any
    // PR prompt write, probe the selected runtime on the resolved target.
    // For local targets this reuses the session snapshot; for remote targets
    // this is a no-install/no-setup/side-effect-free ssh -T probe for the
    // exact binary executed as the effective run_as_user. Unavailable remote
    // means no prompt write operation.
    if !super::remote_probe::pre_side_effect_runtime_available_or_error(
        app_state,
        &target,
        &send_info.work_dir,
        launch_sig.agent_kind,
    ) {
        return;
    }

    // Write the PR prompt to the selected WorkTarget (local fs or remote
    // ssh -T with prompt bytes via stdin). The remote path reuses the exact
    // production remote prompt planning seam from `remote_probe` so `.jefe/
    // pr-prompt.md` is targeted, prompt bytes are stdin, and adversarial
    // content is absent from argv.
    let prompt_content = prs_dispatch::format_pr_prompt(&send_info.payload);
    let write_result = match &target {
        super::issue_prep::WorkTarget::Local => super::issue_prep::write_prompt_to_target(
            &target,
            &send_info.work_dir,
            PR_PROMPT_RELATIVE_PATH,
            &prompt_content,
        ),
        super::issue_prep::WorkTarget::Remote(remote) => super::remote_probe::write_remote_prompt(
            remote,
            &send_info.work_dir,
            PR_PROMPT_RELATIVE_PATH,
            &prompt_content,
        ),
    };
    if let Err(error) = write_result {
        apply_pr_send_to_agent_failed(app_state, ctx, error);
        return;
    }

    if preflight_or_prompt(app_state, ctx, &send_info.agent_id, &launch_sig, None) {
        launch_pr_agent(
            app_state,
            ctx,
            send_info.agent_id,
            send_info.work_dir,
            launch_sig,
        );
    }
}

/// Write the PR agent prompt to disk (local only).
///
/// Retained for tests that verify the local PR prompt write step in isolation.
/// The production dispatch path uses [`issue_prep::write_prompt_to_target`] so
/// remote PR prompts travel via stdin over `ssh -T`.
///
/// @plan PLAN-20260624-PR-MODE.P11
/// @requirement REQ-PR-011
#[cfg(test)]
pub(super) fn write_pr_prompt(
    work_dir: &std::path::Path,
    payload: &jefe::github::PrSendPayload,
) -> Result<(), String> {
    let prompt_dir = work_dir.join(".jefe");
    std::fs::create_dir_all(&prompt_dir)
        .map_err(|error| format!("Failed to create .jefe dir: {error}"))?;
    let prompt_path = prompt_dir.join("pr-prompt.md");
    let prompt_content = prs_dispatch::format_pr_prompt(payload);
    std::fs::write(&prompt_path, &prompt_content)
        .map_err(|error| format!("Failed to write PR prompt: {error}"))
}

/// Resolved context needed to send a PR to an agent (mirrors `IssueSendInfo`).
///
/// @plan PLAN-20260624-PR-MODE.P11
/// @requirement REQ-PR-011
/// @pseudocode component-003 lines 164-175
pub(super) struct PrSendInfo {
    pub(super) agent_id: AgentId,
    pub(super) work_dir: std::path::PathBuf,
    pub(super) signature: LaunchSignature,
    pub(super) payload: jefe::github::PrSendPayload,
}

/// Resolve the agent, repo, focused comment, work dir, signature, and payload
/// for sending the selected PR to an agent (mirrors `issue_send_info`).
///
/// Sources from `state.prs_state.agent_chooser` + `state.prs_state.pr_detail`.
/// Returns `None` (via `?`) when chooser/detail/agent/repo are absent.
///
/// @plan PLAN-20260624-PR-MODE.P11
/// @requirement REQ-PR-011
/// @pseudocode component-003 lines 164-175
fn pr_send_info(app_state: &AppStateHandle) -> Option<PrSendInfo> {
    let state = app_state.read();
    let result = pr_send_info_from_state(&state);
    drop(state);
    result
}

/// Resolve the PR send info from a raw `AppState` (testable without
/// `AppStateHandle`).
///
/// @plan PLAN-20260624-PR-MODE.P11
/// @requirement REQ-PR-011
/// @pseudocode component-003 lines 164-175
pub(super) fn pr_send_info_from_state(state: &AppState) -> Option<PrSendInfo> {
    let chooser = state.prs_state.agent_chooser.as_ref()?;
    let detail = state.prs_state.pr_detail.as_ref()?;
    let (agent_id, _) = chooser.agents.get(chooser.selected_index)?.clone();
    let agent = state
        .agents
        .iter()
        .find(|agent| agent.id == agent_id)?
        .clone();
    let repo = state.repository_by_id(&agent.repository_id)?;
    let focused_comment = focused_pr_comment(state, detail);
    let work_dir = agent.work_dir.clone();
    let signature = launch_signature_for_agent(&agent, repo);
    let payload = jefe::github::GhClient::build_pr_send_payload(
        &repo.slug,
        detail,
        focused_comment.as_ref(),
        pr_base_prompt(repo),
    );

    Some(PrSendInfo {
        agent_id,
        work_dir,
        signature,
        payload,
    })
}

/// Resolve the focused PR comment when `detail_subfocus` targets a comment
/// (mirrors `focused_issue_comment`).
///
/// @plan PLAN-20260624-PR-MODE.P11
/// @requirement REQ-PR-011
/// @pseudocode component-003 lines 164-175
fn focused_pr_comment(
    state: &AppState,
    detail: &jefe::domain::PullRequestDetail,
) -> Option<jefe::domain::IssueComment> {
    match state.prs_state.detail_subfocus {
        jefe::state::PrDetailSubfocus::Comment(idx) => detail.comments.get(idx).cloned(),
        _ => None,
    }
}

/// Resolve the base prompt for a PR send.
///
/// `Repository` does not yet carry a dedicated `pr_base_prompt` field; this
/// reuses the issue base prompt as a stand-in.
///
/// @plan PLAN-20260624-PR-MODE.P11
/// @requirement REQ-PR-011
/// @pseudocode component-003 lines 164-175
fn pr_base_prompt(repo: &Repository) -> &str {
    &repo.issue_base_prompt
}

/// Launch the runtime agent for a PR send.
///
/// Mirrors `launch_issue_agent`: spawn + attach the agent session (same runtime
/// path issues uses), then deliver success/failure. When `ctx` is `None`
/// (tests), `spawn_and_attach_fresh_for_pr` returns `false` (the shared helper
/// guards on `ctx` being present) so the failure event is delivered without a
/// real spawn — replicating the issues guard exactly.
///
/// @plan PLAN-20260624-PR-MODE.P11
/// @requirement REQ-PR-011
/// @pseudocode component-003 lines 155-163
fn launch_pr_agent(
    app_state: &mut AppStateHandle,
    ctx: &SharedContext,
    agent_id: AgentId,
    work_dir: std::path::PathBuf,
    launch_sig: LaunchSignature,
) {
    let launched = spawn_and_attach_fresh_for_pr(ctx, &agent_id, &work_dir, &launch_sig);
    // Resolve the worker PID before taking the app-state write lock
    // (lock-ordering constraint). Skipped on the failure path.
    let pid = pid_on_success(ctx, &agent_id, launched);
    let mut state = app_state.write();
    if launched {
        persist_pr_agent_launch_success(&mut state, &agent_id, launch_sig, pid);
    } else {
        *state = std::mem::take(&mut *state).apply(AppEvent::PrSendToAgentFailed {
            error: "Failed to launch agent".to_string(),
        });
    }
    let persisted = to_persisted_state(&state);
    drop(state);
    persist_state(ctx, &persisted);
}

/// Spawn a fresh runtime session and attach it for a PR send.
///
/// Mirrors `spawn_and_attach_fresh_for_issue`: when `ctx` is `None` (no runtime
/// context, as in unit tests), returns `false` without spawning. Otherwise
/// spawns a fresh session and attaches it.
///
/// @plan PLAN-20260624-PR-MODE.P11
/// @requirement REQ-PR-011
/// @pseudocode component-003 lines 147-175
fn spawn_and_attach_fresh_for_pr(
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
        Ok(()) => {
            std::thread::sleep(REMOTE_ATTACH_SETTLE_DELAY);
            match ctx_guard.runtime.attach(agent_id) {
                Ok(()) => true,
                Err(error) => {
                    warn!(agent_id = %agent_id.0, error = %error, "could not attach agent after PR send");
                    let _ = ctx_guard.runtime.mark_session_dead(agent_id);
                    false
                }
            }
        }
        Err(error) => {
            warn!(agent_id = %agent_id.0, error = %error, "could not spawn agent for PR send");
            false
        }
    }
}

/// Persist the PR agent launch success: set runtime binding, clear attachments,
/// mark the launched agent attached.
///
/// Mirrors `persist_issue_agent_launch_success`, reusing the shared helpers
/// (`clear_agent_runtime_attachment`, `mark_agent_runtime_attached`).
///
/// @plan PLAN-20260624-PR-MODE.P11
/// @requirement REQ-PR-011
/// @pseudocode component-003 lines 147-175
fn persist_pr_agent_launch_success(
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

/// Apply a `PrSendToAgentFailed` event + persist (mirrors
/// `apply_send_to_agent_failed` for issues).
///
/// @plan PLAN-20260624-PR-MODE.P11
/// @requirement REQ-PR-011
/// @pseudocode component-003 lines 155-163
fn apply_pr_send_to_agent_failed(
    app_state: &mut AppStateHandle,
    ctx: &SharedContext,
    error: String,
) {
    let mut state = app_state.write();
    *state = std::mem::take(&mut *state).apply(AppEvent::PrSendToAgentFailed { error });
    let persisted = to_persisted_state(&state);
    drop(state);
    persist_state(ctx, &persisted);
}

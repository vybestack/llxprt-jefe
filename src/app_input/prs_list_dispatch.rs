//! PR-mode PR-list reload/fetch dispatch helpers.
//!
//! Mirrors `issues_list_dispatch.rs`. These helpers own the background fetch of
//! PR list pages via the `gh` CLI and persist the resulting state transitions.
//! All `gh` I/O runs off the UI thread via `spawn_gh_task_with_panic`.
//!
//! @plan PLAN-20260624-PR-MODE.P11
//! @requirement REQ-PR-006
//! @requirement REQ-PR-007
//! @pseudocode component-004 lines 127-137

use jefe::messages::PullRequestsMessage;
use jefe::state::AppEvent;

use super::{
    AppStateHandle, SharedContext, apply_and_persist, gh_async, github_client, persist_state,
    prs_dispatch, to_persisted_state,
};

/// Apply a list-reload message through the reducer, then fetch the list.
///
/// @plan PLAN-20260624-PR-MODE.P11
/// @requirement REQ-PR-006
/// @requirement REQ-PR-007
/// @pseudocode component-004 lines 101-102
pub fn dispatch_pr_list_reload(
    app_state: &mut AppStateHandle,
    ctx: &SharedContext,
    message: PullRequestsMessage,
) {
    let fresh_reload = is_fresh_pr_list_reload(&message);
    apply_and_persist(app_state, ctx, AppEvent::from(message));
    dispatch_pr_list_fetch(app_state, ctx, fresh_reload, false);
}

/// Whether the message triggers a fresh (cursor-resetting) list reload.
///
/// @plan PLAN-20260624-PR-MODE.P11
/// @requirement REQ-PR-006
/// @pseudocode component-004 lines 101-102
fn is_fresh_pr_list_reload(message: &PullRequestsMessage) -> bool {
    matches!(
        message,
        PullRequestsMessage::EnterMode
            | PullRequestsMessage::RefocusList
            | PullRequestsMessage::ApplyFilter
            | PullRequestsMessage::ClearFilter
            | PullRequestsMessage::ApplySearch
    )
}

/// Fetch the PR list page via gh (off the UI thread).
///
/// Validates the slug, sets loading + a monotonic request id, then spawns
/// `GhClient::list_pull_requests` via `spawn_gh_task_with_panic`, delivering
/// `PrListLoaded`/`PrListPageLoaded` on success or `PrListLoadFailed` on
/// Err/panic.
///
/// @plan PLAN-20260624-PR-MODE.P11
/// @requirement REQ-PR-006
/// @pseudocode component-004 lines 127-137
pub(super) fn dispatch_pr_list_fetch(
    app_state: &mut AppStateHandle,
    ctx: &SharedContext,
    fresh_reload: bool,
    silent: bool,
) {
    let mut params = pr_fetch_params(app_state, fresh_reload, silent);

    if params.owner.is_empty() || params.repo.is_empty() {
        if params.silent {
            // Silent refresh of a repo with no GitHub slug: silently no-op
            // (do NOT surface a visible error — issue #128).
            return;
        }
        persist_missing_github_repo(app_state, ctx);
        return;
    }

    params.request_id = mark_pr_list_fetch_loading(app_state, &params);
    let panic_params = params.clone();
    gh_async::spawn_gh_task_with_panic(
        app_state,
        ctx,
        move |mut app_state, ctx| {
            let result = fetch_pr_list(&ctx, &params);
            persist_pr_list_result(&mut app_state, &ctx, &params, result);
        },
        move |mut app_state, ctx, message| {
            persist_pr_list_failed(
                &mut app_state,
                &ctx,
                &panic_params,
                format!("GitHub PR list task panicked: {message}"),
            );
        },
    );
}

/// Request a fresh PR list reload (cursor reset).
///
/// @plan PLAN-20260624-PR-MODE.P11
/// @requirement REQ-PR-006
/// @requirement REQ-PR-007
/// @pseudocode component-004 lines 127-137
pub(super) fn request_pr_list_reload(app_state: &mut AppStateHandle, ctx: &SharedContext) {
    dispatch_pr_list_fetch(app_state, ctx, true, false);
}

/// Request a silent background refresh of the PR list (issue #128). This is a
/// fresh reload that does NOT flash the loading spinner, preserves selection,
/// and is dispatched only when the PR view is open with no in-flight load.
///
/// @requirement issue #128
pub(super) fn request_pr_list_silent_refresh(app_state: &mut AppStateHandle, ctx: &SharedContext) {
    dispatch_pr_list_fetch(app_state, ctx, true, true);
}

/// @plan PLAN-20260624-PR-MODE.P11
/// @requirement REQ-PR-006
/// @pseudocode component-004 lines 130-138
#[derive(Clone)]
struct PrFetchParams {
    scope_repo_id: jefe::domain::RepositoryId,
    owner: String,
    repo: String,
    filter: jefe::domain::PrFilter,
    request_id: u64,
    cursor: Option<String>,
    fresh_reload: bool,
    /// When true, the result is delivered as a silent background refresh
    /// (`PrListSilentRefreshed`/`PrListSilentRefreshFailed`) that preserves
    /// selection/scroll and does NOT flash the loading spinner (issue #128).
    silent: bool,
}

/// Mark the PR list fetch as loading and return the monotonic request id.
///
/// @plan PLAN-20260624-PR-MODE.P11
/// @requirement REQ-PR-NFR-002
/// @pseudocode component-001 lines 209-223
fn mark_pr_list_fetch_loading(app_state: &mut AppStateHandle, params: &PrFetchParams) -> u64 {
    let mut state = app_state.write();
    let request_id = state.prs_state.next_pr_list_request_id.saturating_add(1);
    state.prs_state.next_pr_list_request_id = request_id;
    if params.silent && params.fresh_reload {
        state.mark_pr_list_silent_refresh_loading(
            params.scope_repo_id.clone(),
            params.filter.clone(),
            request_id,
        );
    } else if params.fresh_reload {
        state.mark_pr_list_reload_loading(
            params.scope_repo_id.clone(),
            params.filter.clone(),
            request_id,
        );
    } else {
        state.mark_pr_list_page_loading(
            params.scope_repo_id.clone(),
            params.filter.clone(),
            params.cursor.clone(),
            request_id,
        );
    }
    request_id
}

/// Gather the fetch params (repo slug, filter, cursor) from state.
///
/// @plan PLAN-20260624-PR-MODE.P11
/// @requirement REQ-PR-006
/// @pseudocode component-004 lines 127-137
fn pr_fetch_params(app_state: &AppStateHandle, fresh_reload: bool, silent: bool) -> PrFetchParams {
    let state = app_state.read();
    let gh_repo = prs_dispatch::resolve_pr_gh_repo(&state);
    PrFetchParams {
        scope_repo_id: prs_dispatch::current_pr_scope_repo_id(&state),
        owner: gh_repo.0,
        repo: gh_repo.1,
        filter: state.prs_state.committed_filter.clone(),
        request_id: 0,
        cursor: (!fresh_reload)
            .then(|| state.prs_state.list_cursor.clone())
            .flatten(),
        fresh_reload,
        silent,
    }
}

/// Persist a missing-GitHub-repo error (no spawn).
///
/// @plan PLAN-20260624-PR-MODE.P11
/// @requirement REQ-PR-006
/// @pseudocode component-004 lines 127-137
fn persist_missing_github_repo(app_state: &mut AppStateHandle, ctx: &SharedContext) {
    let mut state = app_state.write();
    state.prs_state.loading.list = false;
    state.prs_state.error = Some(
        "No GitHub repository configured. Set the GitHub Repo field (owner/repo) in repository settings."
            .to_string(),
    );
    let persisted = to_persisted_state(&state);
    drop(state);
    persist_state(ctx, &persisted);
}

/// Fetch the PR list via the gh client (runs on the background thread).
///
/// @plan PLAN-20260624-PR-MODE.P11
/// @requirement REQ-PR-006
/// @pseudocode component-004 lines 130-138
fn fetch_pr_list(
    ctx: &SharedContext,
    params: &PrFetchParams,
) -> Option<Result<jefe::github::PrListResponse, jefe::github::GhError>> {
    let client = github_client(ctx)?;
    Some(client.list_pull_requests(
        &params.owner,
        &params.repo,
        &params.filter,
        params.cursor.as_deref(),
    ))
}

/// Persist the fetch result (success or failure) through the reducer.
///
/// @plan PLAN-20260624-PR-MODE.P11
/// @requirement REQ-PR-006
/// @pseudocode component-004 lines 130-138
fn persist_pr_list_result(
    app_state: &mut AppStateHandle,
    ctx: &SharedContext,
    params: &PrFetchParams,
    result: Option<Result<jefe::github::PrListResponse, jefe::github::GhError>>,
) {
    match result {
        Some(Ok(response)) => persist_pr_list_loaded(app_state, ctx, params, response),
        Some(Err(error)) => persist_pr_list_failed(app_state, ctx, params, error.to_string()),
        None => persist_pr_list_failed(
            app_state,
            ctx,
            params,
            "Application context unavailable".to_string(),
        ),
    }
}

/// Apply + persist a successful PR list load, then preview the first PR.
///
/// @plan PLAN-20260624-PR-MODE.P11
/// @requirement REQ-PR-006
/// @pseudocode component-001 lines 209-223
fn persist_pr_list_loaded(
    app_state: &mut AppStateHandle,
    ctx: &SharedContext,
    params: &PrFetchParams,
    response: jefe::github::PrListResponse,
) {
    let has_prs = !response.pull_requests.is_empty();
    let mut state = app_state.write();
    // Silent refresh skips the preview logic (it must not disrupt selection).
    let should_preview = !params.silent
        && params.fresh_reload
        && has_prs
        && state
            .selected_repository_index
            .and_then(|idx| state.repositories.get(idx))
            .is_some_and(|repo| repo.id == params.scope_repo_id)
        && state
            .prs_state
            .list_reload_pending
            .as_ref()
            .is_some_and(|pending| {
                pending.scope_repo_id == params.scope_repo_id
                    && pending.filter == params.filter
                    && pending.request_id == params.request_id
            });
    let event = if params.silent {
        AppEvent::PrListSilentRefreshed {
            scope_repo_id: params.scope_repo_id.clone(),
            filter: std::boxed::Box::new(params.filter.clone()),
            request_id: params.request_id,
            pull_requests: response.pull_requests,
            cursor: response.cursor,
            has_more: response.has_more,
        }
    } else if params.fresh_reload {
        AppEvent::PrListLoaded {
            scope_repo_id: params.scope_repo_id.clone(),
            filter: std::boxed::Box::new(params.filter.clone()),
            request_id: params.request_id,
            pull_requests: response.pull_requests,
            cursor: response.cursor,
            has_more: response.has_more,
        }
    } else {
        AppEvent::PrListPageLoaded {
            scope_repo_id: params.scope_repo_id.clone(),
            request_id: params.request_id,
            pull_requests: response.pull_requests,
            cursor: response.cursor,
            has_more: response.has_more,
        }
    };
    *state = std::mem::take(&mut *state).apply(event);
    drop(state);
    if should_preview {
        prs_dispatch::preview_pr_from_list(app_state);
    }
    let state = app_state.read();
    let persisted = to_persisted_state(&state);
    drop(state);
    persist_state(ctx, &persisted);
}

/// Apply + persist a PR list load failure (scoped error, never silent).
///
/// @plan PLAN-20260624-PR-MODE.P11
/// @requirement REQ-PR-NFR-002
/// @pseudocode component-001 lines 242-247
fn persist_pr_list_failed(
    app_state: &mut AppStateHandle,
    ctx: &SharedContext,
    params: &PrFetchParams,
    error: String,
) {
    let event = if params.silent {
        AppEvent::PrListSilentRefreshFailed {
            scope_repo_id: params.scope_repo_id.clone(),
            request_id: params.request_id,
        }
    } else {
        AppEvent::PrListLoadFailed {
            scope_repo_id: params.scope_repo_id.clone(),
            request_id: params.request_id,
            error,
        }
    };
    apply_and_persist(app_state, ctx, event);
}

/// Load more PRs if the selection is at the end of the list.
///
/// @plan PLAN-20260624-PR-MODE.P11
/// @requirement REQ-PR-007
/// @pseudocode component-004 lines 127-137
pub(super) fn load_more_prs_if_at_end(app_state: &mut AppStateHandle, ctx: &SharedContext) {
    let should_load = {
        let state = app_state.read();
        let at_end = state
            .prs_state
            .selected_pr_index
            .is_some_and(|idx| idx + 1 >= state.prs_state.pull_requests.len());
        // Guard against a pending silent refresh (`loading.list` is false but
        // `list_reload_pending` is `Some`) so pagination does not clobber an
        // in-flight background refresh (issue #128).
        at_end
            && state.prs_state.has_more_prs
            && !state.prs_state.loading.list
            && state.prs_state.list_reload_pending.is_none()
    };
    if should_load {
        dispatch_pr_list_fetch(app_state, ctx, false, false);
    }
}

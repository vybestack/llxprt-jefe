//! Issues-mode issue-list reload/fetch dispatch helpers.
//!
//! Extracted from `mod.rs` to keep individual source files within the
//! project's length policy. These helpers own the background fetch of issue
//! list pages via the `gh` CLI and persist the resulting state transitions.

use jefe::messages::IssuesMessage;
use jefe::state::AppEvent;

use super::{
    AppStateHandle, SharedContext, apply_and_persist, gh_async, github_client, issues_dispatch,
    persist_state, to_persisted_state,
};

pub(super) fn load_more_issues_if_at_end(app_state: &mut AppStateHandle, ctx: &SharedContext) {
    let should_load = {
        let state = app_state.read();
        let at_end = state
            .issues_state
            .selected_issue_index
            .is_some_and(|idx| idx + 1 >= state.issues_state.issues.len());
        at_end && state.issues_state.has_more_issues && !state.issues_state.loading.list
    };
    if should_load {
        dispatch_issue_list_fetch(app_state, ctx, false);
    }
}

pub(super) fn dispatch_issue_list_reload(
    app_state: &mut AppStateHandle,
    ctx: &SharedContext,
    message: IssuesMessage,
) {
    let fresh_reload = is_fresh_issue_list_reload(&message);
    apply_and_persist(app_state, ctx, AppEvent::from(message));
    dispatch_issue_list_fetch(app_state, ctx, fresh_reload);
}

pub(super) fn is_fresh_issue_list_reload(message: &IssuesMessage) -> bool {
    matches!(
        message,
        IssuesMessage::EnterMode
            | IssuesMessage::RefocusList
            | IssuesMessage::ApplyFilter
            | IssuesMessage::ClearFilter
            | IssuesMessage::ApplySearch
    )
}

pub(super) fn dispatch_issue_list_fetch(
    app_state: &mut AppStateHandle,
    ctx: &SharedContext,
    fresh_reload: bool,
) {
    let mut params = issue_fetch_params(app_state, fresh_reload);

    if params.owner.is_empty() || params.repo.is_empty() {
        persist_missing_github_repo(app_state, ctx);
        return;
    }

    params.request_id = mark_issue_list_fetch_loading(app_state, &params);
    let panic_params = params.clone();
    gh_async::spawn_gh_task_with_panic(
        app_state,
        ctx,
        move |mut app_state, ctx| {
            let result = fetch_issue_list(&ctx, &params);
            persist_issue_list_result(&mut app_state, &ctx, &params, result);
        },
        move |mut app_state, ctx, message| {
            persist_issue_list_failed(
                &mut app_state,
                &ctx,
                &panic_params,
                format!("GitHub issue list task panicked: {message}"),
            );
        },
    );
}

#[derive(Clone)]
struct IssueFetchParams {
    scope_repo_id: jefe::domain::RepositoryId,
    owner: String,
    repo: String,
    filter: jefe::domain::IssueFilter,
    request_id: u64,
    cursor: Option<String>,
    page_size: u32,
    fresh_reload: bool,
}

fn mark_issue_list_fetch_loading(app_state: &mut AppStateHandle, params: &IssueFetchParams) -> u64 {
    let mut state = app_state.write();
    let request_id = state
        .issues_state
        .next_issue_list_request_id
        .saturating_add(1);
    state.issues_state.next_issue_list_request_id = request_id;
    if params.fresh_reload {
        state.mark_issue_list_reload_loading(
            params.scope_repo_id.clone(),
            params.filter.clone(),
            request_id,
        );
    } else {
        state.mark_issue_list_page_loading_with_request_id(
            params.scope_repo_id.clone(),
            params.filter.clone(),
            params.cursor.clone(),
            request_id,
        );
    }
    request_id
}

fn issue_fetch_params(app_state: &AppStateHandle, fresh_reload: bool) -> IssueFetchParams {
    let state = app_state.read();
    let gh_repo = issues_dispatch::resolve_gh_repo(&state);
    IssueFetchParams {
        scope_repo_id: issues_dispatch::current_scope_repo_id(&state),
        owner: gh_repo.0,
        repo: gh_repo.1,
        filter: state.issues_state.committed_filter.clone(),
        request_id: 0,
        cursor: (!fresh_reload)
            .then(|| state.issues_state.list_cursor.clone())
            .flatten(),
        page_size: 30,
        fresh_reload,
    }
}

fn persist_missing_github_repo(app_state: &mut AppStateHandle, ctx: &SharedContext) {
    let mut state = app_state.write();
    state.issues_state.loading.list = false;
    state.issues_state.error = Some(
        "No GitHub repository configured. Set the GitHub Repo field (owner/repo) in repository settings."
            .to_string(),
    );
    let persisted = to_persisted_state(&state);
    drop(state);
    persist_state(ctx, &persisted);
}

fn fetch_issue_list(
    ctx: &SharedContext,
    params: &IssueFetchParams,
) -> Option<Result<jefe::github::IssueListResponse, jefe::github::GhError>> {
    let client = github_client(ctx)?;
    Some(client.list_issues(
        &params.owner,
        &params.repo,
        &params.filter,
        params.cursor.as_deref(),
        params.page_size,
    ))
}

fn persist_issue_list_result(
    app_state: &mut AppStateHandle,
    ctx: &SharedContext,
    params: &IssueFetchParams,
    result: Option<Result<jefe::github::IssueListResponse, jefe::github::GhError>>,
) {
    match result {
        Some(Ok(response)) => persist_issue_list_loaded(app_state, ctx, params, response),
        Some(Err(error)) => persist_issue_list_failed(app_state, ctx, params, error.to_string()),
        None => persist_issue_list_failed(
            app_state,
            ctx,
            params,
            "Application context unavailable".to_string(),
        ),
    }
}

fn persist_issue_list_loaded(
    app_state: &mut AppStateHandle,
    ctx: &SharedContext,
    params: &IssueFetchParams,
    response: jefe::github::IssueListResponse,
) {
    let has_issues = !response.issues.is_empty();
    let mut state = app_state.write();
    let should_preview = params.fresh_reload
        && has_issues
        && state
            .selected_repository_index
            .and_then(|idx| state.repositories.get(idx))
            .is_some_and(|repo| repo.id == params.scope_repo_id)
        && state
            .issues_state
            .list_reload_pending
            .as_ref()
            .is_some_and(|pending| {
                pending.scope_repo_id == params.scope_repo_id
                    && pending.filter == params.filter
                    && pending.request_id == params.request_id
            });
    let event = if params.fresh_reload {
        AppEvent::IssueListLoaded {
            scope_repo_id: params.scope_repo_id.clone(),
            filter: std::boxed::Box::new(params.filter.clone()),
            request_id: params.request_id,
            issues: response.issues,
            cursor: response.cursor,
            has_more: response.has_more,
        }
    } else {
        AppEvent::IssueListPageLoaded {
            scope_repo_id: params.scope_repo_id.clone(),
            filter: std::boxed::Box::new(params.filter.clone()),
            request_id: params.request_id,
            request_cursor: params.cursor.clone(),
            issues: response.issues,
            cursor: response.cursor,
            has_more: response.has_more,
        }
    };
    *state = std::mem::take(&mut *state).apply(event);
    drop(state);
    if should_preview {
        issues_dispatch::preview_issue_from_list(app_state);
    }
    let state = app_state.read();
    let persisted = to_persisted_state(&state);
    drop(state);
    persist_state(ctx, &persisted);
}

fn persist_issue_list_failed(
    app_state: &mut AppStateHandle,
    ctx: &SharedContext,
    params: &IssueFetchParams,
    error: String,
) {
    // When gh is not authenticated, open the in-app device-code auth dialog
    // instead of surfacing a bare "run gh auth login" error string (issue #244).
    // The dialog is the remediation surface; the original operation can be
    // retried after success.
    if super::auth_remediation::offer_auth_remediation(app_state, ctx, &error) {
        return;
    }
    apply_and_persist(
        app_state,
        ctx,
        AppEvent::IssueListLoadFailed {
            scope_repo_id: params.scope_repo_id.clone(),
            filter: std::boxed::Box::new(params.filter.clone()),
            request_id: params.request_id,
            request_cursor: params.cursor.clone(),
            error,
        },
    );
}

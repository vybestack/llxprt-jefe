//! Issues-mode dispatch helpers.
//!
//! Extracted from mod.rs to keep file sizes manageable.

use jefe::domain::PageToken;
use jefe::messages::IssuesMessage;
use jefe::state::AppEvent;

use super::{
    AppStateHandle, SharedContext, apply_and_persist, dispatch_issues_lifecycle,
    dispatch_issues_navigation, gh_async, github_client,
};
use super::{issues_list_dispatch, issues_mutation, issues_send, issues_subfocus_dispatch};

/// Resolve the GitHub owner/repo for the currently selected repository.
/// Reads from the explicit `github_repo` field (format: `"owner/repo"`).
///
/// @plan PLAN-20260329-ISSUES-MODE.P15
/// @requirement REQ-ISS-013
pub(super) fn resolve_gh_repo(state: &jefe::state::AppState) -> (String, String) {
    let repo = state
        .selected_repository_index
        .and_then(|idx| state.repositories.get(idx));

    let Some(repo) = repo else {
        return (String::new(), String::new());
    };

    let gh = repo.github_repo.trim();
    if gh.is_empty() {
        return (String::new(), String::new());
    }

    let mut parts = gh.split('/');
    let owner = parts.next().map(str::trim).unwrap_or_default();
    let name = parts.next().map(str::trim).unwrap_or_default();
    if parts.next().is_none() && !owner.is_empty() && !name.is_empty() {
        return (owner.to_owned(), name.to_owned());
    }

    (String::new(), String::new())
}

pub(super) fn current_scope_repo_id(state: &jefe::state::AppState) -> jefe::domain::RepositoryId {
    state
        .selected_repository_index
        .and_then(|idx| state.repositories.get(idx))
        .map_or_else(
            || jefe::domain::RepositoryId(String::new()),
            |r| r.id.clone(),
        )
}

/// Build a lightweight issue detail preview from list data (no I/O).
/// Used for instant preview while arrowing through the issue list.
pub(super) fn preview_issue_from_list(app_state: &mut AppStateHandle) {
    let preview = {
        let state = app_state.read();
        state
            .issues_state
            .selected_issue_index()
            .and_then(|idx| state.issues_state.issues().get(idx))
            .map(|issue| {
                let gh_repo = resolve_gh_repo(&state);
                jefe::domain::IssueDetail {
                    repo_owner_name: format!("{}/{}", gh_repo.0, gh_repo.1),
                    number: issue.number,
                    node_id: issue.node_id.clone(),
                    title: issue.title.clone(),
                    state: issue.state,
                    author_login: issue.author_login.clone(),
                    created_at: String::new(),
                    updated_at: issue.updated_at.clone(),
                    labels: issue.labels.clone(),
                    assignees: issue.assignees.clone(),
                    milestone: None,
                    body: preview_body_from_list(&issue.body),
                    external_url: String::new(),
                    comments: jefe::domain::PaginatedList::from_loaded(
                        jefe::domain::CommentDetailIdentity {
                            scope_repo_id: current_scope_repo_id(&state),
                            number: issue.number,
                        },
                        Vec::new(),
                        PageToken::Done,
                    ),
                }
            })
    };

    if let Some(detail) = preview {
        let mut state = app_state.write();
        if let Some(previous_detail) = &mut state.issues_state.issue_detail {
            previous_detail.comments.cancel_pending();
        }
        state.issues_state.issue_detail = Some(detail);
        state.issues_state.loading.detail = false;
        state.issues_state.loading.comments = false;
        state.issues_state.detail_pending = None;
        state.issues_state.detail_subfocus = jefe::state::DetailSubfocus::Body;
        state.issues_state.detail_scroll_offset = 0;
    }
}

/// Body text for instant issue previews built from lightweight list rows.
/// @plan PLAN-20260630-ISSUES-REGRESSION.P01
/// @requirement REQ-ISS-006
/// @pseudocode component-004 lines 1-5
fn preview_body_from_list(body: &str) -> String {
    if body.is_empty() {
        "Press Enter to load issue body.".to_string()
    } else {
        body.to_string()
    }
}

/// Load issue detail for the currently selected issue in the list.
/// Used by IssuesEnter to get the full detail with comments.
pub(super) fn load_issue_detail_for_selection(app_state: &mut AppStateHandle, ctx: &SharedContext) {
    let Some(mut params) = detail_load_params(app_state) else {
        return;
    };
    mark_detail_loading(app_state, &mut params);
    if params.owner.is_empty() || params.repo.is_empty() {
        apply_and_persist(app_state, ctx, missing_detail_repo_event(&params));
        return;
    }

    let panic_params = params.clone();
    gh_async::spawn_gh_task_with_panic(
        app_state,
        ctx,
        move |mut app_state, ctx| {
            let event = detail_load_event(&ctx, params);
            // Offer the in-app auth dialog when gh is unauthenticated (issue #244).
            if let AppEvent::IssueDetailLoadFailed { error, .. } = &event
                && super::auth_remediation::offer_auth_remediation(&mut app_state, &ctx, error)
            {
                return;
            }
            apply_and_persist(&mut app_state, &ctx, event);
        },
        move |mut app_state, ctx, message| {
            apply_and_persist(
                &mut app_state,
                &ctx,
                detail_load_panic_event(&panic_params, message),
            );
        },
    );
}

fn detail_load_params(app_state: &AppStateHandle) -> Option<DetailLoadParams> {
    let state = app_state.read();
    let issue_number = state
        .issues_state
        .selected_issue_index()
        .and_then(|idx| state.issues_state.issues().get(idx))
        .map(|issue| issue.number)?;
    let (owner, repo) = resolve_gh_repo(&state);
    let params = DetailLoadParams {
        scope_repo_id: current_scope_repo_id(&state),
        issue_number,
        owner,
        repo,
        request_id: 0,
    };
    drop(state);
    Some(params)
}

fn mark_detail_loading(app_state: &mut AppStateHandle, params: &mut DetailLoadParams) {
    let mut state = app_state.write();
    let request_id = state.next_issue_detail_request_id();
    state.mark_issue_detail_loading_with_request_id(
        params.scope_repo_id.clone(),
        params.issue_number,
        request_id,
    );
    drop(state);
    params.request_id = request_id;
}

fn detail_load_event(ctx: &SharedContext, params: DetailLoadParams) -> AppEvent {
    let result = github_client(ctx)
        .map(|client| client.get_issue_detail(&params.owner, &params.repo, params.issue_number));
    match result {
        Some(Ok(detail)) => AppEvent::IssueDetailLoaded {
            scope_repo_id: params.scope_repo_id,
            issue_number: params.issue_number,
            request_id: params.request_id,
            detail: std::boxed::Box::new(detail),
        },
        Some(Err(error)) => AppEvent::IssueDetailLoadFailed {
            scope_repo_id: params.scope_repo_id,
            issue_number: params.issue_number,
            request_id: params.request_id,
            error: error.to_string(),
        },
        None => AppEvent::IssueDetailLoadFailed {
            scope_repo_id: params.scope_repo_id,
            issue_number: params.issue_number,
            request_id: params.request_id,
            error: "Application context unavailable".to_string(),
        },
    }
}

fn missing_detail_repo_event(params: &DetailLoadParams) -> AppEvent {
    AppEvent::IssueDetailLoadFailed {
        scope_repo_id: params.scope_repo_id.clone(),
        issue_number: params.issue_number,
        request_id: params.request_id,
        error: "No GitHub repository configured. Set the GitHub Repo field (owner/repo) in repository settings.".to_string(),
    }
}

fn detail_load_panic_event(params: &DetailLoadParams, message: String) -> AppEvent {
    AppEvent::IssueDetailLoadFailed {
        scope_repo_id: params.scope_repo_id.clone(),
        issue_number: params.issue_number,
        request_id: params.request_id,
        error: format!("GitHub issue detail task panicked: {message}"),
    }
}

#[derive(Clone)]
struct DetailLoadParams {
    scope_repo_id: jefe::domain::RepositoryId,
    issue_number: u64,
    owner: String,
    repo: String,
    request_id: u64,
}

/// Load the next comments page when the detail view is scrolled to the bottom.
pub(super) fn load_more_comments(app_state: &mut AppStateHandle, ctx: &SharedContext) {
    let mut params = match comment_page_params(app_state) {
        CommentPageRequest::Ready(params) => params,
        CommentPageRequest::Fail(event) => {
            if let Some(event) = mark_comment_failure_pending(app_state, event) {
                apply_and_persist(app_state, ctx, event);
            }
            return;
        }
        CommentPageRequest::Skip => return,
    };

    let request_id = {
        let mut state = app_state.write();
        state.begin_issue_comment_page(
            &params.scope_repo_id,
            params.issue_number,
            params.cursor.clone(),
        )
    };
    let Some(request_id) = request_id else {
        return;
    };
    params.request_id = request_id;

    let panic_params = params.clone();
    gh_async::spawn_gh_task_with_panic(
        app_state,
        ctx,
        move |mut app_state, ctx| {
            let event = comment_page_event(&ctx, &params);
            apply_and_persist(&mut app_state, &ctx, event);
        },
        move |mut app_state, ctx, message| {
            apply_and_persist(
                &mut app_state,
                &ctx,
                AppEvent::IssueCommentsPageFailed {
                    scope_repo_id: panic_params.scope_repo_id,
                    issue_number: panic_params.issue_number,
                    request_id: panic_params.request_id,
                    request_cursor: panic_params.cursor,
                    error: format!("GitHub comments task panicked: {message}"),
                },
            );
        },
    );
}

fn mark_comment_failure_pending(
    app_state: &mut AppStateHandle,
    event: AppEvent,
) -> Option<AppEvent> {
    let AppEvent::IssueCommentsPageFailed {
        scope_repo_id,
        issue_number,
        request_cursor,
        error,
        ..
    } = event
    else {
        return None;
    };
    let request_id = app_state.write().begin_issue_comment_page(
        &scope_repo_id,
        issue_number,
        request_cursor.clone(),
    )?;
    Some(AppEvent::IssueCommentsPageFailed {
        scope_repo_id,
        issue_number,
        request_id,
        request_cursor,
        error,
    })
}

/// Return the GraphQL cursor for issue comments.
///
/// Comment pagination is cursor-only. `PageNumber` is a REST-list token and is
/// intentionally rejected here rather than translated into unrelated behavior.
fn issue_comment_cursor(token: &PageToken) -> Option<String> {
    match token {
        PageToken::Cursor(cursor) => Some(cursor.clone()),
        PageToken::PageNumber(_) | PageToken::Done => None,
    }
}

fn comment_page_params(app_state: &AppStateHandle) -> CommentPageRequest {
    let state = app_state.read();
    let Some(detail) = state.issues_state.issue_detail.as_ref() else {
        return CommentPageRequest::Skip;
    };
    if !detail.comments.has_more() || state.issues_state.loading.comments {
        return CommentPageRequest::Skip;
    }
    if state.issues_state.detail_scroll_offset < state.issues_state.max_detail_scroll_offset() {
        return CommentPageRequest::Skip;
    }
    let scope_repo_id = current_scope_repo_id(&state);
    let issue_number = detail.number;
    let (owner, repo) = resolve_gh_repo(&state);
    if owner.is_empty() || repo.is_empty() {
        return CommentPageRequest::Fail(AppEvent::IssueCommentsPageFailed {
            scope_repo_id,
            issue_number,
            request_id: 0,
            request_cursor: issue_comment_cursor(detail.comments.next_page()),
            error: "No GitHub repository configured. Set the GitHub Repo field (owner/repo) in repository settings.".to_string(),
        });
    }
    let params = CommentPageParams {
        scope_repo_id,
        issue_number,
        owner,
        repo,
        cursor: issue_comment_cursor(detail.comments.next_page()),
        page_size: 30,
        request_id: 0,
    };
    drop(state);
    CommentPageRequest::Ready(params)
}

fn comment_page_event(ctx: &SharedContext, params: &CommentPageParams) -> AppEvent {
    let result = github_client(ctx).map(|client| {
        client.list_comments(
            &params.owner,
            &params.repo,
            params.issue_number,
            params.cursor.as_deref(),
            params.page_size,
        )
    });

    match result {
        Some(Ok(response)) => AppEvent::IssueCommentsPageLoaded {
            scope_repo_id: params.scope_repo_id.clone(),
            issue_number: params.issue_number,
            request_id: params.request_id,
            request_cursor: params.cursor.clone(),
            comments: response.comments,
            cursor: response.cursor,
            has_more: response.has_more,
        },
        Some(Err(error)) => AppEvent::IssueCommentsPageFailed {
            scope_repo_id: params.scope_repo_id.clone(),
            issue_number: params.issue_number,
            request_id: params.request_id,
            request_cursor: params.cursor.clone(),
            error: error.to_string(),
        },
        None => AppEvent::IssueCommentsPageFailed {
            scope_repo_id: params.scope_repo_id.clone(),
            issue_number: params.issue_number,
            request_id: params.request_id,
            request_cursor: params.cursor.clone(),
            error: "Application context unavailable".to_string(),
        },
    }
}

#[derive(Clone)]
struct CommentPageParams {
    scope_repo_id: jefe::domain::RepositoryId,
    issue_number: u64,
    owner: String,
    repo: String,
    cursor: Option<String>,
    page_size: u32,
    request_id: u64,
}

enum CommentPageRequest {
    Ready(CommentPageParams),
    Fail(AppEvent),
    Skip,
}

/// Format a `SendPayload` into a markdown issue prompt for the agent.
pub(super) fn format_issue_prompt(payload: &jefe::github::SendPayload) -> String {
    use std::fmt::Write;
    let mut out = String::new();
    let _ = writeln!(
        out,
        "# GitHub Issue #{}: {}",
        payload.issue_number, payload.issue_title
    );
    let _ = writeln!(out);
    let _ = writeln!(out, "**Repository:** {}", payload.repository);
    let _ = writeln!(out, "**State:** {}", payload.issue_state);
    if !payload.issue_labels.is_empty() {
        let _ = writeln!(out, "**Labels:** {}", payload.issue_labels.join(", "));
    }
    if !payload.issue_assignees.is_empty() {
        let _ = writeln!(out, "**Assignees:** {}", payload.issue_assignees.join(", "));
    }
    let _ = writeln!(out);
    let _ = writeln!(out, "## Body");
    let _ = writeln!(out);
    let _ = writeln!(out, "{}", payload.issue_body);

    if let Some(comment) = &payload.focused_comment {
        let _ = writeln!(out);
        if let Some(author) = &payload.focused_comment_author {
            let _ = writeln!(out, "## Focused Comment (by @{author})");
        } else {
            let _ = writeln!(out, "## Focused Comment");
        }
        let _ = writeln!(out);
        let _ = writeln!(out, "{comment}");
    }

    if !payload.issue_base_prompt.is_empty() {
        let _ = writeln!(out);
        let _ = writeln!(out, "## Instructions");
        let _ = writeln!(out);
        let _ = writeln!(out, "{}", payload.issue_base_prompt);
    }

    out
}

/// Dispatch Issues-mode messages that need orchestration beyond a plain reducer
/// event (navigation reloads, detail load, send-to-agent, inline submit, and
/// the close/delete lifecycle). Plain issues messages fall through to the
/// generic `AppEvent::from` arm in `dispatch_app_message`.
pub(super) fn dispatch_issues_message(
    app_state: &mut AppStateHandle,
    ctx: &SharedContext,
    message: IssuesMessage,
) {
    match message {
        message @ (IssuesMessage::NavigateUp
        | IssuesMessage::NavigateDown
        | IssuesMessage::NavigatePageUp
        | IssuesMessage::NavigatePageDown
        | IssuesMessage::NavigateHome
        | IssuesMessage::NavigateEnd) => {
            dispatch_issues_navigation(app_state, ctx, message);
        }
        message @ (IssuesMessage::EnterMode
        | IssuesMessage::RefocusList
        | IssuesMessage::ApplyFilter
        | IssuesMessage::ClearFilter
        | IssuesMessage::ApplySearch) => {
            issues_list_dispatch::dispatch_issue_list_reload(app_state, ctx, message);
        }
        IssuesMessage::Enter => {
            apply_and_persist(app_state, ctx, AppEvent::IssuesEnter);
            load_issue_detail_for_selection(app_state, ctx);
        }
        message @ (IssuesMessage::ScrollDetailDown
        | IssuesMessage::ScrollDetailPageDown
        | IssuesMessage::DetailSubfocusNext
        | IssuesMessage::DetailSubfocusPrev) => {
            issues_subfocus_dispatch::dispatch_issues_detail_scroll_or_subfocus(
                app_state, ctx, message,
            );
        }
        IssuesMessage::AgentChooserConfirm => {
            issues_send::dispatch_agent_chooser_confirm(app_state, ctx);
        }
        IssuesMessage::InlineSubmit => {
            issues_mutation::handle_inline_submit(app_state, ctx);
        }
        message @ (IssuesMessage::CloseIssue
        | IssuesMessage::OpenDeleteIssueConfirm
        | IssuesMessage::IssueDeleteConfirm
        | IssuesMessage::IssueDeleteCancel
        | IssuesMessage::OpenCloseReasonChooser
        | IssuesMessage::CloseReasonNavigateUp
        | IssuesMessage::CloseReasonNavigateDown
        | IssuesMessage::CloseReasonSelect
        | IssuesMessage::CloseReasonDuplicateSearchChar(_)
        | IssuesMessage::CloseReasonDuplicateSearchBackspace
        | IssuesMessage::CloseReasonDuplicateSearchNavigateUp
        | IssuesMessage::CloseReasonDuplicateSearchNavigateDown
        | IssuesMessage::CloseReasonCancel
        | IssuesMessage::CloseReasonConfirm) => {
            dispatch_issues_lifecycle(app_state, ctx, message);
        }
        message => apply_and_persist(app_state, ctx, AppEvent::from(message)),
    }
}

#[cfg(test)]
mod tests {
    use super::{issue_comment_cursor, preview_body_from_list};
    use jefe::domain::PageToken;

    #[test]
    fn issue_comment_cursor_rejects_rest_page_tokens() {
        assert_eq!(issue_comment_cursor(&PageToken::PageNumber(2)), None);
    }

    #[test]
    fn issue_comment_cursor_extracts_graphql_cursor() {
        assert_eq!(
            issue_comment_cursor(&PageToken::Cursor("next".to_string())),
            Some("next".to_string())
        );
    }

    #[test]
    fn empty_list_preview_body_prompts_for_detail_load() {
        assert_eq!(
            preview_body_from_list(""),
            "Press Enter to load issue body."
        );
    }

    #[test]
    fn populated_list_preview_body_is_preserved() {
        assert_eq!(preview_body_from_list("existing body"), "existing body");
    }
}

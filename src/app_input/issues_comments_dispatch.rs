//! Issue comment pagination dispatch.
//!
//! Extracted from `issues_dispatch.rs` to keep that handler module within the
//! architecture boundary line limit. Owns the "load the next comments page"
//! route end to end: eligibility, request-id allocation, the blocking `gh`
//! call, and the typed success/failure events.
//!
//! The blocking work runs through `gh_async::spawn_gh_work`, so it never
//! touches iocraft state; results are applied on the render thread.

use jefe::domain::PageToken;
use jefe::state::AppEvent;

use super::issues_dispatch::{MISSING_DETAIL_REPO_MSG, current_scope_repo_id};
use super::{AppStateHandle, SharedContext, apply_and_persist, gh_async, github_client};

/// Comments requested per page.
///
/// Matches the issue-list page size so detail pagination advances at the same
/// rate the list does.
const COMMENT_PAGE_SIZE: u32 = 30;

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
    let Some(deliveries) =
        gh_async::delivery_handle_or_report(app_state, ctx, comment_page_abandoned(params.clone()))
    else {
        return;
    };
    gh_async::spawn_gh_work(
        &deliveries,
        ctx,
        move |ctx| comment_page_event(ctx, &params),
        apply_and_persist,
        comment_page_abandoned(panic_params),
    );
}

/// Report an abandoned comments page so the pending marker is cleared.
fn comment_page_abandoned(
    params: CommentPageParams,
) -> impl FnOnce(&mut AppStateHandle, &SharedContext, String) {
    move |app_state, ctx, message| {
        apply_and_persist(
            app_state,
            ctx,
            AppEvent::IssueCommentsPageFailed {
                scope_repo_id: params.scope_repo_id,
                issue_number: params.issue_number,
                request_id: params.request_id,
                request_cursor: params.cursor,
                error: format!("GitHub comments request abandoned: {message}"),
            },
        );
    }
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
    let requested_cursor = issue_comment_cursor(detail.comments.next_page());
    let tracker = match jefe::domain::GitHubRepoRef::parse(&detail.repo_owner_name) {
        Ok(Some(tracker)) => tracker,
        Ok(None) => {
            return CommentPageRequest::Fail(AppEvent::IssueCommentsPageFailed {
                scope_repo_id,
                issue_number,
                request_id: 0,
                request_cursor: requested_cursor,
                error: MISSING_DETAIL_REPO_MSG.to_owned(),
            });
        }
        Err(error) => {
            return CommentPageRequest::Fail(AppEvent::IssueCommentsPageFailed {
                scope_repo_id,
                issue_number,
                request_id: 0,
                request_cursor: requested_cursor,
                error: error.to_string(),
            });
        }
    };
    let params = CommentPageParams {
        scope_repo_id,
        issue_number,
        owner: tracker.owner().to_owned(),
        repo: tracker.repo().to_owned(),
        cursor: requested_cursor,
        page_size: COMMENT_PAGE_SIZE,
        request_id: 0,
    };
    // Release the read guard before returning: the caller immediately takes a
    // write guard, and `significant_drop_tightening` requires the explicit drop.
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

#[cfg(test)]
mod tests {
    use super::issue_comment_cursor;
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
}

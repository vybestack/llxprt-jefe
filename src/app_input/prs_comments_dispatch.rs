//! PR-mode comments-page loading dispatch.
//!
//! Extracted from `prs_dispatch.rs` to keep handler modules under the
//! architecture per-file line limit. All `gh` I/O runs off the UI thread via
//! `spawn_gh_task_with_panic`.
//!
//! @plan PLAN-20260624-PR-MODE.P11
//! @requirement REQ-PR-010
//! @pseudocode component-004 lines 146-155

use jefe::domain::RepositoryId;
use jefe::state::AppEvent;

use super::prs_dispatch::{current_pr_scope_repo_id, resolve_pr_gh_repo};
use super::{AppStateHandle, SharedContext, apply_and_persist, gh_async, github_client};

// ── PR comments page loading ──────────────────────────────────────────────

/// Load the next comments page when the detail view is scrolled to the bottom.
///
/// @plan PLAN-20260624-PR-MODE.P11
/// @requirement REQ-PR-010
/// @pseudocode component-004 lines 147-155
pub(super) fn load_more_pr_comments(app_state: &mut AppStateHandle, ctx: &SharedContext) {
    let mut params = match pr_comment_page_params(app_state) {
        PrCommentPageRequest::Ready(params) => params,
        PrCommentPageRequest::Fail(event) => {
            mark_pr_comment_failure_pending(app_state, &event);
            apply_and_persist(app_state, ctx, event);
            return;
        }
        PrCommentPageRequest::Skip => return,
    };

    {
        let mut state = app_state.write();
        let request_id = state
            .prs_state
            .next_comments_page_request_id
            .saturating_add(1);
        state.prs_state.next_comments_page_request_id = request_id;
        state.prs_state.loading.comments = true;
        state.prs_state.comments_page_pending = Some(jefe::state::PrCommentsPagePending {
            scope_repo_id: params.scope_repo_id.clone(),
            pr_number: params.pr_number,
            cursor: params.cursor.clone(),
            request_id,
        });
        drop(state);
        params.request_id = request_id;
    }

    let panic_params = params.clone();
    gh_async::spawn_gh_task_with_panic(
        app_state,
        ctx,
        move |mut app_state, ctx| {
            let event = pr_comment_page_event(&ctx, &params);
            apply_and_persist(&mut app_state, &ctx, event);
        },
        move |mut app_state, ctx, message| {
            apply_and_persist(
                &mut app_state,
                &ctx,
                AppEvent::PrCommentsPageFailed {
                    scope_repo_id: panic_params.scope_repo_id,
                    pr_number: panic_params.pr_number,
                    request_id: panic_params.request_id,
                    error: format!("GitHub PR comments task panicked: {message}"),
                },
            );
        },
    );
}

/// @plan PLAN-20260624-PR-MODE.P11
/// @requirement REQ-PR-010
/// @pseudocode component-004 lines 146-155
#[derive(Clone)]
struct PrCommentPageParams {
    scope_repo_id: RepositoryId,
    pr_number: u64,
    owner: String,
    repo: String,
    cursor: Option<String>,
    request_id: u64,
}

/// @plan PLAN-20260624-PR-MODE.P11
/// @requirement REQ-PR-010
/// @pseudocode component-004 lines 146-155
enum PrCommentPageRequest {
    Ready(PrCommentPageParams),
    Fail(AppEvent),
    Skip,
}

/// Estimate rendered detail content lines (mirrors `prs_inline_ops` heuristic).
/// @plan PLAN-20260624-PR-MODE.P11
/// @requirement REQ-PR-009
/// @pseudocode component-004 lines 146-155
fn pr_rendered_detail_lines(detail: &jefe::domain::PullRequestDetail) -> usize {
    let body_lines = detail.body.lines().count().max(1);
    let comment_lines: usize = detail
        .comments
        .iter()
        .map(|c| c.body.lines().count().max(1) + 2)
        .sum();
    let header_lines = 5;
    header_lines + body_lines + comment_lines
}

/// Resolve comment-page params or a Skip/Fail outcome from state.
/// @plan PLAN-20260624-PR-MODE.P11
/// @requirement REQ-PR-010
/// @pseudocode component-004 lines 146-155
fn pr_comment_page_params(app_state: &AppStateHandle) -> PrCommentPageRequest {
    let state = app_state.read();
    let Some(detail) = state.prs_state.pr_detail.as_ref() else {
        return PrCommentPageRequest::Skip;
    };
    if !detail.has_more_comments || state.prs_state.loading.comments {
        return PrCommentPageRequest::Skip;
    }
    // Only load more comments if scrolled near the bottom. We approximate the
    // max scroll offset from the stored viewport rows and rendered line count.
    let rendered_lines = pr_rendered_detail_lines(detail);
    let max_offset = rendered_lines.saturating_sub(state.prs_state.detail_viewport_rows);
    if state.prs_state.detail_scroll_offset < max_offset {
        return PrCommentPageRequest::Skip;
    }
    let scope_repo_id = current_pr_scope_repo_id(&state);
    let pr_number = detail.number;
    let (owner, repo) = resolve_pr_gh_repo(&state);
    if owner.is_empty() || repo.is_empty() {
        return PrCommentPageRequest::Fail(AppEvent::PrCommentsPageFailed {
            scope_repo_id,
            pr_number,
            request_id: 0,
            error: "No GitHub repository configured. Set the GitHub Repo field (owner/repo) in repository settings.".to_string(),
        });
    }
    let params = PrCommentPageParams {
        scope_repo_id,
        pr_number,
        owner,
        repo,
        cursor: detail.comments_cursor.clone(),
        request_id: 0,
    };
    drop(state);
    PrCommentPageRequest::Ready(params)
}

/// Mark the comment-page failure as pending so the reducer clears loading.
/// @plan PLAN-20260624-PR-MODE.P11
/// @requirement REQ-PR-010
/// @pseudocode component-004 lines 146-155
fn mark_pr_comment_failure_pending(app_state: &mut AppStateHandle, event: &AppEvent) {
    if let AppEvent::PrCommentsPageFailed {
        scope_repo_id,
        pr_number,
        ..
    } = event
    {
        let mut state = app_state.write();
        state.prs_state.loading.comments = true;
        state.prs_state.comments_page_pending = Some(jefe::state::PrCommentsPagePending {
            scope_repo_id: scope_repo_id.clone(),
            pr_number: *pr_number,
            cursor: None,
            request_id: 0,
        });
    }
}

/// Build the comments-page-loaded/failed event from the gh result.
/// @plan PLAN-20260624-PR-MODE.P11
/// @requirement REQ-PR-010
/// @pseudocode component-004 lines 146-155
fn pr_comment_page_event(ctx: &SharedContext, params: &PrCommentPageParams) -> AppEvent {
    let result = github_client(ctx).map(|client| {
        client.list_pr_comments(
            &params.owner,
            &params.repo,
            params.pr_number,
            params.cursor.as_deref(),
            30,
        )
    });
    match result {
        Some(Ok(response)) => AppEvent::PrCommentsPageLoaded {
            scope_repo_id: params.scope_repo_id.clone(),
            pr_number: params.pr_number,
            request_id: params.request_id,
            comments: response.comments,
            cursor: response.cursor,
            has_more: response.has_more,
        },
        Some(Err(error)) => AppEvent::PrCommentsPageFailed {
            scope_repo_id: params.scope_repo_id.clone(),
            pr_number: params.pr_number,
            request_id: params.request_id,
            error: error.to_string(),
        },
        None => AppEvent::PrCommentsPageFailed {
            scope_repo_id: params.scope_repo_id.clone(),
            pr_number: params.pr_number,
            request_id: params.request_id,
            error: "Application context unavailable".to_string(),
        },
    }
}

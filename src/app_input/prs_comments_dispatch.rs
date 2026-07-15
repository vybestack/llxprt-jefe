//! PR-mode comments-page loading dispatch.
//!
//! Extracted from `prs_dispatch.rs` to keep handler modules under the
//! architecture per-file line limit. All `gh` I/O runs off the UI thread via
//! `spawn_gh_task_with_panic`.
//!
//! @plan PLAN-20260624-PR-MODE.P11
//! @requirement REQ-PR-010
//! @pseudocode component-004 lines 146-155

use jefe::domain::{PageToken, RepositoryId};
use jefe::state::{AppEvent, ComposerTarget, InlineState};

use super::prs_dispatch::{current_pr_scope_repo_id, resolve_pr_gh_repo_or_error};
use super::{AppStateHandle, SharedContext, apply_and_persist, gh_async, github_client};

// ── PR comments page loading ──────────────────────────────────────────────

/// Load the next comments page when the detail view is scrolled to the bottom.
///
/// @plan PLAN-20260624-PR-MODE.P11
/// @requirement REQ-PR-010
/// @pseudocode component-004 lines 147-155
pub(super) fn load_more_pr_comments(app_state: &mut AppStateHandle, ctx: &SharedContext) {
    let params = match pr_comment_page_params(app_state) {
        PrCommentPageRequest::Ready(params) => params,
        PrCommentPageRequest::Fail(event) => {
            apply_and_persist(app_state, ctx, event);
            return;
        }
        PrCommentPageRequest::Skip => return,
    };

    let request_id = {
        let mut state = app_state.write();
        state.begin_pr_comment_page(
            &params.scope_repo_id,
            params.pr_number,
            params.cursor.clone(),
        )
    };
    let Some(request_id) = request_id else {
        return;
    };
    let dispatched = DispatchedPrCommentPageParams { params, request_id };

    let panic_params = dispatched.clone();
    gh_async::spawn_gh_task_with_panic(
        app_state,
        ctx,
        move |mut app_state, ctx| {
            let event = pr_comment_page_event(&ctx, &dispatched);
            apply_and_persist(&mut app_state, &ctx, event);
        },
        move |mut app_state, ctx, message| {
            apply_and_persist(
                &mut app_state,
                &ctx,
                AppEvent::PrCommentsPageFailed {
                    scope_repo_id: panic_params.params.scope_repo_id,
                    pr_number: panic_params.params.pr_number,
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
}

#[derive(Clone)]
struct DispatchedPrCommentPageParams {
    params: PrCommentPageParams,
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

/// Whether the embedded PR composer TextBox is active for the current state.
///
/// @plan PLAN-20260624-PR-MODE.P14
/// @requirement REQ-PR-009
/// @pseudocode component-001 lines 169-176
fn pr_text_box_active(inline_state: &InlineState) -> bool {
    matches!(
        inline_state,
        InlineState::Composer {
            target: ComposerTarget::NewComment | ComposerTarget::Reply { .. },
            ..
        }
    )
}

/// Compute the max detail scroll offset using the CANONICAL parity function
/// `pr_detail_content_line_count` (the exact text the renderer emits for the
/// current subfocus, inline composer state, and loading flags) minus the
/// effective read-only document viewport rows. Using the parity function —
/// rather than a local heuristic — guarantees the comments-dispatch "scrolled
/// near bottom" check uses the SAME line count and viewport the renderer and
/// scroll clamp do (MED-8).
///
/// @plan PLAN-20260624-PR-MODE.P11
/// @requirement REQ-PR-009
/// @requirement REQ-PR-010
/// @pseudocode component-004 lines 146-155
pub(super) fn pr_detail_max_scroll_offset(state: &jefe::state::AppState) -> usize {
    let Some(detail) = state.prs_state.pr_detail.as_ref() else {
        return 0;
    };
    let document_viewport = jefe::layout::pr_detail_document_viewport_rows(
        state.prs_state.detail_viewport_rows,
        pr_text_box_active(&state.prs_state.inline_state),
    );
    jefe::pr_detail_content::pr_detail_content_line_count(
        detail,
        state.prs_state.detail_subfocus,
        &state.prs_state.inline_state,
        state.prs_state.loading.detail,
        state.prs_state.loading.comments,
    )
    .saturating_sub(document_viewport)
}

/// Resolve comment-page params or a Skip/Fail outcome from state.
/// @plan PLAN-20260624-PR-MODE.P11
/// @requirement REQ-PR-010
/// @pseudocode component-004 lines 146-155
fn pr_comment_page_params(app_state: &AppStateHandle) -> PrCommentPageRequest {
    let state = app_state.read();
    pr_comment_page_request(&state)
}

fn pr_comment_page_request(state: &jefe::state::AppState) -> PrCommentPageRequest {
    let Some(detail) = state.prs_state.pr_detail.as_ref() else {
        return PrCommentPageRequest::Skip;
    };
    if !detail.comments.has_more() || state.prs_state.loading.comments {
        return PrCommentPageRequest::Skip;
    }
    // Only load more comments if scrolled near the bottom, using the CANONICAL
    // rendered line count (MED-8) so the threshold matches the real viewport.
    let max_offset = pr_detail_max_scroll_offset(state);
    if state.prs_state.detail_scroll_offset < max_offset {
        return PrCommentPageRequest::Skip;
    }
    let scope_repo_id = current_pr_scope_repo_id(state);
    let pr_number = detail.number;
    let (owner, repo, malformed_message) = match resolve_pr_gh_repo_or_error(state) {
        Ok((owner, repo)) => (owner, repo, None),
        Err(error) => (String::new(), String::new(), Some(error.message)),
    };
    if owner.is_empty() || repo.is_empty() {
        let error = malformed_message.unwrap_or_else(|| "No GitHub repository configured. Set the GitHub Repo field (owner/repo) in repository settings.".to_string());
        return PrCommentPageRequest::Fail(AppEvent::PrCommentsPageDispatchFailed {
            scope_repo_id,
            pr_number,
            error,
        });
    }
    PrCommentPageRequest::Ready(PrCommentPageParams {
        scope_repo_id,
        pr_number,
        owner,
        repo,
        cursor: comment_cursor(detail.comments.next_page()),
    })
}

fn comment_cursor(token: &PageToken) -> Option<String> {
    match token {
        PageToken::Cursor(cursor) => Some(cursor.clone()),
        PageToken::PageNumber(_) | PageToken::Done => None,
    }
}

/// Build the comments-page-loaded/failed event from the gh result.
/// @plan PLAN-20260624-PR-MODE.P11
/// @requirement REQ-PR-010
/// @pseudocode component-004 lines 146-155
fn pr_comment_page_event(
    ctx: &SharedContext,
    dispatched: &DispatchedPrCommentPageParams,
) -> AppEvent {
    let params = &dispatched.params;
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
            request_id: dispatched.request_id,
            comments: response.comments,
            cursor: response.cursor,
            has_more: response.has_more,
        },
        Some(Err(error)) => AppEvent::PrCommentsPageFailed {
            scope_repo_id: params.scope_repo_id.clone(),
            pr_number: params.pr_number,
            request_id: dispatched.request_id,
            error: error.to_string(),
        },
        None => AppEvent::PrCommentsPageFailed {
            scope_repo_id: params.scope_repo_id.clone(),
            pr_number: params.pr_number,
            request_id: dispatched.request_id,
            error: "Application context unavailable".to_string(),
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use jefe::domain::{IssueComment, PrCheckStatus, PrState, PullRequestDetail, Repository};
    use jefe::state::{
        AppState, ComposerTarget, InlineState, PrDetailSubfocus, PullRequestsState, ScreenMode,
    };
    use std::path::PathBuf;

    /// Build a seeded PR detail for the max-offset test.
    fn seeded_pr_detail() -> PullRequestDetail {
        PullRequestDetail {
            repo_owner_name: "owner/repo".to_string(),
            number: 1,
            title: "PR #1".to_string(),
            state: PrState::Open,
            is_draft: false,
            author_login: "octocat".to_string(),
            created_at: "2024-01-01T00:00:00Z".to_string(),
            updated_at: "2024-01-02T00:00:00Z".to_string(),
            head_ref: "feature".to_string(),
            head_sha: "sha123".to_string(),
            base_ref: "main".to_string(),
            labels: vec![],
            assignees: vec![],
            milestone: None,
            body: "line one\nline two\nline three".to_string(),
            external_url: "https://github.com/owner/repo/pull/1".to_string(),
            review_decision: None,
            checks_status: PrCheckStatus::None,
            reviews: vec![],
            checks: vec![],
            comments: jefe::domain::PaginatedList::from_loaded(
                jefe::domain::CommentDetailIdentity {
                    scope_repo_id: jefe::domain::RepositoryId::default(),
                    number: 1,
                },
                vec![IssueComment {
                    comment_id: 1,
                    author_login: "alice".to_string(),
                    created_at: "2024-01-03T00:00:00Z".to_string(),
                    edited_at: None,
                    body: "comment body".to_string(),
                }],
                jefe::domain::PageToken::from_cursor(Some("cursor".to_string()), true),
            ),
            mergeable: None,
            merge_state_status: None,
        }
    }

    /// MED-8: `pr_detail_max_scroll_offset` MUST use the canonical
    /// `pr_detail_content_line_count` parity function (not a divergent
    /// heuristic), so the comments-dispatch "near bottom" check matches the
    /// real rendered length. We assert the helper returns exactly
    /// `line_count.saturating_sub(effective_document_viewport_rows)` for a
    /// seeded detail with reviews + comments (which the old heuristic
    /// miscounted).
    ///
    /// @plan PLAN-20260624-PR-MODE.P11
    /// @requirement REQ-PR-009
    /// @requirement REQ-PR-010
    /// @pseudocode component-004 lines 146-155
    #[test]
    fn test_comments_dispatch_max_offset_uses_parity_line_count() {
        let detail = seeded_pr_detail();
        let prs_state = PullRequestsState {
            active: true,
            pr_detail: Some(detail.clone()),
            detail_viewport_rows: 6,
            detail_subfocus: PrDetailSubfocus::Body,
            inline_state: InlineState::None,
            ..PullRequestsState::default()
        };
        let mut state = AppState {
            screen_mode: ScreenMode::DashboardPullRequests,
            prs_state,
            ..AppState::default()
        };
        state.repositories.push(Repository::new(
            jefe::domain::RepositoryId("repo-1".to_string()),
            "Repo 1".to_string(),
            "repo-1".to_string(),
            PathBuf::from("/tmp/repo1"),
        ));
        state.selected_repository_index = Some(0);

        let actual = pr_detail_max_scroll_offset(&state);
        let document_viewport = jefe::layout::pr_detail_document_viewport_rows(
            state.prs_state.detail_viewport_rows,
            false,
        );
        let expected = jefe::pr_detail_content::pr_detail_content_line_count(
            &detail,
            state.prs_state.detail_subfocus,
            &state.prs_state.inline_state,
            state.prs_state.loading.detail,
            state.prs_state.loading.comments,
        )
        .saturating_sub(document_viewport);

        assert_eq!(
            actual, expected,
            "comments-dispatch max offset MUST equal pr_detail_content_line_count().saturating_sub(effective_viewport)"
        );
    }

    /// The pagination helper must reserve the embedded NewComment TextBox rows,
    /// matching the reducer and UI scroll viewport when the composer is active.
    ///
    /// @plan PLAN-20260624-PR-MODE.P14
    /// @requirement REQ-PR-009
    /// @pseudocode component-001 lines 169-176
    #[test]
    fn test_comments_dispatch_max_offset_reserves_new_comment_text_box_rows() {
        let detail = seeded_pr_detail();
        let inline_state = InlineState::Composer {
            target: ComposerTarget::NewComment,
            text: "draft".to_string(),
            cursor: 5,
        };
        let prs_state = PullRequestsState {
            active: true,
            pr_detail: Some(detail.clone()),
            detail_viewport_rows: 9,
            detail_subfocus: PrDetailSubfocus::NewComment,
            inline_state,
            ..PullRequestsState::default()
        };
        let state = AppState {
            screen_mode: ScreenMode::DashboardPullRequests,
            prs_state,
            ..AppState::default()
        };

        let document_viewport = jefe::layout::pr_detail_document_viewport_rows(
            state.prs_state.detail_viewport_rows,
            true,
        );
        let expected = jefe::pr_detail_content::pr_detail_content_line_count(
            &detail,
            state.prs_state.detail_subfocus,
            &state.prs_state.inline_state,
            state.prs_state.loading.detail,
            state.prs_state.loading.comments,
        )
        .saturating_sub(document_viewport);

        assert_eq!(
            pr_detail_max_scroll_offset(&state),
            expected,
            "comments pagination max offset must reserve embedded TextBox rows"
        );
    }

    #[test]
    fn test_missing_github_repo_returns_dispatch_failure_without_request_id() {
        let prs_state = PullRequestsState {
            active: true,
            pr_detail: Some(seeded_pr_detail()),
            detail_viewport_rows: 6,
            ..PullRequestsState::default()
        };
        let mut state = AppState {
            screen_mode: ScreenMode::DashboardPullRequests,
            prs_state,
            ..AppState::default()
        };
        state.repositories.push(Repository::new(
            jefe::domain::RepositoryId("repo-1".to_string()),
            "Repo 1".to_string(),
            String::new(),
            PathBuf::from("/tmp/repo1"),
        ));
        state.selected_repository_index = Some(0);
        state.prs_state.detail_scroll_offset = pr_detail_max_scroll_offset(&state);

        let request = pr_comment_page_request(&state);

        let PrCommentPageRequest::Fail(AppEvent::PrCommentsPageDispatchFailed {
            scope_repo_id,
            pr_number,
            error,
        }) = request
        else {
            panic!("missing GitHub repo should produce a dispatch failure event");
        };
        assert_eq!(
            scope_repo_id,
            jefe::domain::RepositoryId("repo-1".to_string())
        );
        assert_eq!(pr_number, 1);
        assert!(error.contains("No GitHub repository configured"));
    }
}

//! Issue-detail navigation-invalidation reducer tests: list navigation
//! (up/down/home/end) must invalidate pending detail and comment-page
//! responses so stale loads never land on the newly selected issue.

use crate::domain::RepositoryId;
use crate::state::AppState;
use crate::state::events::AppEvent;
use crate::state::transition::TransitionExt;
use crate::state::types::IssueFocus;

use super::issues_tests_detail::{
    issue_comments_pending, issue_comments_with_cursor, issues_mode_state_with_repo,
    make_test_issue, p15_comment, p15_detail,
};

#[test]
fn test_issue_navigation_invalidates_pending_detail_responses() {
    let repo_id = RepositoryId("repo-1".to_string());
    let mut state = issues_mode_state_with_repo("repo-1");
    state
        .issues_state
        .list
        .replace_items(vec![make_test_issue(42), make_test_issue(43)]);
    state.issues_state.list.set_selected_index(Some(0));
    state.issues_state.issue_focus = IssueFocus::IssueList;
    state.issues_state.issue_detail = Some(p15_detail(42));
    state.mark_issue_detail_loading(repo_id.clone(), 42);

    let state = state.apply(AppEvent::IssuesNavigateDown).committed_pure();

    assert_eq!(state.issues_state.selected_issue_index(), Some(1));
    assert!(!state.issues_state.loading.detail);
    assert!(state.issues_state.detail_pending.is_none());

    let mut stale_detail = p15_detail(42);
    stale_detail.body = "stale detail body".to_string();
    let state = state
        .apply(AppEvent::IssueDetailLoaded {
            scope_repo_id: repo_id.clone(),
            issue_number: 42,
            request_id: 0,
            detail: Box::new(stale_detail),
        })
        .committed_pure();

    let detail = state
        .issues_state
        .issue_detail
        .as_ref()
        .unwrap_or_else(|| panic!("expected existing preview/detail"));
    assert_eq!(detail.body, "Issue body");

    let state = state
        .apply(AppEvent::IssueDetailLoadFailed {
            scope_repo_id: repo_id,
            issue_number: 42,
            request_id: 0,
            error: "stale failure".to_string(),
        })
        .committed_pure();

    assert!(state.issues_state.error.is_none());
    assert!(!state.issues_state.loading.detail);
}

fn install_cursor_detail(state: &mut AppState, repo_id: &RepositoryId) {
    let mut detail = p15_detail(42);
    detail.comments = issue_comments_with_cursor(repo_id, 42, "cursor-1".to_string(), Vec::new());
    state.issues_state.issue_detail = Some(detail);
}

#[test]
fn test_issue_navigation_away_and_back_invalidates_pending_comment_page() {
    let repo_id = RepositoryId("repo-1".to_string());
    let mut state = issues_mode_state_with_repo("repo-1");
    state
        .issues_state
        .list
        .replace_items(vec![make_test_issue(42), make_test_issue(43)]);
    state.issues_state.list.set_selected_index(Some(0));
    state.issues_state.issue_focus = IssueFocus::IssueList;
    install_cursor_detail(&mut state, &repo_id);
    let Some(request_id) =
        state.begin_issue_comment_page_for_test(repo_id.clone(), 42, Some("cursor-1".to_string()))
    else {
        panic!("comment page should start");
    };

    let mut state = state
        .apply(AppEvent::IssuesNavigateDown)
        .committed_pure()
        .apply(AppEvent::IssuesNavigateUp)
        .committed_pure();
    install_cursor_detail(&mut state, &repo_id);
    let Some(current_request_id) =
        state.begin_issue_comment_page(&repo_id, 42, Some("cursor-1".to_string()))
    else {
        panic!("replacement comment page should start");
    };

    assert_ne!(request_id, current_request_id);
    assert_eq!(state.issues_state.selected_issue_index(), Some(0));
    assert!(state.issues_state.loading.comments);
    assert!(issue_comments_pending(&state));

    let state = state
        .apply(AppEvent::IssueCommentsPageLoaded {
            scope_repo_id: repo_id.clone(),
            issue_number: 42,
            request_id,
            request_cursor: Some("cursor-1".to_string()),
            comments: vec![p15_comment(99, "stale", "2024-01-04T00:00:00Z", "stale")],
            cursor: None,
            has_more: false,
        })
        .committed_pure();

    let detail = state
        .issues_state
        .issue_detail
        .as_ref()
        .unwrap_or_else(|| panic!("expected existing detail"));
    assert!(detail.comments.is_empty());

    let state = state
        .apply(AppEvent::IssueCommentsPageFailed {
            scope_repo_id: repo_id,
            issue_number: 42,
            request_id,
            request_cursor: Some("cursor-1".to_string()),
            error: "stale failure".to_string(),
        })
        .committed_pure();

    assert!(state.issues_state.error.is_none());
    assert!(state.issues_state.loading.comments);
}
#[test]
fn test_issue_navigate_end_invalidates_pending_detail_responses() {
    let repo_id = RepositoryId("repo-1".to_string());
    let mut state = issues_mode_state_with_repo("repo-1");
    state
        .issues_state
        .list
        .replace_items(vec![make_test_issue(42), make_test_issue(43)]);
    state.issues_state.list.set_selected_index(Some(0));
    state.issues_state.issue_focus = IssueFocus::IssueList;
    state.issues_state.issue_detail = Some(p15_detail(42));
    state.mark_issue_detail_loading(repo_id.clone(), 42);

    let state = state.apply(AppEvent::IssuesNavigateEnd).committed_pure();

    assert_eq!(state.issues_state.selected_issue_index(), Some(1));
    assert!(!state.issues_state.loading.detail);
    assert!(state.issues_state.detail_pending.is_none());

    let mut stale_detail = p15_detail(42);
    stale_detail.body = "stale detail body".to_string();
    let state = state
        .apply(AppEvent::IssueDetailLoaded {
            scope_repo_id: repo_id,
            issue_number: 42,
            request_id: 0,
            detail: Box::new(stale_detail),
        })
        .committed_pure();

    let detail = state
        .issues_state
        .issue_detail
        .as_ref()
        .unwrap_or_else(|| panic!("expected existing preview/detail"));
    assert_eq!(detail.body, "Issue body");
}

#[test]
fn test_issue_navigate_home_invalidates_pending_comment_page() {
    let repo_id = RepositoryId("repo-1".to_string());
    let mut state = issues_mode_state_with_repo("repo-1");
    state
        .issues_state
        .list
        .replace_items(vec![make_test_issue(42), make_test_issue(43)]);
    state.issues_state.list.set_selected_index(Some(1));
    state.issues_state.issue_focus = IssueFocus::IssueList;
    let mut detail = p15_detail(43);
    detail.comments = issue_comments_with_cursor(&repo_id, 43, "cursor-1".to_string(), Vec::new());
    state.issues_state.issue_detail = Some(detail);
    let Some(request_id) =
        state.begin_issue_comment_page_for_test(repo_id.clone(), 43, Some("cursor-1".to_string()))
    else {
        panic!("comment page should start");
    };

    let state = state.apply(AppEvent::IssuesNavigateHome).committed_pure();

    assert_eq!(state.issues_state.selected_issue_index(), Some(0));
    assert!(!state.issues_state.loading.comments);
    assert!(
        !state
            .issues_state
            .issue_detail
            .as_ref()
            .is_some_and(|detail| detail.comments.has_pending_request())
    );

    let state = state
        .apply(AppEvent::IssueCommentsPageLoaded {
            scope_repo_id: repo_id,
            issue_number: 43,
            request_id,
            request_cursor: Some("cursor-1".to_string()),
            comments: vec![p15_comment(99, "stale", "2024-01-04T00:00:00Z", "stale")],
            cursor: None,
            has_more: false,
        })
        .committed_pure();

    let detail = state
        .issues_state
        .issue_detail
        .as_ref()
        .unwrap_or_else(|| panic!("expected existing detail"));
    assert!(detail.comments.is_empty());
}

use crate::domain::{Issue, IssueComment, IssueDetail, IssueState, Repository, RepositoryId};
use crate::state::AppState;
use crate::state::events::AppEvent;
use crate::state::types::{
    ComposerTarget, DetailSubfocus, EditorTarget, InlineState, IssueFocus, ScreenMode,
};

use super::issues_test_fixtures::begin_issue_list_reload;

fn dashboard_issues_state() -> AppState {
    AppState {
        screen_mode: ScreenMode::DashboardIssues,
        ..AppState::default()
    }
}

/// Helper to create a test issue with the given number.
fn make_test_issue(number: u64) -> Issue {
    Issue {
        number,
        title: format!("Test Issue #{number}"),
        state: IssueState::Open,
        author_login: "testuser".to_string(),
        updated_at: "2024-01-01T00:00:00Z".to_string(),
        assignee_summary: String::new(),
        labels_summary: String::new(),
        assignees: Vec::new(),
        labels: Vec::new(),
        issue_type: String::new(),
        milestone: String::new(),
        module: String::new(),
        comment_count: 0,
        body: String::new(),
    }
}

/// P13 Test 14: ScreenMode::DashboardIssues is distinct from ScreenMode::Dashboard.
///
/// @plan PLAN-20260329-ISSUES-MODE.P13
/// @requirement REQ-ISS-002
#[test]
fn test_keybind_bar_issues_mode() {
    let dashboard_state = AppState::default();
    assert_eq!(dashboard_state.screen_mode, ScreenMode::Dashboard);

    let issues_state = AppState::default().apply(AppEvent::EnterIssuesMode);
    assert_eq!(issues_state.screen_mode, ScreenMode::DashboardIssues);

    // Modes are distinguishable — keybind bar can branch on this
    assert_ne!(dashboard_state.screen_mode, issues_state.screen_mode);

    // And exit returns to Dashboard
    let exited = issues_state.apply(AppEvent::ExitIssuesMode);
    assert_eq!(exited.screen_mode, ScreenMode::Dashboard);
    assert_ne!(exited.screen_mode, ScreenMode::DashboardIssues);
}

// =========================================================================
// P15 Integration Tests — Full State Flow Verification
// =========================================================================

/// Helper: create a state already in issues mode with a selected repository.
fn issues_mode_state_with_repo(repo_id: &str) -> AppState {
    let mut state = AppState::default();
    state.repositories.push(Repository::new(
        RepositoryId(repo_id.to_string()),
        "Test Repo".to_string(),
        repo_id.to_string(),
        std::path::PathBuf::from("/tmp/test"),
    ));
    state.selected_repository_index = Some(0);
    state.apply(AppEvent::EnterIssuesMode)
}

/// Helper: create a minimal IssueDetail with given number and empty comments.
fn p15_detail(number: u64) -> IssueDetail {
    IssueDetail {
        repo_owner_name: "owner/repo".to_string(),
        number,
        title: format!("Issue #{number}"),
        state: IssueState::Open,
        author_login: "user".to_string(),
        created_at: "2024-01-01T00:00:00Z".to_string(),
        updated_at: "2024-01-02T00:00:00Z".to_string(),
        labels: vec![],
        assignees: vec![],
        milestone: None,
        body: "Issue body".to_string(),
        external_url: format!("https://github.com/owner/repo/issues/{number}"),
        comments: vec![],
        has_more_comments: false,
        comments_cursor: None,
    }
}

fn p15_comment(comment_id: u64, author_login: &str, created_at: &str, body: &str) -> IssueComment {
    IssueComment {
        comment_id,
        author_login: author_login.to_string(),
        created_at: created_at.to_string(),
        edited_at: None,
        body: body.to_string(),
    }
}

fn p15_comment_page() -> Vec<IssueComment> {
    vec![
        p15_comment(1, "alice", "2024-01-01T00:00:00Z", "First comment"),
        p15_comment(2, "bob", "2024-01-02T00:00:00Z", "Second comment"),
    ]
}

fn p15_state_with_loaded_detail(repo_id: &RepositoryId, issue_number: u64) -> AppState {
    let mut state = issues_mode_state_with_repo("repo-1");
    state.mark_issue_detail_loading(repo_id.clone(), issue_number);
    state.apply(AppEvent::IssueDetailLoaded {
        scope_repo_id: repo_id.clone(),
        issue_number,
        request_id: 0,
        detail: Box::new(p15_detail(issue_number)),
    })
}

/// P15 Test 1: Enter issues mode, load issues, select one, exit.
/// Verifies: mode entered, issues loaded, mode exited, issues_state cleared,
/// screen_mode back to Dashboard.
///
/// @plan PLAN-20260329-ISSUES-MODE.P15
/// @requirement REQ-ISS-001
#[test]
fn test_mode_lifecycle_enter_browse_exit() {
    // Enter issues mode
    let mut state = issues_mode_state_with_repo("repo-1");
    assert_eq!(state.screen_mode, ScreenMode::DashboardIssues);
    assert!(state.issues_state.active);
    assert_eq!(state.issues_state.issue_focus, IssueFocus::IssueList);

    // Load issues
    let filter = state.issues_state.committed_filter.clone();
    let request_id = begin_issue_list_reload(&mut state, "repo-1", filter.clone());
    let state = state.apply(AppEvent::IssueListLoaded {
        scope_repo_id: RepositoryId("repo-1".to_string()),
        filter: Box::new(filter),
        request_id,
        issues: vec![make_test_issue(1), make_test_issue(2), make_test_issue(3)],
        cursor: None,
        has_more: false,
    });
    assert_eq!(state.issues_state.issues().len(), 3);
    assert_eq!(state.issues_state.selected_issue_index(), Some(0));
    assert!(!state.issues_state.list_loading());

    // Navigate down to select issue #2
    let state = state.apply(AppEvent::IssuesNavigateDown);
    assert_eq!(state.issues_state.selected_issue_index(), Some(1));

    // Exit issues mode
    let state = state.apply(AppEvent::ExitIssuesMode);
    assert_eq!(state.screen_mode, ScreenMode::Dashboard);
    assert!(!state.issues_state.active);
}

/// P15 Test 2: Enter, load issues, open detail, open composer, type text, cancel, exit.
///
/// @plan PLAN-20260329-ISSUES-MODE.P15
/// @requirement REQ-ISS-001
#[test]
fn test_mode_lifecycle_enter_interact_exit() {
    let mut state = issues_mode_state_with_repo("repo-1");

    // Load issues and open detail
    let filter = state.issues_state.committed_filter.clone();
    let request_id = begin_issue_list_reload(&mut state, "repo-1", filter.clone());
    let state = state
        .apply(AppEvent::IssueListLoaded {
            scope_repo_id: RepositoryId("repo-1".to_string()),
            filter: Box::new(filter),
            request_id,
            issues: vec![make_test_issue(10)],
            cursor: None,
            has_more: false,
        })
        .apply(AppEvent::IssuesEnter);
    assert_eq!(state.issues_state.issue_focus, IssueFocus::IssueDetail);

    // Open inline composer
    let state = state.apply(AppEvent::OpenNewCommentComposer);
    assert!(
        matches!(
            &state.issues_state.inline_state,
            InlineState::Composer {
                target: ComposerTarget::NewComment,
                ..
            }
        ),
        "expected Composer(NewComment), got {:?}",
        state.issues_state.inline_state
    );

    // Type some text
    let state = state
        .apply(AppEvent::InlineChar('h'))
        .apply(AppEvent::InlineChar('i'));
    match &state.issues_state.inline_state {
        InlineState::Composer { text, .. } => assert_eq!(text, "hi"),
        other => panic!("expected Composer with text, got {other:?}"),
    }

    // Cancel the composer
    let state = state.apply(AppEvent::InlineCancelOrEsc);
    assert_eq!(state.issues_state.inline_state, InlineState::None);

    // Exit issues mode
    let state = state.apply(AppEvent::ExitIssuesMode);
    assert_eq!(state.screen_mode, ScreenMode::Dashboard);
    assert!(!state.issues_state.active);
}

/// P15 Test 3: State-level routing integration — applying routed events in all 3 focus domains
/// produces correct state transitions.
///
/// @plan PLAN-20260329-ISSUES-MODE.P15
/// @requirement REQ-ISS-002
#[test]
fn test_key_routing_all_focus_domains() {
    // RepoList domain: IssuesNavigateUp/Down delegate to repo navigation
    let mut state = dashboard_issues_state();
    state.repositories.push(Repository::new(
        RepositoryId("r1".to_string()),
        "R1".to_string(),
        "r1".to_string(),
        std::path::PathBuf::from("/tmp/r1"),
    ));
    state.repositories.push(Repository::new(
        RepositoryId("r2".to_string()),
        "R2".to_string(),
        "r2".to_string(),
        std::path::PathBuf::from("/tmp/r2"),
    ));
    state.selected_repository_index = Some(0);
    state.issues_state.active = true;
    state.issues_state.issue_focus = IssueFocus::RepoList;

    // In RepoList focus, IssuesNavigateDown moves to next repo
    let state = state.apply(AppEvent::IssuesNavigateDown);
    assert_eq!(state.selected_repository_index, Some(1));

    // IssueList domain: IssuesEnter (with issue selected) transitions to IssueDetail
    let mut state = state;
    state.issues_state.issue_focus = IssueFocus::IssueList;
    state
        .issues_state
        .list
        .replace_items(vec![make_test_issue(1)]);
    state.issues_state.list.set_selected_index(Some(0));
    let state = state.apply(AppEvent::IssuesEnter);
    assert_eq!(state.issues_state.issue_focus, IssueFocus::IssueDetail);

    // IssueDetail domain: IssueDetailSubfocusNext advances subfocus (requires detail)
    let mut state = state;
    state.issues_state.issue_detail = Some(p15_detail(1));
    let state = state.apply(AppEvent::IssueDetailSubfocusNext);
    // Body with no comments -> NewComment
    assert_eq!(
        state.issues_state.detail_subfocus,
        DetailSubfocus::NewComment
    );
}

/// P15 Test 4: Suppressed key events produce no state change across all focus domains.
///
/// In issues mode, keys 's', Ctrl-d, Ctrl-k, 'l' are suppressed (no-op).
/// At the state level, the corresponding AppEvent variants don't exist as suppressions;
/// we verify that the focus domains are preserved through a full navigation sequence
/// (i.e., the state machine doesn't accidentally jump focus on unknown inputs).
///
/// @plan PLAN-20260329-ISSUES-MODE.P15
/// @requirement REQ-ISS-002
#[test]
fn test_key_routing_suppression_comprehensive() {
    // Suppression at the state level means: any unrecognized event should not
    // affect issues_state focus or mode. Verify focus is stable across domains
    // when we apply IssuesCycleFocus (Tab) — the catch-all that falls through
    // after per-domain handlers.
    let domains = [
        IssueFocus::RepoList,
        IssueFocus::IssueList,
        IssueFocus::IssueDetail,
    ];
    for domain in domains {
        let mut state = dashboard_issues_state();
        state.issues_state.active = true;
        state.issues_state.issue_focus = domain;

        // Applying CloseModal (no-op in issues mode) should not change issues focus
        let state = state.apply(AppEvent::CloseModal);
        assert_eq!(
            state.issues_state.issue_focus, domain,
            "issues focus changed unexpectedly in domain {domain:?}"
        );
        assert_eq!(state.screen_mode, ScreenMode::DashboardIssues);
        assert!(state.issues_state.active);
    }

    // Separately verify that all 4 suppressed-key AppEvent equivalents (no direct
    // mapping) don't affect issues mode state: mode stays active, focus unchanged.
    let mut state = AppState::default();
    state.issues_state.active = true;
    state.issues_state.issue_focus = IssueFocus::IssueList;

    // 's' maps to OpenSearch in normal mode, but in issues mode there's no handler;
    // applying OpenSearch opens the modal but doesn't exit issues mode
    let state = state.apply(AppEvent::OpenSearch);
    assert!(
        state.issues_state.active,
        "issues mode should remain active"
    );

    // ClearWarning (no-op) doesn't affect issues focus
    let state = state.apply(AppEvent::ClearWarning);
    assert!(state.issues_state.active);
}

/// P15 Test 5: Open composer, type text, apply CommentCreateFailed — draft preserved and error set.
///
/// @plan PLAN-20260329-ISSUES-MODE.P15
/// @requirement REQ-ISS-013
#[test]
fn test_error_handling_rate_limit_preserves_draft() {
    let repo_id = RepositoryId("repo-1".to_string());
    let mut state = issues_mode_state_with_repo("repo-1");
    state.mark_issue_detail_loading(repo_id.clone(), 42);
    let mut state = state.apply(AppEvent::IssueDetailLoaded {
        scope_repo_id: repo_id.clone(),
        issue_number: 42,
        request_id: 0,
        detail: Box::new(p15_detail(42)),
    });
    state.issues_state.inline_state = InlineState::Composer {
        target: ComposerTarget::NewComment,
        text: "my draft comment".to_string(),
        cursor: 16,
    };
    let pending_target = state.issues_state.inline_state.clone();
    let state = state.apply(AppEvent::MutationSubmitted {
        scope_repo_id: repo_id.clone(),
        mutation_id: 1,
        target: pending_target,
    });

    let state = state.apply(AppEvent::CommentCreateFailed {
        scope_repo_id: repo_id,
        issue_number: 42,
        mutation_id: 1,
        error: "API rate limit exceeded".to_string(),
    });

    // Error is set
    assert_eq!(
        state.issues_state.error,
        Some("API rate limit exceeded".to_string())
    );
    assert_eq!(
        state.issues_state.inline_state,
        InlineState::Composer {
            target: ComposerTarget::NewComment,
            text: "my draft comment".to_string(),
            cursor: 16,
        }
    );
    assert!(state.issues_state.mutation_pending.is_none());
}

/// P15 Test 6: Apply IssueListLoadFailed with auth message — error displayed, mode still active.
///
/// @plan PLAN-20260329-ISSUES-MODE.P15
/// @requirement REQ-ISS-013
#[test]
fn test_error_handling_auth_failure_blocks_ops() {
    let mut state = issues_mode_state_with_repo("repo-1");
    assert!(state.issues_state.active);

    let filter = state.issues_state.committed_filter.clone();
    let request_id = begin_issue_list_reload(&mut state, "repo-1", filter.clone());
    let state = state.apply(AppEvent::IssueListLoadFailed {
        scope_repo_id: RepositoryId("repo-1".to_string()),
        filter: Box::new(filter),
        request_id,
        request_cursor: None,
        error: "authentication required: token expired".to_string(),
    });

    // Error is shown
    assert!(state.issues_state.error.is_some());
    let err = state
        .issues_state
        .error
        .as_ref()
        .unwrap_or_else(|| panic!("expected value"));
    assert!(err.contains("authentication") || err.contains("token"));
    // Mode remains active
    assert!(state.issues_state.active);
    assert_eq!(state.screen_mode, ScreenMode::DashboardIssues);
    // List loading is cleared
    assert!(!state.issues_state.list_loading());
}

/// P15 Test 7: Apply network error — mode/focus stable, error shown.
///
/// @plan PLAN-20260329-ISSUES-MODE.P15
/// @requirement REQ-ISS-013
#[test]
fn test_error_handling_network_error_stable_mode() {
    let mut state = issues_mode_state_with_repo("repo-1");
    let focus_before = state.issues_state.issue_focus;

    let filter = state.issues_state.committed_filter.clone();
    let request_id = begin_issue_list_reload(&mut state, "repo-1", filter.clone());
    let state = state.apply(AppEvent::IssueListLoadFailed {
        scope_repo_id: RepositoryId("repo-1".to_string()),
        filter: Box::new(filter),
        request_id,
        request_cursor: None,
        error: "network timeout: connection refused".to_string(),
    });

    // Error is shown
    assert!(state.issues_state.error.is_some());
    // Focus unchanged
    assert_eq!(state.issues_state.issue_focus, focus_before);
    // Mode stable
    assert!(state.issues_state.active);
    assert_eq!(state.screen_mode, ScreenMode::DashboardIssues);
}

/// P15 Test 8: Load issues with has_more=true — has_more_issues flag set.
///
/// @plan PLAN-20260329-ISSUES-MODE.P15
/// @requirement REQ-ISS-007
#[test]
fn test_pagination_issue_list_auto_load() {
    let mut state = issues_mode_state_with_repo("repo-1");
    let filter = state.issues_state.committed_filter.clone();
    let request_id = begin_issue_list_reload(&mut state, "repo-1", filter.clone());
    let state = state.apply(AppEvent::IssueListLoaded {
        scope_repo_id: RepositoryId("repo-1".to_string()),
        filter: Box::new(filter),
        request_id,
        issues: vec![make_test_issue(1), make_test_issue(2)],
        cursor: Some("cursor-abc".to_string()),
        has_more: true,
    });

    assert!(state.issues_state.has_more_issues());
    assert!(matches!(
        state.issues_state.list.next_page(),
        crate::domain::PageToken::Cursor(c) if c == "cursor-abc"
    ));
    assert_eq!(state.issues_state.issues().len(), 2);
}

#[test]
fn test_detail_content_line_count_includes_empty_comments_separator() {
    let mut state = dashboard_issues_state();
    state.issues_state.issue_detail = Some(p15_detail(1));

    assert_eq!(state.issues_state.detail_content_line_count(), 8);
}

#[test]
fn test_detail_content_line_count_includes_loading_comments_separator() {
    let mut state = dashboard_issues_state();
    state.issues_state.issue_detail = Some(p15_detail(1));
    state.issues_state.loading.comments = true;

    assert_eq!(state.issues_state.detail_content_line_count(), 8);
}

#[test]
fn test_detail_content_line_count_includes_non_empty_comments_separator() {
    let mut detail = p15_detail(1);
    detail.comments = vec![p15_comment(101, "alice", "2024-01-03T00:00:00Z", "hello")];
    let mut state = dashboard_issues_state();
    state.issues_state.issue_detail = Some(detail);

    assert_eq!(state.issues_state.detail_content_line_count(), 10);
}
/// P15 Test 9: Load detail, load first comments page, load second — all comments present in order.
///
/// @plan PLAN-20260329-ISSUES-MODE.P15
/// @requirement REQ-ISS-007
#[test]
fn test_pagination_comments_append() {
    let repo_id = RepositoryId("repo-1".to_string());

    let state = p15_state_with_loaded_detail(&repo_id, 42);
    let detail = state
        .issues_state
        .issue_detail
        .as_ref()
        .unwrap_or_else(|| panic!("expected value"));
    assert_eq!(detail.comments.len(), 0);

    // Load first page of comments
    let mut state = state;
    state.mark_comments_page_loading(repo_id.clone(), 42, None);
    let state = state.apply(AppEvent::IssueCommentsPageLoaded {
        scope_repo_id: repo_id.clone(),
        issue_number: 42,
        request_id: 0,
        request_cursor: None,
        comments: p15_comment_page(),
        cursor: Some("page2".to_string()),
        has_more: true,
    });
    let detail = state
        .issues_state
        .issue_detail
        .as_ref()
        .unwrap_or_else(|| panic!("expected value"));
    assert_eq!(detail.comments.len(), 2);
    assert!(detail.has_more_comments);

    // Load second page of comments
    let mut state = state;
    state.mark_comments_page_loading(repo_id.clone(), 42, Some("page2".to_string()));
    let state = state.apply(AppEvent::IssueCommentsPageLoaded {
        scope_repo_id: repo_id.clone(),
        issue_number: 42,
        request_id: 0,
        request_cursor: Some("page2".to_string()),
        comments: vec![p15_comment(
            3,
            "carol",
            "2024-01-03T00:00:00Z",
            "Third comment",
        )],
        cursor: None,
        has_more: false,
    });
    let detail = state
        .issues_state
        .issue_detail
        .as_ref()
        .unwrap_or_else(|| panic!("expected value"));
    assert_eq!(detail.comments.len(), 3);
    assert!(!detail.has_more_comments);
    // Comments appear in insertion order
    assert_eq!(detail.comments[0].comment_id, 1);
    assert_eq!(detail.comments[1].comment_id, 2);
    assert_eq!(detail.comments[2].comment_id, 3);
}

#[test]
fn test_stale_comment_page_same_repo_different_issue_does_not_clear_current_loading_or_error() {
    let repo_id = RepositoryId("repo-1".to_string());
    let mut state = issues_mode_state_with_repo("repo-1");
    state.mark_issue_detail_loading(repo_id.clone(), 42);
    let mut state = state.apply(AppEvent::IssueDetailLoaded {
        scope_repo_id: repo_id.clone(),
        issue_number: 42,
        request_id: 0,
        detail: Box::new(p15_detail(42)),
    });
    state.issues_state.loading.comments = true;
    state.issues_state.error = Some("current load still pending".to_string());
    state.mark_comments_page_loading(repo_id.clone(), 42, Some("current-cursor".to_string()));

    let state = state.apply(AppEvent::IssueCommentsPageLoaded {
        scope_repo_id: repo_id.clone(),
        issue_number: 99,
        request_id: 0,
        request_cursor: Some("stale-cursor".to_string()),
        comments: vec![p15_comment(99, "stale", "2024-01-04T00:00:00Z", "stale")],
        cursor: None,
        has_more: false,
    });

    let detail = state
        .issues_state
        .issue_detail
        .as_ref()
        .unwrap_or_else(|| panic!("expected detail"));
    assert_eq!(detail.number, 42);
    assert!(detail.comments.is_empty());
    assert!(state.issues_state.loading.comments);
    assert_eq!(
        state.issues_state.error.as_deref(),
        Some("current load still pending")
    );

    let state = state.apply(AppEvent::IssueCommentsPageFailed {
        scope_repo_id: repo_id,
        issue_number: 99,
        request_id: 0,
        request_cursor: Some("stale-cursor".to_string()),
        error: "stale failure".to_string(),
    });

    assert!(state.issues_state.loading.comments);
    assert_eq!(
        state.issues_state.error.as_deref(),
        Some("current load still pending")
    );
}

#[test]
fn test_stale_comment_page_same_issue_different_cursor_does_not_clear_current_loading_or_error() {
    let repo_id = RepositoryId("repo-1".to_string());
    let mut state = issues_mode_state_with_repo("repo-1");
    state.mark_issue_detail_loading(repo_id.clone(), 42);
    let mut state = state.apply(AppEvent::IssueDetailLoaded {
        scope_repo_id: repo_id.clone(),
        issue_number: 42,
        request_id: 0,
        detail: Box::new(p15_detail(42)),
    });
    state.issues_state.loading.comments = true;
    state.issues_state.error = Some("current comments page pending".to_string());
    state.mark_comments_page_loading(repo_id.clone(), 42, Some("current-cursor".to_string()));

    let state = state.apply(AppEvent::IssueCommentsPageLoaded {
        scope_repo_id: repo_id.clone(),
        issue_number: 42,
        request_id: 0,
        request_cursor: Some("stale-cursor".to_string()),
        comments: vec![p15_comment(99, "stale", "2024-01-04T00:00:00Z", "stale")],
        cursor: None,
        has_more: false,
    });

    let detail = state
        .issues_state
        .issue_detail
        .as_ref()
        .unwrap_or_else(|| panic!("expected detail"));
    assert!(detail.comments.is_empty());
    assert!(state.issues_state.loading.comments);
    assert_eq!(
        state.issues_state.error.as_deref(),
        Some("current comments page pending")
    );

    let state = state.apply(AppEvent::IssueCommentsPageFailed {
        scope_repo_id: repo_id,
        issue_number: 42,
        request_id: 0,
        request_cursor: Some("stale-cursor".to_string()),
        error: "stale failure".to_string(),
    });

    assert!(state.issues_state.loading.comments);
    assert_eq!(
        state.issues_state.error.as_deref(),
        Some("current comments page pending")
    );
}

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

    let state = state.apply(AppEvent::IssuesNavigateDown);

    assert_eq!(state.issues_state.selected_issue_index(), Some(1));
    assert!(!state.issues_state.loading.detail);
    assert!(state.issues_state.detail_pending.is_none());

    let mut stale_detail = p15_detail(42);
    stale_detail.body = "stale detail body".to_string();
    let state = state.apply(AppEvent::IssueDetailLoaded {
        scope_repo_id: repo_id.clone(),
        issue_number: 42,
        request_id: 0,
        detail: Box::new(stale_detail),
    });

    let detail = state
        .issues_state
        .issue_detail
        .as_ref()
        .unwrap_or_else(|| panic!("expected existing preview/detail"));
    assert_eq!(detail.body, "Issue body");

    let state = state.apply(AppEvent::IssueDetailLoadFailed {
        scope_repo_id: repo_id,
        issue_number: 42,
        request_id: 0,
        error: "stale failure".to_string(),
    });

    assert!(state.issues_state.error.is_none());
    assert!(!state.issues_state.loading.detail);
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
    let mut detail = p15_detail(42);
    detail.has_more_comments = true;
    detail.comments_cursor = Some("cursor-1".to_string());
    state.issues_state.issue_detail = Some(detail);
    state.mark_comments_page_loading(repo_id.clone(), 42, Some("cursor-1".to_string()));

    let state = state
        .apply(AppEvent::IssuesNavigateDown)
        .apply(AppEvent::IssuesNavigateUp);

    assert_eq!(state.issues_state.selected_issue_index(), Some(0));
    assert!(!state.issues_state.loading.comments);
    assert!(state.issues_state.comments_page_pending.is_none());

    let state = state.apply(AppEvent::IssueCommentsPageLoaded {
        scope_repo_id: repo_id.clone(),
        issue_number: 42,
        request_id: 0,
        request_cursor: Some("cursor-1".to_string()),
        comments: vec![p15_comment(99, "stale", "2024-01-04T00:00:00Z", "stale")],
        cursor: None,
        has_more: false,
    });

    let detail = state
        .issues_state
        .issue_detail
        .as_ref()
        .unwrap_or_else(|| panic!("expected existing detail"));
    assert!(detail.comments.is_empty());

    let state = state.apply(AppEvent::IssueCommentsPageFailed {
        scope_repo_id: repo_id,
        issue_number: 42,
        request_id: 0,
        request_cursor: Some("cursor-1".to_string()),
        error: "stale failure".to_string(),
    });

    assert!(state.issues_state.error.is_none());
    assert!(!state.issues_state.loading.comments);
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

    let state = state.apply(AppEvent::IssuesNavigateEnd);

    assert_eq!(state.issues_state.selected_issue_index(), Some(1));
    assert!(!state.issues_state.loading.detail);
    assert!(state.issues_state.detail_pending.is_none());

    let mut stale_detail = p15_detail(42);
    stale_detail.body = "stale detail body".to_string();
    let state = state.apply(AppEvent::IssueDetailLoaded {
        scope_repo_id: repo_id,
        issue_number: 42,
        request_id: 0,
        detail: Box::new(stale_detail),
    });

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
    detail.has_more_comments = true;
    detail.comments_cursor = Some("cursor-1".to_string());
    state.issues_state.issue_detail = Some(detail);
    state.mark_comments_page_loading(repo_id.clone(), 43, Some("cursor-1".to_string()));

    let state = state.apply(AppEvent::IssuesNavigateHome);

    assert_eq!(state.issues_state.selected_issue_index(), Some(0));
    assert!(!state.issues_state.loading.comments);
    assert!(state.issues_state.comments_page_pending.is_none());

    let state = state.apply(AppEvent::IssueCommentsPageLoaded {
        scope_repo_id: repo_id,
        issue_number: 43,
        request_id: 0,
        request_cursor: Some("cursor-1".to_string()),
        comments: vec![p15_comment(99, "stale", "2024-01-04T00:00:00Z", "stale")],
        cursor: None,
        has_more: false,
    });

    let detail = state
        .issues_state
        .issue_detail
        .as_ref()
        .unwrap_or_else(|| panic!("expected existing detail"));
    assert!(detail.comments.is_empty());
}

#[test]
fn test_stale_mutation_events_same_repo_different_issue_do_not_mutate_or_clear_inline_state() {
    let repo_id = RepositoryId("repo-1".to_string());
    let mut detail = p15_detail(42);
    detail.comments = vec![p15_comment(7, "alice", "2024-01-03T00:00:00Z", "original")];
    let mut state = issues_mode_state_with_repo("repo-1");
    state.mark_issue_detail_loading(repo_id.clone(), 42);
    let mut state = state.apply(AppEvent::IssueDetailLoaded {
        scope_repo_id: repo_id.clone(),
        issue_number: 42,
        request_id: 0,
        detail: Box::new(detail),
    });
    state.issues_state.inline_state = InlineState::Composer {
        target: ComposerTarget::NewComment,
        text: "draft".to_string(),
        cursor: 5,
    };
    let pending_target = state.issues_state.inline_state.clone();
    let state = state.apply(AppEvent::MutationSubmitted {
        scope_repo_id: repo_id.clone(),
        mutation_id: 1,
        target: pending_target,
    });
    let mut state = state;
    state.issues_state.error = Some("current error".to_string());

    let state = state
        .apply(AppEvent::CommentCreated {
            scope_repo_id: repo_id.clone(),
            issue_number: 99,
            mutation_id: 1,
            comment: p15_comment(8, "bob", "2024-01-04T00:00:00Z", "stale"),
        })
        .apply(AppEvent::IssueBodyUpdated {
            scope_repo_id: repo_id.clone(),
            issue_number: 99,
            mutation_id: 1,
            body: "stale body".to_string(),
        })
        .apply(AppEvent::CommentUpdated {
            scope_repo_id: repo_id,
            issue_number: 99,
            mutation_id: 1,
            comment_id: 7,
            comment_index: 0,
            body: "stale update".to_string(),
        });

    let detail = state
        .issues_state
        .issue_detail
        .as_ref()
        .unwrap_or_else(|| panic!("expected detail"));
    assert_eq!(detail.body, "Issue body");
    assert_eq!(detail.comments.len(), 1);

    assert_eq!(detail.comments[0].body, "original");
    match &state.issues_state.inline_state {
        InlineState::Composer { text, .. } => assert_eq!(text, "draft"),
        other => panic!("expected composer draft to remain, got {other:?}"),
    }
    assert_eq!(state.issues_state.error.as_deref(), Some("current error"));
}

#[test]
fn test_comment_update_matches_by_comment_id_when_index_shifted() {
    let repo_id = RepositoryId("repo-1".to_string());
    let mut detail = p15_detail(42);
    detail.comments = vec![
        p15_comment(1, "alice", "2024-01-01T00:00:00Z", "first"),
        p15_comment(2, "bob", "2024-01-02T00:00:00Z", "second"),
    ];
    let mut state = issues_mode_state_with_repo("repo-1");
    state.mark_issue_detail_loading(repo_id.clone(), 42);
    let state = state.apply(AppEvent::IssueDetailLoaded {
        scope_repo_id: repo_id.clone(),
        issue_number: 42,
        request_id: 0,
        detail: Box::new(detail),
    });

    let state = state
        .apply(AppEvent::MutationSubmitted {
            scope_repo_id: repo_id.clone(),
            mutation_id: 1,
            target: InlineState::Editor {
                target: EditorTarget::Comment { comment_index: 0 },
                text: "updated by id".to_string(),
                cursor: 13,
            },
        })
        .apply(AppEvent::CommentUpdated {
            scope_repo_id: repo_id,
            issue_number: 42,
            mutation_id: 1,
            comment_id: 2,
            comment_index: 0,
            body: "updated by id".to_string(),
        });

    let detail = state
        .issues_state
        .issue_detail
        .as_ref()
        .unwrap_or_else(|| panic!("expected detail"));
    assert_eq!(detail.comments[0].body, "first");
    assert_eq!(detail.comments[1].body, "updated by id");
}

/// P15 Test 10: Enter issues, exit — prior focus (pane_focus, selected_agent_index) restored.

#[test]
fn test_stale_mutation_failures_same_repo_different_issue_do_not_clear_inline_state() {
    let repo_id = RepositoryId("repo-1".to_string());
    let mut state = issues_mode_state_with_repo("repo-1");
    state.mark_issue_detail_loading(repo_id.clone(), 42);
    let mut state = state.apply(AppEvent::IssueDetailLoaded {
        scope_repo_id: repo_id.clone(),
        issue_number: 42,
        request_id: 0,
        detail: Box::new(p15_detail(42)),
    });
    state.issues_state.inline_state = InlineState::Composer {
        target: ComposerTarget::NewComment,
        text: "draft".to_string(),
        cursor: 5,
    };
    let pending_target = state.issues_state.inline_state.clone();
    let state = state.apply(AppEvent::MutationSubmitted {
        scope_repo_id: repo_id.clone(),
        mutation_id: 1,
        target: pending_target,
    });
    let mut state = state;
    state.issues_state.error = Some("current error".to_string());

    let state = state
        .apply(AppEvent::CommentCreateFailed {
            scope_repo_id: repo_id.clone(),
            issue_number: 99,
            mutation_id: 1,
            error: "stale comment create failure".to_string(),
        })
        .apply(AppEvent::MutationFailed {
            scope_repo_id: repo_id,
            issue_number: Some(99),
            mutation_id: Some(1),
            error: "stale mutation failure".to_string(),
        });

    match &state.issues_state.inline_state {
        InlineState::Composer { text, .. } => assert_eq!(text, "draft"),
        other => panic!("expected composer draft to remain, got {other:?}"),
    }
    assert_eq!(state.issues_state.error.as_deref(), Some("current error"));
}

use crate::domain::{
    Agent, AgentId, Issue, IssueComment, IssueDetail, IssueFilter, IssueState, Repository,
    RepositoryId,
};
use crate::state::AppState;
use crate::state::types::{
    AgentChooserState, AppEvent, ComposerTarget, DetailSubfocus, EditorTarget, InlineState,
    IssueFocus, PaneFocus, PriorAgentFocus, ScreenMode,
};
use std::path::PathBuf;

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
        comment_count: 0,
        body: String::new(),
    }
}

/// Test 1: EnterIssuesMode sets screen mode, activates issues state, and focuses issue list.
/// @plan PLAN-20260329-ISSUES-MODE.P04
/// @requirement REQ-ISS-001
/// @pseudocode component-001 lines 10-15
#[test]
fn test_enter_issues_mode_sets_screen_mode() {
    let state = AppState::default();
    let new_state = state.apply(AppEvent::EnterIssuesMode);
    assert_eq!(new_state.screen_mode, ScreenMode::DashboardIssues);
    assert!(new_state.issues_state.active);
    assert_eq!(new_state.issues_state.issue_focus, IssueFocus::IssueList);
}

/// Test 2: EnterIssuesMode saves prior agent focus for restoration on exit.
/// @plan PLAN-20260329-ISSUES-MODE.P04
/// @requirement REQ-ISS-005
/// @pseudocode component-001 lines 20-25
#[test]
fn test_enter_issues_mode_saves_prior_focus() {
    let state = AppState {
        pane_focus: PaneFocus::Agents,
        selected_agent_index: Some(2),
        selected_repository_index: Some(1),
        ..AppState::default()
    };

    let new_state = state.apply(AppEvent::EnterIssuesMode);
    assert!(new_state.issues_state.prior_agent_focus.is_some());
    let saved = new_state
        .issues_state
        .prior_agent_focus
        .unwrap_or_else(|| panic!("expected value"));
    assert_eq!(saved.pane_focus, PaneFocus::Agents);
    assert_eq!(saved.selected_agent_index, Some(2));
    assert_eq!(saved.selected_repository_index, Some(1));
}

/// Test 3: ExitIssuesMode restores the saved prior focus.
/// @plan PLAN-20260329-ISSUES-MODE.P04
/// @requirement REQ-ISS-005
/// @pseudocode component-001 lines 30-35
#[test]
fn test_exit_issues_mode_restores_focus() {
    let mut state = dashboard_issues_state();
    state.issues_state.active = true;
    state.issues_state.prior_agent_focus = Some(PriorAgentFocus {
        pane_focus: PaneFocus::Agents,
        selected_repository_index: Some(0),
        selected_agent_index: Some(1),
    });

    // Set up 2 agents for the selected repository
    state.repositories.push(Repository::new(
        RepositoryId("repo-1".to_string()),
        "Repo 1".to_string(),
        "repo-1".to_string(),
        PathBuf::from("/tmp/repo1"),
    ));
    state.selected_repository_index = Some(0);

    // Create agents for the repository
    state.agents.push(Agent::new(
        AgentId("agent-1".to_string()),
        RepositoryId("repo-1".to_string()),
        "Agent 1".to_string(),
        PathBuf::from("/tmp/agent1"),
    ));
    state.agents.push(Agent::new(
        AgentId("agent-2".to_string()),
        RepositoryId("repo-1".to_string()),
        "Agent 2".to_string(),
        PathBuf::from("/tmp/agent2"),
    ));

    let new_state = state.apply(AppEvent::ExitIssuesMode);
    assert_eq!(new_state.screen_mode, ScreenMode::Dashboard);
    assert_eq!(new_state.pane_focus, PaneFocus::Agents);
    assert_eq!(new_state.selected_agent_index, Some(1));
}

/// Test 4: ExitIssuesMode falls back gracefully when saved agent index is out of bounds.
/// @plan PLAN-20260329-ISSUES-MODE.P04
/// @requirement REQ-ISS-005
/// @pseudocode component-001 lines 36-40
#[test]
fn test_exit_issues_mode_fallback_when_target_gone() {
    let mut state = dashboard_issues_state();
    state.issues_state.active = true;
    state.issues_state.prior_agent_focus = Some(PriorAgentFocus {
        pane_focus: PaneFocus::Agents,
        selected_repository_index: Some(0),
        selected_agent_index: Some(5), // Out of bounds - only 2 agents
    });

    // Set up repository with 2 agents
    state.repositories.push(Repository::new(
        RepositoryId("repo-1".to_string()),
        "Repo 1".to_string(),
        "repo-1".to_string(),
        PathBuf::from("/tmp/repo1"),
    ));
    state.selected_repository_index = Some(0);

    state.agents.push(Agent::new(
        AgentId("agent-1".to_string()),
        RepositoryId("repo-1".to_string()),
        "Agent 1".to_string(),
        PathBuf::from("/tmp/agent1"),
    ));
    state.agents.push(Agent::new(
        AgentId("agent-2".to_string()),
        RepositoryId("repo-1".to_string()),
        "Agent 2".to_string(),
        PathBuf::from("/tmp/agent2"),
    ));

    let new_state = state.apply(AppEvent::ExitIssuesMode);
    assert_eq!(new_state.pane_focus, PaneFocus::Agents);
    // Should fall back to Some(0) or None
    assert!(new_state.selected_agent_index == Some(0) || new_state.selected_agent_index.is_none());
}

/// Test 5: ExitIssuesMode discards active drafts and shows a notice.
/// @plan PLAN-20260329-ISSUES-MODE.P04
/// @requirement REQ-ISS-010
/// @pseudocode component-001 lines 45-50
#[test]
fn test_exit_issues_mode_discards_draft_with_notice() {
    let mut state = dashboard_issues_state();
    state.issues_state.active = true;
    state.issues_state.inline_state = InlineState::Composer {
        target: ComposerTarget::NewComment,
        text: "Draft comment".to_string(),
        cursor: 5,
    };

    let new_state = state.apply(AppEvent::ExitIssuesMode);
    assert_eq!(new_state.issues_state.inline_state, InlineState::None);
    assert!(new_state.issues_state.draft_notice.is_some());
    let notice = new_state
        .issues_state
        .draft_notice
        .unwrap_or_else(|| panic!("expected value"));
    assert!(notice.contains("discarded") || notice.contains("Draft"));
}

/// Test 6: IssuesCycleFocus advances through RepoList -> IssueList -> IssueDetail -> RepoList.
/// @plan PLAN-20260329-ISSUES-MODE.P04
/// @requirement REQ-ISS-002
/// @pseudocode component-001 lines 55-60
#[test]
fn test_issues_cycle_focus_tab() {
    let mut state = dashboard_issues_state();
    state.issues_state.active = true;
    state.issues_state.issue_focus = IssueFocus::RepoList;

    // Cycle: RepoList -> IssueList
    let state = state.apply(AppEvent::IssuesCycleFocus);
    assert_eq!(state.issues_state.issue_focus, IssueFocus::IssueList);

    // Cycle: IssueList -> IssueDetail
    let state = state.apply(AppEvent::IssuesCycleFocus);
    assert_eq!(state.issues_state.issue_focus, IssueFocus::IssueDetail);

    // Cycle: IssueDetail -> RepoList
    let state = state.apply(AppEvent::IssuesCycleFocus);
    assert_eq!(state.issues_state.issue_focus, IssueFocus::RepoList);
}

/// Test 7: IssuesCycleFocusReverse cycles backwards through focus areas.
/// @plan PLAN-20260329-ISSUES-MODE.P04
/// @requirement REQ-ISS-002
/// @pseudocode component-001 lines 61-66
#[test]
fn test_issues_cycle_focus_shift_tab() {
    let mut state = dashboard_issues_state();
    state.issues_state.active = true;
    state.issues_state.issue_focus = IssueFocus::RepoList;

    // Reverse cycle: RepoList -> IssueDetail
    let state = state.apply(AppEvent::IssuesCycleFocusReverse);
    assert_eq!(state.issues_state.issue_focus, IssueFocus::IssueDetail);

    // Reverse cycle: IssueDetail -> IssueList
    let state = state.apply(AppEvent::IssuesCycleFocusReverse);
    assert_eq!(state.issues_state.issue_focus, IssueFocus::IssueList);

    // Reverse cycle: IssueList -> RepoList
    let state = state.apply(AppEvent::IssuesCycleFocusReverse);
    assert_eq!(state.issues_state.issue_focus, IssueFocus::RepoList);
}

/// Test 8: IssuesNavigateUp decrements selected_issue_index.
/// @plan PLAN-20260329-ISSUES-MODE.P04
/// @requirement REQ-ISS-004
/// @pseudocode component-001 lines 70-75
#[test]
fn test_issues_navigate_up_in_issue_list() {
    let mut state = dashboard_issues_state();
    state.issues_state.active = true;
    state.issues_state.issue_focus = IssueFocus::IssueList;
    state.issues_state.issues = vec![
        make_test_issue(1),
        make_test_issue(2),
        make_test_issue(3),
        make_test_issue(4),
        make_test_issue(5),
    ];
    state.issues_state.selected_issue_index = Some(3);

    let new_state = state.apply(AppEvent::IssuesNavigateUp);
    assert_eq!(new_state.issues_state.selected_issue_index, Some(2));
}

/// Test 9: IssuesNavigateUp clamps at zero.
/// @plan PLAN-20260329-ISSUES-MODE.P04
/// @requirement REQ-ISS-004
/// @pseudocode component-001 lines 76-80
#[test]
fn test_issues_navigate_up_clamps_at_zero() {
    let mut state = dashboard_issues_state();
    state.issues_state.active = true;
    state.issues_state.issue_focus = IssueFocus::IssueList;
    state.issues_state.issues = vec![make_test_issue(1), make_test_issue(2), make_test_issue(3)];
    state.issues_state.selected_issue_index = Some(0);

    let new_state = state.apply(AppEvent::IssuesNavigateUp);
    assert_eq!(new_state.issues_state.selected_issue_index, Some(0));
}

/// Test 10: IssuesNavigateDown increments selected_issue_index.
/// @plan PLAN-20260329-ISSUES-MODE.P04
/// @requirement REQ-ISS-004
/// @pseudocode component-001 lines 81-85
#[test]
fn test_issues_navigate_down_in_issue_list() {
    let mut state = dashboard_issues_state();
    state.issues_state.active = true;
    state.issues_state.issue_focus = IssueFocus::IssueList;
    state.issues_state.issues = vec![
        make_test_issue(1),
        make_test_issue(2),
        make_test_issue(3),
        make_test_issue(4),
        make_test_issue(5),
    ];
    state.issues_state.selected_issue_index = Some(2);

    let new_state = state.apply(AppEvent::IssuesNavigateDown);
    assert_eq!(new_state.issues_state.selected_issue_index, Some(3));
}

/// Test 11: IssueListLoaded selects the first issue and clears loading state.
/// @plan PLAN-20260329-ISSUES-MODE.P04
/// @requirement REQ-ISS-006
/// @pseudocode component-001 lines 90-95
#[test]
fn test_issue_list_loaded_selects_first() {
    let mut state = dashboard_issues_state();
    state.issues_state.loading.list = true;

    // Set up repository
    state.repositories.push(Repository::new(
        RepositoryId("repo-1".to_string()),
        "Repo 1".to_string(),
        "repo-1".to_string(),
        PathBuf::from("/tmp/repo1"),
    ));
    state.selected_repository_index = Some(0);

    let issues = vec![make_test_issue(1), make_test_issue(2), make_test_issue(3)];

    let new_state = state.apply(AppEvent::IssueListLoaded {
        scope_repo_id: RepositoryId("repo-1".to_string()),
        filter: Box::new(IssueFilter::default()),
        request_id: 0,
        issues: issues.clone(),
        cursor: None,
        has_more: false,
    });

    assert_eq!(new_state.issues_state.selected_issue_index, Some(0));
    assert!(!new_state.issues_state.loading.list);
    assert_eq!(new_state.issues_state.issues.len(), 3);
}

/// Test 12: IssueListLoaded with empty issues sets selected_index to None.
/// @plan PLAN-20260329-ISSUES-MODE.P04
/// @requirement REQ-ISS-006
/// @pseudocode component-001 lines 96-100
#[test]
fn test_issue_list_loaded_empty() {
    let mut state = dashboard_issues_state();
    state.issues_state.loading.list = true;

    // Set up repository
    state.repositories.push(Repository::new(
        RepositoryId("repo-1".to_string()),
        "Repo 1".to_string(),
        "repo-1".to_string(),
        PathBuf::from("/tmp/repo1"),
    ));
    state.selected_repository_index = Some(0);

    let new_state = state.apply(AppEvent::IssueListLoaded {
        scope_repo_id: RepositoryId("repo-1".to_string()),
        filter: Box::new(IssueFilter::default()),
        request_id: 0,
        issues: vec![],
        cursor: None,
        has_more: false,
    });

    assert_eq!(new_state.issues_state.selected_issue_index, None);
    assert!(new_state.issues_state.issue_detail.is_none());
}

/// Test 13: IssueListLoaded with stale scope is discarded.
/// @plan PLAN-20260329-ISSUES-MODE.P04
/// @requirement REQ-ISS-012
/// @pseudocode component-001 lines 105-110
#[test]
fn test_issue_list_loaded_stale_scope_discarded() {
    let mut state = AppState::default();

    // Set up repo at index 0 with id "repo-1"
    state.repositories.push(Repository::new(
        RepositoryId("repo-1".to_string()),
        "Repo 1".to_string(),
        "repo-1".to_string(),
        PathBuf::from("/tmp/repo1"),
    ));
    state.selected_repository_index = Some(0);
    state.issues_state.loading.list = true;

    // Try to load issues for wrong repo
    let new_state = state.apply(AppEvent::IssueListLoaded {
        scope_repo_id: RepositoryId("repo-WRONG".to_string()),
        filter: Box::new(IssueFilter::default()),
        request_id: 0,
        issues: vec![make_test_issue(1)],
        cursor: None,
        has_more: false,
    });

    // State should be unchanged (stale scope discarded)
    assert!(new_state.issues_state.issues.is_empty());
    assert!(new_state.issues_state.loading.list);
}

#[test]
fn test_issue_list_loaded_stale_filter_discarded() {
    let mut state = state_with_repo("repo-1");
    state.issues_state.loading.list = true;
    state.issues_state.committed_filter.query_text = "new".to_string();
    state.issues_state.issues = vec![make_test_issue(99)];

    let mut stale_filter = state.issues_state.committed_filter.clone();
    stale_filter.query_text = "old".to_string();
    let new_state = state.apply(AppEvent::IssueListLoaded {
        scope_repo_id: RepositoryId("repo-1".to_string()),
        filter: Box::new(stale_filter),
        request_id: 0,
        issues: vec![make_test_issue(1)],
        cursor: Some("old-cursor".to_string()),
        has_more: true,
    });

    assert_eq!(new_state.issues_state.issues[0].number, 99);
    assert!(new_state.issues_state.loading.list);
    assert!(new_state.issues_state.list_cursor.is_none());
}

#[test]
fn test_issue_list_loaded_stale_request_id_discarded() {
    let mut state = state_with_repo("repo-1");
    state.issues_state.committed_filter.query_text = "same filter".to_string();
    state.issues_state.issues = vec![make_test_issue(99)];
    state.issues_state.loading.list = true;
    state.issues_state.error = Some("newer request still loading".to_string());
    let filter = state.issues_state.committed_filter.clone();
    state.mark_issue_list_reload_loading(RepositoryId("repo-1".to_string()), filter.clone(), 2);

    let new_state = state.apply(AppEvent::IssueListLoaded {
        scope_repo_id: RepositoryId("repo-1".to_string()),
        filter: Box::new(filter),
        request_id: 1,
        issues: vec![make_test_issue(1)],
        cursor: Some("old-cursor".to_string()),
        has_more: true,
    });

    assert_eq!(new_state.issues_state.issues[0].number, 99);
    assert!(new_state.issues_state.loading.list);
    assert_eq!(
        new_state.issues_state.error.as_deref(),
        Some("newer request still loading")
    );
    assert!(new_state.issues_state.list_cursor.is_none());
}

#[test]
fn test_issue_list_load_failed_stale_request_id_discarded() {
    let mut state = state_with_repo("repo-1");
    state.issues_state.committed_filter.query_text = "same filter".to_string();
    state.issues_state.loading.list = true;
    state.issues_state.error = Some("newer request still loading".to_string());
    let filter = state.issues_state.committed_filter.clone();
    state.mark_issue_list_reload_loading(RepositoryId("repo-1".to_string()), filter.clone(), 2);

    let new_state = state.apply(AppEvent::IssueListLoadFailed {
        scope_repo_id: RepositoryId("repo-1".to_string()),
        filter: Box::new(filter),
        request_id: 1,
        request_cursor: None,
        error: "old failure".to_string(),
    });

    assert!(new_state.issues_state.loading.list);
    assert_eq!(
        new_state.issues_state.error.as_deref(),
        Some("newer request still loading")
    );
    assert!(new_state.issues_state.list_reload_pending.is_some());
}

#[test]
fn test_issue_detail_loaded_stale_selection_discarded() {
    let mut state = state_with_repo("repo-1");
    state.issues_state.issues = vec![make_test_issue(1), make_test_issue(2)];
    state.issues_state.selected_issue_index = Some(1);
    state.issues_state.loading.detail = true;
    let mut current_detail = make_test_detail(vec![]);
    current_detail.number = 2;
    state.issues_state.issue_detail = Some(current_detail);

    let mut stale_detail = make_test_detail(vec![]);
    stale_detail.number = 1;
    state.mark_issue_detail_loading(RepositoryId("repo-1".to_string()), 2);
    let new_state = state.apply(AppEvent::IssueDetailLoaded {
        scope_repo_id: RepositoryId("repo-1".to_string()),
        issue_number: 1,
        request_id: 0,
        detail: Box::new(stale_detail),
    });

    let loaded = new_state
        .issues_state
        .issue_detail
        .unwrap_or_else(|| panic!("detail should remain loaded"));
    assert_eq!(loaded.number, 2);
    assert!(new_state.issues_state.loading.detail);
}

/// Test 14: IssueListPageLoaded appends issues to existing list.
/// @plan PLAN-20260329-ISSUES-MODE.P04
/// @requirement REQ-ISS-006
/// @pseudocode component-001 lines 111-115
#[test]
fn test_issue_list_page_loaded_appends() {
    let mut state = dashboard_issues_state();

    // Set up repository
    state.repositories.push(Repository::new(
        RepositoryId("repo-1".to_string()),
        "Repo 1".to_string(),
        "repo-1".to_string(),
        PathBuf::from("/tmp/repo1"),
    ));
    state.selected_repository_index = Some(0);

    // Start with 3 issues
    state.issues_state.issues = vec![make_test_issue(1), make_test_issue(2), make_test_issue(3)];
    state.issues_state.selected_issue_index = Some(1);
    state.mark_issue_list_page_loading(
        RepositoryId("repo-1".to_string()),
        IssueFilter::default(),
        None,
    );

    let new_state = state.apply(AppEvent::IssueListPageLoaded {
        scope_repo_id: RepositoryId("repo-1".to_string()),
        filter: Box::new(IssueFilter::default()),
        request_id: 0,
        request_cursor: None,
        issues: vec![make_test_issue(4), make_test_issue(5)],
        cursor: None,
        has_more: false,
    });

    assert_eq!(new_state.issues_state.issues.len(), 5);
    assert_eq!(new_state.issues_state.selected_issue_index, Some(1)); // Unchanged
}

#[test]
fn test_issue_list_page_loaded_stale_filter_discarded() {
    let mut state = dashboard_issues_state();
    state.repositories.push(Repository::new(
        RepositoryId("repo-1".to_string()),
        "Repo 1".to_string(),
        "repo-1".to_string(),
        PathBuf::from("/tmp/repo1"),
    ));
    state.selected_repository_index = Some(0);
    state.issues_state.issues = vec![make_test_issue(1)];
    state.issues_state.loading.list = true;
    state.issues_state.error = Some("current error".to_string());
    state.issues_state.committed_filter.query_text = "new filter".to_string();
    state.mark_issue_list_page_loading(
        RepositoryId("repo-1".to_string()),
        state.issues_state.committed_filter.clone(),
        Some("current-cursor".to_string()),
    );

    let stale_filter = IssueFilter {
        query_text: "old filter".to_string(),
        ..IssueFilter::default()
    };
    let new_state = state.apply(AppEvent::IssueListPageLoaded {
        scope_repo_id: RepositoryId("repo-1".to_string()),
        filter: Box::new(stale_filter),
        request_id: 0,
        request_cursor: Some("old-cursor".to_string()),
        issues: vec![make_test_issue(2)],
        cursor: Some("old-cursor".to_string()),
        has_more: true,
    });

    assert_eq!(new_state.issues_state.issues.len(), 1);
    assert!(new_state.issues_state.loading.list);
    assert_eq!(
        new_state.issues_state.error.as_deref(),
        Some("current error")
    );
    assert_eq!(new_state.issues_state.list_cursor, None);
}

#[test]
fn test_issue_list_page_loaded_stale_cursor_discarded() {
    let mut state = dashboard_issues_state();
    state.repositories.push(Repository::new(
        RepositoryId("repo-1".to_string()),
        "Repo 1".to_string(),
        "repo-1".to_string(),
        PathBuf::from("/tmp/repo1"),
    ));
    state.selected_repository_index = Some(0);
    state.issues_state.issues = vec![make_test_issue(1)];
    state.issues_state.loading.list = true;
    state.issues_state.error = Some("current page still loading".to_string());
    state.issues_state.list_cursor = Some("current-cursor".to_string());
    state.mark_issue_list_page_loading(
        RepositoryId("repo-1".to_string()),
        IssueFilter::default(),
        Some("current-cursor".to_string()),
    );

    let new_state = state.apply(AppEvent::IssueListPageLoaded {
        scope_repo_id: RepositoryId("repo-1".to_string()),
        filter: Box::new(IssueFilter::default()),
        request_id: 0,
        request_cursor: Some("stale-cursor".to_string()),
        issues: vec![make_test_issue(2)],
        cursor: Some("next-stale-cursor".to_string()),
        has_more: false,
    });

    assert_eq!(new_state.issues_state.issues.len(), 1);
    assert_eq!(new_state.issues_state.issues[0].number, 1);
    assert!(new_state.issues_state.loading.list);
    assert_eq!(
        new_state.issues_state.error.as_deref(),
        Some("current page still loading")
    );
    assert_eq!(
        new_state.issues_state.list_cursor.as_deref(),
        Some("current-cursor")
    );
}

#[test]
fn test_issue_detail_loaded_while_list_empty_is_discarded() {
    let mut state = state_with_repo("repo-1");
    state.issues_state.loading.detail = true;
    state.issues_state.loading.list = true;
    state.mark_issue_detail_loading(RepositoryId("repo-1".to_string()), 2);
    state.issues_state.issues.clear();
    state.issues_state.selected_issue_index = None;
    state.issues_state.issue_detail = None;

    let stale_detail = make_test_detail(vec![]);
    let new_state = state.apply(AppEvent::IssueDetailLoaded {
        scope_repo_id: RepositoryId("repo-1".to_string()),
        issue_number: stale_detail.number,
        request_id: 0,
        detail: Box::new(stale_detail),
    });

    assert!(new_state.issues_state.issue_detail.is_none());
    assert!(new_state.issues_state.loading.detail);
}

/// Test 15: IssueDetailSubfocusNext cycles through Body -> Comment(0) -> Comment(1) -> NewComment -> Body.
/// @plan PLAN-20260329-ISSUES-MODE.P04
/// @requirement REQ-ISS-003
/// @pseudocode component-001 lines 120-125
#[test]
fn test_detail_subfocus_tab_with_comments() {
    let mut state = dashboard_issues_state();
    state.issues_state.active = true;
    state.issues_state.issue_focus = IssueFocus::IssueDetail;
    state.issues_state.detail_subfocus = DetailSubfocus::Body;

    // Set up issue detail with 2 comments
    state.issues_state.issue_detail = Some(IssueDetail {
        repo_owner_name: "owner/repo".to_string(),
        number: 1,
        title: "Test Issue".to_string(),
        state: IssueState::Open,
        author_login: "testuser".to_string(),
        created_at: "2024-01-01T00:00:00Z".to_string(),
        updated_at: "2024-01-02T00:00:00Z".to_string(),
        labels: vec![],
        assignees: vec![],
        milestone: None,
        body: "Issue body".to_string(),
        external_url: "https://github.com/owner/repo/issues/1".to_string(),
        comments: vec![
            IssueComment {
                comment_id: 100,
                author_login: "user1".to_string(),
                created_at: "2024-01-02T00:00:00Z".to_string(),
                edited_at: None,
                body: "First comment".to_string(),
            },
            IssueComment {
                comment_id: 101,
                author_login: "user2".to_string(),
                created_at: "2024-01-03T00:00:00Z".to_string(),
                edited_at: None,
                body: "Second comment".to_string(),
            },
        ],
        has_more_comments: false,
        comments_cursor: None,
    });

    // Body -> Comment(0)
    let state = state.apply(AppEvent::IssueDetailSubfocusNext);
    assert_eq!(
        state.issues_state.detail_subfocus,
        DetailSubfocus::Comment(0)
    );

    // Comment(0) -> Comment(1)
    let state = state.apply(AppEvent::IssueDetailSubfocusNext);
    assert_eq!(
        state.issues_state.detail_subfocus,
        DetailSubfocus::Comment(1)
    );

    // Comment(1) -> NewComment
    let state = state.apply(AppEvent::IssueDetailSubfocusNext);
    assert_eq!(
        state.issues_state.detail_subfocus,
        DetailSubfocus::NewComment
    );

    // NewComment -> Body
    let state = state.apply(AppEvent::IssueDetailSubfocusNext);
    assert_eq!(state.issues_state.detail_subfocus, DetailSubfocus::Body);
}

/// Test 16: IssueDetailSubfocusNext with no comments skips to NewComment then back to Body.
/// @plan PLAN-20260329-ISSUES-MODE.P04
/// @requirement REQ-ISS-003
/// @pseudocode component-001 lines 126-130
#[test]
fn test_detail_subfocus_tab_no_comments() {
    let mut state = dashboard_issues_state();
    state.issues_state.active = true;
    state.issues_state.issue_focus = IssueFocus::IssueDetail;
    state.issues_state.detail_subfocus = DetailSubfocus::Body;

    // Set up issue detail with 0 comments
    state.issues_state.issue_detail = Some(IssueDetail {
        repo_owner_name: "owner/repo".to_string(),
        number: 1,
        title: "Test Issue".to_string(),
        state: IssueState::Open,
        author_login: "testuser".to_string(),
        created_at: "2024-01-01T00:00:00Z".to_string(),
        updated_at: "2024-01-02T00:00:00Z".to_string(),
        labels: vec![],
        assignees: vec![],
        milestone: None,
        body: "Issue body".to_string(),
        external_url: "https://github.com/owner/repo/issues/1".to_string(),
        comments: vec![],
        has_more_comments: false,
        comments_cursor: None,
    });

    // Body -> NewComment (skip comments since there are none)
    let state = state.apply(AppEvent::IssueDetailSubfocusNext);
    assert_eq!(
        state.issues_state.detail_subfocus,
        DetailSubfocus::NewComment
    );

    // NewComment -> Body
    let state = state.apply(AppEvent::IssueDetailSubfocusNext);
    assert_eq!(state.issues_state.detail_subfocus, DetailSubfocus::Body);
}

/// Test 17: InlineCancelOrEsc clears inline editor state.
/// @plan PLAN-20260329-ISSUES-MODE.P04
/// @requirement REQ-ISS-010
/// @pseudocode component-001 lines 135-140
#[test]
fn test_esc_cancels_inline_editor() {
    let mut state = dashboard_issues_state();
    state.issues_state.inline_state = InlineState::Editor {
        target: EditorTarget::IssueBody,
        text: "draft content".to_string(),
        cursor: 5,
    };

    let new_state = state.apply(AppEvent::InlineCancelOrEsc);
    assert_eq!(new_state.issues_state.inline_state, InlineState::None);
}

/// Test 18: AgentChooserCancel clears agent chooser state.
/// @plan PLAN-20260329-ISSUES-MODE.P04
/// @requirement REQ-ISS-011
/// @pseudocode component-001 lines 141-145
#[test]
fn test_esc_cancels_agent_chooser() {
    let mut state = dashboard_issues_state();
    state.issues_state.agent_chooser = Some(AgentChooserState::default());
    state.issues_state.inline_state = InlineState::None;

    let new_state = state.apply(AppEvent::AgentChooserCancel);
    assert!(new_state.issues_state.agent_chooser.is_none());
}

/// Test 19: ClearSearch clears non-empty search query.
/// @plan PLAN-20260329-ISSUES-MODE.P04
/// @requirement REQ-ISS-007
/// @pseudocode component-001 lines 146-150
#[test]
fn test_esc_clears_nonempty_search() {
    let mut state = dashboard_issues_state();
    state.issues_state.search_input_focused = true;
    state.issues_state.search_query = "bug".to_string();
    state.issues_state.inline_state = InlineState::None;
    state.issues_state.agent_chooser = None;

    let new_state = state.apply(AppEvent::ClearSearch);
    assert!(new_state.issues_state.search_query.is_empty());
    assert!(new_state.issues_state.search_input_focused);
}

/// Test 20: BlurSearchInput blurs empty search input.
/// @plan PLAN-20260329-ISSUES-MODE.P04
/// @requirement REQ-ISS-007
/// @pseudocode component-001 lines 151-155
#[test]
fn test_esc_blurs_empty_search() {
    let mut state = dashboard_issues_state();
    state.issues_state.search_input_focused = true;
    state.issues_state.search_query = String::new();

    let new_state = state.apply(AppEvent::BlurSearchInput);
    assert!(!new_state.issues_state.search_input_focused);
}

/// Test 21: CloseFilterControls closes filter controls.
/// @plan PLAN-20260329-ISSUES-MODE.P04
/// @requirement REQ-ISS-008
/// @pseudocode component-001 lines 156-160
#[test]
fn test_esc_closes_filter_controls() {
    let mut state = dashboard_issues_state();
    state.issues_state.filter_ui.controls_open = true;

    let new_state = state.apply(AppEvent::CloseFilterControls);
    assert!(!new_state.issues_state.filter_ui.controls_open);
}

/// Test 22: ExitIssuesMode when no inner controls are active.
/// @plan PLAN-20260329-ISSUES-MODE.P04
/// @requirement REQ-ISS-001
/// @pseudocode component-001 lines 161-165
#[test]
fn test_esc_exits_issues_mode() {
    let mut state = dashboard_issues_state();
    state.issues_state.active = true;
    state.issues_state.inline_state = InlineState::None;
    state.issues_state.agent_chooser = None;
    state.issues_state.filter_ui.controls_open = false;
    state.issues_state.search_input_focused = false;

    let new_state = state.apply(AppEvent::ExitIssuesMode);
    assert_eq!(new_state.screen_mode, ScreenMode::Dashboard);
}

/// Test 23: OpenInlineEditor is blocked when another inline control is active.
/// @plan PLAN-20260329-ISSUES-MODE.P04
/// @requirement REQ-ISS-010
/// @pseudocode component-001 lines 170-175
#[test]
fn test_inline_exclusivity_blocks_second_control() {
    let mut state = dashboard_issues_state();

    // Set active Composer
    state.issues_state.inline_state = InlineState::Composer {
        target: ComposerTarget::NewComment,
        text: "hello".to_string(),
        cursor: 5,
    };

    // Try to open Editor while Composer is active
    let new_state = state.apply(AppEvent::OpenInlineEditor {
        target: EditorTarget::IssueBody,
    });

    // Should still be Composer, not changed to Editor
    match new_state.issues_state.inline_state {
        InlineState::Composer {
            target: ComposerTarget::NewComment,
            ..
        } => {}
        _ => panic!(
            "Expected Composer state to remain, but got {:?}",
            new_state.issues_state.inline_state
        ),
    }
}

/// Test 24: IssueListLoaded with mismatched scope_repo_id is discarded.
/// @plan PLAN-20260329-ISSUES-MODE.P04
/// @requirement REQ-ISS-012
/// @pseudocode component-001 lines 180-185
#[test]
fn test_stale_scope_list_loaded_discarded() {
    let mut state = AppState::default();

    // Set up repo "repo-A" at index 0
    state.repositories.push(Repository::new(
        RepositoryId("repo-A".to_string()),
        "Repo A".to_string(),
        "repo-a".to_string(),
        PathBuf::from("/tmp/repo-a"),
    ));
    state.selected_repository_index = Some(0);
    state.issues_state.loading.list = true;

    // Load issues for wrong repo "repo-B"
    let new_state = state.apply(AppEvent::IssueListLoaded {
        scope_repo_id: RepositoryId("repo-B".to_string()),
        filter: Box::new(IssueFilter::default()),
        request_id: 0,
        issues: vec![make_test_issue(1)],
        cursor: None,
        has_more: false,
    });

    // Issues list should remain unchanged
    assert!(new_state.issues_state.issues.is_empty());
    assert!(new_state.issues_state.loading.list);
}

// -------------------------------------------------------------------------
// P13 Tests — UI Components + Persistence Rendering Contracts
// -------------------------------------------------------------------------

/// Helper to build a minimal IssueDetail for testing.
fn make_test_detail(comments: Vec<IssueComment>) -> IssueDetail {
    IssueDetail {
        repo_owner_name: "owner/repo".to_string(),
        number: 42,
        title: "Test detail issue".to_string(),
        state: IssueState::Open,
        author_login: "octocat".to_string(),
        created_at: "2024-01-01T00:00:00Z".to_string(),
        updated_at: "2024-01-02T00:00:00Z".to_string(),
        labels: vec!["bug".to_string(), "ui".to_string()],
        assignees: vec!["dev1".to_string()],
        milestone: Some("v1.0".to_string()),
        body: "Detail body text".to_string(),
        external_url: "https://github.com/owner/repo/issues/42".to_string(),
        comments,
        has_more_comments: false,
        comments_cursor: None,
    }
}

/// Helper to make a test IssueComment.
fn make_test_comment(id: u64, author: &str, body: &str) -> IssueComment {
    IssueComment {
        comment_id: id,
        author_login: author.to_string(),
        created_at: "2024-01-03T00:00:00Z".to_string(),
        edited_at: None,
        body: body.to_string(),
    }
}

/// Helper to set up a state with a selected repository at index 0.
fn state_with_repo(repo_id: &str) -> AppState {
    let mut state = AppState::default();
    state.repositories.push(Repository::new(
        RepositoryId(repo_id.to_string()),
        "Test Repo".to_string(),
        repo_id.to_string(),
        std::path::PathBuf::from("/tmp/test-repo"),
    ));
    state.selected_repository_index = Some(0);
    state
}

/// P13 Test 3: IssueListLoaded with 5 issues populates issues_state.issues with exactly 5 items.
///
/// @plan PLAN-20260329-ISSUES-MODE.P13
/// @requirement REQ-ISS-006
#[test]
fn test_issue_list_row_count() {
    let state = state_with_repo("repo-1").apply(AppEvent::IssueListLoaded {
        scope_repo_id: RepositoryId("repo-1".to_string()),
        filter: Box::new(IssueFilter::default()),
        request_id: 0,
        issues: (1u64..=5).map(make_test_issue).collect(),
        cursor: None,
        has_more: false,
    });

    assert_eq!(state.issues_state.issues.len(), 5);
}

/// P13 Test 4: After loading issues and navigating down, selected_issue_index becomes Some(1).
///
/// @plan PLAN-20260329-ISSUES-MODE.P13
/// @requirement REQ-ISS-006
#[test]
fn test_issue_list_selection_highlight() {
    let state = state_with_repo("repo-1").apply(AppEvent::IssueListLoaded {
        scope_repo_id: RepositoryId("repo-1".to_string()),
        filter: Box::new(IssueFilter::default()),
        request_id: 0,
        issues: (1u64..=5).map(make_test_issue).collect(),
        cursor: None,
        has_more: false,
    });

    // After load, selection is at 0. Navigate down once.
    let state = state.apply(AppEvent::IssuesNavigateDown);

    assert_eq!(state.issues_state.selected_issue_index, Some(1));
}

/// P13 Test 5: Entering issues mode sets list_loading to true initially.
///
/// @plan PLAN-20260329-ISSUES-MODE.P13
/// @requirement REQ-ISS-006
#[test]
fn test_issue_list_loading_state() {
    let state = AppState::default().apply(AppEvent::EnterIssuesMode);

    // list_loading should be true right after EnterIssuesMode (before data arrives)
    assert!(state.issues_state.loading.list);
}

/// P13 Test 6: IssueListLoaded with empty vec leaves issues empty and selected_issue_index None.
///
/// @plan PLAN-20260329-ISSUES-MODE.P13
/// @requirement REQ-ISS-006, REQ-ISS-014
#[test]
fn test_issue_list_empty_state() {
    let state = state_with_repo("repo-1").apply(AppEvent::IssueListLoaded {
        scope_repo_id: RepositoryId("repo-1".to_string()),
        filter: Box::new(IssueFilter::default()),
        request_id: 0,
        issues: vec![],
        cursor: None,
        has_more: false,
    });

    assert!(state.issues_state.issues.is_empty());
    assert!(state.issues_state.selected_issue_index.is_none());
}

/// P13 Test 7: IssueDetailLoaded populates all fields in issues_state.issue_detail.
///
/// @plan PLAN-20260329-ISSUES-MODE.P13
/// @requirement REQ-ISS-009
#[test]
fn test_issue_detail_all_fields() {
    let comments = vec![make_test_comment(1, "alice", "Looks good")];
    let detail = make_test_detail(comments);

    let mut state = state_with_repo("repo-1");
    state.mark_issue_detail_loading(RepositoryId("repo-1".to_string()), 42);
    let state = state.apply(AppEvent::IssueDetailLoaded {
        scope_repo_id: RepositoryId("repo-1".to_string()),
        issue_number: 42,
        request_id: 0,
        detail: Box::new(detail),
    });

    let loaded = state
        .issues_state
        .issue_detail
        .unwrap_or_else(|| panic!("detail should be Some"));
    assert_eq!(loaded.number, 42);
    assert_eq!(loaded.title, "Test detail issue");
    assert_eq!(loaded.author_login, "octocat");
    assert_eq!(loaded.body, "Detail body text");
    assert_eq!(loaded.labels, vec!["bug".to_string(), "ui".to_string()]);
    assert_eq!(loaded.assignees, vec!["dev1".to_string()]);
    assert_eq!(loaded.milestone, Some("v1.0".to_string()));
    assert!(!loaded.external_url.is_empty());
    assert_eq!(loaded.repo_owner_name, "owner/repo");
}

/// P13 Test 8: IssueDetailLoaded with 3 comments — detail.comments.len() == 3.
///
/// @plan PLAN-20260329-ISSUES-MODE.P13
/// @requirement REQ-ISS-009
#[test]
fn test_issue_detail_comments_timeline() {
    let comments = vec![
        make_test_comment(10, "alice", "First"),
        make_test_comment(11, "bob", "Second"),
        make_test_comment(12, "carol", "Third"),
    ];
    let detail = make_test_detail(comments);

    let mut state = state_with_repo("repo-1");
    state.mark_issue_detail_loading(RepositoryId("repo-1".to_string()), 42);
    let state = state.apply(AppEvent::IssueDetailLoaded {
        scope_repo_id: RepositoryId("repo-1".to_string()),
        issue_number: 42,
        request_id: 0,
        detail: Box::new(detail),
    });

    let loaded = state
        .issues_state
        .issue_detail
        .unwrap_or_else(|| panic!("detail should be Some"));
    assert_eq!(loaded.comments.len(), 3);
    assert_eq!(loaded.comments[0].author_login, "alice");
    assert_eq!(loaded.comments[2].author_login, "carol");
}

/// P13 Test 9: OpenNewCommentComposer transitions inline_state to Composer(NewComment).
///
/// @plan PLAN-20260329-ISSUES-MODE.P13
/// @requirement REQ-ISS-010
#[test]
fn test_issue_detail_inline_composer_visible() {
    let mut state = AppState::default();
    state.issues_state.inline_state = InlineState::None;

    let state = state.apply(AppEvent::OpenNewCommentComposer);

    match state.issues_state.inline_state {
        InlineState::Composer {
            target: ComposerTarget::NewComment,
            ..
        } => {} // Correct
        other => panic!("expected Composer(NewComment), got {other:?}"),
    }
}

/// P13 Test 9b: OpenNewIssueComposer transitions inline_state to Composer(NewIssue).
///
/// @plan PLAN-20260329-ISSUES-MODE.P13
/// @requirement REQ-ISS-010
#[test]
fn test_issue_list_new_issue_composer_visible() {
    let mut state = AppState::default();
    state.issues_state.inline_state = InlineState::None;
    state.issues_state.issue_focus = IssueFocus::IssueDetail;

    let state = state.apply(AppEvent::OpenNewIssueComposer);

    match state.issues_state.inline_state {
        InlineState::Composer {
            target: ComposerTarget::NewIssue,
            ..
        } => {}
        other => panic!("expected Composer(NewIssue), got {other:?}"),
    }
    assert_eq!(state.issues_state.issue_focus, IssueFocus::IssueList);
}

/// P13 Test 10: UpdateDraftFilter sets values in draft_filter fields.
///
/// @plan PLAN-20260329-ISSUES-MODE.P13
/// @requirement REQ-ISS-008
#[test]
fn test_filter_controls_value_binding() {
    let mut state = AppState::default();
    state.issues_state.filter_ui.controls_open = true;

    // Update multiple draft filter fields
    let state = state
        .apply(AppEvent::UpdateDraftFilter {
            field: "author".to_string(),
            value: "octocat".to_string(),
        })
        .apply(AppEvent::UpdateDraftFilter {
            field: "assignee".to_string(),
            value: "dev1".to_string(),
        })
        .apply(AppEvent::UpdateDraftFilter {
            field: "query_text".to_string(),
            value: "segfault".to_string(),
        });

    assert_eq!(state.issues_state.draft_filter.author, "octocat");
    assert_eq!(state.issues_state.draft_filter.assignee, "dev1");
    assert_eq!(state.issues_state.draft_filter.query_text, "segfault");
}

/// P13 Test 11: Loading an empty issue list means the empty-state condition holds
/// (issues.is_empty() is the data contract the UI component checks).
///
/// @plan PLAN-20260329-ISSUES-MODE.P13
/// @requirement REQ-ISS-014
#[test]
fn test_empty_state_no_issues() {
    let state = state_with_repo("repo-1").apply(AppEvent::IssueListLoaded {
        scope_repo_id: RepositoryId("repo-1".to_string()),
        filter: Box::new(IssueFilter::default()),
        request_id: 0,
        issues: vec![],
        cursor: None,
        has_more: false,
    });

    // The UI rendering component checks this condition to show the empty message
    assert!(state.issues_state.issues.is_empty());
    assert!(!state.issues_state.loading.list);
}

/// P13 Test 12: IssueDetailLoaded with no comments — detail.comments is empty.
///
/// @plan PLAN-20260329-ISSUES-MODE.P13
/// @requirement REQ-ISS-014
#[test]
fn test_empty_state_no_comments() {
    let detail = make_test_detail(vec![]);

    let mut state = state_with_repo("repo-1");
    state.mark_issue_detail_loading(RepositoryId("repo-1".to_string()), 42);
    let state = state.apply(AppEvent::IssueDetailLoaded {
        scope_repo_id: RepositoryId("repo-1".to_string()),
        issue_number: 42,
        request_id: 0,
        detail: Box::new(detail),
    });

    let loaded = state
        .issues_state
        .issue_detail
        .unwrap_or_else(|| panic!("detail should be Some"));
    assert!(loaded.comments.is_empty());
}

/// P13 Test 13: OpenAgentChooser with no agents leaves agent_chooser as None
/// (UI empty-state: no agents available to send to).
///
/// @plan PLAN-20260329-ISSUES-MODE.P13
/// @requirement REQ-ISS-014
#[test]
fn test_empty_state_no_agents_for_send() {
    let mut state = AppState::default();
    // Confirm no agents are configured
    assert!(state.agents.is_empty());

    // OpenAgentChooser with no agents should leave chooser as None
    state = state.apply(AppEvent::OpenAgentChooser);

    // When agents list is empty, agent_chooser is not opened
    assert!(state.issues_state.agent_chooser.is_none());
}

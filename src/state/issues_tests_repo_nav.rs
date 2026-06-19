use crate::domain::{Issue, IssueDetail, IssueState, Repository, RepositoryId};
use crate::state::AppState;
use crate::state::types::{AppEvent, IssueFocus, PaneFocus, ScreenMode};

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

/// Helper: create a minimal IssueDetail with given number and empty comments.
fn make_detail(number: u64) -> IssueDetail {
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

/// Issue #47: Repo navigation in issues mode must work regardless of pane_focus.
///
/// When pane_focus is Agents or Terminal (not Repositories), Up/Down in the
/// RepoList focus should still navigate between repositories.
#[test]
fn test_issues_repo_navigation_independent_of_pane_focus() {
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
    state.repositories.push(Repository::new(
        RepositoryId("r3".to_string()),
        "R3".to_string(),
        "r3".to_string(),
        std::path::PathBuf::from("/tmp/r3"),
    ));
    state.selected_repository_index = Some(0);
    state.issues_state.active = true;
    state.issues_state.issue_focus = IssueFocus::RepoList;

    // Set pane_focus to Agents (the bug scenario — not Repositories)
    state.pane_focus = PaneFocus::Agents;

    // Down should move to repo index 1 even though pane_focus is Agents
    let state = state.apply(AppEvent::IssuesNavigateDown);
    assert_eq!(state.selected_repository_index, Some(1));
    assert!(
        state.issues_state.loading.list,
        "issues should reload for new repo"
    );

    // Down again to repo index 2
    let state = state.apply(AppEvent::IssuesNavigateDown);
    assert_eq!(state.selected_repository_index, Some(2));

    // Down at bottom should stay
    let state = state.apply(AppEvent::IssuesNavigateDown);
    assert_eq!(state.selected_repository_index, Some(2));

    // Up should move back to repo index 1
    let state = state.apply(AppEvent::IssuesNavigateUp);
    assert_eq!(state.selected_repository_index, Some(1));
    assert!(
        state.issues_state.loading.list,
        "issues should reload for new repo"
    );

    // Up again to repo index 0
    let state = state.apply(AppEvent::IssuesNavigateUp);
    assert_eq!(state.selected_repository_index, Some(0));

    // Up at top should stay
    let state = state.apply(AppEvent::IssuesNavigateUp);
    assert_eq!(state.selected_repository_index, Some(0));
}

/// Issue #47: Navigating repos in issues mode with pane_focus=Terminal.
#[test]
fn test_issues_repo_navigation_with_terminal_focus() {
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
    state.pane_focus = PaneFocus::Terminal;

    let state = state.apply(AppEvent::IssuesNavigateDown);
    assert_eq!(state.selected_repository_index, Some(1));
}

/// Issue #47: Repo change in issues mode resets issues state (clears list, detail, etc.).
#[test]
fn test_issues_repo_navigation_resets_issues_state() {
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
    state.issues_state.issues = vec![make_test_issue(1), make_test_issue(2)];
    state.issues_state.selected_issue_index = Some(1);
    state.issues_state.issue_detail = Some(make_detail(1));
    state.issues_state.loading.list = false;
    state.pane_focus = PaneFocus::Agents;

    let state = state.apply(AppEvent::IssuesNavigateDown);
    assert_eq!(state.selected_repository_index, Some(1));
    assert!(
        state.issues_state.issues.is_empty(),
        "issues should be cleared"
    );
    assert_eq!(state.issues_state.selected_issue_index, None);
    assert!(
        state.issues_state.issue_detail.is_none(),
        "detail should be cleared"
    );
    assert!(
        state.issues_state.loading.list,
        "list_loading should be set for new fetch"
    );
}

use crate::domain::{Issue, IssueFilter, IssueFilterState, IssueState, Repository, RepositoryId};
use crate::state::AppState;
use crate::state::types::{AppEvent, ScreenMode};

fn dashboard_issues_state() -> AppState {
    AppState {
        screen_mode: ScreenMode::DashboardIssues,
        ..AppState::default()
    }
}

/// Helper: enter issues mode with filter controls open.
fn filter_open_state() -> AppState {
    let mut state = dashboard_issues_state();
    state.issues_state.active = true;
    state.issues_state.filter_ui.controls_open = true;
    state.issues_state.filter_ui.field_index = 0;
    state
}

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

fn state_with_repo() -> AppState {
    let mut state = filter_open_state();
    state.repositories.push(Repository::new(
        RepositoryId("repo-1".to_string()),
        "Repo 1".to_string(),
        "repo-1".to_string(),
        std::path::PathBuf::from("/tmp/repo1"),
    ));
    state.selected_repository_index = Some(0);
    state
}

/// OpenFilterControls resets filter_field_index to 0.
#[test]
fn test_open_filter_resets_field_index() {
    let mut state = dashboard_issues_state();
    state.issues_state.active = true;
    state.issues_state.filter_ui.field_index = 3;

    let state = state.apply(AppEvent::OpenFilterControls);
    assert!(state.issues_state.filter_ui.controls_open);
    assert_eq!(state.issues_state.filter_ui.field_index, 0);
}

/// FilterNavigateNext cycles through fields 0..4.
#[test]
fn test_filter_navigate_next_cycles() {
    let state = filter_open_state();
    assert_eq!(state.issues_state.filter_ui.field_index, 0);

    let state = state.apply(AppEvent::FilterNavigateNext);
    assert_eq!(state.issues_state.filter_ui.field_index, 1);

    let state = state.apply(AppEvent::FilterNavigateNext);
    assert_eq!(state.issues_state.filter_ui.field_index, 2);

    let state = state.apply(AppEvent::FilterNavigateNext);
    assert_eq!(state.issues_state.filter_ui.field_index, 3);

    let state = state.apply(AppEvent::FilterNavigateNext);
    assert_eq!(state.issues_state.filter_ui.field_index, 4);

    // Wraps around
    let state = state.apply(AppEvent::FilterNavigateNext);
    assert_eq!(state.issues_state.filter_ui.field_index, 0);
}

/// FilterNavigatePrev cycles backward through fields.
#[test]
fn test_filter_navigate_prev_cycles() {
    let state = filter_open_state();
    assert_eq!(state.issues_state.filter_ui.field_index, 0);

    // Wraps to last field
    let state = state.apply(AppEvent::FilterNavigatePrev);
    assert_eq!(state.issues_state.filter_ui.field_index, 4);

    let state = state.apply(AppEvent::FilterNavigatePrev);
    assert_eq!(state.issues_state.filter_ui.field_index, 3);
}

/// CycleFilterState cycles through Open -> Closed -> All -> Open.
#[test]
fn test_cycle_filter_state() {
    let state = filter_open_state();
    // Default is None (treated as Open)
    assert!(state.issues_state.draft_filter.state.is_none());

    let state = state.apply(AppEvent::CycleFilterState);
    assert_eq!(
        state.issues_state.draft_filter.state,
        Some(IssueFilterState::Closed)
    );

    let state = state.apply(AppEvent::CycleFilterState);
    assert_eq!(
        state.issues_state.draft_filter.state,
        Some(IssueFilterState::All)
    );

    let state = state.apply(AppEvent::CycleFilterState);
    assert_eq!(
        state.issues_state.draft_filter.state,
        Some(IssueFilterState::Open)
    );
}

/// UpdateDraftFilter with "labels" field parses comma-separated values.
#[test]
fn test_update_draft_filter_labels() {
    let state = filter_open_state();

    let state = state.apply(AppEvent::UpdateDraftFilter {
        field: "labels".to_string(),
        value: "bug,enhancement".to_string(),
    });
    assert_eq!(
        state.issues_state.draft_filter.labels,
        vec!["bug", "enhancement"]
    );

    // Empty value clears labels
    let state = state.apply(AppEvent::UpdateDraftFilter {
        field: "labels".to_string(),
        value: String::new(),
    });
    assert!(state.issues_state.draft_filter.labels.is_empty());
}

/// ApplyFilter commits draft to committed, closes controls, and marks for reload.
#[test]
fn test_apply_filter_commits_and_reloads() {
    let mut state = filter_open_state();
    state.issues_state.draft_filter.author = "alice".to_string();
    state.issues_state.loading.list = false;

    let state = state.apply(AppEvent::ApplyFilter);
    assert!(!state.issues_state.filter_ui.controls_open);
    assert_eq!(state.issues_state.committed_filter.author, "alice");
    assert!(state.issues_state.loading.list, "should trigger reload");
    assert!(state.issues_state.issues.is_empty());
}

/// ClearFilter resets both committed and draft, closes controls, and marks for reload.
#[test]
fn test_clear_filter_resets_and_reloads() {
    let mut state = filter_open_state();
    state.issues_state.draft_filter.author = "bob".to_string();
    state.issues_state.committed_filter.author = "bob".to_string();
    state.issues_state.loading.list = false;

    let state = state.apply(AppEvent::ClearFilter);
    assert!(!state.issues_state.filter_ui.controls_open);
    assert!(state.issues_state.committed_filter.author.is_empty());
    assert!(state.issues_state.draft_filter.author.is_empty());
    assert!(state.issues_state.loading.list, "should trigger reload");
}

/// UpdateDraftFilter for text fields works as expected.
#[test]
fn test_apply_filter_fresh_list_loaded_selects_first_issue() {
    let mut state = state_with_repo();
    state.issues_state.draft_filter.author = "alice".to_string();

    let state = state.apply(AppEvent::ApplyFilter);
    assert!(state.issues_state.loading.list);
    assert_eq!(state.issues_state.selected_issue_index, None);
    let committed_filter = state.issues_state.committed_filter.clone();

    let state = state.apply(AppEvent::IssueListLoaded {
        scope_repo_id: RepositoryId("repo-1".to_string()),
        filter: Box::new(committed_filter),
        request_id: 0,
        issues: vec![make_test_issue(1), make_test_issue(2)],
        cursor: Some("next".to_string()),
        has_more: true,
    });

    assert!(!state.issues_state.loading.list);
    assert_eq!(state.issues_state.selected_issue_index, Some(0));
    assert_eq!(state.issues_state.issues.len(), 2);
}

#[test]
fn test_clear_filter_fresh_list_loaded_selects_first_issue() {
    let mut state = state_with_repo();
    state.issues_state.draft_filter.author = "bob".to_string();
    state.issues_state.committed_filter.author = "bob".to_string();

    let state = state.apply(AppEvent::ClearFilter);
    assert!(state.issues_state.loading.list);
    assert_eq!(state.issues_state.committed_filter, IssueFilter::default());

    let state = state.apply(AppEvent::IssueListLoaded {
        scope_repo_id: RepositoryId("repo-1".to_string()),
        filter: Box::new(IssueFilter::default()),
        request_id: 0,
        issues: vec![make_test_issue(3)],
        cursor: None,
        has_more: false,
    });

    assert!(!state.issues_state.loading.list);
    assert_eq!(state.issues_state.selected_issue_index, Some(0));
    assert_eq!(state.issues_state.issues.len(), 1);
}
/// ApplyFilter must invalidate any in-flight detail/comments requests so a
/// late response for the previous filter cannot overwrite the reloaded list.
#[test]
fn test_apply_filter_clears_stale_detail_and_comment_pending() {
    use crate::state::types::{IssueCommentsPagePending, IssueDetailPending};

    let mut state = state_with_repo();
    state.issues_state.loading.detail = true;
    state.issues_state.loading.comments = true;
    state.issues_state.detail_pending = Some(IssueDetailPending {
        scope_repo_id: RepositoryId("repo-1".to_string()),
        issue_number: 7,
        request_id: 1,
    });
    state.issues_state.comments_page_pending = Some(IssueCommentsPagePending {
        scope_repo_id: RepositoryId("repo-1".to_string()),
        issue_number: 7,
        cursor: None,
        request_id: 1,
    });

    let state = state.apply(AppEvent::ApplyFilter);

    assert!(!state.issues_state.loading.detail);
    assert!(!state.issues_state.loading.comments);
    assert!(state.issues_state.detail_pending.is_none());
    assert!(state.issues_state.comments_page_pending.is_none());
    assert!(state.issues_state.issue_detail.is_none());
}

/// ApplySearch must likewise invalidate in-flight detail/comments requests.
#[test]
fn test_apply_search_clears_stale_detail_pending() {
    use crate::messages::IssuesMessage;
    use crate::state::types::IssueDetailPending;

    let mut state = state_with_repo();
    state.issues_state.search_query = "needle".to_string();
    state.issues_state.loading.detail = true;
    state.issues_state.detail_pending = Some(IssueDetailPending {
        scope_repo_id: RepositoryId("repo-1".to_string()),
        issue_number: 9,
        request_id: 2,
    });

    let state = state.apply(AppEvent::from(IssuesMessage::ApplySearch));

    assert!(!state.issues_state.loading.detail);
    assert!(state.issues_state.detail_pending.is_none());
    assert!(state.issues_state.issue_detail.is_none());
}

#[test]
fn test_update_draft_filter_text_fields() {
    let state = filter_open_state()
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
            value: "crash".to_string(),
        });

    assert_eq!(state.issues_state.draft_filter.author, "octocat");
    assert_eq!(state.issues_state.draft_filter.assignee, "dev1");
    assert_eq!(state.issues_state.draft_filter.query_text, "crash");
}

/// Simulate sequential label keystrokes and verify round-trip through state.
/// Typing b,u,g,comma,u,i should produce labels (bug, ui)
/// while preserving the raw text in draft_labels_text.
#[test]
fn test_labels_sequential_typing_round_trip() {
    let mut state = filter_open_state();
    state.issues_state.filter_ui.field_index = 3; // labels field

    // Simulate typing "bug,ui" one character at a time
    for ch in ['b', 'u', 'g', ',', 'u', 'i'] {
        let raw = state.issues_state.filter_ui.draft_labels_text.clone();
        let mut value = raw;
        value.push(ch);
        state = state.apply(AppEvent::UpdateDraftFilter {
            field: "labels".to_string(),
            value,
        });
    }

    assert_eq!(state.issues_state.filter_ui.draft_labels_text, "bug,ui");
    assert_eq!(
        state.issues_state.draft_filter.labels,
        vec!["bug".to_string(), "ui".to_string()]
    );
}

/// Trailing comma in labels is preserved in draft_labels_text during editing.
#[test]
fn test_labels_trailing_comma_preserved() {
    let mut state = filter_open_state();
    state.issues_state.filter_ui.field_index = 3;

    // Type "bug,"
    for ch in ['b', 'u', 'g', ','] {
        let mut value = state.issues_state.filter_ui.draft_labels_text.clone();
        value.push(ch);
        state = state.apply(AppEvent::UpdateDraftFilter {
            field: "labels".to_string(),
            value,
        });
    }

    // Raw text preserves trailing comma
    assert_eq!(state.issues_state.filter_ui.draft_labels_text, "bug,");
    // Parsed labels only has "bug" (no empty segment)
    assert_eq!(
        state.issues_state.draft_filter.labels,
        vec!["bug".to_string()]
    );
}

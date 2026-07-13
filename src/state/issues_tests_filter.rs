use crate::domain::{Issue, IssueFilter, IssueFilterState, IssueState, Repository, RepositoryId};
use crate::state::events::AppEvent;
use crate::state::types::ScreenMode;
use crate::state::{AppState, ISSUE_FILTER_FIELD_COUNT};

use super::issues_test_fixtures::begin_issue_list_reload;

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
        node_id: String::new(),
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

/// OpenFilterControls preserves the live filter_field_index (issue #163: the
/// cursor remembers its last position, clamped to the valid field range).
#[test]
fn test_open_filter_preserves_field_index() {
    let mut state = dashboard_issues_state();
    state.issues_state.active = true;
    state.issues_state.filter_ui.field_index = 3;

    let state = state.apply(AppEvent::OpenFilterControls);
    assert!(state.issues_state.filter_ui.controls_open);
    assert_eq!(state.issues_state.filter_ui.field_index, 3);
}

/// OpenFilterControls clamps an out-of-range field_index back into bounds.
#[test]
fn test_open_filter_clamps_out_of_range_field_index() {
    let mut state = dashboard_issues_state();
    state.issues_state.active = true;
    state.issues_state.filter_ui.field_index = ISSUE_FILTER_FIELD_COUNT + 5;

    let state = state.apply(AppEvent::OpenFilterControls);
    assert_eq!(
        state.issues_state.filter_ui.field_index,
        ISSUE_FILTER_FIELD_COUNT - 1,
        "out-of-range field_index must clamp to the last valid index"
    );
}

/// FilterNavigateNext cycles through every configured filter field.
#[test]
fn test_filter_navigate_next_cycles() {
    let mut state = filter_open_state();
    assert_eq!(state.issues_state.filter_ui.field_index, 0);

    for expected in 1..ISSUE_FILTER_FIELD_COUNT {
        state = state.apply(AppEvent::FilterNavigateNext);
        assert_eq!(state.issues_state.filter_ui.field_index, expected);
    }

    let state = state.apply(AppEvent::FilterNavigateNext);
    assert_eq!(state.issues_state.filter_ui.field_index, 0);
}

/// FilterNavigatePrev cycles backward through every configured filter field.
#[test]
fn test_filter_navigate_prev_cycles() {
    let state = filter_open_state();
    assert_eq!(state.issues_state.filter_ui.field_index, 0);

    let state = state.apply(AppEvent::FilterNavigatePrev);
    assert_eq!(
        state.issues_state.filter_ui.field_index,
        ISSUE_FILTER_FIELD_COUNT - 1
    );

    let state = state.apply(AppEvent::FilterNavigatePrev);
    assert_eq!(
        state.issues_state.filter_ui.field_index,
        ISSUE_FILTER_FIELD_COUNT - 2
    );
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
    state.issues_state.draft_filter.issue_type = "bug".to_string();
    state.issues_state.draft_filter.milestone = "v1".to_string();
    state.issues_state.draft_filter.module = "ui".to_string();

    let state = state.apply(AppEvent::ApplyFilter);
    assert!(!state.issues_state.filter_ui.controls_open);
    assert_eq!(state.issues_state.committed_filter.author, "alice");
    assert_eq!(state.issues_state.committed_filter.issue_type, "bug");
    assert_eq!(state.issues_state.committed_filter.milestone, "v1");
    assert_eq!(state.issues_state.committed_filter.module, "ui");
    assert!(!state.issues_state.list_loading());
    assert!(state.issues_state.issues().is_empty());
}

/// ClearFilter resets both committed and draft, closes controls, and marks for reload.
#[test]
fn test_clear_filter_resets_and_reloads() {
    let mut state = filter_open_state();
    state.issues_state.draft_filter.author = "bob".to_string();
    state.issues_state.committed_filter.author = "bob".to_string();

    let state = state.apply(AppEvent::ClearFilter);
    assert!(!state.issues_state.filter_ui.controls_open);
    assert!(state.issues_state.committed_filter.author.is_empty());
    assert!(state.issues_state.draft_filter.author.is_empty());
    assert!(!state.issues_state.list_loading());
}

fn populate_all_draft_filter_fields(state: AppState) -> AppState {
    state
        .apply(AppEvent::CycleFilterState)
        .apply(AppEvent::UpdateDraftFilter {
            field: "author".to_string(),
            value: "alice".to_string(),
        })
        .apply(AppEvent::UpdateDraftFilter {
            field: "assignee".to_string(),
            value: "none".to_string(),
        })
        .apply(AppEvent::UpdateDraftFilter {
            field: "labels".to_string(),
            value: "bug,module:ui".to_string(),
        })
        .apply(AppEvent::UpdateDraftFilter {
            field: "issue_type".to_string(),
            value: "Bug".to_string(),
        })
        .apply(AppEvent::UpdateDraftFilter {
            field: "milestone".to_string(),
            value: "Sprint 1".to_string(),
        })
        .apply(AppEvent::UpdateDraftFilter {
            field: "module".to_string(),
            value: "ui".to_string(),
        })
        .apply(AppEvent::UpdateDraftFilter {
            field: "query_text".to_string(),
            value: "panic".to_string(),
        })
        .apply(AppEvent::UpdateDraftFilter {
            field: "mentioned".to_string(),
            value: "carol".to_string(),
        })
        .apply(AppEvent::UpdateDraftFilter {
            field: "updated_before".to_string(),
            value: "2026-07-01".to_string(),
        })
        .apply(AppEvent::UpdateDraftFilter {
            field: "updated_after".to_string(),
            value: "2026-06-01".to_string(),
        })
}

fn assert_all_draft_filter_fields_populated(filter: &IssueFilter) {
    assert_eq!(filter.state, Some(IssueFilterState::Closed));
    assert_eq!(filter.author, "alice");
    assert_eq!(filter.assignee, "none");
    assert_eq!(
        filter.labels,
        vec!["bug".to_string(), "module:ui".to_string()]
    );
    assert_eq!(filter.issue_type, "Bug");
    assert_eq!(filter.milestone, "Sprint 1");
    assert_eq!(filter.module, "ui");
    assert_eq!(filter.query_text, "panic");
    assert_eq!(filter.mentioned, "carol");
    assert_eq!(filter.updated_before, "2026-07-01");
    assert_eq!(filter.updated_after, "2026-06-01");
}

/// ClearDraftFilter from filter controls clears the draft form but keeps controls open.
#[test]
fn test_clear_draft_filter_keeps_controls_open_and_resets_draft() {
    let mut initial = filter_open_state();
    initial.issues_state.committed_filter.author = "committed-author".to_string();
    initial.issues_state.filter_ui.field_index = 5;

    let populated = populate_all_draft_filter_fields(initial);
    assert_all_draft_filter_fields_populated(&populated.issues_state.draft_filter);

    let state = populated.apply(AppEvent::ClearDraftFilter);

    // ClearDraftFilter resets the draft to the Open default (issue #163).
    assert_eq!(
        state.issues_state.draft_filter,
        IssueFilter {
            state: Some(IssueFilterState::Open),
            ..IssueFilter::default()
        }
    );
    assert_eq!(
        state.issues_state.committed_filter.author,
        "committed-author"
    );
    assert_eq!(state.screen_mode, ScreenMode::DashboardIssues);
    assert!(state.issues_state.active);
    assert!(state.issues_state.filter_ui.controls_open);
    assert_eq!(state.issues_state.filter_ui.field_index, 5);
    assert!(!state.issues_state.list_loading());
    assert!(state.issues_state.filter_ui.draft_labels_text.is_empty());
}

/// UpdateDraftFilter for text fields works as expected.
#[test]
fn test_apply_filter_fresh_list_loaded_selects_first_issue() {
    let mut state = state_with_repo();
    state.issues_state.draft_filter.author = "alice".to_string();

    let mut state = state.apply(AppEvent::ApplyFilter);
    assert!(!state.issues_state.list_loading());
    assert_eq!(state.issues_state.selected_issue_index(), None);
    let committed_filter = state.issues_state.committed_filter.clone();
    let request_id = begin_issue_list_reload(&mut state, "repo-1", committed_filter.clone());

    let state = state.apply(AppEvent::IssueListLoaded {
        scope_repo_id: RepositoryId("repo-1".to_string()),
        filter: Box::new(committed_filter),
        request_id,
        issues: vec![make_test_issue(1), make_test_issue(2)],
        cursor: Some("next".to_string()),
        has_more: true,
    });

    assert!(!state.issues_state.list_loading());
    assert_eq!(state.issues_state.selected_issue_index(), Some(0));
    assert_eq!(state.issues_state.issues().len(), 2);
}

#[test]
fn test_clear_filter_fresh_list_loaded_selects_first_issue() {
    let mut state = state_with_repo();
    state.issues_state.draft_filter.author = "bob".to_string();
    state.issues_state.committed_filter.author = "bob".to_string();

    let mut state = state.apply(AppEvent::ClearFilter);
    assert!(!state.issues_state.list_loading());
    assert_eq!(
        state.issues_state.committed_filter,
        IssueFilter {
            state: Some(IssueFilterState::Open),
            ..IssueFilter::default()
        }
    );
    let open_filter = IssueFilter {
        state: Some(IssueFilterState::Open),
        ..IssueFilter::default()
    };
    let request_id = begin_issue_list_reload(&mut state, "repo-1", open_filter.clone());

    let state = state.apply(AppEvent::IssueListLoaded {
        scope_repo_id: RepositoryId("repo-1".to_string()),
        filter: Box::new(open_filter),
        request_id,
        issues: vec![make_test_issue(3)],
        cursor: None,
        has_more: false,
    });

    assert!(!state.issues_state.list_loading());
    assert_eq!(state.issues_state.selected_issue_index(), Some(0));
    assert_eq!(state.issues_state.issues().len(), 1);
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

/// Applying a search preserves literal free-text, including `any`.
#[test]
fn test_apply_search_preserves_literal_any_query() {
    use crate::messages::IssuesMessage;

    let mut state = state_with_repo();
    state.issues_state.search_query = "ANY".to_string();
    state.issues_state.committed_filter.query_text = "old".to_string();

    let state = state.apply(AppEvent::from(IssuesMessage::ApplySearch));

    assert_eq!(state.issues_state.committed_filter.query_text, "ANY");
    assert!(!state.issues_state.list_loading());
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

/// Updating extended draft filter fields sets each field independently.
#[test]
fn test_update_draft_filter_extended_fields() {
    let state = filter_open_state()
        .apply(AppEvent::UpdateDraftFilter {
            field: "issue_type".to_string(),
            value: "bug".to_string(),
        })
        .apply(AppEvent::UpdateDraftFilter {
            field: "milestone".to_string(),
            value: "v1".to_string(),
        })
        .apply(AppEvent::UpdateDraftFilter {
            field: "module".to_string(),
            value: "ui".to_string(),
        });

    assert_eq!(state.issues_state.draft_filter.issue_type, "bug");
    assert_eq!(state.issues_state.draft_filter.milestone, "v1");
    assert_eq!(state.issues_state.draft_filter.module, "ui");
}

/// Updating the state draft field with an unknown value preserves the current state.
#[test]
fn test_update_draft_filter_state_ignores_unknown_value() {
    let state = filter_open_state()
        .apply(AppEvent::UpdateDraftFilter {
            field: "state".to_string(),
            value: "all".to_string(),
        })
        .apply(AppEvent::UpdateDraftFilter {
            field: "state".to_string(),
            value: "invalid".to_string(),
        });

    assert_eq!(
        state.issues_state.draft_filter.state,
        Some(IssueFilterState::All)
    );
}
#[test]
fn test_update_draft_filter_state_clears_to_default() {
    let state =
        filter_open_state()
            .apply(AppEvent::CycleFilterState)
            .apply(AppEvent::UpdateDraftFilter {
                field: "state".to_string(),
                value: String::new(),
            });

    assert_eq!(state.issues_state.draft_filter.state, None);
}

/// Updating the state draft field to all preserves the explicit all filter.
#[test]
fn test_update_draft_filter_state_all_stays_explicit() {
    let state = filter_open_state().apply(AppEvent::UpdateDraftFilter {
        field: "state".to_string(),
        value: "all".to_string(),
    });

    assert_eq!(
        state.issues_state.draft_filter.state,
        Some(IssueFilterState::All)
    );
}

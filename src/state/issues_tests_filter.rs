use crate::domain::IssueFilterState;
use crate::state::AppState;
use crate::state::types::{AppEvent, ScreenMode};

/// Helper: enter issues mode with filter controls open.
fn filter_open_state() -> AppState {
    let mut state = AppState::default();
    state.screen_mode = ScreenMode::DashboardIssues;
    state.issues_state.active = true;
    state.issues_state.filter_controls_open = true;
    state.issues_state.filter_field_index = 0;
    state
}

/// OpenFilterControls resets filter_field_index to 0.
#[test]
fn test_open_filter_resets_field_index() {
    let mut state = AppState::default();
    state.screen_mode = ScreenMode::DashboardIssues;
    state.issues_state.active = true;
    state.issues_state.filter_field_index = 3;

    let state = state.apply(AppEvent::OpenFilterControls);
    assert!(state.issues_state.filter_controls_open);
    assert_eq!(state.issues_state.filter_field_index, 0);
}

/// FilterNavigateNext cycles through fields 0..4.
#[test]
fn test_filter_navigate_next_cycles() {
    let state = filter_open_state();
    assert_eq!(state.issues_state.filter_field_index, 0);

    let state = state.apply(AppEvent::FilterNavigateNext);
    assert_eq!(state.issues_state.filter_field_index, 1);

    let state = state.apply(AppEvent::FilterNavigateNext);
    assert_eq!(state.issues_state.filter_field_index, 2);

    let state = state.apply(AppEvent::FilterNavigateNext);
    assert_eq!(state.issues_state.filter_field_index, 3);

    let state = state.apply(AppEvent::FilterNavigateNext);
    assert_eq!(state.issues_state.filter_field_index, 4);

    // Wraps around
    let state = state.apply(AppEvent::FilterNavigateNext);
    assert_eq!(state.issues_state.filter_field_index, 0);
}

/// FilterNavigatePrev cycles backward through fields.
#[test]
fn test_filter_navigate_prev_cycles() {
    let state = filter_open_state();
    assert_eq!(state.issues_state.filter_field_index, 0);

    // Wraps to last field
    let state = state.apply(AppEvent::FilterNavigatePrev);
    assert_eq!(state.issues_state.filter_field_index, 4);

    let state = state.apply(AppEvent::FilterNavigatePrev);
    assert_eq!(state.issues_state.filter_field_index, 3);
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
    state.issues_state.list_loading = false;

    let state = state.apply(AppEvent::ApplyFilter);
    assert!(!state.issues_state.filter_controls_open);
    assert_eq!(state.issues_state.committed_filter.author, "alice");
    assert!(state.issues_state.list_loading, "should trigger reload");
    assert!(state.issues_state.issues.is_empty());
}

/// ClearFilter resets both committed and draft, closes controls, and marks for reload.
#[test]
fn test_clear_filter_resets_and_reloads() {
    let mut state = filter_open_state();
    state.issues_state.draft_filter.author = "bob".to_string();
    state.issues_state.committed_filter.author = "bob".to_string();
    state.issues_state.list_loading = false;

    let state = state.apply(AppEvent::ClearFilter);
    assert!(!state.issues_state.filter_controls_open);
    assert!(state.issues_state.committed_filter.author.is_empty());
    assert!(state.issues_state.draft_filter.author.is_empty());
    assert!(state.issues_state.list_loading, "should trigger reload");
}

/// UpdateDraftFilter for text fields works as expected.
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
    state.issues_state.filter_field_index = 3; // labels field

    // Simulate typing "bug,ui" one character at a time
    for ch in ['b', 'u', 'g', ',', 'u', 'i'] {
        let raw = state.issues_state.draft_labels_text.clone();
        let mut value = raw;
        value.push(ch);
        state = state.apply(AppEvent::UpdateDraftFilter {
            field: "labels".to_string(),
            value,
        });
    }

    assert_eq!(state.issues_state.draft_labels_text, "bug,ui");
    assert_eq!(
        state.issues_state.draft_filter.labels,
        vec!["bug".to_string(), "ui".to_string()]
    );
}

/// Trailing comma in labels is preserved in draft_labels_text during editing.
#[test]
fn test_labels_trailing_comma_preserved() {
    let mut state = filter_open_state();
    state.issues_state.filter_field_index = 3;

    // Type "bug,"
    for ch in ['b', 'u', 'g', ','] {
        let mut value = state.issues_state.draft_labels_text.clone();
        value.push(ch);
        state = state.apply(AppEvent::UpdateDraftFilter {
            field: "labels".to_string(),
            value,
        });
    }

    // Raw text preserves trailing comma
    assert_eq!(state.issues_state.draft_labels_text, "bug,");
    // Parsed labels only has "bug" (no empty segment)
    assert_eq!(
        state.issues_state.draft_filter.labels,
        vec!["bug".to_string()]
    );
}

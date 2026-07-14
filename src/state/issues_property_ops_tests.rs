//! Tests for `issues_property_ops.rs` (extracted to keep that file under the
//! per-file line limit).

use super::*;
use crate::domain::{IssueDetail, IssueState, RepositoryId};
use crate::state::IssuesState;

fn make_state_with_detail() -> AppState {
    let detail = IssueDetail {
        repo_owner_name: "owner/repo".to_string(),
        number: 42,
        node_id: String::new(),
        title: "Test Issue".to_string(),
        state: IssueState::Open,
        author_login: "alice".to_string(),
        created_at: "2024-01-01".to_string(),
        updated_at: "2024-01-02".to_string(),
        labels: vec!["bug".to_string()],
        assignees: vec!["alice".to_string()],
        milestone: Some("v1.0".to_string()),
        issue_type_name: None,
        body: "body".to_string(),
        external_url: "url".to_string(),
        comments: crate::domain::PaginatedList::default(),
    };
    AppState {
        issues_state: IssuesState {
            active: true,
            issue_focus: IssueFocus::IssueDetail,
            issue_detail: Some(detail),
            ..IssuesState::default()
        },
        ..AppState::default()
    }
}

fn add_repo(state: &mut AppState) {
    state.repositories.push(crate::domain::Repository::new(
        RepositoryId("r1".to_string()),
        "repo".to_string(),
        "owner/repo".to_string(),
        std::path::PathBuf::from("/tmp/repo"),
    ));
    state.selected_repository_index = Some(0);
}

#[test]
fn full_detail_load_preserves_issue_type_from_list_row() {
    let mut state = make_state_with_detail();
    add_repo(&mut state);
    let issue = crate::domain::Issue {
        number: 42,
        node_id: String::new(),
        title: "Test Issue".to_string(),
        state: IssueState::Open,
        author_login: "alice".to_string(),
        updated_at: "2024-01-02".to_string(),
        assignee_summary: String::new(),
        labels_summary: String::new(),
        assignees: Vec::new(),
        labels: Vec::new(),
        issue_type: "Bug".to_string(),
        milestone: String::new(),
        module: String::new(),
        comment_count: 0,
        body: String::new(),
    };
    state.issues_state.list.replace_items(vec![issue]);
    state.issues_state.list.set_selected_index(Some(0));
    state.mark_issue_detail_loading(RepositoryId("r1".to_string()), 42);
    let request_id = state
        .issues_state
        .detail_pending
        .as_ref()
        .map_or(0, |pending| pending.request_id);
    let mut detail = state
        .issues_state
        .issue_detail
        .take()
        .unwrap_or_else(|| panic!("fixture detail should exist"));
    detail.issue_type_name = None;

    let state = state.apply(AppEvent::IssueDetailLoaded {
        scope_repo_id: RepositoryId("r1".to_string()),
        issue_number: 42,
        request_id,
        detail: Box::new(detail),
    });
    let state = state.apply(AppEvent::IssueOpenPropertyEditor {
        kind: IssuePropertyKind::Type,
    });
    let Some(editor) = state.issues_state.property_editor.as_ref() else {
        panic!("type editor should open");
    };
    assert!(
        editor
            .options
            .iter()
            .any(|option| option.label == "Bug" && option.selected)
    );
}

fn issue_row_with_type(issue_type: &str) -> crate::domain::Issue {
    crate::domain::Issue {
        number: 42,
        node_id: String::new(),
        title: "Test Issue".to_string(),
        state: IssueState::Open,
        author_login: "alice".to_string(),
        updated_at: "2024-01-02".to_string(),
        assignee_summary: String::new(),
        labels_summary: String::new(),
        assignees: Vec::new(),
        labels: Vec::new(),
        issue_type: issue_type.to_string(),
        milestone: String::new(),
        module: String::new(),
        comment_count: 0,
        body: String::new(),
    }
}

fn issue_type_refresh_state(initial_type: &str) -> AppState {
    let mut state = make_state_with_detail();
    add_repo(&mut state);
    state
        .issues_state
        .list
        .replace_items(vec![issue_row_with_type(initial_type)]);
    state.issues_state.list.set_selected_index(Some(0));
    state.issues_state.list.begin_silent_reload(
        crate::state::IssueListIdentity {
            scope_repo_id: RepositoryId("r1".to_string()),
            filter: crate::domain::IssueFilter::default(),
        },
        crate::domain::ListRequestId::from_raw(7),
    );
    state.mark_issue_detail_silent_loading(RepositoryId("r1".to_string()), 42, 8);
    state
}

fn apply_issue_type_refreshes(
    mut state: AppState,
    refreshed_type: &str,
    detail_first: bool,
) -> AppState {
    let mut detail = state
        .issues_state
        .issue_detail
        .clone()
        .unwrap_or_else(|| panic!("fixture detail should exist"));
    detail.issue_type_name = None;
    let detail_event = AppEvent::IssueDetailSilentRefreshed {
        scope_repo_id: RepositoryId("r1".to_string()),
        issue_number: 42,
        request_id: 8,
        detail: Box::new(detail),
    };
    let list_event = AppEvent::IssueListSilentRefreshed {
        scope_repo_id: RepositoryId("r1".to_string()),
        filter: Box::new(crate::domain::IssueFilter::default()),
        request_id: 7,
        issues: vec![issue_row_with_type(refreshed_type)],
        cursor: None,
        has_more: false,
    };
    if detail_first {
        state = state.apply(detail_event);
        state.apply(list_event)
    } else {
        state = state.apply(list_event);
        state.apply(detail_event)
    }
}

fn assert_refreshed_issue_type(initial: &str, refreshed: &str, detail_first: bool) {
    let state =
        apply_issue_type_refreshes(issue_type_refresh_state(initial), refreshed, detail_first);
    let actual = state
        .issues_state
        .issue_detail
        .as_ref()
        .and_then(|detail| detail.issue_type_name.as_deref());
    let expected = (!refreshed.is_empty()).then_some(refreshed);
    assert_eq!(actual, expected);
}

#[test]
fn issue_type_set_converges_when_detail_finishes_before_list() {
    assert_refreshed_issue_type("", "Bug", true);
}

#[test]
fn issue_type_set_converges_when_list_finishes_before_detail() {
    assert_refreshed_issue_type("", "Bug", false);
}

#[test]
fn issue_type_clear_converges_when_detail_finishes_before_list() {
    assert_refreshed_issue_type("Bug", "", true);
}

#[test]
fn issue_type_clear_converges_when_list_finishes_before_detail() {
    assert_refreshed_issue_type("Bug", "", false);
}

fn require_issue_editor(state: &AppState) -> &IssuePropertyEditorState {
    state
        .issues_state
        .property_editor
        .as_ref()
        .unwrap_or_else(|| panic!("expected property editor to be open"))
}

fn open_editor_with_load_request_id(state: AppState, kind: IssuePropertyKind) -> (AppState, u64) {
    let state = state.apply(AppEvent::IssueOpenPropertyEditor { kind });
    let load_request_id = require_issue_editor(&state).load_request_id;
    (state, load_request_id)
}

// ── H1: Title editing tests ─────────────────────────────────────────

#[test]
fn title_char_insert() {
    let mut state = make_state_with_detail();
    state = state.apply(AppEvent::IssueOpenPropertyEditor {
        kind: IssuePropertyKind::Title,
    });
    state = state.apply(AppEvent::IssuePropertyEditorTitleChar('X'));
    let editor = require_issue_editor(&state);
    assert_eq!(editor.title_text, "XTest Issue");
    assert_eq!(editor.title_cursor, 1);
}

#[test]
fn title_multibyte_char_insert() {
    let mut state = make_state_with_detail();
    state = state.apply(AppEvent::IssueOpenPropertyEditor {
        kind: IssuePropertyKind::Title,
    });
    state = state.apply(AppEvent::IssuePropertyEditorTitleChar('é'));
    let editor = require_issue_editor(&state);
    assert_eq!(editor.title_text, "éTest Issue");
    assert_eq!(editor.title_cursor, 2);
}

#[test]
fn title_backspace() {
    let mut state = make_state_with_detail();
    state = state.apply(AppEvent::IssueOpenPropertyEditor {
        kind: IssuePropertyKind::Title,
    });
    // Move cursor to end, then backspace removes the trailing char.
    let title_len = require_issue_editor(&state).title_text.len();
    for _ in 0..title_len {
        state = state.apply(AppEvent::IssuePropertyEditorTitleCursorRight);
    }
    state = state.apply(AppEvent::IssuePropertyEditorTitleBackspace);
    let editor = require_issue_editor(&state);
    assert_eq!(editor.title_text, "Test Issu");
}

#[test]
fn title_delete() {
    let mut state = make_state_with_detail();
    state = state.apply(AppEvent::IssueOpenPropertyEditor {
        kind: IssuePropertyKind::Title,
    });
    // Cursor at 0, delete removes 'T'
    state = state.apply(AppEvent::IssuePropertyEditorTitleDelete);
    let editor = require_issue_editor(&state);
    assert_eq!(editor.title_text, "est Issue");
}

#[test]
fn title_cursor_move() {
    let mut state = make_state_with_detail();
    state = state.apply(AppEvent::IssueOpenPropertyEditor {
        kind: IssuePropertyKind::Title,
    });
    state = state.apply(AppEvent::IssuePropertyEditorTitleCursorRight);
    let editor = require_issue_editor(&state);
    assert_eq!(editor.title_cursor, 1);
    state = state.apply(AppEvent::IssuePropertyEditorTitleCursorLeft);
    let editor = require_issue_editor(&state);
    assert_eq!(editor.title_cursor, 0);
}

// ── H3: Single-select uses selected_index ──────────────────────────

#[test]
fn single_select_state_down_then_highlights_closed() {
    let mut state = make_state_with_detail();
    state = state.apply(AppEvent::IssueOpenPropertyEditor {
        kind: IssuePropertyKind::State,
    });
    state = state.apply(AppEvent::IssuePropertyEditorNavigateDown);
    let editor = require_issue_editor(&state);
    assert_eq!(editor.selected_index, 1);
    // The highlighted option is "Closed", not "Open"
    assert_eq!(editor.options[1].label, "Closed");
}

// ── H4: Mutation pending tests ──────────────────────────────────────

#[test]
fn double_confirm_debounced() {
    let mut state = make_state_with_detail();
    add_repo(&mut state);
    let Some(rid) = state.mark_issue_property_mutation_pending(RepositoryId("r1".to_string()), 42)
    else {
        panic!("first confirm should allocate request_id")
    };
    // Second confirm while pending should return None (debounced)
    let second = state.mark_issue_property_mutation_pending(RepositoryId("r1".to_string()), 42);
    assert!(second.is_none(), "second confirm should be debounced");
    let _ = rid;
}

#[test]
fn stale_completion_ignored() {
    let mut state = make_state_with_detail();
    add_repo(&mut state);
    // Open editor and mark pending with request_id 0.
    state = state.apply(AppEvent::IssueOpenPropertyEditor {
        kind: IssuePropertyKind::Labels,
    });
    let Some(rid) = state.mark_issue_property_mutation_pending(RepositoryId("r1".to_string()), 42)
    else {
        panic!("confirm should allocate request_id")
    };
    // Cancel the editor (pending token stays for late-failure correlation).
    state = state.apply(AppEvent::IssuePropertyEditorCancel);
    assert!(state.issues_state.property_editor.is_none());
    // A late SUCCESS after cancel is applied silently (pending cleared, no
    // crash, editor stays closed).
    state = state.apply(AppEvent::IssuePropertyEditSucceeded {
        scope_repo_id: RepositoryId("r1".to_string()),
        issue_number: 42,
        kind: IssuePropertyKind::Labels,
        request_id: rid,
    });
    assert!(state.issues_state.property_editor.is_none());
    assert!(state.issues_state.property_mutation_pending.is_none());
}

#[test]
fn out_of_order_completion_ignored() {
    let mut state = make_state_with_detail();
    add_repo(&mut state);
    state = state.apply(AppEvent::IssueOpenPropertyEditor {
        kind: IssuePropertyKind::Labels,
    });
    let Some(rid) = state.mark_issue_property_mutation_pending(RepositoryId("r1".to_string()), 42)
    else {
        panic!("confirm should allocate request_id")
    };
    // Wrong request_id should be ignored
    state = state.apply(AppEvent::IssuePropertyEditSucceeded {
        scope_repo_id: RepositoryId("r1".to_string()),
        issue_number: 42,
        kind: IssuePropertyKind::Labels,
        request_id: rid + 100,
    });
    assert!(state.issues_state.property_editor.is_some());
}

// ── H5: Options failed keeps existing options ───────────────────────

#[test]
fn options_failed_keeps_existing_milestone() {
    let mut state = make_state_with_detail();
    add_repo(&mut state);
    let (mut state, load_rid) =
        open_editor_with_load_request_id(state, IssuePropertyKind::Milestone);
    // The editor should have the existing milestone selected
    let editor = require_issue_editor(&state);
    assert!(
        editor
            .options
            .iter()
            .any(|o| o.label == "v1.0" && o.selected)
    );
    // Simulate options fetch failure
    state = state.apply(AppEvent::IssuePropertyEditorOptionsFailed {
        scope_repo_id: RepositoryId("r1".to_string()),
        issue_number: 42,
        kind: IssuePropertyKind::Milestone,
        request_id: load_rid,
        error: "network error".to_string(),
    });
    let editor = require_issue_editor(&state);
    // Options should NOT be replaced with empty
    assert!(
        editor
            .options
            .iter()
            .any(|o| o.label == "v1.0" && o.selected)
    );
    assert!(editor.loading_failed);
    assert_eq!(editor.error.as_deref(), Some("network error"));
    let _ = load_rid;
}

// ── M6: Options load correlation ────────────────────────────────────

#[test]
fn stale_options_response_ignored() {
    let mut state = make_state_with_detail();
    add_repo(&mut state);
    let (mut state, labels_rid) =
        open_editor_with_load_request_id(state, IssuePropertyKind::Labels);
    // Cancel labels editor
    state = state.apply(AppEvent::IssuePropertyEditorCancel);
    // Open milestone editor (gets a new load_request_id)
    let (mut state, ms_rid) = open_editor_with_load_request_id(state, IssuePropertyKind::Milestone);
    // Stale labels response arrives — should be ignored
    state = state.apply(AppEvent::IssuePropertyEditorOptionsLoaded {
        scope_repo_id: RepositoryId("r1".to_string()),
        issue_number: 42,
        kind: IssuePropertyKind::Labels,
        request_id: labels_rid,
        options: vec![(None, "stale".to_string(), false)],
    });
    let editor = require_issue_editor(&state);
    // Milestone options should NOT contain "stale"
    assert!(!editor.options.iter().any(|o| o.label == "stale"));
    let _ = ms_rid;
}

// ── M7: Repo change clears property editor ──────────────────────────

#[test]
fn repo_change_clears_property_editor() {
    let mut state = make_state_with_detail();
    state = state.apply(AppEvent::IssueOpenPropertyEditor {
        kind: IssuePropertyKind::Labels,
    });
    assert!(state.issues_state.property_editor.is_some());
    state.reset_issues_for_repo_change();
    assert!(state.issues_state.property_editor.is_none());
    assert!(state.issues_state.property_mutation_pending.is_none());
}

// ── Existing tests (updated signatures) ─────────────────────────────

#[test]
fn open_property_editor_labels() {
    let mut state = make_state_with_detail();
    state = state.apply(AppEvent::IssueOpenPropertyEditor {
        kind: IssuePropertyKind::Labels,
    });
    let editor = require_issue_editor(&state);
    assert_eq!(editor.kind, IssuePropertyKind::Labels);
    assert_eq!(editor.options.len(), 1);
    assert!(editor.options[0].selected);
    // M8: baseline should be set
    assert_eq!(editor.baseline, vec!["bug".to_string()]);
}

#[test]
fn open_property_editor_title_prepopulates() {
    let mut state = make_state_with_detail();
    state = state.apply(AppEvent::IssueOpenPropertyEditor {
        kind: IssuePropertyKind::Title,
    });
    let editor = require_issue_editor(&state);
    assert_eq!(editor.title_text, "Test Issue");
}

#[test]
fn navigate_wraps() {
    let mut state = make_state_with_detail();
    state = state.apply(AppEvent::IssueOpenPropertyEditor {
        kind: IssuePropertyKind::Labels,
    });
    state = state.apply(AppEvent::IssuePropertyEditorNavigateUp);
    let editor = require_issue_editor(&state);
    assert_eq!(editor.selected_index, 0);
}

#[test]
fn toggle_labels_flips_selected() {
    let mut state = make_state_with_detail();
    state = state.apply(AppEvent::IssueOpenPropertyEditor {
        kind: IssuePropertyKind::Labels,
    });
    state = state.apply(AppEvent::IssuePropertyEditorToggle);
    let editor = require_issue_editor(&state);
    assert!(!editor.options[0].selected);
}

#[test]
fn cancel_closes_editor() {
    let mut state = make_state_with_detail();
    state = state.apply(AppEvent::IssueOpenPropertyEditor {
        kind: IssuePropertyKind::Labels,
    });
    state = state.apply(AppEvent::IssuePropertyEditorCancel);
    assert!(state.issues_state.property_editor.is_none());
}

#[test]
fn succeeded_clears_editor() {
    let mut state = make_state_with_detail();
    add_repo(&mut state);
    state = state.apply(AppEvent::IssueOpenPropertyEditor {
        kind: IssuePropertyKind::Labels,
    });
    let Some(rid) = state.mark_issue_property_mutation_pending(RepositoryId("r1".to_string()), 42)
    else {
        panic!("confirm should allocate request_id")
    };
    state = state.apply(AppEvent::IssuePropertyEditSucceeded {
        scope_repo_id: RepositoryId("r1".to_string()),
        issue_number: 42,
        kind: IssuePropertyKind::Labels,
        request_id: rid,
    });
    assert!(state.issues_state.property_editor.is_none());
}

#[test]
fn succeeded_requests_one_coalesced_refresh() {
    let mut state = make_state_with_detail();
    add_repo(&mut state);
    state = state.apply(AppEvent::IssueOpenPropertyEditor {
        kind: IssuePropertyKind::Labels,
    });
    let Some(request_id) =
        state.mark_issue_property_mutation_pending(RepositoryId("r1".to_string()), 42)
    else {
        panic!("confirm should allocate request_id");
    };

    state = state.apply(AppEvent::IssuePropertyEditSucceeded {
        scope_repo_id: RepositoryId("r1".to_string()),
        issue_number: 42,
        kind: IssuePropertyKind::Labels,
        request_id,
    });
    assert!(state.issue_post_mutation_refresh_ready());

    state = state.apply(AppEvent::IssuePostMutationRefreshStarted);
    assert!(!state.issue_post_mutation_refresh_ready());
}

#[test]
fn failed_sets_error_keeps_editor_open() {
    let mut state = make_state_with_detail();
    add_repo(&mut state);
    state = state.apply(AppEvent::IssueOpenPropertyEditor {
        kind: IssuePropertyKind::Labels,
    });
    let Some(rid) = state.mark_issue_property_mutation_pending(RepositoryId("r1".to_string()), 42)
    else {
        panic!("confirm should allocate request_id")
    };
    state = state.apply(AppEvent::IssuePropertyEditFailed {
        scope_repo_id: RepositoryId("r1".to_string()),
        issue_number: 42,
        kind: IssuePropertyKind::Labels,
        request_id: rid,
        error: "boom".to_string(),
    });
    let editor = require_issue_editor(&state);
    assert_eq!(editor.error.as_deref(), Some("boom"));
}

#[test]
fn options_loaded_preserves_selection() {
    let mut state = make_state_with_detail();
    add_repo(&mut state);
    let (mut state, load_rid) = open_editor_with_load_request_id(state, IssuePropertyKind::Labels);
    state = state.apply(AppEvent::IssuePropertyEditorOptionsLoaded {
        scope_repo_id: RepositoryId("r1".to_string()),
        issue_number: 42,
        kind: IssuePropertyKind::Labels,
        request_id: load_rid,
        options: vec![
            (None, "bug".to_string(), false),
            (None, "enhancement".to_string(), false),
        ],
    });
    let editor = require_issue_editor(&state);
    assert_eq!(editor.options.len(), 2);
    assert!(editor.options[0].selected);
    assert!(!editor.options[1].selected);
}

// ── M10: Preserves currently-applied values ─────────────────────────

#[test]
fn options_loaded_preserves_baseline_labels_not_in_page() {
    let mut state = make_state_with_detail();
    add_repo(&mut state);
    let (mut state, load_rid) = open_editor_with_load_request_id(state, IssuePropertyKind::Labels);
    // Simulate a page that does NOT include "bug" (the baseline label)
    state = state.apply(AppEvent::IssuePropertyEditorOptionsLoaded {
        scope_repo_id: RepositoryId("r1".to_string()),
        issue_number: 42,
        kind: IssuePropertyKind::Labels,
        request_id: load_rid,
        options: vec![(None, "enhancement".to_string(), false)],
    });
    let editor = require_issue_editor(&state);
    // "bug" should be present (preserved from baseline)
    assert!(
        editor
            .options
            .iter()
            .any(|o| o.label == "bug" && o.selected)
    );
}

// ── M11: Failure surfaces warning when editor navigated away ────────

#[test]
fn failure_after_navigation_sets_warning() {
    let mut state = make_state_with_detail();
    add_repo(&mut state);
    state = state.apply(AppEvent::IssueOpenPropertyEditor {
        kind: IssuePropertyKind::Labels,
    });
    let Some(rid) = state.mark_issue_property_mutation_pending(RepositoryId("r1".to_string()), 42)
    else {
        panic!("confirm should allocate request_id")
    };
    // Cancel the editor (simulates navigating away)
    state = state.apply(AppEvent::IssuePropertyEditorCancel);
    // Now a failure arrives — should surface as a warning, not crash
    state = state.apply(AppEvent::IssuePropertyEditFailed {
        scope_repo_id: RepositoryId("r1".to_string()),
        issue_number: 42,
        kind: IssuePropertyKind::Labels,
        request_id: rid,
        error: "network timeout".to_string(),
    });
    // A draft_notice should be set (M11 scoped warning)
    assert!(
        state
            .issues_state
            .draft_notice
            .as_ref()
            .is_some_and(|n| n.contains("Failed to edit") && n.contains("#42"))
    );
}

// ── M1: User deselections survive options-load ──────────────────────

#[test]
fn options_loaded_preserves_user_deselection() {
    let mut state = make_state_with_detail();
    add_repo(&mut state);
    let (mut state, load_rid) = open_editor_with_load_request_id(state, IssuePropertyKind::Labels);
    // Baseline label "bug" is selected. User deselects it before load.
    {
        let mut s = state.clone();
        if let Some(editor) = s.issues_state.property_editor.as_mut() {
            for opt in &mut editor.options {
                opt.selected = false;
            }
        }
        state = s;
    }
    // Options arrive including the baseline "bug".
    state = state.apply(AppEvent::IssuePropertyEditorOptionsLoaded {
        scope_repo_id: RepositoryId("r1".to_string()),
        issue_number: 42,
        kind: IssuePropertyKind::Labels,
        request_id: load_rid,
        options: vec![
            (None, "bug".to_string(), false),
            (None, "enhancement".to_string(), false),
        ],
    });
    let editor = require_issue_editor(&state);
    // The deselected baseline label must remain deselected.
    let bug = editor
        .options
        .iter()
        .find(|o| o.label == "bug")
        .unwrap_or_else(|| panic!("bug option must exist"));
    assert!(
        !bug.selected,
        "user deselection of a baseline label must survive options-load"
    );
}

// ── H1: Single-select cursor starts at the current value ────────────

#[test]
fn state_editor_cursor_starts_at_closed_for_closed_issue() {
    use crate::domain::IssueState;
    let mut state = make_state_with_detail();
    // Make the issue closed.
    if let Some(detail) = state.issues_state.issue_detail.as_mut() {
        detail.state = IssueState::Closed;
    }
    add_repo(&mut state);
    state = state.apply(AppEvent::IssueOpenPropertyEditor {
        kind: IssuePropertyKind::State,
    });
    let editor = require_issue_editor(&state);
    assert_eq!(
        editor.options.len(),
        2,
        "State editor must have Open and Closed options"
    );
    assert_eq!(
        editor.options[1].label, "Closed",
        "index 1 must be the Closed option"
    );
    assert_eq!(
        editor.selected_index, 1,
        "a closed issue must open the State editor with the cursor on Closed"
    );
}

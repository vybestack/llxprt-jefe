//! Tests for `prs_property_ops` (extracted to keep the ops file under the
//! architecture boundary handler-module line limit).

use super::*;
use crate::domain::{PrState, PullRequestDetail, RepositoryId};
use crate::state::PullRequestsState;

fn make_state_with_detail() -> AppState {
    let detail = PullRequestDetail {
        repo_owner_name: "owner/repo".to_string(),
        number: 42,
        title: "Test PR".to_string(),
        state: PrState::Open,
        is_draft: false,
        author_login: "alice".to_string(),
        created_at: "2024-01-01".to_string(),
        updated_at: "2024-01-02".to_string(),
        head_ref: "feature".to_string(),
        head_sha: "sha123".to_string(),
        base_ref: "main".to_string(),
        labels: vec!["bug".to_string()],
        assignees: vec!["alice".to_string()],
        milestone: Some("v1.0".to_string()),
        body: "body".to_string(),
        external_url: "url".to_string(),
        review_decision: None,
        checks_status: crate::domain::PrCheckStatus::None,
        reviews: Vec::new(),
        checks: Vec::new(),
        comments: Vec::new(),
        has_more_comments: false,
        comments_cursor: None,
        mergeable: Some(true),
        merge_state_status: None,
    };
    AppState {
        prs_state: PullRequestsState {
            active: true,
            pr_focus: PrFocus::PrDetail,
            pr_detail: Some(detail),
            ..PullRequestsState::default()
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

fn require_pr_editor(state: &AppState) -> &PrPropertyEditorState {
    state
        .prs_state
        .property_editor
        .as_ref()
        .unwrap_or_else(|| panic!("expected property editor to be open"))
}

fn open_editor_with_load_request_id(state: AppState, kind: PrPropertyKind) -> (AppState, u64) {
    let state = state.apply(AppEvent::PrOpenPropertyEditor { kind });
    let load_request_id = require_pr_editor(&state).load_request_id;
    (state, load_request_id)
}

// ── H1: Title editing tests ─────────────────────────────────────────

#[test]
fn title_char_insert() {
    let mut state = make_state_with_detail();
    state = state.apply(AppEvent::PrOpenPropertyEditor {
        kind: PrPropertyKind::Title,
    });
    state = state.apply(AppEvent::PrPropertyEditorTitleChar('X'));
    let editor = require_pr_editor(&state);
    assert_eq!(editor.title_text, "XTest PR");
    assert_eq!(editor.title_cursor, 1);
}

#[test]
fn title_multibyte_char_insert() {
    let mut state = make_state_with_detail();
    state = state.apply(AppEvent::PrOpenPropertyEditor {
        kind: PrPropertyKind::Title,
    });
    state = state.apply(AppEvent::PrPropertyEditorTitleChar('é'));
    let editor = require_pr_editor(&state);
    assert_eq!(editor.title_text, "éTest PR");
    assert_eq!(editor.title_cursor, 2);
}

#[test]
fn title_backspace() {
    let mut state = make_state_with_detail();
    state = state.apply(AppEvent::PrOpenPropertyEditor {
        kind: PrPropertyKind::Title,
    });
    // Move cursor to end, then backspace removes the trailing char.
    let title_len = require_pr_editor(&state).title_text.len();
    for _ in 0..title_len {
        state = state.apply(AppEvent::PrPropertyEditorTitleCursorRight);
    }
    state = state.apply(AppEvent::PrPropertyEditorTitleBackspace);
    let editor = require_pr_editor(&state);
    assert_eq!(editor.title_text, "Test P");
}

#[test]
fn title_delete() {
    let mut state = make_state_with_detail();
    state = state.apply(AppEvent::PrOpenPropertyEditor {
        kind: PrPropertyKind::Title,
    });
    state = state.apply(AppEvent::PrPropertyEditorTitleDelete);
    let editor = require_pr_editor(&state);
    assert_eq!(editor.title_text, "est PR");
}

#[test]
fn title_cursor_move() {
    let mut state = make_state_with_detail();
    state = state.apply(AppEvent::PrOpenPropertyEditor {
        kind: PrPropertyKind::Title,
    });
    state = state.apply(AppEvent::PrPropertyEditorTitleCursorRight);
    let editor = require_pr_editor(&state);
    assert_eq!(editor.title_cursor, 1);
    state = state.apply(AppEvent::PrPropertyEditorTitleCursorLeft);
    let editor = require_pr_editor(&state);
    assert_eq!(editor.title_cursor, 0);
}

// ── H3: Single-select uses selected_index ──────────────────────────

#[test]
fn single_select_state_down_then_highlights_closed() {
    let mut state = make_state_with_detail();
    state = state.apply(AppEvent::PrOpenPropertyEditor {
        kind: PrPropertyKind::State,
    });
    state = state.apply(AppEvent::PrPropertyEditorNavigateDown);
    let editor = require_pr_editor(&state);
    assert_eq!(editor.selected_index, 1);
    assert_eq!(editor.options[1].label, "Closed");
}

// ── H4: Mutation pending tests ──────────────────────────────────────

#[test]
fn double_confirm_debounced() {
    let mut state = make_state_with_detail();
    add_repo(&mut state);
    let Some(rid) = state.mark_pr_property_mutation_pending(RepositoryId("r1".to_string()), 42)
    else {
        panic!("first confirm should allocate request_id")
    };
    let second = state.mark_pr_property_mutation_pending(RepositoryId("r1".to_string()), 42);
    assert!(second.is_none());
    let _ = rid;
}

#[test]
fn stale_completion_ignored() {
    let mut state = make_state_with_detail();
    add_repo(&mut state);
    // Open editor and mark pending.
    state = state.apply(AppEvent::PrOpenPropertyEditor {
        kind: PrPropertyKind::Labels,
    });
    let Some(rid) = state.mark_pr_property_mutation_pending(RepositoryId("r1".to_string()), 42)
    else {
        panic!("confirm should allocate request_id")
    };
    // Cancel the editor (pending token stays for late-failure correlation).
    state = state.apply(AppEvent::PrPropertyEditorCancel);
    assert!(state.prs_state.property_editor.is_none());
    // A late SUCCESS after cancel is applied silently (pending cleared,
    // no crash, editor stays closed).
    state = state.apply(AppEvent::PrPropertyEditSucceeded {
        scope_repo_id: RepositoryId("r1".to_string()),
        pr_number: 42,
        kind: PrPropertyKind::Labels,
        request_id: rid,
    });
    assert!(state.prs_state.property_editor.is_none());
    assert!(state.prs_state.property_mutation_pending.is_none());
}

// ── H5: Options failed keeps existing ───────────────────────────────

#[test]
fn options_failed_keeps_existing_milestone() {
    let mut state = make_state_with_detail();
    add_repo(&mut state);
    let (mut state, load_rid) = open_editor_with_load_request_id(state, PrPropertyKind::Milestone);
    let editor = require_pr_editor(&state);
    assert!(
        editor
            .options
            .iter()
            .any(|o| o.label == "v1.0" && o.selected)
    );
    state = state.apply(AppEvent::PrPropertyEditorOptionsFailed {
        scope_repo_id: RepositoryId("r1".to_string()),
        pr_number: 42,
        kind: PrPropertyKind::Milestone,
        request_id: load_rid,
        error: "network error".to_string(),
    });
    let editor = require_pr_editor(&state);
    assert!(
        editor
            .options
            .iter()
            .any(|o| o.label == "v1.0" && o.selected)
    );
    assert!(editor.loading_failed);
    let _ = load_rid;
}

// ── M6: Stale options response ──────────────────────────────────────

#[test]
fn stale_options_response_ignored() {
    let mut state = make_state_with_detail();
    add_repo(&mut state);
    let (mut state, labels_rid) = open_editor_with_load_request_id(state, PrPropertyKind::Labels);
    state = state.apply(AppEvent::PrPropertyEditorCancel);
    let (mut state, ms_rid) = open_editor_with_load_request_id(state, PrPropertyKind::Milestone);
    state = state.apply(AppEvent::PrPropertyEditorOptionsLoaded {
        scope_repo_id: RepositoryId("r1".to_string()),
        pr_number: 42,
        kind: PrPropertyKind::Labels,
        request_id: labels_rid,
        options: vec![(None, "stale".to_string(), false)],
    });
    let editor = require_pr_editor(&state);
    assert!(!editor.options.iter().any(|o| o.label == "stale"));
    let _ = ms_rid;
}

// ── Existing tests (updated signatures) ─────────────────────────────

#[test]
fn open_property_editor_labels() {
    let mut state = make_state_with_detail();
    state = state.apply(AppEvent::PrOpenPropertyEditor {
        kind: PrPropertyKind::Labels,
    });
    let editor = require_pr_editor(&state);
    assert_eq!(editor.kind, PrPropertyKind::Labels);
    assert_eq!(editor.options.len(), 1);
    assert!(editor.options[0].selected);
    assert_eq!(editor.baseline, vec!["bug".to_string()]);
}

#[test]
fn open_property_editor_title_prepopulates() {
    let mut state = make_state_with_detail();
    state = state.apply(AppEvent::PrOpenPropertyEditor {
        kind: PrPropertyKind::Title,
    });
    let editor = require_pr_editor(&state);
    assert_eq!(editor.title_text, "Test PR");
}

#[test]
fn navigate_wraps() {
    let mut state = make_state_with_detail();
    state = state.apply(AppEvent::PrOpenPropertyEditor {
        kind: PrPropertyKind::Labels,
    });
    state = state.apply(AppEvent::PrPropertyEditorNavigateUp);
    let editor = require_pr_editor(&state);
    assert_eq!(editor.selected_index, 0);
}

#[test]
fn toggle_labels_flips_selected() {
    let mut state = make_state_with_detail();
    state = state.apply(AppEvent::PrOpenPropertyEditor {
        kind: PrPropertyKind::Labels,
    });
    state = state.apply(AppEvent::PrPropertyEditorToggle);
    let editor = require_pr_editor(&state);
    assert!(!editor.options[0].selected);
}

#[test]
fn cancel_closes_editor() {
    let mut state = make_state_with_detail();
    state = state.apply(AppEvent::PrOpenPropertyEditor {
        kind: PrPropertyKind::Labels,
    });
    state = state.apply(AppEvent::PrPropertyEditorCancel);
    assert!(state.prs_state.property_editor.is_none());
}

#[test]
fn succeeded_clears_editor() {
    let mut state = make_state_with_detail();
    add_repo(&mut state);
    state = state.apply(AppEvent::PrOpenPropertyEditor {
        kind: PrPropertyKind::Labels,
    });
    let Some(rid) = state.mark_pr_property_mutation_pending(RepositoryId("r1".to_string()), 42)
    else {
        panic!("confirm should allocate request_id")
    };
    state = state.apply(AppEvent::PrPropertyEditSucceeded {
        scope_repo_id: RepositoryId("r1".to_string()),
        pr_number: 42,
        kind: PrPropertyKind::Labels,
        request_id: rid,
    });
    assert!(state.prs_state.property_editor.is_none());
}

#[test]
fn succeeded_requests_one_coalesced_pr_refresh() {
    let mut state = make_state_with_detail();
    add_repo(&mut state);
    state = state.apply(AppEvent::PrOpenPropertyEditor {
        kind: PrPropertyKind::Labels,
    });
    let Some(request_id) =
        state.mark_pr_property_mutation_pending(RepositoryId("r1".to_string()), 42)
    else {
        panic!("confirm should allocate request_id");
    };

    state = state.apply(AppEvent::PrPropertyEditSucceeded {
        scope_repo_id: RepositoryId("r1".to_string()),
        pr_number: 42,
        kind: PrPropertyKind::Labels,
        request_id,
    });
    assert!(state.pr_post_mutation_refresh_ready());

    state = state.apply(AppEvent::PrPostMutationRefreshStarted);
    assert!(!state.pr_post_mutation_refresh_ready());
}

#[test]
fn failed_sets_error_keeps_editor_open() {
    let mut state = make_state_with_detail();
    add_repo(&mut state);
    state = state.apply(AppEvent::PrOpenPropertyEditor {
        kind: PrPropertyKind::Labels,
    });
    let Some(rid) = state.mark_pr_property_mutation_pending(RepositoryId("r1".to_string()), 42)
    else {
        panic!("confirm should allocate request_id")
    };
    state = state.apply(AppEvent::PrPropertyEditFailed {
        scope_repo_id: RepositoryId("r1".to_string()),
        pr_number: 42,
        kind: PrPropertyKind::Labels,
        request_id: rid,
        error: "boom".to_string(),
    });
    let editor = require_pr_editor(&state);
    assert_eq!(editor.error.as_deref(), Some("boom"));
}

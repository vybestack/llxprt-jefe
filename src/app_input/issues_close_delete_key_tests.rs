//! Issue close + delete key routing tests (issue #182).
//!
//! Extracted from `issues_key_tests.rs` to keep that file under the
//! source-size hard limit. Compiled as a submodule via
//! `#[path = "..."] mod ...;`, so `use super::*;` re-imports the parent
//! module's helpers (`issues_base_state`, `issues_state_with_focus`, `key`,
//! `resolve_issues_key_event`).

use super::*;

// ─── Issue close + delete key routing (issue #182) ──────────────────────

fn issues_state_with_issue_list() -> AppState {
    use jefe::domain::{Issue, IssueState};
    let mut state = issues_base_state();
    state.issues_state.list.replace_items(vec![Issue {
        number: 1,
        node_id: String::new(),
        title: "Test".to_string(),
        state: IssueState::Open,
        author_login: String::new(),
        updated_at: String::new(),
        assignee_summary: String::new(),
        labels_summary: String::new(),
        assignees: Vec::new(),
        labels: Vec::new(),
        issue_type: String::new(),
        milestone: String::new(),
        module: String::new(),
        comment_count: 0,
        body: String::new(),
    }]);
    state.issues_state.list.set_selected_index(Some(0));
    state
}

#[test]
fn shift_c_in_list_resolves_to_close_issue() {
    let state = issues_state_with_issue_list();
    let result = resolve_issues_key_event(&state, &key(KeyCode::Char('C')));
    assert!(
        matches!(result, Some(AppEvent::CloseIssue)),
        "Shift-C should resolve to CloseIssue, got {result:?}"
    );
}

#[test]
fn shift_d_in_list_resolves_to_open_delete_confirm() {
    let state = issues_state_with_issue_list();
    let result = resolve_issues_key_event(&state, &key(KeyCode::Char('D')));
    assert!(
        matches!(result, Some(AppEvent::OpenDeleteIssueConfirm)),
        "Shift-D should resolve to OpenDeleteIssueConfirm, got {result:?}"
    );
}

#[test]
fn shift_c_in_detail_resolves_to_close_issue() {
    let state = issues_state_with_focus(IssueFocus::IssueDetail);
    let result = resolve_issues_key_event(&state, &key(KeyCode::Char('C')));
    assert!(
        matches!(result, Some(AppEvent::CloseIssue)),
        "Shift-C in detail should resolve to CloseIssue, got {result:?}"
    );
}

#[test]
fn shift_d_in_detail_resolves_to_open_delete_confirm() {
    let state = issues_state_with_focus(IssueFocus::IssueDetail);
    let result = resolve_issues_key_event(&state, &key(KeyCode::Char('D')));
    assert!(
        matches!(result, Some(AppEvent::OpenDeleteIssueConfirm)),
        "Shift-D in detail should resolve to OpenDeleteIssueConfirm, got {result:?}"
    );
}

#[test]
fn lowercase_c_in_detail_resolves_to_new_comment() {
    let state = issues_state_with_focus(IssueFocus::IssueDetail);
    let result = resolve_issues_key_event(&state, &key(KeyCode::Char('c')));
    assert!(
        matches!(result, Some(AppEvent::OpenNewCommentComposer)),
        "lowercase c in detail should resolve to OpenNewCommentComposer, got {result:?}"
    );
}

#[test]
fn delete_confirm_enter_resolves_to_confirm() {
    let mut state = issues_state_with_issue_list();
    state.issues_state.delete_confirm = Some(jefe::state::IssueDeleteConfirmState {
        issue_number: 1,
        awaiting_confirmation: false,
    });
    let result = resolve_issues_key_event(&state, &key(KeyCode::Enter));
    assert!(
        matches!(result, Some(AppEvent::IssueDeleteConfirm)),
        "Enter with delete confirm open should resolve to IssueDeleteConfirm, got {result:?}"
    );
}

#[test]
fn delete_confirm_esc_resolves_to_cancel() {
    let mut state = issues_state_with_issue_list();
    state.issues_state.delete_confirm = Some(jefe::state::IssueDeleteConfirmState {
        issue_number: 1,
        awaiting_confirmation: false,
    });
    let result = resolve_issues_key_event(&state, &key(KeyCode::Esc));
    assert!(
        matches!(result, Some(AppEvent::IssueDeleteCancel)),
        "Esc with delete confirm open should resolve to IssueDeleteCancel, got {result:?}"
    );
}

#[test]
fn delete_confirm_consumes_other_keys() {
    let mut state = issues_state_with_issue_list();
    state.issues_state.delete_confirm = Some(jefe::state::IssueDeleteConfirmState {
        issue_number: 1,
        awaiting_confirmation: false,
    });
    // Any key other than Enter/Esc should be consumed (None)
    let result = resolve_issues_key_event(&state, &key(KeyCode::Char('x')));
    assert!(
        result.is_none(),
        "delete confirm overlay should consume non-Enter/Esc keys, got {result:?}"
    );
}

#[test]
fn shift_d_blocked_when_composer_active() {
    let mut state = issues_state_with_issue_list();
    state.issues_state.inline_state = InlineState::Composer {
        target: ComposerTarget::NewComment,
        text: String::new(),
        cursor: 0,
    };
    // When a composer is active, Shift-D routes to the inline composer (as a
    // typed 'D' char), NOT to the delete-confirm handler.
    let result = resolve_issues_key_event(&state, &key(KeyCode::Char('D')));
    assert!(
        matches!(result, Some(AppEvent::InlineChar('D'))),
        "Shift-D should route to the inline composer when active, got {result:?}"
    );
}

#[test]
fn shift_c_blocked_when_composer_active() {
    let mut state = issues_state_with_issue_list();
    state.issues_state.inline_state = InlineState::Composer {
        target: ComposerTarget::NewComment,
        text: String::new(),
        cursor: 0,
    };
    // When a composer is active, Shift-C routes to the inline composer (as a
    // typed 'C' char), NOT to the close-issue handler.
    let result = resolve_issues_key_event(&state, &key(KeyCode::Char('C')));
    assert!(
        matches!(result, Some(AppEvent::InlineChar('C'))),
        "Shift-C should route to the inline composer when active, got {result:?}"
    );
}

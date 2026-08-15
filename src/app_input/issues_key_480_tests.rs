//! Issue #480: New Issue form bare-Enter must not submit.
//!
//! Extracted from `issues_key_tests.rs` to keep that file under the
//! source-size hard limit. Compiled as a submodule via
//! `#[path = "..."] mod ...;`, so `use super::*;` re-imports the parent
//! module's helpers (`key`, `key_with_mods`, `resolve_issues_key_event`).

use jefe::state::{NewIssueFormFocus, NewIssueFormState};

use super::*;

/// Build an Issues-mode state with the inline New Issue form open and the
/// given field focused. The form coexists with the NewIssue composer
/// `InlineState` (issue #407), which is what routes keys through
/// `resolve_new_issue_inline_key_event`.
fn issues_state_with_new_issue_form(focus: NewIssueFormFocus) -> AppState {
    let mut state = crate::test_app_state();
    state.nav = crate::state::navigation::NavState::rooted(ScreenId::Issues);
    state.issues_state = IssuesState {
        active: true,
        issue_focus: IssueFocus::IssueDetail,
        inline_state: InlineState::Composer {
            target: ComposerTarget::NewIssue,
            text: String::new(),
            cursor: 0,
        },
        new_issue_form: Some(NewIssueFormState {
            focus,
            ..NewIssueFormState::default()
        }),
        ..IssuesState::default()
    };
    state
}

/// Plain Enter on the Title field must advance focus to the next field,
/// NOT submit (issue #480). Only Alt+Enter / Ctrl+Enter submit.
#[test]
fn bare_enter_on_new_issue_title_advances_focus_not_submit() {
    let state = issues_state_with_new_issue_form(NewIssueFormFocus::Title);
    let event = resolve_issues_key_event(&state, &key(KeyCode::Enter));
    assert!(
        matches!(event, Some(AppEvent::NewIssueFocusNext)),
        "bare Enter on Title must dispatch NewIssueFocusNext (advance to Body), got {event:?}"
    );
    assert!(
        !matches!(event, Some(AppEvent::NewIssueSubmit)),
        "bare Enter on Title must NOT dispatch NewIssueSubmit, got {event:?}"
    );
}

/// Alt+Enter on the Title field still submits (regression guard, issue #480).
#[test]
fn alt_enter_on_new_issue_title_submits() {
    let state = issues_state_with_new_issue_form(NewIssueFormFocus::Title);
    let event = resolve_issues_key_event(&state, &key_with_mods(KeyCode::Enter, KeyModifiers::ALT));
    assert!(
        matches!(event, Some(AppEvent::NewIssueSubmit)),
        "Alt+Enter on Title must dispatch NewIssueSubmit, got {event:?}"
    );
}

/// Ctrl+Enter on the Title field still submits (terminal-portable compat,
/// regression guard, issue #480).
#[test]
fn ctrl_enter_on_new_issue_title_submits() {
    let state = issues_state_with_new_issue_form(NewIssueFormFocus::Title);
    let event = resolve_issues_key_event(
        &state,
        &key_with_mods(KeyCode::Enter, KeyModifiers::CONTROL),
    );
    assert!(
        matches!(event, Some(AppEvent::NewIssueSubmit)),
        "Ctrl+Enter on Title must dispatch NewIssueSubmit, got {event:?}"
    );
}

/// Plain Enter on the Body field still inserts a newline (unchanged, issue #480).
#[test]
fn bare_enter_on_new_issue_body_inserts_newline() {
    let state = issues_state_with_new_issue_form(NewIssueFormFocus::Body);
    let event = resolve_issues_key_event(&state, &key(KeyCode::Enter));
    assert!(
        matches!(event, Some(AppEvent::NewIssueBodyNewline)),
        "bare Enter on Body must dispatch NewIssueBodyNewline, got {event:?}"
    );
}

/// Plain Enter on a selection field (Template) still advances focus
/// (unchanged, issue #480).
#[test]
fn bare_enter_on_new_issue_template_advances_focus() {
    let state = issues_state_with_new_issue_form(NewIssueFormFocus::Template);
    let event = resolve_issues_key_event(&state, &key(KeyCode::Enter));
    assert!(
        matches!(event, Some(AppEvent::NewIssueFocusNext)),
        "bare Enter on Template must dispatch NewIssueFocusNext, got {event:?}"
    );
}

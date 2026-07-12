//! Reducer tests for the non-blocking issue self-assignment warning
//! (issue #186). Split out of `issues_tests_detail_flow.rs` to keep that file
//! under the source-file length limit.

use crate::state::{AppEvent, AppState};

/// A self-assignment failure surfaces a `warning_message` that identifies the
/// repo and issue, WITHOUT setting the issues error state or acting like a
/// `SendToAgentFailed` (the launch itself succeeded).
#[test]
fn test_issue_self_assignment_failed_sets_warning_not_error() {
    let mut state = AppState::default();
    state.issues_state.active = true;
    state.warning_message = None;
    state.issues_state.error = None;

    let state = state.apply(AppEvent::IssueSelfAssignmentFailed {
        owner_repo: "acme/widgets".to_string(),
        issue_number: 166,
        error: "repo restricts assignees".to_string(),
    });

    assert!(
        state.issues_state.active,
        "issues mode must remain active after a non-blocking assignment failure"
    );
    assert!(
        state.issues_state.error.is_none(),
        "assignment failure must NOT set the issues error state"
    );
    let warning = state
        .warning_message
        .as_ref()
        .unwrap_or_else(|| panic!("expected a warning message"));
    assert!(
        warning.contains("acme/widgets#166"),
        "warning should identify the repo and issue, got: {warning}"
    );
    assert!(
        warning.contains("repo restricts assignees"),
        "warning should include the underlying error, got: {warning}"
    );
}

/// When no valid GitHub repo is configured, the empty `owner_repo` is carried
/// into the warning and the message still identifies the issue number (issue
/// #186). The launch remains a success.
#[test]
fn test_issue_self_assignment_failed_warns_without_owner_repo() {
    let mut state = AppState::default();
    state.issues_state.active = true;
    state.warning_message = None;
    state.issues_state.error = None;

    let state = state.apply(AppEvent::IssueSelfAssignmentFailed {
        owner_repo: String::new(),
        issue_number: 42,
        error: "No valid GitHub repo (owner/repo) configured for this agent's repository; \
                could not self-assign the issue"
            .to_string(),
    });

    assert!(
        state.issues_state.error.is_none(),
        "assignment failure must NOT set the issues error state"
    );
    let warning = state
        .warning_message
        .as_ref()
        .unwrap_or_else(|| panic!("expected a warning message"));
    assert!(
        warning.contains("#42"),
        "warning should identify the issue number even without owner_repo, got: {warning}"
    );
}

/// A self-assignment failure intentionally overwrites any prior warning, since
/// `warning_message` is a single slot and the most recent outcome is the most
/// actionable (issue #186). This locks the overwrite semantics so a future
/// change to the reducer cannot silently regress the behavior.
#[test]
fn test_issue_self_assignment_failed_overwrites_prior_warning() {
    let mut state = AppState::default();
    state.issues_state.active = true;
    state.warning_message = Some("an earlier unrelated warning".to_string());

    let state = state.apply(AppEvent::IssueSelfAssignmentFailed {
        owner_repo: "acme/widgets".to_string(),
        issue_number: 186,
        error: "repo restricts assignees".to_string(),
    });

    let warning = state
        .warning_message
        .as_ref()
        .unwrap_or_else(|| panic!("expected a warning message"));
    assert!(
        warning.contains("acme/widgets#186"),
        "the self-assignment warning must replace the prior warning, got: {warning}"
    );
    assert!(
        !warning.contains("earlier unrelated warning"),
        "prior warning text must not survive the overwrite, got: {warning}"
    );
}

/// The self-assignment-failed reducer must be safe regardless of whether
/// issues mode is active: a late-arriving background task could fire after the
/// user navigated away (issue #186). It must surface the warning without
/// panicking or spuriously activating issues mode.
#[test]
fn test_issue_self_assignment_failed_outside_issues_mode_is_safe() {
    let mut state = AppState::default();
    state.issues_state.active = false;
    state.issues_state.error = None;

    let state = state.apply(AppEvent::IssueSelfAssignmentFailed {
        owner_repo: "acme/widgets".to_string(),
        issue_number: 186,
        error: "repo restricts assignees".to_string(),
    });

    assert!(
        !state.issues_state.active,
        "a late self-assignment failure must not spuriously activate issues mode"
    );
    assert!(
        state.issues_state.error.is_none(),
        "assignment failure must NOT set the issues error state"
    );
    assert!(
        state.warning_message.is_some(),
        "the warning must still surface outside issues mode"
    );
}

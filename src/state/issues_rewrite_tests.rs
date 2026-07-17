//! Tests for the agent-driven new-issue draft rewrite reducer (issue #214).

use crate::state::{AppEvent, ComposerTarget, IssueFocus};
use crate::state::{AppState, InlineState};

fn state_with_new_issue_composer(draft: &str) -> AppState {
    let mut state = AppState::default();
    state.issues_state.active = true;
    state.issues_state.issue_focus = IssueFocus::IssueList;
    state.issues_state.inline_state = InlineState::Composer {
        target: ComposerTarget::NewIssue,
        text: draft.to_owned(),
        cursor: draft.len(),
    };
    state
}

fn composer_text(state: &AppState) -> Option<String> {
    match &state.issues_state.inline_state {
        InlineState::Composer { text, .. } => Some(text.clone()),
        _ => None,
    }
}

#[test]
fn request_rewrite_sets_pending_and_keeps_draft() {
    let state = state_with_new_issue_composer("fix the bug");
    let state = state.apply(AppEvent::RequestIssueRewrite);

    assert!(state.issues_state.rewrite_pending);
    assert_eq!(composer_text(&state).as_deref(), Some("fix the bug"));
    assert_eq!(
        state.issues_state.draft_notice.as_deref(),
        Some("Rewriting issue draft…")
    );
}

#[test]
fn request_rewrite_is_idempotent_when_already_pending() {
    let state = state_with_new_issue_composer("fix the bug");
    let mut state = state.apply(AppEvent::RequestIssueRewrite);
    // Second request must not error or duplicate.
    state = state.apply(AppEvent::RequestIssueRewrite);
    assert!(state.issues_state.rewrite_pending);
}

#[test]
fn request_rewrite_noop_outside_new_issue_composer() {
    let mut state = AppState::default();
    state.issues_state.inline_state = InlineState::None;
    let state = state.apply(AppEvent::RequestIssueRewrite);
    assert!(!state.issues_state.rewrite_pending);
}

#[test]
fn rewrite_succeeded_replaces_composer_text_and_drops_pending() {
    let state = state_with_new_issue_composer("rough notes");
    let mut state = state.apply(AppEvent::RequestIssueRewrite);
    assert!(state.issues_state.rewrite_pending);

    state = state.apply(AppEvent::IssueRewriteSucceeded {
        text: "Polished title\n\nDetailed body.".to_owned(),
    });

    assert!(!state.issues_state.rewrite_pending);
    assert_eq!(
        composer_text(&state).as_deref(),
        Some("Polished title\n\nDetailed body.")
    );
    // Cursor at end (byte length of replaced text).
    if let InlineState::Composer { cursor, text, .. } = &state.issues_state.inline_state {
        assert_eq!(*cursor, text.len());
    }
    assert_eq!(
        state.issues_state.draft_notice.as_deref(),
        Some("Issue draft rewritten by agent")
    );
}

#[test]
fn rewrite_succeeded_preserves_other_composer_targets_unchanged() {
    // A NewComment composer must not be overwritten by a stray success.
    let mut state = AppState::default();
    state.issues_state.rewrite_pending = true;
    state.issues_state.inline_state = InlineState::Composer {
        target: ComposerTarget::NewComment,
        text: "comment".to_owned(),
        cursor: 7,
    };
    let state = state.apply(AppEvent::IssueRewriteSucceeded {
        text: "rewritten".to_owned(),
    });
    assert_eq!(composer_text(&state).as_deref(), Some("comment"));
    // pending still cleared
    assert!(!state.issues_state.rewrite_pending);
}

#[test]
fn rewrite_failed_clears_pending_and_preserves_draft() {
    let state = state_with_new_issue_composer("my draft");
    let state = state.apply(AppEvent::RequestIssueRewrite);
    let state = state.apply(AppEvent::IssueRewriteFailed {
        error: "agent offline".to_owned(),
    });
    assert!(!state.issues_state.rewrite_pending);
    // Original draft preserved.
    assert_eq!(composer_text(&state).as_deref(), Some("my draft"));
    assert!(
        state
            .issues_state
            .draft_notice
            .as_deref()
            .is_some_and(|n| n.contains("agent offline"))
    );
}

#[test]
fn rewrite_succeeded_stale_when_composer_closed_clears_pending_only() {
    // The user closed the composer while the agent was running. The success
    // must not set a misleading notice or modify any other view — only the
    // pending flag is cleared so the state is never stuck.
    let mut state = AppState::default();
    state.issues_state.rewrite_pending = true;
    state.issues_state.inline_state = InlineState::None;
    let state = state.apply(AppEvent::IssueRewriteSucceeded {
        text: "rewritten".to_owned(),
    });
    assert!(!state.issues_state.rewrite_pending);
    assert!(state.issues_state.draft_notice.is_none());
}

#[test]
fn rewrite_failed_stale_when_composer_closed_clears_pending_only() {
    let mut state = AppState::default();
    state.issues_state.rewrite_pending = true;
    state.issues_state.inline_state = InlineState::None;
    let state = state.apply(AppEvent::IssueRewriteFailed {
        error: "timeout".to_owned(),
    });
    assert!(!state.issues_state.rewrite_pending);
    // No notice surfaced in an unrelated view.
    assert!(state.issues_state.draft_notice.is_none());
}

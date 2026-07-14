//! Issue-detail mutation correlation tests extracted from the detail suite.

use crate::domain::RepositoryId;
use crate::state::AppState;
use crate::state::events::AppEvent;
use crate::state::types::{ComposerTarget, EditorTarget, InlineState};

use super::issues_tests_detail::{issues_mode_state_with_repo, p15_comment, p15_detail};

/// Apply three stale (different-issue) mutation events to `state` (test setup).
fn apply_three_stale_mutation_events(state: AppState, repo_id: &RepositoryId) -> AppState {
    state
        .apply(AppEvent::CommentCreated {
            scope_repo_id: repo_id.clone(),
            issue_number: 99,
            mutation_id: 1,
            comment: p15_comment(8, "bob", "2024-01-04T00:00:00Z", "stale"),
        })
        .apply(AppEvent::IssueBodyUpdated {
            scope_repo_id: repo_id.clone(),
            issue_number: 99,
            mutation_id: 1,
            title: "stale title".to_string(),
            body: "stale body".to_string(),
        })
        .apply(AppEvent::CommentUpdated {
            scope_repo_id: repo_id.clone(),
            issue_number: 99,
            mutation_id: 1,
            comment_id: 7,
            comment_index: 0,
            body: "stale update".to_string(),
        })
}

#[test]
fn test_stale_mutation_events_same_repo_different_issue_do_not_mutate_or_clear_inline_state() {
    let repo_id = RepositoryId("repo-1".to_string());
    let mut detail = p15_detail(42);
    detail.comments.replace_items(vec![p15_comment(
        7,
        "alice",
        "2024-01-03T00:00:00Z",
        "original",
    )]);
    let mut state = issues_mode_state_with_repo("repo-1");
    state.mark_issue_detail_loading(repo_id.clone(), 42);
    let mut state = state.apply(AppEvent::IssueDetailLoaded {
        scope_repo_id: repo_id.clone(),
        issue_number: 42,
        request_id: 0,
        detail: Box::new(detail),
    });
    state.issues_state.inline_state = InlineState::Composer {
        target: ComposerTarget::NewComment,
        text: "draft".to_string(),
        cursor: 5,
    };
    let pending_target = state.issues_state.inline_state.clone();
    let state = state.apply(AppEvent::MutationSubmitted {
        scope_repo_id: repo_id.clone(),
        mutation_id: 1,
        target: pending_target,
    });
    let mut state = state;
    state.issues_state.error = Some("current error".to_string());

    let state = apply_three_stale_mutation_events(state, &repo_id);

    let Some(detail) = state.issues_state.issue_detail.as_ref() else {
        panic!("expected detail");
    };
    assert_eq!(detail.body, "Issue body");
    assert_eq!(detail.comments.len(), 1);

    assert_eq!(detail.comments[0].body, "original");
    match &state.issues_state.inline_state {
        InlineState::Composer { text, .. } => assert_eq!(text, "draft"),
        other => panic!("expected composer draft to remain, got {other:?}"),
    }
    assert_eq!(state.issues_state.error.as_deref(), Some("current error"));
}

#[test]
fn test_comment_update_uses_comment_id_after_submitted_index_shifts() {
    let repo_id = RepositoryId("repo-1".to_string());
    let mut detail = p15_detail(42);
    detail.comments.replace_items(vec![
        p15_comment(1, "alice", "2024-01-01T00:00:00Z", "first"),
        p15_comment(2, "bob", "2024-01-02T00:00:00Z", "second"),
    ]);
    let mut state = issues_mode_state_with_repo("repo-1");
    state.mark_issue_detail_loading(repo_id.clone(), 42);
    let state = state.apply(AppEvent::IssueDetailLoaded {
        scope_repo_id: repo_id.clone(),
        issue_number: 42,
        request_id: 0,
        detail: Box::new(detail),
    });

    let mut state = state.apply(AppEvent::MutationSubmitted {
        scope_repo_id: repo_id.clone(),
        mutation_id: 1,
        target: InlineState::Editor {
            target: EditorTarget::Comment { comment_index: 1 },
            text: "updated by id".to_string(),
            cursor: 13,
        },
    });
    let Some(detail) = state.issues_state.issue_detail.as_mut() else {
        panic!("expected detail");
    };
    detail.comments.items_mut().insert(
        0,
        p15_comment(3, "carol", "2024-01-03T00:00:00Z", "inserted"),
    );

    let state = state.apply(AppEvent::CommentUpdated {
        scope_repo_id: repo_id,
        issue_number: 42,
        mutation_id: 1,
        comment_id: 2,
        comment_index: 1,
        body: "updated by id".to_string(),
    });

    let Some(detail) = state.issues_state.issue_detail.as_ref() else {
        panic!("expected detail");
    };
    assert_eq!(detail.comments[0].body, "inserted");
    assert_eq!(detail.comments[1].body, "first");
    assert_eq!(detail.comments[2].body, "updated by id");
}

/// Stale failures for another issue preserve the active draft and current error.
#[test]
fn test_stale_mutation_failures_same_repo_different_issue_do_not_clear_inline_state() {
    let repo_id = RepositoryId("repo-1".to_string());
    let mut state = issues_mode_state_with_repo("repo-1");
    state.mark_issue_detail_loading(repo_id.clone(), 42);
    let mut state = state.apply(AppEvent::IssueDetailLoaded {
        scope_repo_id: repo_id.clone(),
        issue_number: 42,
        request_id: 0,
        detail: Box::new(p15_detail(42)),
    });
    state.issues_state.inline_state = InlineState::Composer {
        target: ComposerTarget::NewComment,
        text: "draft".to_string(),
        cursor: 5,
    };
    let pending_target = state.issues_state.inline_state.clone();
    let state = state.apply(AppEvent::MutationSubmitted {
        scope_repo_id: repo_id.clone(),
        mutation_id: 1,
        target: pending_target,
    });
    let mut state = state;
    state.issues_state.error = Some("current error".to_string());

    let state = state
        .apply(AppEvent::CommentCreateFailed {
            scope_repo_id: repo_id.clone(),
            issue_number: 99,
            mutation_id: 1,
            error: "stale comment create failure".to_string(),
        })
        .apply(AppEvent::MutationFailed {
            scope_repo_id: repo_id,
            issue_number: Some(99),
            mutation_id: Some(1),
            error: "stale mutation failure".to_string(),
        });

    match &state.issues_state.inline_state {
        InlineState::Composer { text, .. } => assert_eq!(text, "draft"),
        other => panic!("expected composer draft to remain, got {other:?}"),
    }
    assert_eq!(state.issues_state.error.as_deref(), Some("current error"));
}

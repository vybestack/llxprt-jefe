//! Pull Requests Mode review-thread state tests (issue #119) — open thread
//! reply composer, toggle resolve pending, resolve succeeded/failed.
//!
//! @plan PLAN-20260624-PR-MODE.P05
//! @requirement REQ-PR-009

use super::prs_test_fixtures::prs_state_with_detail;
use crate::domain::{IssueComment, PrReview, PrReviewState, PrReviewThread, RepositoryId};
use crate::state::AppState;
use crate::state::events::AppEvent;
use crate::state::types::{ComposerTarget, InlineState, PrDetailSubfocus, PrThreadResolvePending};

/// Helper: build a review thread with the given resolved state + path/line.
fn make_thread(
    thread_id: &str,
    is_resolved: bool,
    path: Option<&str>,
    line: Option<u32>,
) -> PrReviewThread {
    PrReviewThread {
        thread_id: thread_id.to_string(),
        is_resolved,
        is_outdated: false,
        review_id: None,
        path: path.map(String::from),
        line,
        comments: vec![IssueComment {
            comment_id: 1,
            author_login: "reviewer".to_string(),
            created_at: "2024-01-03T00:00:00Z".to_string(),
            edited_at: None,
            body: "needs work".to_string(),
        }],
    }
}

/// Helper: state with a single review that has two threads.
fn state_with_two_threads() -> AppState {
    let mut state = prs_state_with_detail("repo-1", 1);
    let Some(detail) = state.prs_state.pr_detail.as_mut() else {
        panic!("test fixture must have pr_detail");
    };
    detail.reviews = vec![PrReview {
        review_id: None,
        author_login: "ada".to_string(),
        state: PrReviewState::ChangesRequested,
        submitted_at: "2024-01-02T00:00:00Z".to_string(),
        body: Some("please fix".to_string()),
        review_threads: vec![
            make_thread("T1", false, Some("src/main.rs"), Some(10)),
            make_thread("T2", true, Some("src/lib.rs"), Some(20)),
        ],
    }];
    state
}

// ── OpenThreadReplyComposer ───────────────────────────────────────────────

/// PrOpenThreadReplyComposer opens a Composer targeting ReplyToReviewThread
/// with the flat thread_index, prefilled author, and sets subfocus.
#[test]
fn open_thread_reply_composer_sets_composer_target() {
    let state = state_with_two_threads();

    let new_state = state.apply(AppEvent::PrOpenThreadReplyComposer { thread_index: 0 });

    let InlineState::Composer {
        target,
        text,
        cursor,
    } = &new_state.prs_state.inline_state
    else {
        panic!(
            "inline_state must be Composer, got {:?}",
            new_state.prs_state.inline_state
        );
    };
    assert!(
        matches!(
            target,
            ComposerTarget::ReplyToReviewThread {
                thread_index: 0,
                ..
            }
        ),
        "target must be ReplyToReviewThread(0), got {target:?}"
    );
    assert!(
        text.starts_with("@reviewer "),
        "text must prefill @reviewer, got {text:?}"
    );
    assert_eq!(*cursor, text.len(), "cursor must be at end of prefill");
    assert_eq!(
        new_state.prs_state.detail_subfocus,
        PrDetailSubfocus::ReviewThread(0),
        "subfocus must be ReviewThread(0)"
    );
}

/// PrOpenThreadReplyComposer for thread_index 1 still works and maps subfocus.
#[test]
fn open_thread_reply_composer_for_second_thread() {
    let state = state_with_two_threads();

    let new_state = state.apply(AppEvent::PrOpenThreadReplyComposer { thread_index: 1 });

    assert!(
        matches!(
            &new_state.prs_state.inline_state,
            InlineState::Composer {
                target: ComposerTarget::ReplyToReviewThread {
                    thread_index: 1,
                    ..
                },
                ..
            }
        ),
        "target must be ReplyToReviewThread(1)"
    );
    assert_eq!(
        new_state.prs_state.detail_subfocus,
        PrDetailSubfocus::ReviewThread(1)
    );
}

/// PrOpenThreadReplyComposer is a no-op when inline_state is already active.
#[test]
fn open_thread_reply_composer_noop_when_inline_active() {
    let mut state = state_with_two_threads();
    state.prs_state.inline_state = InlineState::Composer {
        target: ComposerTarget::NewComment,
        text: "draft".to_string(),
        cursor: 5,
    };

    let new_state = state.apply(AppEvent::PrOpenThreadReplyComposer { thread_index: 0 });

    // The existing composer must be preserved (not overwritten).
    assert!(
        matches!(
            &new_state.prs_state.inline_state,
            InlineState::Composer {
                target: ComposerTarget::NewComment,
                ..
            }
        ),
        "existing composer must not be overwritten"
    );
}

/// PrOpenThreadReplyComposer on an out-of-range thread_index is a no-op
/// (graceful degradation — never panic).
#[test]
fn open_thread_reply_composer_out_of_range_is_noop() {
    let state = state_with_two_threads();

    let new_state = state.apply(AppEvent::PrOpenThreadReplyComposer { thread_index: 99 });

    assert_eq!(
        new_state.prs_state.inline_state,
        InlineState::None,
        "out-of-range thread_index must not open a composer"
    );
}

// ── ToggleThreadResolve ───────────────────────────────────────────────────

/// PrToggleThreadResolve on an unresolved thread sets pending to resolve=true.
#[test]
fn toggle_thread_resolve_sets_pending_for_unresolved() {
    let state = state_with_two_threads();

    let new_state = state.apply(AppEvent::PrToggleThreadResolve { thread_index: 0 });

    let Some(pending) = &new_state.prs_state.thread_resolve_pending else {
        panic!("thread_resolve_pending must be set");
    };
    assert_eq!(pending.thread_index, 0, "pending thread_index must be 0");
    assert!(
        pending.resolve,
        "pending resolve must be true for unresolved thread"
    );
    assert_eq!(
        pending.scope_repo_id,
        RepositoryId("repo-1".to_string()),
        "scope must match selected repo"
    );
    assert_eq!(
        pending.request_id, 1,
        "request_id must be incremented from 0 to 1"
    );
}
#[test]
fn toggle_thread_resolve_sets_pending_for_resolved() {
    let state = state_with_two_threads();

    let new_state = state.apply(AppEvent::PrToggleThreadResolve { thread_index: 1 });

    let Some(pending) = &new_state.prs_state.thread_resolve_pending else {
        panic!("thread_resolve_pending must be set");
    };
    assert_eq!(pending.thread_index, 1, "pending thread_index must be 1");
    assert!(
        !pending.resolve,
        "pending resolve must be false for resolved thread"
    );
}

/// PrToggleThreadResolve on an out-of-range thread_index is a no-op.
#[test]
fn toggle_thread_resolve_out_of_range_is_noop() {
    let state = state_with_two_threads();

    let new_state = state.apply(AppEvent::PrToggleThreadResolve { thread_index: 99 });

    assert!(
        new_state.prs_state.thread_resolve_pending.is_none(),
        "out-of-range thread_index must not set pending"
    );
}

// ── ThreadResolveSucceeded ────────────────────────────────────────────────

/// PrThreadResolveSucceeded flips the thread is_resolved and clears pending.
#[test]
fn thread_resolve_succeeded_flips_is_resolved_and_clears_pending() {
    let mut state = state_with_two_threads();
    // Set pending for thread 0 (currently unresolved -> resolve=true).
    state.prs_state.thread_resolve_pending = Some(PrThreadResolvePending {
        scope_repo_id: RepositoryId("repo-1".to_string()),
        thread_index: 0,
        resolve: true,
        request_id: 1,
    });

    let new_state = state.apply(AppEvent::PrThreadResolveSucceeded {
        scope_repo_id: RepositoryId("repo-1".to_string()),
        thread_index: 0,
        is_resolved: true,
        request_id: 1,
    });

    assert!(
        new_state.prs_state.thread_resolve_pending.is_none(),
        "pending must be cleared on success"
    );
    let Some(detail) = new_state.prs_state.pr_detail.as_ref() else {
        panic!("detail must remain");
    };
    let thread = &detail.reviews[0].review_threads[0];
    assert!(
        thread.is_resolved,
        "thread 0 is_resolved must be true after resolve succeeded"
    );
}

/// PrThreadResolveSucceeded for an out-of-range thread clears pending without
/// panic (graceful).
#[test]
fn thread_resolve_succeeded_out_of_range_clears_pending() {
    let mut state = state_with_two_threads();
    state.prs_state.thread_resolve_pending = Some(PrThreadResolvePending {
        scope_repo_id: RepositoryId("repo-1".to_string()),
        thread_index: 99,
        resolve: true,
        request_id: 1,
    });

    let new_state = state.apply(AppEvent::PrThreadResolveSucceeded {
        scope_repo_id: RepositoryId("repo-1".to_string()),
        thread_index: 99,
        is_resolved: true,
        request_id: 1,
    });

    assert!(
        new_state.prs_state.thread_resolve_pending.is_none(),
        "pending must clear even for out-of-range thread"
    );
}

/// PrThreadResolveSucceeded with a mismatched request_id is ignored (stale).
#[test]
fn thread_resolve_succeeded_stale_request_id_ignored() {
    let mut state = state_with_two_threads();
    state.prs_state.thread_resolve_pending = Some(PrThreadResolvePending {
        scope_repo_id: RepositoryId("repo-1".to_string()),
        thread_index: 0,
        resolve: true,
        request_id: 5,
    });

    let new_state = state.apply(AppEvent::PrThreadResolveSucceeded {
        scope_repo_id: RepositoryId("repo-1".to_string()),
        thread_index: 0,
        is_resolved: true,
        request_id: 99,
    });

    assert!(
        new_state.prs_state.thread_resolve_pending.is_some(),
        "stale request_id must not clear pending"
    );
}

// ── ThreadResolveFailed ───────────────────────────────────────────────────

/// PrThreadResolveFailed clears pending and sets an error message.
#[test]
fn thread_resolve_failed_clears_pending_and_sets_error() {
    let mut state = state_with_two_threads();
    state.prs_state.thread_resolve_pending = Some(PrThreadResolvePending {
        scope_repo_id: RepositoryId("repo-1".to_string()),
        thread_index: 0,
        resolve: true,
        request_id: 1,
    });

    let new_state = state.apply(AppEvent::PrThreadResolveFailed {
        scope_repo_id: RepositoryId("repo-1".to_string()),
        thread_index: 0,
        request_id: 1,
        error: "network error".to_string(),
    });

    assert!(
        new_state.prs_state.thread_resolve_pending.is_none(),
        "pending must be cleared on failure"
    );
    let Some(error) = new_state.prs_state.error.as_ref() else {
        panic!("error must be set on failure");
    };
    assert!(
        error.contains("network error"),
        "error must contain the failure message, got {error}"
    );
}

/// PrThreadResolveFailed with a stale request_id is ignored.
#[test]
fn thread_resolve_failed_stale_request_id_ignored() {
    let mut state = state_with_two_threads();
    state.prs_state.thread_resolve_pending = Some(PrThreadResolvePending {
        scope_repo_id: RepositoryId("repo-1".to_string()),
        thread_index: 0,
        resolve: true,
        request_id: 5,
    });

    let new_state = state.apply(AppEvent::PrThreadResolveFailed {
        scope_repo_id: RepositoryId("repo-1".to_string()),
        thread_index: 0,
        request_id: 99,
        error: "network error".to_string(),
    });

    assert!(
        new_state.prs_state.thread_resolve_pending.is_some(),
        "stale request_id must not clear pending"
    );
    assert!(
        new_state.prs_state.error.is_none(),
        "stale failure must not set error"
    );
}

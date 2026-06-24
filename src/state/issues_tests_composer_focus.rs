//! Issue #56 regression tests: comment composer focus and auto-scroll.
//!
//! Opening the new-comment composer must move detail subfocus to NewComment and
//! scroll the detail pane so the composer is visible; a successful CommentCreated
//! must scroll to reveal the new comment. Stale mutations and blocked
//! (exclusivity) composer-open attempts must not change subfocus or scroll.

use crate::domain::{IssueComment, IssueDetail, IssueState, Repository, RepositoryId};
use crate::state::AppState;
use crate::state::types::{AppEvent, ComposerTarget, DetailSubfocus, EditorTarget, InlineState};

fn issues_mode_state_with_repo(repo_id: &str) -> AppState {
    let mut state = AppState::default();
    state.repositories.push(Repository::new(
        RepositoryId(repo_id.to_string()),
        "Test Repo".to_string(),
        repo_id.to_string(),
        std::path::PathBuf::from("/tmp/test"),
    ));
    state.selected_repository_index = Some(0);
    state.apply(AppEvent::EnterIssuesMode)
}

fn p15_detail(number: u64) -> IssueDetail {
    IssueDetail {
        repo_owner_name: "owner/repo".to_string(),
        number,
        title: format!("Issue #{number}"),
        state: IssueState::Open,
        author_login: "user".to_string(),
        created_at: "2024-01-01T00:00:00Z".to_string(),
        updated_at: "2024-01-02T00:00:00Z".to_string(),
        labels: vec![],
        assignees: vec![],
        milestone: None,
        body: "Issue body".to_string(),
        external_url: format!("https://github.com/owner/repo/issues/{number}"),
        comments: vec![],
        has_more_comments: false,
        comments_cursor: None,
    }
}

fn p15_comment(comment_id: u64, author_login: &str, created_at: &str, body: &str) -> IssueComment {
    IssueComment {
        comment_id,
        author_login: author_login.to_string(),
        created_at: created_at.to_string(),
        edited_at: None,
        body: body.to_string(),
    }
}

fn p15_state_with_loaded_detail(repo_id: &RepositoryId, issue_number: u64) -> AppState {
    let mut state = issues_mode_state_with_repo("repo-1");
    state.mark_issue_detail_loading(repo_id.clone(), issue_number);
    state.apply(AppEvent::IssueDetailLoaded {
        scope_repo_id: repo_id.clone(),
        issue_number,
        request_id: 0,
        detail: Box::new(p15_detail(issue_number)),
    })
}

fn state_with_long_detail(repo_id: &RepositoryId, issue_number: u64) -> AppState {
    let mut detail = p15_detail(issue_number);
    detail.body = (0..30)
        .map(|line| format!("body line {line}"))
        .collect::<Vec<_>>()
        .join("\n");
    let mut state = issues_mode_state_with_repo("repo-1");
    state.mark_issue_detail_loading(repo_id.clone(), issue_number);
    state.apply(AppEvent::IssueDetailLoaded {
        scope_repo_id: repo_id.clone(),
        issue_number,
        request_id: 0,
        detail: Box::new(detail),
    })
}

/// Issue #56: Opening the new-comment composer moves detail subfocus to NewComment
/// so comment creation is consistent with the selected target regardless of where
/// the user pressed `c`.
#[test]
fn test_open_new_comment_composer_sets_subfocus_to_new_comment() {
    let repo_id = RepositoryId("repo-1".to_string());
    let mut state = p15_state_with_loaded_detail(&repo_id, 42);
    state.issues_state.detail_subfocus = DetailSubfocus::Body;

    let state = state.apply(AppEvent::OpenNewCommentComposer);

    assert_eq!(
        state.issues_state.detail_subfocus,
        DetailSubfocus::NewComment
    );
    assert!(matches!(
        state.issues_state.inline_state,
        InlineState::Composer {
            target: ComposerTarget::NewComment,
            ..
        }
    ));
}

/// Issue #56: Opening the new-comment composer scrolls the detail pane to the
/// bottom so the composer is visible (not rendered below the viewport).
#[test]
fn test_open_new_comment_composer_scrolls_to_bottom() {
    let repo_id = RepositoryId("repo-1".to_string());
    let mut state = state_with_long_detail(&repo_id, 42);
    state.issues_state.detail_viewport_rows = 5;
    state.issues_state.detail_scroll_offset = 0;

    let state = state.apply(AppEvent::OpenNewCommentComposer);

    assert_eq!(
        state.issues_state.detail_scroll_offset,
        state.issues_state.max_detail_scroll_offset()
    );
    assert!(
        state.issues_state.detail_scroll_offset > 0,
        "composer open should scroll down"
    );
}

/// Issue #56: When the composer is blocked by exclusivity (an editor is already
/// active), opening the new-comment composer must NOT change subfocus or scroll.
#[test]
fn test_open_new_comment_composer_blocked_does_not_change_subfocus_or_scroll() {
    let repo_id = RepositoryId("repo-1".to_string());
    let mut state = p15_state_with_loaded_detail(&repo_id, 42);
    state.issues_state.detail_subfocus = DetailSubfocus::Body;
    state.issues_state.detail_scroll_offset = 0;
    state.issues_state.inline_state = InlineState::Editor {
        target: EditorTarget::IssueBody,
        text: "editing".to_string(),
        cursor: 7,
    };

    let state = state.apply(AppEvent::OpenNewCommentComposer);

    assert_eq!(state.issues_state.detail_subfocus, DetailSubfocus::Body);
    assert_eq!(state.issues_state.detail_scroll_offset, 0);
    assert!(matches!(
        state.issues_state.inline_state,
        InlineState::Editor {
            target: EditorTarget::IssueBody,
            ..
        }
    ));
}

/// Issue #56: After a comment is successfully created, the detail viewport
/// scrolls to the bottom so the new comment is visible.
#[test]
fn test_comment_created_scrolls_to_bottom() {
    let repo_id = RepositoryId("repo-1".to_string());
    let state = state_with_long_detail(&repo_id, 42);
    let submitted_target = InlineState::Composer {
        target: ComposerTarget::NewComment,
        text: "fresh comment".to_string(),
        cursor: 13,
    };
    let mut state = state.apply(AppEvent::MutationSubmitted {
        scope_repo_id: repo_id.clone(),
        mutation_id: 1,
        target: submitted_target,
    });
    state.issues_state.detail_viewport_rows = 5;
    state.issues_state.detail_scroll_offset = 0;

    let state = state.apply(AppEvent::CommentCreated {
        scope_repo_id: repo_id,
        issue_number: 42,
        mutation_id: 1,
        comment: p15_comment(99, "bob", "2024-01-05T00:00:00Z", "fresh comment"),
    });

    let detail = state
        .issues_state
        .issue_detail
        .as_ref()
        .unwrap_or_else(|| panic!("expected detail"));
    assert_eq!(detail.comments.len(), 1);
    assert_eq!(detail.comments[0].body, "fresh comment");
    assert_eq!(state.issues_state.inline_state, InlineState::None);
    assert_eq!(
        state.issues_state.detail_scroll_offset,
        state.issues_state.max_detail_scroll_offset()
    );
    assert!(
        state.issues_state.detail_scroll_offset > 0,
        "CommentCreated should scroll down to reveal new comment"
    );
}

/// Issue #56: A stale CommentCreated (wrong issue_number) must NOT mutate the
/// detail or scroll the viewport.
#[test]
fn test_stale_comment_created_does_not_scroll() {
    let repo_id = RepositoryId("repo-1".to_string());
    let state = state_with_long_detail(&repo_id, 42);
    let submitted_target = InlineState::Composer {
        target: ComposerTarget::NewComment,
        text: "draft".to_string(),
        cursor: 5,
    };
    let mut state = state.apply(AppEvent::MutationSubmitted {
        scope_repo_id: repo_id.clone(),
        mutation_id: 1,
        target: submitted_target,
    });
    state.issues_state.detail_viewport_rows = 5;
    state.issues_state.detail_scroll_offset = 0;

    let state = state.apply(AppEvent::CommentCreated {
        scope_repo_id: repo_id,
        issue_number: 99,
        mutation_id: 1,
        comment: p15_comment(8, "bob", "2024-01-04T00:00:00Z", "stale"),
    });

    let detail = state
        .issues_state
        .issue_detail
        .as_ref()
        .unwrap_or_else(|| panic!("expected detail"));
    assert!(detail.comments.is_empty());
    assert_eq!(state.issues_state.detail_scroll_offset, 0);
}

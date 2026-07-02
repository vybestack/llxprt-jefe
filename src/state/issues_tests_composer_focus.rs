//! Issue #56 regression tests: comment composer focus and auto-scroll.
//!
//! Opening the new-comment composer must move detail subfocus to NewComment and
//! scroll the detail pane so the composer is visible; a successful CommentCreated
//! must scroll to reveal the new comment. Stale mutations and blocked
//! (exclusivity) composer-open attempts must not change subfocus or scroll.

use crate::domain::{IssueComment, IssueDetail, IssueState, Repository, RepositoryId};
use crate::state::AppState;
use crate::state::types::{
    AppEvent, ComposerTarget, DetailSubfocus, EditorTarget, InlineState, IssueMutationPending,
};

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

fn type_into_composer(mut state: AppState, text: &str) -> AppState {
    for ch in text.chars() {
        state = if ch == '\n' {
            state.apply(AppEvent::InlineNewline)
        } else {
            state.apply(AppEvent::InlineChar(ch))
        };
    }
    state
}

/// Typing into the Issues NewComment composer must not mutate parent document scroll.
#[test]
fn test_typing_in_issue_composer_does_not_mutate_detail_scroll_offset() {
    let repo_id = RepositoryId("repo-1".to_string());
    let mut state = state_with_long_detail(&repo_id, 42);
    state.issues_state.detail_viewport_rows = 5;
    let state = state.apply(AppEvent::OpenNewCommentComposer);
    let offset_after_open = state.issues_state.detail_scroll_offset;

    let state = type_into_composer(state, "l1\nl2\nl3\nl4\nl5\nl6\nl7\nl8");

    assert!(matches!(
        &state.issues_state.inline_state,
        InlineState::Composer { text, .. } if text == "l1\nl2\nl3\nl4\nl5\nl6\nl7\nl8"
    ));
    assert_eq!(state.issues_state.detail_scroll_offset, offset_after_open);
}

/// Arrowing inside the Issues composer must not mutate parent document scroll.
#[test]
fn test_arrowing_in_issue_composer_does_not_mutate_detail_scroll_offset() {
    let repo_id = RepositoryId("repo-1".to_string());
    let mut state = state_with_long_detail(&repo_id, 42);
    state.issues_state.detail_viewport_rows = 5;
    let state = state.apply(AppEvent::OpenNewCommentComposer);
    let offset_after_open = state.issues_state.detail_scroll_offset;
    let typed = "l1\nl2\nl3\nl4\nl5\nl6\nl7\nl8";
    let mut state = type_into_composer(state, typed);

    for event in [
        AppEvent::InlineCursorLeft,
        AppEvent::InlineCursorRight,
        AppEvent::InlineCursorUp,
        AppEvent::InlineCursorDown,
    ] {
        for _ in 0..typed.chars().count() {
            state = state.apply(event.clone());
        }
        assert_eq!(state.issues_state.detail_scroll_offset, offset_after_open);
        assert!(matches!(
            &state.issues_state.inline_state,
            InlineState::Composer { .. }
        ));
    }
}

/// Backspacing inside the Issues composer must not mutate parent document scroll.
#[test]
fn test_backspacing_in_issue_composer_does_not_mutate_detail_scroll_offset() {
    let repo_id = RepositoryId("repo-1".to_string());
    let mut state = state_with_long_detail(&repo_id, 42);
    state.issues_state.detail_viewport_rows = 5;
    let state = state.apply(AppEvent::OpenNewCommentComposer);
    let offset_after_open = state.issues_state.detail_scroll_offset;
    let typed = "l1\nl2\nl3\nl4\nl5\nl6\nl7\nl8";
    let mut state = type_into_composer(state, typed);

    for _ in 0..typed.chars().count() {
        state = state.apply(AppEvent::InlineBackspace);
    }
    assert!(matches!(
        &state.issues_state.inline_state,
        InlineState::Composer { text, .. } if text.is_empty()
    ));

    assert_eq!(state.issues_state.detail_scroll_offset, offset_after_open);
}

/// Esc/Ctrl-C cancel intent must remain responsive while comment submission is pending.
#[test]
fn test_inline_cancel_clears_pending_issue_comment_mutation() {
    let repo_id = RepositoryId("repo-1".to_string());
    let mut state = state_with_long_detail(&repo_id, 42).apply(AppEvent::OpenNewCommentComposer);
    let pending_target = state.issues_state.inline_state.clone();
    state.issues_state.mutation_pending = Some(IssueMutationPending {
        scope_repo_id: repo_id,
        id: 7,
        target: pending_target,
    });

    let state = state.apply(AppEvent::InlineCancelOrEsc);

    assert_eq!(state.issues_state.inline_state, InlineState::None);
    assert!(state.issues_state.mutation_pending.is_none());
}

/// Opening a reply composer reveals the stable reply anchor above the TextBox.
#[test]
fn test_open_reply_composer_reveals_reply_anchor() {
    let repo_id = RepositoryId("repo-1".to_string());
    let mut state = state_with_long_detail(&repo_id, 42);
    state.issues_state.detail_viewport_rows = 8;
    state.issues_state.detail_scroll_offset = 0;
    state
        .issues_state
        .issue_detail
        .as_mut()
        .unwrap_or_else(|| panic!("expected detail"))
        .comments
        .push(p15_comment(
            1,
            "alice",
            "2026-07-01T00:00:00Z",
            "comment body",
        ));

    let state = state.apply(AppEvent::OpenReplyComposer { comment_index: 0 });
    let detail = state
        .issues_state
        .issue_detail
        .as_ref()
        .unwrap_or_else(|| panic!("expected detail"));
    let content = crate::issue_detail_content::build_detail_content(
        detail,
        state.issues_state.detail_subfocus,
        &state.issues_state.inline_state,
        state.issues_state.loading.comments,
    );
    let anchor_line = content
        .text
        .lines()
        .position(|line| line == crate::issue_detail_content::ISSUE_REPLY_ANCHOR)
        .unwrap_or_else(|| panic!("reply anchor should render"));
    let document_rows = crate::layout::issue_detail_document_viewport_rows(
        state.issues_state.detail_viewport_rows,
        true,
    );

    assert!(state.issues_state.detail_scroll_offset > 0);
    assert!(anchor_line >= state.issues_state.detail_scroll_offset);
    assert!(anchor_line < state.issues_state.detail_scroll_offset + document_rows);
}
/// A blocked reply-open attempt must not perform another parent scroll reveal.
#[test]
fn test_blocked_reply_composer_open_does_not_mutate_scroll() {
    let repo_id = RepositoryId("repo-1".to_string());
    let mut state = state_with_long_detail(&repo_id, 42);
    state.issues_state.detail_viewport_rows = 8;
    state
        .issues_state
        .issue_detail
        .as_mut()
        .unwrap_or_else(|| panic!("expected detail"))
        .comments
        .push(p15_comment(
            1,
            "alice",
            "2026-07-01T00:00:00Z",
            "comment body",
        ));
    let mut state = state.apply(AppEvent::OpenReplyComposer { comment_index: 0 });
    state.issues_state.detail_scroll_offset = 0;

    let state = state.apply(AppEvent::OpenReplyComposer { comment_index: 0 });

    assert_eq!(state.issues_state.detail_scroll_offset, 0);
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

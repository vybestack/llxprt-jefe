//! Issue #406: Home/End cursor movement tests for the Issues inline
//! composer/editor.
//!
//! Home moves the caret to the start of the current logical line; End moves it
//! to the end of the current logical line. These mirror the existing
//! CursorUp/CursorDown line-aware movement, reusing the shared UTF-8-safe
//! helpers so multibyte text never lands mid-codepoint.

use crate::domain::{IssueDetail, IssueState, Repository, RepositoryId};
use crate::state::AppState;
use crate::state::events::AppEvent;
use crate::state::types::{ComposerTarget, InlineState};

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

fn detail(number: u64) -> IssueDetail {
    IssueDetail {
        repo_owner_name: "owner/repo".to_string(),
        number,
        node_id: String::new(),
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
        comments: crate::domain::PaginatedList::from_loaded(
            crate::domain::CommentDetailIdentity {
                scope_repo_id: crate::domain::RepositoryId::default(),
                number,
            },
            vec![],
            crate::domain::PageToken::from_cursor(None, false),
        ),
        issue_type_name: None,
        state_reason: None,
    }
}

fn state_with_loaded_detail(repo_id: &RepositoryId, issue_number: u64) -> AppState {
    let mut state = issues_mode_state_with_repo("repo-1");
    state.mark_issue_detail_loading(repo_id.clone(), issue_number);
    state.apply(AppEvent::IssueDetailLoaded {
        scope_repo_id: repo_id.clone(),
        issue_number,
        request_id: 0,
        detail: Box::new(detail(issue_number)),
    })
}

fn open_new_comment_composer(state: AppState) -> AppState {
    state.apply(AppEvent::OpenNewCommentComposer)
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

fn move_cursor_left(state: AppState, steps: usize) -> AppState {
    let mut s = state;
    for _ in 0..steps {
        s = s.apply(AppEvent::InlineCursorLeft);
    }
    s
}

fn composer_text_cursor(state: &AppState) -> (String, usize) {
    match &state.issues_state.inline_state {
        InlineState::Composer { text, cursor, .. } | InlineState::Editor { text, cursor, .. } => {
            (text.clone(), *cursor)
        }
        InlineState::None => panic!("expected an active composer/editor"),
    }
}

/// Home on a single-line composer moves the caret to byte 0 (start of line).
#[test]
fn home_on_single_line_moves_to_start() {
    let repo_id = RepositoryId("repo-1".to_string());
    let state = state_with_loaded_detail(&repo_id, 42);
    let state = open_new_comment_composer(state);
    let state = type_into_composer(state, "abcdef");
    // Caret at end (byte 6); Home must jump to byte 0.
    let state = state.apply(AppEvent::InlineCursorHome);
    let (_text, cursor) = composer_text_cursor(&state);
    assert_eq!(
        cursor, 0,
        "Home must move the caret to byte 0 on a single line"
    );
}

/// End on a single-line composer moves the caret to text.len() (end of line).
#[test]
fn end_on_single_line_moves_to_end() {
    let repo_id = RepositoryId("repo-1".to_string());
    let state = state_with_loaded_detail(&repo_id, 42);
    let state = open_new_comment_composer(state);
    let state = type_into_composer(state, "abcdef");
    // Walk back to the middle, then End must return to the end.
    let state = move_cursor_left(state, 3);
    let state = state.apply(AppEvent::InlineCursorEnd);
    let (text, cursor) = composer_text_cursor(&state);
    assert_eq!(cursor, text.len(), "End must move the caret to text.len()");
}

/// Home on a multi-line composer moves the caret to the start of the CURRENT
/// line (not the whole document).
#[test]
fn home_on_multiline_moves_to_current_line_start() {
    let repo_id = RepositoryId("repo-1".to_string());
    let state = state_with_loaded_detail(&repo_id, 42);
    let state = open_new_comment_composer(state);
    // "abcd\nefgh" — caret lands at byte 9 (end of second line).
    let state = type_into_composer(state, "abcd\nefgh");
    // Home must move to byte 5 (start of "efgh"), NOT byte 0.
    let state = state.apply(AppEvent::InlineCursorHome);
    let (_text, cursor) = composer_text_cursor(&state);
    assert_eq!(
        cursor, 5,
        "Home must move to the start of the current line (byte 5), not the document start"
    );
}

/// End on a multi-line composer moves the caret to the end of the CURRENT
/// line (not the whole document).
#[test]
fn end_on_multiline_moves_to_current_line_end() {
    let repo_id = RepositoryId("repo-1".to_string());
    let state = state_with_loaded_detail(&repo_id, 42);
    let state = open_new_comment_composer(state);
    // "abcd\nefgh" — caret lands at byte 9.
    let state = type_into_composer(state, "abcd\nefgh");
    // Move to the first line (CursorUp), then End must land at byte 4 (end of
    // "abcd"), NOT byte 9.
    let state = state.apply(AppEvent::InlineCursorUp);
    let state = state.apply(AppEvent::InlineCursorEnd);
    let (_text, cursor) = composer_text_cursor(&state);
    assert_eq!(
        cursor, 4,
        "End must move to the end of the current line (byte 4), not the document end"
    );
}

/// Home/End must be UTF-8 safe: a multibyte caret never lands mid-codepoint.
/// "héllo" — 'é' is two bytes; Home from the middle must land on byte 0, and
/// End must land on byte 6 (the byte length, after the final 'o').
#[test]
fn home_end_are_utf8_safe() {
    let repo_id = RepositoryId("repo-1".to_string());
    let state = state_with_loaded_detail(&repo_id, 42);
    let state = open_new_comment_composer(state);
    let state = type_into_composer(state, "héllo");
    // Caret at byte 6; Home -> 0.
    let state = state.apply(AppEvent::InlineCursorHome);
    let (_text, cursor) = composer_text_cursor(&state);
    assert_eq!(cursor, 0, "Home on multibyte text must land on byte 0");
    // End -> byte length (6).
    let state = state.apply(AppEvent::InlineCursorEnd);
    let (text, cursor) = composer_text_cursor(&state);
    assert_eq!(
        cursor,
        text.len(),
        "End on multibyte text must land on the byte length"
    );
}

/// Home on the first line of an empty composer is a safe no-op (caret stays
/// at 0).
#[test]
fn home_on_empty_composer_is_noop() {
    let repo_id = RepositoryId("repo-1".to_string());
    let state = state_with_loaded_detail(&repo_id, 42);
    let state = open_new_comment_composer(state);
    let state = state.apply(AppEvent::InlineCursorHome);
    let (_text, cursor) = composer_text_cursor(&state);
    assert_eq!(cursor, 0, "Home on an empty composer must be a no-op");
}

/// Home/End also work in the inline Editor (e.g. editing an issue body), not
/// just the Composer.
#[test]
fn home_end_work_in_inline_editor() {
    let repo_id = RepositoryId("repo-1".to_string());
    let mut state = state_with_loaded_detail(&repo_id, 42);
    // Open the inline editor for the issue body, then type to extend the text.
    state.issues_state.inline_state = InlineState::Editor {
        target: crate::state::types::EditorTarget::IssueBody,
        text: "line1\nline2".to_string(),
        cursor: 11, // end of text
    };
    let state = state.apply(AppEvent::InlineCursorHome);
    let (_text, cursor) = composer_text_cursor(&state);
    assert_eq!(
        cursor, 6,
        "Home in the editor must move to the start of the current line (byte 6)"
    );
    let state = state.apply(AppEvent::InlineCursorEnd);
    let (text, cursor) = composer_text_cursor(&state);
    assert_eq!(
        cursor,
        text.len(),
        "End in the editor must move to the end of the current line"
    );
    // Confirm the composer target type is preserved (Editor, not Composer).
    assert!(matches!(
        state.issues_state.inline_state,
        InlineState::Editor { .. }
    ));
    let _ = ComposerTarget::NewComment; // silence unused-import if Editor path changes
}

//! Issue #406: Home/End cursor movement tests for form text fields.
//!
//! FormMoveCursorStart moves the focused field's cursor to char index 0;
//! FormMoveCursorEnd moves it to the field's char count. These apply to all
//! text-entry form fields (repository, agent, workflow-dispatch).

use crate::domain::RepositoryId;
use crate::state::AppState;
use crate::state::events::AppEvent;
use crate::state::transition::TransitionExt;
use crate::state::types::ModalState;

/// A repository row suitable for seeding AppState in form tests.
fn seed_repository() -> crate::domain::Repository {
    crate::domain::Repository::new(
        RepositoryId("repo-1".to_string()),
        crate::domain::shipped_agent_type(3),
        crate::domain::TypedMap::new(),
        "Test Repo".to_string(),
        "repo-1".to_string(),
        std::path::PathBuf::from("/tmp/test"),
    )
}

/// Open a NewAgent modal and type into the Name field so the cursor is at the
/// end of the typed text, ready for a Home/End assertion.
fn new_agent_form_with_typed_name(text: &str) -> AppState {
    let mut state = AppState {
        repositories: vec![seed_repository()],
        ..AppState::default()
    };
    state = state
        .apply(AppEvent::OpenNewAgent(RepositoryId("repo-1".to_owned())))
        .committed_pure();
    // Move focus to the Name field (default focus is Shortcut).
    state = state.apply(AppEvent::FormNextField).committed_pure();
    for ch in text.chars() {
        state = state.apply(AppEvent::FormChar(ch)).committed_pure();
    }
    state
}

/// Extract the agent Name cursor from a NewAgent modal.
fn agent_name_cursor(state: &AppState) -> usize {
    let ModalState::NewAgent { cursor, .. } = &state.modal else {
        panic!("expected NewAgent modal, got {:?}", state.modal);
    };
    cursor.name
}

/// Home moves the focused agent Name cursor to char index 0.
#[test]
fn form_home_moves_agent_name_cursor_to_start() {
    let state = new_agent_form_with_typed_name("hello");
    // Walk the cursor into the middle.
    let state = state.apply(AppEvent::FormMoveCursorLeft).committed_pure();
    let state = state.apply(AppEvent::FormMoveCursorLeft).committed_pure();
    // After two left-moves the cursor should be at 3 (middle of "hello").
    assert_eq!(
        agent_name_cursor(&state),
        3,
        "cursor should be at char 3 after two left-moves"
    );
    // Home -> 0.
    let state = state.apply(AppEvent::FormMoveCursorStart).committed_pure();
    assert_eq!(
        agent_name_cursor(&state),
        0,
        "Home must move to char index 0"
    );
}

/// End moves the focused agent Name cursor to the field's char count.
#[test]
fn form_end_moves_agent_name_cursor_to_end() {
    let state = new_agent_form_with_typed_name("hello");
    // Walk back to the start, then End -> char count (5).
    let state = state.apply(AppEvent::FormMoveCursorStart).committed_pure();
    let state = state.apply(AppEvent::FormMoveCursorEnd).committed_pure();
    assert_eq!(
        agent_name_cursor(&state),
        5,
        "End must move to the field's char count"
    );
}

/// Home/End on a multibyte agent name never lands mid-codepoint (the cursor
/// is char-count based, so this is structurally safe).
#[test]
fn form_home_end_utf8_safe_agent_name() {
    // "héllo" is 5 chars but 6 bytes; the cursor counts chars.
    let state = new_agent_form_with_typed_name("héllo");
    let state = state.apply(AppEvent::FormMoveCursorStart).committed_pure();
    assert_eq!(agent_name_cursor(&state), 0, "Home -> 0");
    let state = state.apply(AppEvent::FormMoveCursorEnd).committed_pure();
    assert_eq!(agent_name_cursor(&state), 5, "End -> char count (5)");
}

/// Home/End also works on a repository text field (BaseDir focus).
#[test]
fn form_home_end_repository_basedir() {
    let mut state = AppState {
        repositories: vec![seed_repository()],
        ..AppState::default()
    };
    state = state.apply(AppEvent::OpenNewRepository).committed_pure();
    // Move focus to BaseDir (Tab advances field focus).
    state = state.apply(AppEvent::FormNextField).committed_pure();
    // Type some text into BaseDir.
    for ch in "my/path".chars() {
        state = state.apply(AppEvent::FormChar(ch)).committed_pure();
    }
    // Home -> 0, End -> char count.
    let state = state.apply(AppEvent::FormMoveCursorStart).committed_pure();
    let ModalState::NewRepository { cursor, .. } = &state.modal else {
        panic!("expected NewRepository modal");
    };
    assert_eq!(cursor.base_dir, 0, "Home on BaseDir -> 0");
    let state = state.apply(AppEvent::FormMoveCursorEnd).committed_pure();
    let ModalState::NewRepository { cursor, .. } = &state.modal else {
        panic!("expected NewRepository modal");
    };
    assert_eq!(cursor.base_dir, 7, "End on BaseDir -> char count (7)");
}

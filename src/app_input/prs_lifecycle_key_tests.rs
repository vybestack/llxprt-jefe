//! Key-layer tests for the PR lifecycle actions (issue #183): closing from the
//! list, deleting a pull request, and opening the New PR composer.

use super::*;
use jefe::state::{
    NewPrFormState, PrDeleteConfirmState, PrLifecycleEvent, PrPropertyKind, PullRequestsState,
    ScreenId,
};

/// The lifecycle event a key resolved to, if it resolved to one.
fn lifecycle_of(event: Option<AppEvent>) -> Option<PrLifecycleEvent> {
    match event {
        Some(AppEvent::PrLifecycle(inner)) => Some(*inner),
        _ => None,
    }
}

fn key(code: KeyCode) -> KeyEvent {
    KeyEvent::new(KeyEventKind::Press, code)
}

fn prs_list_state() -> AppState {
    AppState {
        nav: crate::state::navigation::NavState::rooted(ScreenId::PullRequests),
        prs_state: PullRequestsState {
            active: true,
            pr_focus: PrFocus::PrList,
            ..PullRequestsState::default()
        },
        ..AppState::default()
    }
}

// ── Closing from the list (A1) ─────────────────────────────────────────────

#[test]
fn shift_w_in_the_list_opens_the_state_editor() {
    let state = prs_list_state();
    let event = resolve_prs_key_event(&state, &key(KeyCode::Char('W')));
    assert!(
        matches!(
            event,
            Some(AppEvent::PrOpenPropertyEditor {
                kind: PrPropertyKind::State
            })
        ),
        "Shift+W in the PR list must reach the one close/reopen path (got {event:?})"
    );
}

#[test]
fn the_list_does_not_bind_the_other_property_keys() {
    let state = prs_list_state();
    for character in ['L', 'A', 'M', 'T'] {
        let event = resolve_prs_key_event(&state, &key(KeyCode::Char(character)));
        assert!(
            !matches!(event, Some(AppEvent::PrOpenPropertyEditor { .. })),
            "Shift+{character} must stay a detail action (got {event:?})"
        );
    }
}

#[test]
fn the_repository_pane_does_not_bind_the_close_key() {
    let mut state = prs_list_state();
    state.prs_state.pr_focus = PrFocus::RepoList;
    let event = resolve_prs_key_event(&state, &key(KeyCode::Char('W')));
    assert!(
        !matches!(event, Some(AppEvent::PrOpenPropertyEditor { .. })),
        "the repository pane owns no pull request to close (got {event:?})"
    );
}

// ── Deleting (A2, A3, A4, A9) ──────────────────────────────────────────────

fn with_delete_overlay(awaiting_confirmation: bool) -> AppState {
    let mut state = prs_list_state();
    state.prs_state.delete_confirm = Some(PrDeleteConfirmState {
        pr_number: 42,
        head_ref: "feature".to_string(),
        base_ref: "main".to_string(),
        is_open: true,
        awaiting_confirmation,
    });
    state
}

#[test]
fn shift_d_in_the_list_opens_the_delete_overlay() {
    let event = resolve_prs_key_event(&prs_list_state(), &key(KeyCode::Char('D')));
    assert!(
        matches!(
            lifecycle_of(event),
            Some(PrLifecycleEvent::OpenDeleteConfirm)
        ),
        "Shift+D in the PR list must open the destructive overlay"
    );
}

#[test]
fn shift_d_in_the_detail_view_opens_the_delete_overlay() {
    let mut state = prs_list_state();
    state.prs_state.pr_focus = PrFocus::PrDetail;
    state.prs_state.detail_subfocus = PrDetailSubfocus::Body;
    let event = resolve_prs_key_event(&state, &key(KeyCode::Char('D')));
    assert!(matches!(
        lifecycle_of(event),
        Some(PrLifecycleEvent::OpenDeleteConfirm)
    ));
}

#[test]
fn a_focused_review_thread_does_not_bind_the_delete_key() {
    let mut state = prs_list_state();
    state.prs_state.pr_focus = PrFocus::PrDetail;
    state.prs_state.detail_subfocus = PrDetailSubfocus::Review(0);
    let event = resolve_prs_key_event(&state, &key(KeyCode::Char('D')));
    assert!(
        !matches!(
            lifecycle_of(event),
            Some(PrLifecycleEvent::OpenDeleteConfirm)
        ),
        "a review is not the pull request"
    );
}

#[test]
fn enter_on_the_open_overlay_confirms_it() {
    let event = resolve_prs_key_event(&with_delete_overlay(false), &key(KeyCode::Enter));
    assert!(matches!(
        lifecycle_of(event),
        Some(PrLifecycleEvent::DeleteConfirm)
    ));
}

#[test]
fn enter_on_the_armed_overlay_confirms_it() {
    let event = resolve_prs_key_event(&with_delete_overlay(true), &key(KeyCode::Enter));
    assert!(matches!(
        lifecycle_of(event),
        Some(PrLifecycleEvent::DeleteConfirm)
    ));
}

#[test]
fn escape_on_the_overlay_cancels_it() {
    let event = resolve_prs_key_event(&with_delete_overlay(true), &key(KeyCode::Esc));
    assert!(
        matches!(lifecycle_of(event), Some(PrLifecycleEvent::DeleteCancel)),
        "Esc must cancel rather than fall through to leaving the detail view"
    );
}

#[test]
fn the_overlay_swallows_the_keys_that_would_otherwise_navigate() {
    let state = with_delete_overlay(false);
    for code in [KeyCode::Char('W'), KeyCode::Char('D'), KeyCode::Char('m')] {
        let event = resolve_prs_key_event(&state, &key(code));
        assert!(
            event.is_none(),
            "the destructive overlay owns the keyboard (got {event:?} for {code:?})"
        );
    }
}

// ── The New PR composer (A10, A12-A16) ─────────────────────────────────────

fn with_composer() -> AppState {
    let mut state = prs_list_state();
    state.prs_state.new_pr_form = Some(NewPrFormState {
        branches: vec!["main".to_string(), "topic".to_string()],
        head_index: 1,
        ..NewPrFormState::default()
    });
    state
}

#[test]
fn n_in_the_list_opens_the_composer() {
    for character in ['n', 'N'] {
        let event = resolve_prs_key_event(&prs_list_state(), &key(KeyCode::Char(character)));
        assert!(
            matches!(lifecycle_of(event), Some(PrLifecycleEvent::OpenNewForm)),
            "'{character}' in the PR list must open the New PR composer"
        );
    }
}

#[test]
fn tab_and_backtab_walk_the_composer_fields() {
    let state = with_composer();
    assert!(matches!(
        lifecycle_of(resolve_prs_key_event(&state, &key(KeyCode::Tab))),
        Some(PrLifecycleEvent::NewFormFocusNext)
    ));
    assert!(matches!(
        lifecycle_of(resolve_prs_key_event(&state, &key(KeyCode::BackTab))),
        Some(PrLifecycleEvent::NewFormFocusPrevious)
    ));
}

#[test]
fn the_arrow_keys_move_the_branch_selection() {
    let state = with_composer();
    assert!(matches!(
        lifecycle_of(resolve_prs_key_event(&state, &key(KeyCode::Up))),
        Some(PrLifecycleEvent::NewFormBranchUp)
    ));
    assert!(matches!(
        lifecycle_of(resolve_prs_key_event(&state, &key(KeyCode::Down))),
        Some(PrLifecycleEvent::NewFormBranchDown)
    ));
}

#[test]
fn typing_reaches_the_composer_rather_than_the_pull_request_list() {
    let state = with_composer();
    assert!(
        matches!(
            lifecycle_of(resolve_prs_key_event(&state, &key(KeyCode::Char('W')))),
            Some(PrLifecycleEvent::NewFormChar('W'))
        ),
        "an open composer must swallow the keys the list binds"
    );
    assert!(matches!(
        lifecycle_of(resolve_prs_key_event(&state, &key(KeyCode::Backspace))),
        Some(PrLifecycleEvent::NewFormBackspace)
    ));
}

#[test]
fn a_bare_enter_breaks_the_line_and_a_chorded_one_submits() {
    let state = with_composer();
    assert!(matches!(
        lifecycle_of(resolve_prs_key_event(&state, &key(KeyCode::Enter))),
        Some(PrLifecycleEvent::NewFormNewline)
    ));

    let mut submit = key(KeyCode::Enter);
    submit.modifiers = KeyModifiers::CONTROL;
    assert!(matches!(
        lifecycle_of(resolve_prs_key_event(&state, &submit)),
        Some(PrLifecycleEvent::NewFormSubmit)
    ));
}

#[test]
fn escape_cancels_the_composer() {
    let state = with_composer();
    assert!(matches!(
        lifecycle_of(resolve_prs_key_event(&state, &key(KeyCode::Esc))),
        Some(PrLifecycleEvent::NewFormCancel)
    ));
}

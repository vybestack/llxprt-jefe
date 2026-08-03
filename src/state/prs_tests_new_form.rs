//! Reducer tests for the New PR composer (issue #183).
//!
//! The composer is driven from the pull-request list: pick a head branch, a
//! base branch, a title and a body, then open the pull request. Everything the
//! composer refuses to send is decided here, before any request is built.

use super::prs_test_fixtures::prs_state_with_detail;
use super::types::PrFocus;
use crate::domain::RepositoryId;
use crate::state::transition::TransitionExt;
use crate::state::{AppState, NewPrFormFocus, NewPrFormState, PrLifecycleEvent};

fn lifecycle(state: AppState, event: PrLifecycleEvent) -> AppState {
    state.apply(event.into()).committed_pure()
}

fn list_focused() -> AppState {
    let mut state = prs_state_with_detail("repo-1", 42);
    state.prs_state.pr_focus = PrFocus::PrList;
    state
}

fn form(state: &AppState) -> &NewPrFormState {
    state
        .prs_state
        .new_pr_form
        .as_ref()
        .unwrap_or_else(|| panic!("the composer must be open"))
}

/// Open the composer and answer its branch load.
fn composer_with_branches(branches: &[&str], default_branch: Option<&str>) -> AppState {
    let state = lifecycle(list_focused(), PrLifecycleEvent::OpenNewForm);
    let request_id = form(&state).load_request_id;
    lifecycle(
        state,
        PrLifecycleEvent::BranchesLoaded {
            scope_repo_id: RepositoryId("repo-1".to_string()),
            request_id,
            branches: branches.iter().map(|b| (*b).to_string()).collect(),
            default_branch: default_branch.map(str::to_string),
        },
    )
}

fn ready_composer() -> AppState {
    composer_with_branches(&["main", "feature/login", "feature/logout"], Some("main"))
}

fn type_text(state: AppState, text: &str) -> AppState {
    text.chars().fold(state, |state, character| {
        lifecycle(state, PrLifecycleEvent::NewFormChar(character))
    })
}

// ── Opening (A10) ──────────────────────────────────────────────────────────

#[test]
fn the_composer_opens_from_the_list_and_asks_for_the_branches() {
    let state = lifecycle(list_focused(), PrLifecycleEvent::OpenNewForm);

    let form = form(&state);
    assert!(
        form.branches_loading,
        "the branch list is fetched as the composer opens"
    );
    assert_eq!(form.focus, NewPrFormFocus::Head);
    assert!(form.title_text.is_empty());
    assert!(form.body_text.is_empty());
}

#[test]
fn the_composer_does_not_open_from_the_repository_pane() {
    let mut state = list_focused();
    state.prs_state.pr_focus = PrFocus::RepoList;
    let state = lifecycle(state, PrLifecycleEvent::OpenNewForm);

    assert!(state.prs_state.new_pr_form.is_none());
}

#[test]
fn the_composer_does_not_open_over_another_overlay() {
    let state = lifecycle(list_focused(), PrLifecycleEvent::OpenDeleteConfirm);
    let state = lifecycle(state, PrLifecycleEvent::OpenNewForm);

    assert!(state.prs_state.new_pr_form.is_none());
}

#[test]
fn the_composer_refuses_to_open_without_a_repository_to_open_against() {
    let mut state = list_focused();
    state.selected_repository_index = None;

    let state = lifecycle(state, PrLifecycleEvent::OpenNewForm);

    assert!(
        state.prs_state.new_pr_form.is_none(),
        "a composer with no scope could never finish loading its branches"
    );
    assert!(
        state
            .prs_state
            .error
            .as_deref()
            .is_some_and(|e| e.contains("repository")),
        "got: {:?}",
        state.prs_state.error
    );
}

#[test]
fn cancelling_discards_the_draft() {
    let state = type_text(ready_composer(), "x");
    let state = lifecycle(state, PrLifecycleEvent::NewFormCancel);

    assert!(state.prs_state.new_pr_form.is_none());
}

// ── Branch loading (A11) ───────────────────────────────────────────────────

#[test]
fn the_default_branch_is_the_base_and_something_else_is_the_head() {
    let state = ready_composer();

    let form = form(&state);
    assert!(!form.branches_loading);
    assert_eq!(form.branches[form.base_index], "main");
    assert_ne!(
        form.branches[form.head_index], "main",
        "a pull request from main into main is never what was meant"
    );
}

#[test]
fn a_repository_without_a_default_branch_still_offers_two_different_branches() {
    let state = composer_with_branches(&["alpha", "beta"], None);

    let form = form(&state);
    assert_eq!(form.branches.len(), 2);
    assert!(form.error.is_none());
    assert_ne!(
        form.head_index, form.base_index,
        "with more than one branch the composer must not open head against head"
    );
}

#[test]
fn the_composer_does_not_open_while_a_merge_is_still_in_flight() {
    let mut state = list_focused();
    state.prs_state.merge_mutation_pending = Some(crate::state::PrMergeMutationPending {
        scope_repo_id: RepositoryId("repo-1".to_string()),
        mutation_id: 1,
        pr_number: 42,
        method: crate::domain::MergeMethod::Merge,
    });

    let state = lifecycle(state, PrLifecycleEvent::OpenNewForm);

    assert!(
        state.prs_state.new_pr_form.is_none(),
        "a merge with no overlay left on screen still owns the pull request"
    );
}

#[test]
fn a_single_branch_repository_leaves_head_and_base_equal() {
    let state = composer_with_branches(&["main"], Some("main"));

    let form = form(&state);
    assert_eq!(form.head_index, form.base_index);
}

#[test]
fn a_failed_branch_load_is_reported_and_blocks_the_composer() {
    let state = lifecycle(list_focused(), PrLifecycleEvent::OpenNewForm);
    let request_id = form(&state).load_request_id;
    let state = lifecycle(
        state,
        PrLifecycleEvent::BranchesFailed {
            scope_repo_id: RepositoryId("repo-1".to_string()),
            request_id,
            error: "Bad credentials".to_string(),
        },
    );

    let form = form(&state);
    assert!(!form.branches_loading);
    assert!(
        form.error
            .as_deref()
            .is_some_and(|e| e.contains("Bad credentials")),
        "got: {:?}",
        form.error
    );
}

#[test]
fn a_stale_branch_load_is_ignored() {
    let state = lifecycle(list_focused(), PrLifecycleEvent::OpenNewForm);
    let request_id = form(&state).load_request_id;
    let state = lifecycle(
        state,
        PrLifecycleEvent::BranchesLoaded {
            scope_repo_id: RepositoryId("repo-1".to_string()),
            request_id: request_id.wrapping_add(1),
            branches: vec!["ghost".to_string()],
            default_branch: None,
        },
    );

    assert!(
        form(&state).branches.is_empty(),
        "a load for a composer that is no longer open must not fill this one"
    );
}

// ── Moving around (A12, A13) ───────────────────────────────────────────────

#[test]
fn focus_cycles_through_every_field_and_back() {
    let mut state = ready_composer();
    for expected in [
        NewPrFormFocus::Base,
        NewPrFormFocus::Title,
        NewPrFormFocus::Body,
        NewPrFormFocus::Head,
    ] {
        state = lifecycle(state, PrLifecycleEvent::NewFormFocusNext);
        assert_eq!(form(&state).focus, expected);
    }
}

#[test]
fn focus_walks_backwards_too() {
    let state = lifecycle(ready_composer(), PrLifecycleEvent::NewFormFocusPrevious);
    assert_eq!(form(&state).focus, NewPrFormFocus::Body);
}

#[test]
fn the_branch_selection_moves_only_in_the_focused_branch_field() {
    let state = ready_composer();
    let base_before = form(&state).base_index;
    let state = lifecycle(state, PrLifecycleEvent::NewFormBranchDown);

    assert_eq!(
        form(&state).base_index,
        base_before,
        "moving the head selection must not move the base"
    );
    assert_eq!(form(&state).head_index, 2);
}

#[test]
fn the_branch_selection_stops_at_the_ends_of_the_list() {
    let state = ready_composer();
    let state = lifecycle(state, PrLifecycleEvent::NewFormBranchUp);
    assert_eq!(form(&state).head_index, 0, "clamped at the first branch");

    let state = (0..10).fold(state, |state, _| {
        lifecycle(state, PrLifecycleEvent::NewFormBranchDown)
    });
    assert_eq!(form(&state).head_index, 2, "clamped at the last branch");
}

#[test]
fn the_branch_keys_do_nothing_in_a_text_field() {
    let state = lifecycle(ready_composer(), PrLifecycleEvent::NewFormFocusNext);
    let state = lifecycle(state, PrLifecycleEvent::NewFormFocusNext);
    let head_before = form(&state).head_index;
    let state = lifecycle(state, PrLifecycleEvent::NewFormBranchDown);

    assert_eq!(form(&state).head_index, head_before);
}

// ── Typing (A14) ───────────────────────────────────────────────────────────

fn focused_on(state: AppState, focus: NewPrFormFocus) -> AppState {
    let mut state = state;
    for _ in 0..4 {
        if form(&state).focus == focus {
            return state;
        }
        state = lifecycle(state, PrLifecycleEvent::NewFormFocusNext);
    }
    panic!("focus {focus:?} is unreachable");
}

#[test]
fn typing_lands_in_the_focused_text_field() {
    let state = focused_on(ready_composer(), NewPrFormFocus::Title);
    let state = type_text(state, "Add login");
    assert_eq!(form(&state).title_text, "Add login");
    assert!(form(&state).body_text.is_empty());

    let state = focused_on(state, NewPrFormFocus::Body);
    let state = type_text(state, "why");
    assert_eq!(form(&state).body_text, "why");
    assert_eq!(form(&state).title_text, "Add login");
}

#[test]
fn typing_is_ignored_on_a_branch_field() {
    let state = type_text(ready_composer(), "abc");
    assert!(form(&state).title_text.is_empty());
    assert!(form(&state).body_text.is_empty());
}

#[test]
fn backspace_and_cursor_motion_edit_the_title() {
    let state = focused_on(ready_composer(), NewPrFormFocus::Title);
    let state = type_text(state, "abc");
    let state = lifecycle(state, PrLifecycleEvent::NewFormBackspace);
    assert_eq!(form(&state).title_text, "ab");

    let state = lifecycle(state, PrLifecycleEvent::NewFormCursorLeft);
    let state = type_text(state, "X");
    assert_eq!(form(&state).title_text, "aXb");

    let state = lifecycle(state, PrLifecycleEvent::NewFormCursorEnd);
    let state = type_text(state, "!");
    assert_eq!(form(&state).title_text, "aXb!");

    let state = lifecycle(state, PrLifecycleEvent::NewFormCursorHome);
    let state = lifecycle(state, PrLifecycleEvent::NewFormDelete);
    assert_eq!(form(&state).title_text, "Xb!");
}

#[test]
fn only_the_body_takes_a_newline() {
    let state = focused_on(ready_composer(), NewPrFormFocus::Title);
    let state = lifecycle(state, PrLifecycleEvent::NewFormNewline);
    assert_eq!(form(&state).title_text, "", "a title stays one line");

    let state = focused_on(state, NewPrFormFocus::Body);
    let state = type_text(state, "one");
    let state = lifecycle(state, PrLifecycleEvent::NewFormNewline);
    let state = type_text(state, "two");
    assert_eq!(form(&state).body_text, "one\ntwo");
}

// ── Submitting (A15) ───────────────────────────────────────────────────────

fn titled_composer(title: &str) -> AppState {
    type_text(focused_on(ready_composer(), NewPrFormFocus::Title), title)
}

#[test]
fn a_complete_composer_records_the_pending_create() {
    let state = lifecycle(
        titled_composer("Add login"),
        PrLifecycleEvent::NewFormSubmit,
    );

    let pending = state
        .prs_state
        .create_mutation_pending
        .as_ref()
        .unwrap_or_else(|| panic!("submitting must record a pending create"));
    assert_eq!(pending.scope_repo_id, RepositoryId("repo-1".to_string()));
    assert!(
        state.prs_state.new_pr_form.is_some(),
        "the composer stays open until GitHub answers"
    );
}

#[test]
fn an_empty_title_is_refused_without_a_request() {
    let state = lifecycle(ready_composer(), PrLifecycleEvent::NewFormSubmit);

    assert!(state.prs_state.create_mutation_pending.is_none());
    assert!(
        form(&state)
            .error
            .as_deref()
            .is_some_and(|e| e.to_lowercase().contains("title")),
        "got: {:?}",
        form(&state).error
    );
}

#[test]
fn a_whitespace_title_is_refused_too() {
    let state = lifecycle(titled_composer("   "), PrLifecycleEvent::NewFormSubmit);

    assert!(state.prs_state.create_mutation_pending.is_none());
}

#[test]
fn a_head_that_equals_the_base_is_refused() {
    let state = composer_with_branches(&["main"], Some("main"));
    let state = type_text(focused_on(state, NewPrFormFocus::Title), "Add login");
    let state = lifecycle(state, PrLifecycleEvent::NewFormSubmit);

    assert!(state.prs_state.create_mutation_pending.is_none());
    assert!(
        form(&state)
            .error
            .as_deref()
            .is_some_and(|e| e.contains("base")),
        "got: {:?}",
        form(&state).error
    );
}

#[test]
fn a_composer_still_loading_its_branches_cannot_submit() {
    let state = lifecycle(list_focused(), PrLifecycleEvent::OpenNewForm);
    let state = lifecycle(state, PrLifecycleEvent::NewFormSubmit);

    assert!(state.prs_state.create_mutation_pending.is_none());
    assert!(form(&state).error.is_some());
}

#[test]
fn a_created_pull_request_closes_the_composer_and_is_announced() {
    let state = lifecycle(
        titled_composer("Add login"),
        PrLifecycleEvent::NewFormSubmit,
    );
    let request_id = state
        .prs_state
        .create_mutation_pending
        .as_ref()
        .map_or_else(|| panic!("a create must be pending"), |p| p.mutation_id);
    let state = lifecycle(
        state,
        PrLifecycleEvent::Created {
            scope_repo_id: RepositoryId("repo-1".to_string()),
            mutation_id: request_id,
            pr_number: 77,
        },
    );

    assert!(state.prs_state.new_pr_form.is_none());
    assert!(state.prs_state.create_mutation_pending.is_none());
    assert!(
        state
            .prs_state
            .draft_notice
            .as_deref()
            .is_some_and(|n| n.contains("77")),
        "got: {:?}",
        state.prs_state.draft_notice
    );
}

#[test]
fn a_rejected_create_keeps_the_draft_and_explains_why() {
    let state = lifecycle(
        titled_composer("Add login"),
        PrLifecycleEvent::NewFormSubmit,
    );
    let request_id = state
        .prs_state
        .create_mutation_pending
        .as_ref()
        .map_or_else(|| panic!("a create must be pending"), |p| p.mutation_id);
    let state = lifecycle(
        state,
        PrLifecycleEvent::CreateFailed {
            scope_repo_id: RepositoryId("repo-1".to_string()),
            mutation_id: request_id,
            error: "No commits between main and feature".to_string(),
        },
    );

    assert!(state.prs_state.create_mutation_pending.is_none());
    let form = form(&state);
    assert_eq!(
        form.title_text, "Add login",
        "the draft survives a rejection"
    );
    assert!(
        form.error
            .as_deref()
            .is_some_and(|e| e.contains("No commits")),
        "got: {:?}",
        form.error
    );
}

#[test]
fn a_stale_create_result_never_closes_a_live_composer() {
    let state = lifecycle(
        titled_composer("Add login"),
        PrLifecycleEvent::NewFormSubmit,
    );
    let request_id = state
        .prs_state
        .create_mutation_pending
        .as_ref()
        .map_or_else(|| panic!("a create must be pending"), |p| p.mutation_id);
    let state = lifecycle(
        state,
        PrLifecycleEvent::Created {
            scope_repo_id: RepositoryId("repo-1".to_string()),
            mutation_id: request_id.wrapping_add(1),
            pr_number: 77,
        },
    );

    assert!(state.prs_state.new_pr_form.is_some());
    assert!(state.prs_state.create_mutation_pending.is_some());
}

#[test]
fn a_second_submit_cannot_start_while_one_is_in_flight() {
    let state = lifecycle(
        titled_composer("Add login"),
        PrLifecycleEvent::NewFormSubmit,
    );
    let first = state
        .prs_state
        .create_mutation_pending
        .as_ref()
        .map_or_else(|| panic!("a create must be pending"), |p| p.mutation_id);
    let state = lifecycle(state, PrLifecycleEvent::NewFormSubmit);

    assert_eq!(
        state
            .prs_state
            .create_mutation_pending
            .as_ref()
            .map(|p| p.mutation_id),
        Some(first),
        "a repeated confirmation must not open two pull requests"
    );
}

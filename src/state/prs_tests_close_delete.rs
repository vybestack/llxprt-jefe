//! Reducer tests for closing and deleting a pull request (issue #183).
//!
//! Closing reuses the state property editor delivered by issue #175; the only
//! new thing here is that it is reachable from the list. Deleting is new: it
//! arms a destructive overlay, then closes the pull request and removes its
//! head branch.

use super::prs_test_fixtures::prs_state_with_detail;
use super::types::{PrFocus, PrPropertyKind};
use crate::domain::{PrState, RepositoryId};
use crate::state::transition::TransitionExt;
use crate::state::{AppEvent, AppState, PrLifecycleEvent};

fn lifecycle(state: AppState, event: PrLifecycleEvent) -> AppState {
    state.apply(event.into()).committed_pure()
}

/// A PR-mode state focused on the list with one selected pull request.
fn list_focused(pr_number: u64) -> AppState {
    let mut state = prs_state_with_detail("repo-1", pr_number);
    state.prs_state.pr_focus = PrFocus::PrList;
    state
}

/// Open the destructive overlay and arm it.
fn armed_delete(state: AppState) -> AppState {
    let state = lifecycle(state, PrLifecycleEvent::OpenDeleteConfirm);
    lifecycle(state, PrLifecycleEvent::DeleteConfirm)
}

/// The kind of the open property editor, if one is open.
fn open_editor_kind(state: &crate::state::AppState) -> Option<PrPropertyKind> {
    state.prs_state.property_editor.as_ref().map(|e| e.kind)
}

fn open_editor(state: crate::state::AppState, kind: PrPropertyKind) -> crate::state::AppState {
    state
        .apply(AppEvent::PrOpenPropertyEditor { kind })
        .committed_pure()
}

// ── Closing from the list (A1) ─────────────────────────────────────────────

#[test]
fn the_state_editor_opens_for_a_pull_request_selected_in_the_list() {
    let mut state = prs_state_with_detail("repo-1", 42);
    state.prs_state.pr_focus = PrFocus::PrList;

    let state = open_editor(state, PrPropertyKind::State);

    assert_eq!(
        open_editor_kind(&state),
        Some(PrPropertyKind::State),
        "a pull request must be closable without first opening its detail"
    );
}

#[test]
fn the_state_editor_still_opens_from_the_detail_view() {
    let state = prs_state_with_detail("repo-1", 42);
    let state = open_editor(state, PrPropertyKind::State);

    assert_eq!(open_editor_kind(&state), Some(PrPropertyKind::State));
}

#[test]
fn the_list_offers_only_the_state_editor() {
    // The list preview carries no body, milestone, or full label set, so the
    // other property editors stay a detail-view action.
    for kind in [
        PrPropertyKind::Labels,
        PrPropertyKind::Assignees,
        PrPropertyKind::Milestone,
        PrPropertyKind::Title,
    ] {
        let mut state = prs_state_with_detail("repo-1", 42);
        state.prs_state.pr_focus = PrFocus::PrList;
        let state = open_editor(state, kind);
        assert!(
            open_editor_kind(&state).is_none(),
            "{kind:?} must remain a detail-view action"
        );
    }
}

#[test]
fn no_editor_opens_from_the_repository_pane() {
    let mut state = prs_state_with_detail("repo-1", 42);
    state.prs_state.pr_focus = PrFocus::RepoList;

    let state = open_editor(state, PrPropertyKind::State);

    assert!(open_editor_kind(&state).is_none());
}

#[test]
fn no_editor_opens_when_no_pull_request_is_previewed() {
    let mut state = prs_state_with_detail("repo-1", 42);
    state.prs_state.pr_focus = PrFocus::PrList;
    state.prs_state.pr_detail = None;

    let state = open_editor(state, PrPropertyKind::State);

    assert!(open_editor_kind(&state).is_none());
}

// ── Arming the destructive overlay (A2, A3, A9) ────────────────────────────

#[test]
fn the_overlay_names_the_pull_request_and_the_branch_it_would_remove() {
    let state = lifecycle(list_focused(42), PrLifecycleEvent::OpenDeleteConfirm);

    let confirm = state
        .prs_state
        .delete_confirm
        .unwrap_or_else(|| panic!("the overlay must open for a selected pull request"));
    assert_eq!(confirm.pr_number, 42);
    assert_eq!(confirm.head_ref, "feature");
    assert!(
        !confirm.awaiting_confirmation,
        "the overlay must open unarmed so a stray Enter cannot delete"
    );
}

#[test]
fn the_overlay_also_opens_from_the_detail_view() {
    let state = prs_state_with_detail("repo-1", 42);
    let state = lifecycle(state, PrLifecycleEvent::OpenDeleteConfirm);

    assert!(state.prs_state.delete_confirm.is_some());
}

#[test]
fn the_first_confirmation_only_arms_the_overlay() {
    let state = armed_delete(list_focused(42));

    assert!(
        state
            .prs_state
            .delete_confirm
            .is_some_and(|c| c.awaiting_confirmation),
        "the first Enter must arm rather than delete"
    );
    assert!(
        state.prs_state.delete_mutation_pending.is_none(),
        "nothing may be dispatched until the second Enter"
    );
}

#[test]
fn cancelling_closes_the_overlay_without_dispatching() {
    let state = armed_delete(list_focused(42));
    let state = lifecycle(state, PrLifecycleEvent::DeleteCancel);

    assert!(state.prs_state.delete_confirm.is_none());
    assert!(state.prs_state.delete_mutation_pending.is_none());
}

#[test]
fn no_overlay_opens_from_the_repository_pane() {
    let mut state = prs_state_with_detail("repo-1", 42);
    state.prs_state.pr_focus = PrFocus::RepoList;

    let state = lifecycle(state, PrLifecycleEvent::OpenDeleteConfirm);

    assert!(state.prs_state.delete_confirm.is_none());
    assert!(
        state.prs_state.draft_notice.is_some(),
        "the key must not be silently dropped"
    );
}

#[test]
fn no_overlay_opens_while_another_overlay_owns_the_screen() {
    let state = open_editor(prs_state_with_detail("repo-1", 42), PrPropertyKind::State);
    let state = lifecycle(state, PrLifecycleEvent::OpenDeleteConfirm);

    assert!(state.prs_state.delete_confirm.is_none());
}

#[test]
fn no_overlay_opens_behind_the_new_pr_composer() {
    // Both overlays render at the same place, so two open at once would stack.
    let state = lifecycle(list_focused(42), PrLifecycleEvent::OpenNewForm);
    let state = lifecycle(state, PrLifecycleEvent::OpenDeleteConfirm);

    assert!(state.prs_state.new_pr_form.is_some());
    assert!(state.prs_state.delete_confirm.is_none());
}

// ── Confirming the delete (A4, A5) ─────────────────────────────────────────

#[test]
fn confirming_an_open_pull_request_asks_for_a_close_and_a_branch_removal() {
    let state = armed_delete(list_focused(42));
    let state = lifecycle(state, PrLifecycleEvent::DeleteConfirm);

    let pending = state
        .prs_state
        .delete_mutation_pending
        .unwrap_or_else(|| panic!("the second Enter must record the pending delete"));
    assert_eq!(pending.pr_number, 42);
    assert_eq!(pending.head_ref, "feature");
    assert!(
        pending.close_first,
        "an open pull request is closed before its branch is removed"
    );
    assert_eq!(pending.scope_repo_id, RepositoryId("repo-1".to_string()));
    assert!(
        state.prs_state.delete_confirm.is_none(),
        "the overlay closes once the delete is in flight"
    );
}

#[test]
fn confirming_a_merged_pull_request_only_removes_the_branch() {
    let mut state = list_focused(42);
    if let Some(detail) = state.prs_state.pr_detail.as_mut() {
        detail.state = PrState::Merged;
    }
    let state = lifecycle(armed_delete(state), PrLifecycleEvent::DeleteConfirm);

    assert!(
        state
            .prs_state
            .delete_mutation_pending
            .is_some_and(|p| !p.close_first),
        "a merged pull request must not be closed again"
    );
}

#[test]
fn confirming_an_already_closed_pull_request_only_removes_the_branch() {
    let mut state = list_focused(42);
    if let Some(detail) = state.prs_state.pr_detail.as_mut() {
        detail.state = PrState::Closed;
    }
    let state = lifecycle(armed_delete(state), PrLifecycleEvent::DeleteConfirm);

    assert!(
        state
            .prs_state
            .delete_mutation_pending
            .is_some_and(|p| !p.close_first)
    );
}

// ── Refusals that never reach GitHub (A6, A7) ──────────────────────────────

#[test]
fn a_pull_request_whose_head_branch_is_unknown_is_not_deleted() {
    let mut state = list_focused(42);
    if let Some(detail) = state.prs_state.pr_detail.as_mut() {
        detail.head_ref = String::new();
    }
    let state = lifecycle(armed_delete(state), PrLifecycleEvent::DeleteConfirm);

    assert!(state.prs_state.delete_mutation_pending.is_none());
    assert!(state.prs_state.delete_confirm.is_none());
    assert!(
        state
            .prs_state
            .error
            .as_deref()
            .is_some_and(|e| e.contains("head branch")),
        "the diagnostic must say the head branch is unknown: {:?}",
        state.prs_state.error
    );
}

#[test]
fn a_pull_request_that_targets_its_own_head_branch_is_not_deleted() {
    let mut state = list_focused(42);
    if let Some(detail) = state.prs_state.pr_detail.as_mut() {
        detail.base_ref = "feature".to_string();
    }
    let state = lifecycle(armed_delete(state), PrLifecycleEvent::DeleteConfirm);

    assert!(state.prs_state.delete_mutation_pending.is_none());
    assert!(
        state
            .prs_state
            .error
            .as_deref()
            .is_some_and(|e| e.contains("base branch")),
        "the diagnostic must say the branch is the base: {:?}",
        state.prs_state.error
    );
}

// ── Results (A4 success and failure) ───────────────────────────────────────

/// Drive a state to an in-flight delete and return it with its mutation id.
fn in_flight_delete() -> (AppState, u64) {
    let state = lifecycle(
        armed_delete(list_focused(42)),
        PrLifecycleEvent::DeleteConfirm,
    );
    let mutation_id = state
        .prs_state
        .delete_mutation_pending
        .as_ref()
        .map_or_else(|| panic!("a delete must be in flight"), |p| p.mutation_id);
    (state, mutation_id)
}

#[test]
fn a_completed_delete_shows_the_pull_request_closed_and_reports_the_branch() {
    let (state, mutation_id) = in_flight_delete();
    let state = lifecycle(
        state,
        PrLifecycleEvent::Deleted {
            scope_repo_id: RepositoryId("repo-1".to_string()),
            pr_number: 42,
            mutation_id,
            branch: "feature".to_string(),
            closed: true,
        },
    );

    assert!(state.prs_state.delete_mutation_pending.is_none());
    assert_eq!(
        state.prs_state.pr_detail.as_ref().map(|d| d.state),
        Some(PrState::Closed)
    );
    assert_eq!(
        state.prs_state.pull_requests().first().map(|pr| pr.state),
        Some(PrState::Closed)
    );
    let notice = state.prs_state.draft_notice.unwrap_or_default();
    assert!(notice.contains("42"), "got: {notice}");
    assert!(notice.contains("feature"), "got: {notice}");
}

#[test]
fn a_delete_that_only_removed_the_branch_leaves_the_state_alone() {
    let mut state = list_focused(42);
    if let Some(detail) = state.prs_state.pr_detail.as_mut() {
        detail.state = PrState::Merged;
    }
    let state = lifecycle(armed_delete(state), PrLifecycleEvent::DeleteConfirm);
    let mutation_id = state
        .prs_state
        .delete_mutation_pending
        .as_ref()
        .map_or_else(|| panic!("a delete must be in flight"), |p| p.mutation_id);
    let state = lifecycle(
        state,
        PrLifecycleEvent::Deleted {
            scope_repo_id: RepositoryId("repo-1".to_string()),
            pr_number: 42,
            mutation_id,
            branch: "feature".to_string(),
            closed: false,
        },
    );

    assert_eq!(
        state.prs_state.pr_detail.as_ref().map(|d| d.state),
        Some(PrState::Merged),
        "a merged pull request stays merged"
    );
}

#[test]
fn a_failed_delete_clears_the_pending_and_names_the_pull_request() {
    let (state, mutation_id) = in_flight_delete();
    let state = lifecycle(
        state,
        PrLifecycleEvent::DeleteFailed {
            scope_repo_id: RepositoryId("repo-1".to_string()),
            pr_number: 42,
            mutation_id,
            closed: false,
            error: "Ref not found".to_string(),
        },
    );

    assert!(state.prs_state.delete_mutation_pending.is_none());
    assert!(
        state
            .prs_state
            .error
            .as_deref()
            .is_some_and(|e| e.contains("42") && e.contains("Ref not found")),
        "got: {:?}",
        state.prs_state.error
    );
    assert_eq!(
        state.prs_state.pr_detail.as_ref().map(|d| d.state),
        Some(PrState::Open),
        "a delete that never closed the pull request leaves it open"
    );
}

#[test]
fn a_delete_that_closed_before_it_failed_still_shows_the_pull_request_closed() {
    let (state, mutation_id) = in_flight_delete();
    let state = lifecycle(
        state,
        PrLifecycleEvent::DeleteFailed {
            scope_repo_id: RepositoryId("repo-1".to_string()),
            pr_number: 42,
            mutation_id,
            closed: true,
            error: "could not delete branch feature".to_string(),
        },
    );

    assert_eq!(
        state.prs_state.pr_detail.as_ref().map(|d| d.state),
        Some(PrState::Closed),
        "the close already happened on GitHub, so the screen must not claim otherwise"
    );
    assert_eq!(
        state.prs_state.pull_requests().first().map(|pr| pr.state),
        Some(PrState::Closed)
    );
}

#[test]
fn a_result_for_a_different_operation_never_clears_the_pending() {
    let (state, mutation_id) = in_flight_delete();
    let state = lifecycle(
        state,
        PrLifecycleEvent::Deleted {
            scope_repo_id: RepositoryId("repo-1".to_string()),
            pr_number: 42,
            mutation_id: mutation_id.wrapping_add(1),
            branch: "feature".to_string(),
            closed: true,
        },
    );

    assert!(
        state.prs_state.delete_mutation_pending.is_some(),
        "a stale result must not retire the live delete"
    );
}

#[test]
fn a_second_delete_cannot_start_while_one_is_in_flight() {
    let (state, _) = in_flight_delete();
    let state = lifecycle(state, PrLifecycleEvent::OpenDeleteConfirm);

    assert!(state.prs_state.delete_confirm.is_none());
}

//! Tests for PR merge-chooser + merge-lifecycle state transitions (issue #92).
//!
//! @requirement REQ-PR-009

use super::AppEvent;
use super::prs_test_fixtures::prs_state_with_detail;
use super::types::{InlineState, PrFocus};
use crate::domain::{MergeMethod, PrState, RepositoryId};

// ── Open merge chooser ────────────────────────────────────────────────────

#[test]
fn open_merge_chooser_from_detail_with_open_pr() {
    let state = prs_state_with_detail("repo-1", 42);
    let state = state.apply(AppEvent::PrOpenMergeChooser);
    let chooser = state.prs_state.merge_chooser.as_ref();
    match chooser {
        Some(c) => {
            assert_eq!(c.selected_index, 0);
            assert!(!c.awaiting_confirmation);
            assert!(c.allowed_methods.is_none());
        }
        None => panic!("merge chooser must be open"),
    }
}

#[test]
fn open_merge_chooser_from_list_is_noop() {
    let mut state = prs_state_with_detail("repo-1", 42);
    state.prs_state.pr_focus = PrFocus::PrList;
    let state = state.apply(AppEvent::PrOpenMergeChooser);
    assert!(
        state.prs_state.merge_chooser.is_none(),
        "merge chooser must NOT open from list focus"
    );
}

#[test]
fn open_merge_chooser_when_composer_active_is_noop() {
    let mut state = prs_state_with_detail("repo-1", 42);
    state.prs_state.inline_state = InlineState::Composer {
        target: super::types::ComposerTarget::NewComment,
        text: String::new(),
        cursor: 0,
    };
    let state = state.apply(AppEvent::PrOpenMergeChooser);
    assert!(
        state.prs_state.merge_chooser.is_none(),
        "merge chooser must NOT open while composer is active"
    );
}

#[test]
fn open_merge_chooser_for_merged_pr_sets_notice() {
    let mut state = prs_state_with_detail("repo-1", 42);
    if let Some(detail) = &mut state.prs_state.pr_detail {
        detail.state = PrState::Merged;
    }
    let state = state.apply(AppEvent::PrOpenMergeChooser);
    assert!(
        state.prs_state.merge_chooser.is_none(),
        "merge chooser must NOT open for a merged PR"
    );
    assert!(
        state.prs_state.draft_notice.is_some(),
        "merged PR must set a draft_notice"
    );
}

#[test]
fn open_merge_chooser_for_unmergeable_pr_sets_notice() {
    let mut state = prs_state_with_detail("repo-1", 42);
    if let Some(detail) = &mut state.prs_state.pr_detail {
        detail.mergeable = Some(false);
    }
    let state = state.apply(AppEvent::PrOpenMergeChooser);
    assert!(
        state.prs_state.merge_chooser.is_none(),
        "merge chooser must NOT open for an unmergeable PR"
    );
    assert!(
        state.prs_state.draft_notice.is_some(),
        "unmergeable PR must set a draft_notice"
    );
}

// ── Merge chooser navigation ──────────────────────────────────────────────

fn chooser_selected(state: &super::AppState) -> usize {
    state
        .prs_state
        .merge_chooser
        .as_ref()
        .map_or(usize::MAX, |c| c.selected_index)
}

#[test]
fn merge_navigate_down_wraps() {
    let state = prs_state_with_detail("repo-1", 42);
    let state = state.apply(AppEvent::PrOpenMergeChooser);
    let state = state.apply(AppEvent::PrMergeNavigateDown);
    assert_eq!(chooser_selected(&state), 1);
    let state = state.apply(AppEvent::PrMergeNavigateDown);
    assert_eq!(chooser_selected(&state), 2);
    let state = state.apply(AppEvent::PrMergeNavigateDown);
    assert_eq!(chooser_selected(&state), 0);
}

#[test]
fn merge_navigate_up_wraps() {
    let state = prs_state_with_detail("repo-1", 42);
    let state = state.apply(AppEvent::PrOpenMergeChooser);
    let state = state.apply(AppEvent::PrMergeNavigateUp);
    assert_eq!(chooser_selected(&state), 2);
}

#[test]
fn merge_navigate_skips_disabled_methods() {
    let state = prs_state_with_detail("repo-1", 42);
    let state = state.apply(AppEvent::PrOpenMergeChooser);
    let state = state.apply(AppEvent::PrMergeMethodsLoaded {
        scope_repo_id: RepositoryId("repo-1".to_string()),
        pr_number: 42,
        allowed_methods: vec![MergeMethod::Merge, MergeMethod::Rebase],
    });
    let state = state.apply(AppEvent::PrMergeNavigateDown);
    assert_eq!(
        chooser_selected(&state),
        2,
        "navigation must skip disabled Squash (index 1) to land on Rebase (index 2)"
    );
}

// ── Merge confirm/cancel ─────────────────────────────────────────────────

#[test]
fn merge_confirm_first_enter_arms_confirmation() {
    let state = prs_state_with_detail("repo-1", 42);
    let state = state.apply(AppEvent::PrOpenMergeChooser);
    let state = state.apply(AppEvent::PrMergeConfirm);
    let confirming = state
        .prs_state
        .merge_chooser
        .as_ref()
        .is_some_and(|c| c.awaiting_confirmation);
    assert!(confirming, "first Enter must arm confirmation");
}

#[test]
fn merge_confirm_second_enter_sets_mutation_pending() {
    let state = prs_state_with_detail("repo-1", 42);
    let state = state.apply(AppEvent::PrOpenMergeChooser);
    let state = state.apply(AppEvent::PrMergeConfirm);
    let state = state.apply(AppEvent::PrMergeConfirm);
    assert!(
        state.prs_state.merge_chooser.is_none(),
        "second Enter must close the chooser"
    );
    match &state.prs_state.merge_mutation_pending {
        Some(pending) => {
            assert_eq!(pending.pr_number, 42);
            assert_eq!(pending.method, MergeMethod::Merge);
        }
        None => panic!("merge_mutation_pending must be set"),
    }
}

#[test]
fn merge_cancel_clears_chooser() {
    let state = prs_state_with_detail("repo-1", 42);
    let state = state.apply(AppEvent::PrOpenMergeChooser);
    let state = state.apply(AppEvent::PrMergeCancel);
    assert!(
        state.prs_state.merge_chooser.is_none(),
        "cancel must clear the chooser"
    );
}

// ── Merge lifecycle ──────────────────────────────────────────────────────

#[test]
fn pr_merged_updates_detail_state_and_clears_pending() {
    let state = prs_state_with_detail("repo-1", 42);
    let state = state.apply(AppEvent::PrOpenMergeChooser);
    let state = state.apply(AppEvent::PrMergeConfirm);
    let state = state.apply(AppEvent::PrMergeConfirm);
    let state = state.apply(AppEvent::PrMerged {
        scope_repo_id: RepositoryId("repo-1".to_string()),
        pr_number: 42,
        method: MergeMethod::Merge,
    });
    assert!(
        state.prs_state.merge_mutation_pending.is_none(),
        "PrMerged must clear merge_mutation_pending"
    );
    let detail_state = state.prs_state.pr_detail.as_ref().map(|d| d.state);
    assert_eq!(
        detail_state,
        Some(PrState::Merged),
        "PrMerged must update detail state to Merged"
    );
    assert!(
        state.prs_state.draft_notice.is_some(),
        "PrMerged must set a draft_notice"
    );
}

#[test]
fn pr_merge_failed_sets_error_and_clears_pending() {
    let state = prs_state_with_detail("repo-1", 42);
    let state = state.apply(AppEvent::PrOpenMergeChooser);
    let state = state.apply(AppEvent::PrMergeConfirm);
    let state = state.apply(AppEvent::PrMergeConfirm);
    let pending_id = state
        .prs_state
        .merge_mutation_pending
        .as_ref()
        .map_or(0, |p| p.mutation_id);
    let state = state.apply(AppEvent::PrMergeFailed {
        scope_repo_id: RepositoryId("repo-1".to_string()),
        pr_number: 42,
        mutation_id: pending_id,
        error: "Branch protection requires reviews".to_string(),
    });
    assert!(
        state.prs_state.merge_mutation_pending.is_none(),
        "PrMergeFailed must clear merge_mutation_pending"
    );
    assert!(
        state.prs_state.error.is_some(),
        "PrMergeFailed must set an error"
    );
}

#[test]
fn merge_methods_loaded_updates_chooser() {
    let state = prs_state_with_detail("repo-1", 42);
    let state = state.apply(AppEvent::PrOpenMergeChooser);
    let is_none = state
        .prs_state
        .merge_chooser
        .as_ref()
        .is_some_and(|c| c.allowed_methods.is_none());
    assert!(is_none, "allowed_methods must be None before load");
    let state = state.apply(AppEvent::PrMergeMethodsLoaded {
        scope_repo_id: RepositoryId("repo-1".to_string()),
        pr_number: 42,
        allowed_methods: vec![MergeMethod::Merge, MergeMethod::Squash],
    });
    let allowed = state
        .prs_state
        .merge_chooser
        .as_ref()
        .and_then(|c| c.allowed_methods.as_ref());
    match allowed {
        Some(methods) => {
            assert_eq!(methods.len(), 2);
            assert!(methods.contains(&MergeMethod::Merge));
            assert!(methods.contains(&MergeMethod::Squash));
        }
        None => panic!("allowed_methods must be loaded"),
    }
}

// ── Edge cases: list sync, stale responses, pending guard ────────────────

#[test]
fn merged_updates_pull_requests_list_state() {
    let state = prs_state_with_detail("repo-1", 42);
    let state = state.apply(AppEvent::PrMerged {
        scope_repo_id: RepositoryId("repo-1".to_string()),
        pr_number: 42,
        method: MergeMethod::Merge,
    });
    let list_state = state
        .prs_state
        .pull_requests()
        .iter()
        .find(|p| p.number == 42)
        .map(|p| p.state);
    assert_eq!(
        list_state,
        Some(PrState::Merged),
        "pull_requests list entry must reflect Merged state"
    );
    let detail_state = state.prs_state.pr_detail.as_ref().map(|d| d.state);
    assert_eq!(
        detail_state,
        Some(PrState::Merged),
        "pr_detail must reflect Merged state"
    );
}

#[test]
fn merge_failed_ignored_for_wrong_scope() {
    let state = prs_state_with_detail("repo-1", 42);
    let state = state.apply(AppEvent::PrMergeFailed {
        scope_repo_id: RepositoryId("wrong-repo".to_string()),
        pr_number: 42,
        mutation_id: 1,
        error: "some error".to_string(),
    });
    assert!(
        state.prs_state.error.is_none(),
        "PrMergeFailed with wrong scope must NOT set an error"
    );
}

#[test]
fn merge_failed_ignored_for_wrong_mutation_id() {
    let state = prs_state_with_detail("repo-1", 42);
    let state = state.apply(AppEvent::PrMergeFailed {
        scope_repo_id: RepositoryId("repo-1".to_string()),
        pr_number: 42,
        mutation_id: 999,
        error: "stale".to_string(),
    });
    assert!(
        state.prs_state.error.is_none(),
        "PrMergeFailed with wrong mutation_id must NOT set an error"
    );
}

#[test]
fn merge_methods_loaded_ignored_for_wrong_pr_number() {
    let state = prs_state_with_detail("repo-1", 42);
    let state = state.apply(AppEvent::PrOpenMergeChooser);
    let state = state.apply(AppEvent::PrMergeMethodsLoaded {
        scope_repo_id: RepositoryId("repo-1".to_string()),
        pr_number: 99,
        allowed_methods: vec![MergeMethod::Squash],
    });
    let still_none = state
        .prs_state
        .merge_chooser
        .as_ref()
        .is_some_and(|c| c.allowed_methods.is_none());
    assert!(
        still_none,
        "PrMergeMethodsLoaded for wrong pr_number must NOT update chooser"
    );
}

#[test]
fn open_merge_chooser_blocked_while_merge_pending() {
    let mut state = prs_state_with_detail("repo-1", 42);
    state.prs_state.merge_mutation_pending = Some(super::types::PrMergeMutationPending {
        scope_repo_id: RepositoryId("repo-1".to_string()),
        mutation_id: 1,
        pr_number: 42,
        method: MergeMethod::Merge,
    });
    let state = state.apply(AppEvent::PrOpenMergeChooser);
    assert!(
        state.prs_state.merge_chooser.is_none(),
        "merge chooser must NOT open while a merge is pending"
    );
}

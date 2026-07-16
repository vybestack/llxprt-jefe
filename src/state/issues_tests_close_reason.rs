//! Tests for issue close-reason chooser state transitions (issue #188).
//!
//! Mirrors `issues_tests_close_delete.rs`. Covers: chooser open guards,
//! navigate up/down bounds, select arms confirmation vs enters duplicate search,
//! duplicate search char/backspace refilters candidates, confirm sets pending
//! with reason + duplicate_of, cancel clears chooser, IssueClosed with reason
//! updates issue state.

use crate::domain::{CloseReason, Issue, IssueState, RepositoryId};
use crate::state::{AppEvent, AppState, IssueFocus};

fn issues_state_with_list(repo_id: &str) -> AppState {
    let mut state = AppState::default();
    state.issues_state.active = true;
    state.issues_state.list.replace_items(vec![
        make_issue(1, "I_1"),
        make_issue(2, "I_2"),
        make_issue(3, "I_3"),
    ]);
    state.issues_state.list.set_selected_index(Some(0));
    state.issues_state.issue_focus = IssueFocus::IssueList;
    let repo = crate::domain::Repository::new(
        RepositoryId(repo_id.to_string()),
        "test".to_string(),
        "test".to_string(),
        std::path::PathBuf::from("/tmp"),
    );
    state.repositories.push(repo);
    state.selected_repository_index = Some(0);
    state
}

fn make_issue(number: u64, node_id: &str) -> Issue {
    Issue {
        number,
        node_id: node_id.to_string(),
        title: format!("Issue {number}"),
        state: IssueState::Open,
        author_login: "octocat".to_string(),
        updated_at: "2026-01-01T00:00:00Z".to_string(),
        assignee_summary: String::new(),
        labels_summary: String::new(),
        assignees: Vec::new(),
        labels: Vec::new(),
        issue_type: String::new(),
        milestone: String::new(),
        module: String::new(),
        comment_count: 0,
        body: String::new(),
        state_reason: None,
    }
}

fn navigate_to_duplicate(state: AppState) -> AppState {
    let dup_index = crate::domain::CLOSE_REASONS
        .iter()
        .position(|&r| r == CloseReason::Duplicate);
    let Some(dup_index) = dup_index else {
        panic!("CLOSE_REASONS must contain Duplicate");
    };
    let mut s = state;
    for _ in 0..dup_index {
        s = s.apply(AppEvent::CloseReasonNavigateDown);
    }
    s
}

/// Navigate the chooser to the Completed reason by computed index, robust to
/// CLOSE_REASONS reordering.
fn navigate_to_completed(state: AppState) -> AppState {
    let completed_index = crate::domain::CLOSE_REASONS
        .iter()
        .position(|&r| r == CloseReason::Completed);
    let Some(completed_index) = completed_index else {
        panic!("CLOSE_REASONS must contain Completed");
    };
    let mut s = state;
    for _ in 0..completed_index {
        s = s.apply(AppEvent::CloseReasonNavigateDown);
    }
    s
}

// ── OpenCloseReasonChooser ────────────────────────────────────────────────

#[test]
fn open_close_reason_chooser_when_issue_focused_and_open() {
    let state = issues_state_with_list("repo-1");
    let state = state.apply(AppEvent::OpenCloseReasonChooser);
    assert!(
        state.issues_state.close_reason_chooser.is_some(),
        "chooser should open when issue is focused and open"
    );
    let Some(c) = state.issues_state.close_reason_chooser.as_ref() else {
        return;
    };
    assert_eq!(c.issue_number, 1);
    assert_eq!(c.selected_index, 0);
    assert!(!c.awaiting_confirmation);
    assert!(c.duplicate_search.is_none());
}

#[test]
fn open_close_reason_chooser_blocked_when_already_closed() {
    let mut state = issues_state_with_list("repo-1");
    let mut issues = state.issues_state.list.items().to_vec();
    issues[0].state = IssueState::Closed;
    state.issues_state.list.replace_items(issues);
    let state = state.apply(AppEvent::OpenCloseReasonChooser);
    assert!(
        state.issues_state.close_reason_chooser.is_none(),
        "chooser must NOT open for already-closed issue"
    );
    assert!(
        state.issues_state.draft_notice.is_some(),
        "should show a notice"
    );
}

#[test]
fn open_close_reason_chooser_blocked_when_delete_confirm_active() {
    let mut state = issues_state_with_list("repo-1");
    state.issues_state.delete_confirm = Some(crate::state::IssueDeleteConfirmState {
        issue_number: 1,
        awaiting_confirmation: false,
    });
    let state = state.apply(AppEvent::OpenCloseReasonChooser);
    assert!(
        state.issues_state.close_reason_chooser.is_none(),
        "chooser must NOT open when another overlay is active"
    );
}

#[test]
fn open_close_reason_chooser_blocked_when_no_issue_focused() {
    let mut state = issues_state_with_list("repo-1");
    state.issues_state.list.set_selected_index(None);
    let state = state.apply(AppEvent::OpenCloseReasonChooser);
    assert!(
        state.issues_state.close_reason_chooser.is_none(),
        "chooser must NOT open when no issue is focused"
    );
}

// ── Navigate up/down ──────────────────────────────────────────────────────

#[test]
fn navigate_down_moves_selection() {
    let state = issues_state_with_list("repo-1");
    let state = state.apply(AppEvent::OpenCloseReasonChooser);
    let state = state.apply(AppEvent::CloseReasonNavigateDown);
    let Some(c) = state.issues_state.close_reason_chooser.as_ref() else {
        panic!("chooser should still be open");
    };
    assert_eq!(c.selected_index, 1);
}

#[test]
fn navigate_up_clamps_at_first() {
    let state = issues_state_with_list("repo-1");
    let state = state.apply(AppEvent::OpenCloseReasonChooser);
    let state = state.apply(AppEvent::CloseReasonNavigateUp);
    let Some(c) = state.issues_state.close_reason_chooser.as_ref() else {
        panic!("chooser should still be open");
    };
    assert_eq!(
        c.selected_index, 0,
        "should clamp at first item (bounded, like the agent chooser)"
    );
}

#[test]
fn navigate_down_clamps_at_last() {
    let state = issues_state_with_list("repo-1");
    let state = state.apply(AppEvent::OpenCloseReasonChooser);
    // Navigate down past the last item (CLOSE_REASONS has 4 items)
    let state = state.apply(AppEvent::CloseReasonNavigateDown);
    let state = state.apply(AppEvent::CloseReasonNavigateDown);
    let state = state.apply(AppEvent::CloseReasonNavigateDown);
    let state = state.apply(AppEvent::CloseReasonNavigateDown);
    let Some(c) = state.issues_state.close_reason_chooser.as_ref() else {
        panic!("chooser should still be open");
    };
    assert_eq!(
        c.selected_index,
        crate::domain::CLOSE_REASONS.len() - 1,
        "should clamp at last item (bounded, like the agent chooser)"
    );
}

// ── CloseReasonSelect ─────────────────────────────────────────────────────

#[test]
fn select_non_duplicate_arms_confirmation() {
    let state = issues_state_with_list("repo-1");
    let state = state.apply(AppEvent::OpenCloseReasonChooser);
    // Navigate to Completed by computed index (robust to CLOSE_REASONS order).
    let state = navigate_to_completed(state);
    let state = state.apply(AppEvent::CloseReasonSelect);
    let Some(c) = state.issues_state.close_reason_chooser.as_ref() else {
        panic!("chooser should still be open");
    };
    assert!(
        c.awaiting_confirmation,
        "non-duplicate select should arm confirmation"
    );
    assert!(
        c.duplicate_search.is_none(),
        "non-duplicate select should not enter duplicate search"
    );
}

#[test]
fn select_duplicate_enters_search_sub_state() {
    let state = issues_state_with_list("repo-1");
    let state = state.apply(AppEvent::OpenCloseReasonChooser);
    // Navigate to Duplicate by computed index (robust to CLOSE_REASONS order).
    let state = navigate_to_duplicate(state);
    // Select Duplicate
    let state = state.apply(AppEvent::CloseReasonSelect);
    let Some(c) = state.issues_state.close_reason_chooser.as_ref() else {
        panic!("chooser should still be open");
    };
    assert!(
        c.duplicate_search.is_some(),
        "Duplicate select should enter duplicate search"
    );
    let Some(search) = c.duplicate_search.as_ref() else {
        panic!("search sub-state should be present");
    };
    assert!(search.query.is_empty());
    assert_eq!(search.selected_index, 0);
    // Candidates should be seeded from loaded issues excluding the issue being closed (#1)
    assert_eq!(
        search.candidates.len(),
        2,
        "should have 2 candidates (issues #2 and #3)"
    );
    assert_eq!(search.candidates[0].0, 2);
    assert_eq!(search.candidates[1].0, 3);
}

// ── Duplicate search char/backspace ───────────────────────────────────────

#[test]
fn duplicate_search_char_updates_query() {
    let state = issues_state_with_list("repo-1");
    let state = state.apply(AppEvent::OpenCloseReasonChooser);
    let state = navigate_to_duplicate(state);
    let state = state.apply(AppEvent::CloseReasonSelect);
    // Type '3' (matches issue #3 by number-prefix).
    let state = state.apply(AppEvent::CloseReasonDuplicateSearchChar('3'));
    let Some(c) = state.issues_state.close_reason_chooser.as_ref() else {
        panic!("chooser should still be open");
    };
    let Some(search) = c.duplicate_search.as_ref() else {
        panic!("search sub-state should be present");
    };
    assert_eq!(search.query, "3");
}

#[test]
fn duplicate_search_backspace_removes_last_char() {
    let state = issues_state_with_list("repo-1");
    let state = state.apply(AppEvent::OpenCloseReasonChooser);
    let state = navigate_to_duplicate(state);
    let state = state.apply(AppEvent::CloseReasonSelect);
    let state = state.apply(AppEvent::CloseReasonDuplicateSearchChar('3'));
    let state = state.apply(AppEvent::CloseReasonDuplicateSearchBackspace);
    let Some(c) = state.issues_state.close_reason_chooser.as_ref() else {
        panic!("chooser should still be open");
    };
    let Some(search) = c.duplicate_search.as_ref() else {
        panic!("search sub-state should be present");
    };
    assert!(search.query.is_empty(), "backspace should clear the query");
}

// ── CloseReasonConfirm ────────────────────────────────────────────────────

#[test]
fn confirm_completed_sets_close_mutation_pending_with_reason() {
    let state = issues_state_with_list("repo-1");
    let state = state.apply(AppEvent::OpenCloseReasonChooser);
    let state = state.apply(AppEvent::CloseReasonSelect);
    let state = state.apply(AppEvent::CloseReasonConfirm);
    assert!(
        state.issues_state.close_reason_chooser.is_none(),
        "chooser should be cleared after confirm"
    );
    let Some(pending) = state.issues_state.close_mutation_pending.as_ref() else {
        panic!("close_mutation_pending should be set");
    };
    assert_eq!(pending.issue_number, 1);
    assert_eq!(
        pending.close_reason,
        Some(CloseReason::Completed),
        "should carry the Completed reason"
    );
    assert!(
        pending.duplicate_of.is_none(),
        "non-duplicate should not carry duplicate_of"
    );
}

#[test]
fn confirm_duplicate_sets_pending_with_reason_and_duplicate_of() {
    let state = issues_state_with_list("repo-1");
    let state = state.apply(AppEvent::OpenCloseReasonChooser);
    let state = navigate_to_duplicate(state);
    let state = state.apply(AppEvent::CloseReasonSelect);
    let state = state.apply(AppEvent::CloseReasonConfirm);
    assert!(
        state.issues_state.close_reason_chooser.is_none(),
        "chooser should be cleared after confirm"
    );
    let Some(pending) = state.issues_state.close_mutation_pending.as_ref() else {
        panic!("close_mutation_pending should be set");
    };
    assert_eq!(
        pending.close_reason,
        Some(CloseReason::Duplicate),
        "should carry the Duplicate reason"
    );
    assert_eq!(
        pending.duplicate_of,
        Some(2),
        "Duplicate confirm should carry duplicate_of pointing to the first candidate (issue #2)"
    );
}

#[test]
fn confirm_duplicate_with_no_target_preserves_chooser_and_blocks_mutation() {
    // When there are no duplicate candidates at all, the fallback cannot
    // resolve a target, so confirm must bail, preserve the chooser, and emit a
    // notice rather than dispatching an incomplete Duplicate mutation.
    let mut state = issues_state_with_list("repo-1");
    // Remove the other issues so there are no candidates for issue #1.
    let only_one = state
        .issues_state
        .list
        .items()
        .iter()
        .filter(|i| i.number == 1)
        .cloned()
        .collect::<Vec<_>>();
    state.issues_state.list.replace_items(only_one);
    let state = state.apply(AppEvent::OpenCloseReasonChooser);
    let state = navigate_to_duplicate(state);
    let state = state.apply(AppEvent::CloseReasonSelect);
    let state = state.apply(AppEvent::CloseReasonConfirm);
    assert!(
        state.issues_state.close_reason_chooser.is_some(),
        "chooser should be preserved when no duplicate target is available"
    );
    assert!(
        state.issues_state.close_mutation_pending.is_none(),
        "no mutation should be dispatched without a duplicate target"
    );
}

// ── CloseReasonCancel ─────────────────────────────────────────────────────

#[test]
fn cancel_clears_chooser() {
    let state = issues_state_with_list("repo-1");
    let state = state.apply(AppEvent::OpenCloseReasonChooser);
    assert!(
        state.issues_state.close_reason_chooser.is_some(),
        "chooser should be open"
    );
    let state = state.apply(AppEvent::CloseReasonCancel);
    assert!(
        state.issues_state.close_reason_chooser.is_none(),
        "chooser should be cleared after cancel"
    );
}

// ── IssueClosed with reason ───────────────────────────────────────────────

#[test]
fn issue_closed_with_reason_updates_issue_state() {
    let state = issues_state_with_list("repo-1");
    let state = state.apply(AppEvent::OpenCloseReasonChooser);
    let state = state.apply(AppEvent::CloseReasonSelect);
    let state = state.apply(AppEvent::CloseReasonConfirm);
    let mutation_id = state
        .issues_state
        .close_mutation_pending
        .as_ref()
        .map(|p| p.mutation_id);
    let Some(mutation_id) = mutation_id else {
        panic!("pending should exist");
    };
    let scope = state
        .selected_repository_index
        .and_then(|idx| state.repositories.get(idx))
        .map(|r| r.id.clone());
    let Some(scope) = scope else {
        panic!("scope should exist");
    };

    let state = state.apply(AppEvent::IssueClosed {
        scope_repo_id: scope,
        issue_number: 1,
        mutation_id,
        close_reason: Some(CloseReason::Completed),
        duplicate_of: None,
    });
    assert!(
        state.issues_state.close_mutation_pending.is_none(),
        "pending should be cleared after IssueClosed"
    );
    let issue = state.issues_state.issues().iter().find(|i| i.number == 1);
    assert!(
        issue.is_some_and(|i| i.state == IssueState::Closed),
        "issue should be marked closed"
    );
    assert!(
        state.issues_state.draft_notice.is_some(),
        "should show a close notice"
    );
}

// ── Pure projection: filter_duplicate_candidates ──────────────────────────

#[test]
fn filter_duplicate_candidates_empty_query_returns_all() {
    use crate::state::filter_duplicate_candidates;
    let candidates = vec![
        (1u64, "First".to_string()),
        (10u64, "Second".to_string()),
        (100u64, "Third".to_string()),
    ];
    let filtered = filter_duplicate_candidates(&candidates, "");
    assert_eq!(filtered.len(), 3, "empty query should return all");
}

#[test]
fn filter_duplicate_candidates_prefix_match() {
    use crate::state::filter_duplicate_candidates;
    let candidates = vec![
        (1u64, "First".to_string()),
        (10u64, "Second".to_string()),
        (100u64, "Third".to_string()),
        (20u64, "Fourth".to_string()),
    ];
    let filtered = filter_duplicate_candidates(&candidates, "1");
    assert_eq!(filtered.len(), 3, "should match issues 1, 10, 100");
    assert_eq!(filtered[0].0, 1);
    assert_eq!(filtered[1].0, 10);
    assert_eq!(filtered[2].0, 100);
}

#[test]
fn filter_duplicate_candidates_no_match() {
    use crate::state::filter_duplicate_candidates;
    let candidates = vec![(1u64, "First".to_string()), (10u64, "Second".to_string())];
    let filtered = filter_duplicate_candidates(&candidates, "999");
    assert!(filtered.is_empty(), "should return empty when no match");
}

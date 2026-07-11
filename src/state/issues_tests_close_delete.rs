//! Tests for issue close + delete lifecycle state transitions (issue #182).
//!
//! Mirrors `prs_tests_merge.rs`. Covers: delete confirm overlay open/arm/
//! cancel, close updates list+detail, delete removes from list + clears detail,
//! failure clears pending + sets scoped error, stale-mutation-id guards,
//! scope-mismatch guards.

use crate::domain::{Issue, IssueDetail, IssueState, RepositoryId};
use crate::state::AppState;
use crate::state::types::{
    AppEvent, InlineState, IssueDeleteConfirmState, IssueFocus, IssueLifecycleMutationPending,
};

fn issues_state_with_list(repo_id: &str) -> AppState {
    let mut state = AppState::default();
    state.issues_state.active = true;
    state.issues_state.issues = vec![
        make_issue(1, "I_1"),
        make_issue(2, "I_2"),
        make_issue(3, "I_3"),
    ];
    state.issues_state.selected_issue_index = Some(0);
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
    }
}

fn make_detail(number: u64, node_id: &str) -> IssueDetail {
    IssueDetail {
        repo_owner_name: "owner/repo".to_string(),
        number,
        node_id: node_id.to_string(),
        title: format!("Detail {number}"),
        state: IssueState::Open,
        author_login: "octocat".to_string(),
        created_at: "2026-01-01T00:00:00Z".to_string(),
        updated_at: "2026-01-02T00:00:00Z".to_string(),
        labels: Vec::new(),
        assignees: Vec::new(),
        milestone: None,
        body: "Body".to_string(),
        external_url: String::new(),
        comments: Vec::new(),
        has_more_comments: false,
        comments_cursor: None,
    }
}

// ── Close mutation ────────────────────────────────────────────────────────

#[test]
fn close_issue_sets_close_mutation_pending() {
    let state = issues_state_with_list("repo-1");
    let state = state.apply(AppEvent::CloseIssue);
    let pending = state.issues_state.close_mutation_pending.as_ref();
    assert!(pending.is_some(), "close_mutation_pending should be set");
    let Some(p) = pending else {
        return;
    };
    assert_eq!(p.issue_number, 1, "should close the focused issue");
}

#[test]
fn close_issue_when_already_closed_sets_notice() {
    let mut state = issues_state_with_list("repo-1");
    state.issues_state.issues[0].state = IssueState::Closed;
    let state = state.apply(AppEvent::CloseIssue);
    assert!(
        state.issues_state.close_mutation_pending.is_none(),
        "close pending must NOT be set for already-closed issue"
    );
    assert!(
        state.issues_state.draft_notice.is_some(),
        "already-closed notice must be shown"
    );
}

#[test]
fn close_issue_with_no_issue_focused_shows_notice() {
    let mut state = issues_state_with_list("repo-1");
    state.issues_state.selected_issue_index = None;
    state.issues_state.issue_detail = None;
    let state = state.apply(AppEvent::CloseIssue);
    assert!(
        state.issues_state.close_mutation_pending.is_none(),
        "no close pending without focused issue"
    );
    assert!(
        state.issues_state.draft_notice.is_some(),
        "no-issue notice must be shown"
    );
}

#[test]
fn issue_closed_updates_list_and_detail_state() {
    let mut state = issues_state_with_list("repo-1");
    state.issues_state.issue_detail = Some(make_detail(1, "I_1"));
    let mutation_id = state.issues_state.next_mutation_id + 1;
    let scope = RepositoryId("repo-1".to_string());
    // First set the close pending via CloseIssue
    let state = state.apply(AppEvent::CloseIssue);
    // Now apply IssueClosed with the same mutation_id
    let state = state.apply(AppEvent::IssueClosed {
        scope_repo_id: scope,
        issue_number: 1,
        mutation_id,
    });
    let list_issue = state.issues_state.issues.iter().find(|i| i.number == 1);
    assert!(
        list_issue.is_some_and(|i| i.state == IssueState::Closed),
        "list row state should be Closed"
    );
    let detail = state.issues_state.issue_detail.as_ref();
    assert!(
        detail.is_some_and(|d| d.state == IssueState::Closed),
        "detail state should be Closed"
    );
    assert!(
        state.issues_state.close_mutation_pending.is_none(),
        "close pending should be cleared"
    );
    let notice = state.issues_state.draft_notice.as_deref();
    assert!(
        notice.is_some_and(|n| n.contains("Closed issue #1")),
        "draft notice should mention closing, got {notice:?}"
    );
}

#[test]
fn issue_closed_with_wrong_mutation_id_is_ignored() {
    let mut state = issues_state_with_list("repo-1");
    state.issues_state.close_mutation_pending = Some(IssueLifecycleMutationPending {
        scope_repo_id: RepositoryId("repo-1".to_string()),
        mutation_id: 100,
        issue_number: 1,
        node_id: None,
    });
    let state = state.apply(AppEvent::IssueClosed {
        scope_repo_id: RepositoryId("repo-1".to_string()),
        issue_number: 1,
        mutation_id: 999,
    });
    assert!(
        state.issues_state.close_mutation_pending.is_some(),
        "stale mutation id should NOT clear the pending"
    );
}

#[test]
fn issue_closed_with_wrong_scope_is_ignored() {
    let mut state = issues_state_with_list("repo-1");
    state.issues_state.close_mutation_pending = Some(IssueLifecycleMutationPending {
        scope_repo_id: RepositoryId("repo-1".to_string()),
        mutation_id: 100,
        issue_number: 1,
        node_id: None,
    });
    // An IssueClosed from a different repository must NOT be applied to this
    // repo's pending close (defense against cross-repo state corruption).
    let state = state.apply(AppEvent::IssueClosed {
        scope_repo_id: RepositoryId("repo-2".to_string()),
        issue_number: 1,
        mutation_id: 100,
    });
    assert!(
        state.issues_state.close_mutation_pending.is_some(),
        "wrong-scope close result should NOT clear the pending"
    );
    assert!(
        state
            .issues_state
            .issues
            .iter()
            .find(|i| i.number == 1)
            .is_some_and(|i| i.state == IssueState::Open),
        "wrong-scope close result must NOT mutate the local issue state"
    );
}

#[test]
fn issue_deleted_with_wrong_scope_is_ignored() {
    let mut state = issues_state_with_list("repo-1");
    state.issues_state.delete_mutation_pending = Some(IssueLifecycleMutationPending {
        scope_repo_id: RepositoryId("repo-1".to_string()),
        mutation_id: 100,
        issue_number: 1,
        node_id: Some("I_1".to_string()),
    });
    let state = state.apply(AppEvent::IssueDeleted {
        scope_repo_id: RepositoryId("repo-2".to_string()),
        issue_number: 1,
        mutation_id: 100,
    });
    assert!(
        state.issues_state.delete_mutation_pending.is_some(),
        "wrong-scope delete result should NOT clear the pending"
    );
    assert!(
        state.issues_state.issues.iter().any(|i| i.number == 1),
        "wrong-scope delete result must NOT remove the local issue"
    );
}

#[test]
fn open_delete_confirm_from_list() {
    let state = issues_state_with_list("repo-1");
    let state = state.apply(AppEvent::OpenDeleteIssueConfirm);
    let confirm = state.issues_state.delete_confirm.as_ref();
    assert!(confirm.is_some(), "delete confirm should be open");
    let Some(c) = confirm else {
        return;
    };
    assert_eq!(c.issue_number, 1);
    assert!(!c.awaiting_confirmation, "should not be armed initially");
}

#[test]
fn open_delete_confirm_with_no_issue_shows_notice() {
    let mut state = issues_state_with_list("repo-1");
    state.issues_state.selected_issue_index = None;
    state.issues_state.issue_detail = None;
    let state = state.apply(AppEvent::OpenDeleteIssueConfirm);
    assert!(
        state.issues_state.delete_confirm.is_none(),
        "no delete confirm without focused issue"
    );
    assert!(state.issues_state.draft_notice.is_some());
}

#[test]
fn delete_confirm_first_enter_arms() {
    let mut state = issues_state_with_list("repo-1");
    state.issues_state.delete_confirm = Some(IssueDeleteConfirmState {
        issue_number: 1,
        awaiting_confirmation: false,
    });
    let state = state.apply(AppEvent::IssueDeleteConfirm);
    let confirm = state.issues_state.delete_confirm.as_ref();
    assert!(
        confirm.is_some_and(|c| c.awaiting_confirmation),
        "first confirm should arm the overlay"
    );
    assert!(
        state.issues_state.delete_mutation_pending.is_none(),
        "first confirm should NOT dispatch yet"
    );
}

#[test]
fn delete_confirm_second_enter_dispatches() {
    let mut state = issues_state_with_list("repo-1");
    state.issues_state.delete_confirm = Some(IssueDeleteConfirmState {
        issue_number: 1,
        awaiting_confirmation: true,
    });
    let state = state.apply(AppEvent::IssueDeleteConfirm);
    assert!(
        state.issues_state.delete_confirm.is_none(),
        "overlay should be cleared on confirm"
    );
    assert!(
        state.issues_state.delete_mutation_pending.is_some(),
        "delete mutation should be pending after confirm"
    );
}

#[test]
fn delete_confirm_cancel_clears_overlay() {
    let mut state = issues_state_with_list("repo-1");
    state.issues_state.delete_confirm = Some(IssueDeleteConfirmState {
        issue_number: 1,
        awaiting_confirmation: false,
    });
    let state = state.apply(AppEvent::IssueDeleteCancel);
    assert!(
        state.issues_state.delete_confirm.is_none(),
        "cancel should clear the overlay"
    );
}

#[test]
fn open_delete_confirm_blocked_when_composer_active() {
    let mut state = issues_state_with_list("repo-1");
    state.issues_state.inline_state = InlineState::Composer {
        target: crate::state::ComposerTarget::NewComment,
        text: String::new(),
        cursor: 0,
    };
    let state = state.apply(AppEvent::OpenDeleteIssueConfirm);
    assert!(
        state.issues_state.delete_confirm.is_none(),
        "delete confirm must NOT open while composer is active"
    );
}

#[test]
fn close_issue_blocked_when_composer_active() {
    let mut state = issues_state_with_list("repo-1");
    state.issues_state.inline_state = InlineState::Composer {
        target: crate::state::ComposerTarget::NewComment,
        text: String::new(),
        cursor: 0,
    };
    let state = state.apply(AppEvent::CloseIssue);
    assert!(
        state.issues_state.close_mutation_pending.is_none(),
        "close must NOT begin while composer is active"
    );
}

// ── Delete result ─────────────────────────────────────────────────────────

#[test]
fn issue_deleted_removes_from_list_and_clears_detail() {
    let mut state = issues_state_with_list("repo-1");
    state.issues_state.issue_detail = Some(make_detail(1, "I_1"));
    state.issues_state.delete_mutation_pending = Some(IssueLifecycleMutationPending {
        scope_repo_id: RepositoryId("repo-1".to_string()),
        mutation_id: 1,
        issue_number: 1,
        node_id: None,
    });
    let state = state.apply(AppEvent::IssueDeleted {
        scope_repo_id: RepositoryId("repo-1".to_string()),
        issue_number: 1,
        mutation_id: 1,
    });
    assert!(
        !state.issues_state.issues.iter().any(|i| i.number == 1),
        "deleted issue should be removed from list"
    );
    assert!(
        state.issues_state.issue_detail.is_none(),
        "detail should be cleared when deleted"
    );
    assert_eq!(
        state.issues_state.issue_focus,
        IssueFocus::IssueList,
        "should refocus to list after delete"
    );
    assert!(state.issues_state.delete_mutation_pending.is_none());
    let notice = state.issues_state.draft_notice.as_deref();
    assert!(
        notice.is_some_and(|n| n.contains("Deleted issue #1")),
        "notice should mention deletion"
    );
}

#[test]
fn issue_deleted_with_wrong_mutation_id_is_ignored() {
    let mut state = issues_state_with_list("repo-1");
    state.issues_state.delete_mutation_pending = Some(IssueLifecycleMutationPending {
        scope_repo_id: RepositoryId("repo-1".to_string()),
        mutation_id: 100,
        issue_number: 1,
        node_id: None,
    });
    let state = state.apply(AppEvent::IssueDeleted {
        scope_repo_id: RepositoryId("repo-1".to_string()),
        issue_number: 1,
        mutation_id: 999,
    });
    assert!(
        state.issues_state.delete_mutation_pending.is_some(),
        "stale mutation id should NOT clear pending"
    );
    assert!(
        state.issues_state.issues.iter().any(|i| i.number == 1),
        "issue should NOT be removed for stale mutation id"
    );
}

// ── Mutation failure ──────────────────────────────────────────────────────

#[test]
fn mutation_failed_clears_close_pending_and_sets_error() {
    let mut state = issues_state_with_list("repo-1");
    state.issues_state.close_mutation_pending = Some(IssueLifecycleMutationPending {
        scope_repo_id: RepositoryId("repo-1".to_string()),
        mutation_id: 5,
        issue_number: 1,
        node_id: None,
    });
    let state = state.apply(AppEvent::MutationFailed {
        scope_repo_id: RepositoryId("repo-1".to_string()),
        issue_number: Some(1),
        mutation_id: Some(5),
        error: "network error".to_string(),
    });
    assert!(
        state.issues_state.close_mutation_pending.is_none(),
        "close pending should be cleared on failure"
    );
    assert!(
        state.issues_state.error.is_some(),
        "error should be set on failure"
    );
}

#[test]
fn mutation_failed_clears_delete_pending_and_sets_error() {
    let mut state = issues_state_with_list("repo-1");
    state.issues_state.delete_mutation_pending = Some(IssueLifecycleMutationPending {
        scope_repo_id: RepositoryId("repo-1".to_string()),
        mutation_id: 7,
        issue_number: 2,
        node_id: None,
    });
    let state = state.apply(AppEvent::MutationFailed {
        scope_repo_id: RepositoryId("repo-1".to_string()),
        issue_number: Some(2),
        mutation_id: Some(7),
        error: "forbidden".to_string(),
    });
    assert!(
        state.issues_state.delete_mutation_pending.is_none(),
        "delete pending should be cleared on failure"
    );
    assert!(state.issues_state.error.is_some());
}

#[test]
fn mutation_failed_with_unrelated_mutation_id_is_ignored_by_lifecycle() {
    let mut state = issues_state_with_list("repo-1");
    state.issues_state.close_mutation_pending = Some(IssueLifecycleMutationPending {
        scope_repo_id: RepositoryId("repo-1".to_string()),
        mutation_id: 50,
        issue_number: 1,
        node_id: None,
    });
    // A MutationFailed with mutation_id=99 does NOT match the lifecycle pending
    // (mutation_id=50). The lifecycle handler should return false, letting the
    // regular error handler process it.
    let state = state.apply(AppEvent::MutationFailed {
        scope_repo_id: RepositoryId("repo-1".to_string()),
        issue_number: Some(1),
        mutation_id: Some(99),
        error: "unrelated".to_string(),
    });
    assert!(
        state.issues_state.close_mutation_pending.is_some(),
        "unrelated mutation id should NOT clear lifecycle pending"
    );
}

// ── Failure scope/issue guards (issue #182) ───────────────────────────────
// A failure must match the FULL operation identity (mutation id + scope +
// issue number). A failure for a different scope or issue must not clear a
// lifecycle pending or display a wrong scoped error.

#[test]
fn close_failure_with_wrong_scope_is_ignored_by_lifecycle() {
    let mut state = issues_state_with_list("repo-1");
    state.issues_state.close_mutation_pending = Some(IssueLifecycleMutationPending {
        scope_repo_id: RepositoryId("repo-1".to_string()),
        mutation_id: 50,
        issue_number: 1,
        node_id: None,
    });
    let state = state.apply(AppEvent::MutationFailed {
        scope_repo_id: RepositoryId("repo-OTHER".to_string()),
        issue_number: Some(1),
        mutation_id: Some(50),
        error: "wrong scope".to_string(),
    });
    assert!(
        state.issues_state.close_mutation_pending.is_some(),
        "wrong-scope failure should NOT clear lifecycle pending"
    );
}

#[test]
fn close_failure_with_wrong_issue_number_is_ignored_by_lifecycle() {
    let mut state = issues_state_with_list("repo-1");
    state.issues_state.close_mutation_pending = Some(IssueLifecycleMutationPending {
        scope_repo_id: RepositoryId("repo-1".to_string()),
        mutation_id: 50,
        issue_number: 1,
        node_id: None,
    });
    let state = state.apply(AppEvent::MutationFailed {
        scope_repo_id: RepositoryId("repo-1".to_string()),
        issue_number: Some(999),
        mutation_id: Some(50),
        error: "wrong issue".to_string(),
    });
    assert!(
        state.issues_state.close_mutation_pending.is_some(),
        "wrong-issue-number failure should NOT clear lifecycle pending"
    );
}

#[test]
fn delete_failure_with_wrong_scope_is_ignored_by_lifecycle() {
    let mut state = issues_state_with_list("repo-1");
    state.issues_state.delete_mutation_pending = Some(IssueLifecycleMutationPending {
        scope_repo_id: RepositoryId("repo-1".to_string()),
        mutation_id: 70,
        issue_number: 2,
        node_id: None,
    });
    let state = state.apply(AppEvent::MutationFailed {
        scope_repo_id: RepositoryId("repo-OTHER".to_string()),
        issue_number: Some(2),
        mutation_id: Some(70),
        error: "wrong scope".to_string(),
    });
    assert!(
        state.issues_state.delete_mutation_pending.is_some(),
        "wrong-scope failure should NOT clear delete pending"
    );
}

// ── Exclusivity guards (issue #182) ───────────────────────────────────────
// While a close or delete mutation is in flight, beginning another close or
// opening the delete overlay is suppressed to avoid overwriting the pending
// record or running concurrent gh tasks for the same issue.

#[test]
fn repeated_close_while_close_pending_is_suppressed() {
    let mut state = issues_state_with_list("repo-1");
    state.issues_state.close_mutation_pending = Some(IssueLifecycleMutationPending {
        scope_repo_id: RepositoryId("repo-1".to_string()),
        mutation_id: 1,
        issue_number: 1,
        node_id: None,
    });
    // A second CloseIssue should NOT overwrite the existing pending (mutation_id stays 1).
    let state = state.apply(AppEvent::CloseIssue);
    match &state.issues_state.close_mutation_pending {
        Some(pending) => assert_eq!(
            pending.mutation_id, 1,
            "repeated close must not overwrite the in-flight pending"
        ),
        None => panic!("close pending should still be set after a suppressed repeat"),
    }
}

#[test]
fn open_delete_while_close_pending_is_suppressed() {
    let mut state = issues_state_with_list("repo-1");
    state.issues_state.close_mutation_pending = Some(IssueLifecycleMutationPending {
        scope_repo_id: RepositoryId("repo-1".to_string()),
        mutation_id: 1,
        issue_number: 1,
        node_id: None,
    });
    let state = state.apply(AppEvent::OpenDeleteIssueConfirm);
    assert!(
        state.issues_state.delete_confirm.is_none(),
        "delete overlay must not open while a close is in flight"
    );
}

#[test]
fn close_while_delete_pending_is_suppressed() {
    let mut state = issues_state_with_list("repo-1");
    state.issues_state.delete_mutation_pending = Some(IssueLifecycleMutationPending {
        scope_repo_id: RepositoryId("repo-1".to_string()),
        mutation_id: 2,
        issue_number: 1,
        node_id: None,
    });
    let state = state.apply(AppEvent::CloseIssue);
    assert!(
        state.issues_state.close_mutation_pending.is_none(),
        "close must not begin while a delete is in flight"
    );
}

#[test]
fn repeated_close_does_not_increment_mutation_id_indefinitely() {
    let mut state = issues_state_with_list("repo-1");
    let before = state.issues_state.next_mutation_id;
    state.issues_state.close_mutation_pending = Some(IssueLifecycleMutationPending {
        scope_repo_id: RepositoryId("repo-1".to_string()),
        mutation_id: 1,
        issue_number: 1,
        node_id: None,
    });
    let state = state.apply(AppEvent::CloseIssue);
    assert_eq!(
        state.issues_state.next_mutation_id, before,
        "suppressed close must not allocate a new mutation id"
    );
}

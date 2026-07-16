//! Tests for optimistic issue-create list insertion (issue #215).

use crate::domain::{Issue, IssueState, RepositoryId};
use crate::state::events::AppEvent;
use crate::state::types::{ComposerTarget, InlineState, IssueFocus};

use super::issues_tests_detail::{issues_mode_state_with_repo, p15_detail};

fn make_test_issue(number: u64) -> Issue {
    Issue {
        number,
        node_id: String::new(),
        title: format!("Test Issue #{number}"),
        state: IssueState::Open,
        author_login: "testuser".to_string(),
        updated_at: "2024-01-01T00:00:00Z".to_string(),
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

#[test]
fn test_create_issue_success_for_current_repo_sets_notice_and_clears_pending() {
    let submitted_target = InlineState::Composer {
        target: ComposerTarget::NewIssue,
        text: "title".to_string(),
        cursor: 5,
    };
    let state = issues_mode_state_with_repo("repo-1");
    let mut state = state.apply(AppEvent::MutationSubmitted {
        scope_repo_id: RepositoryId("repo-1".to_string()),
        mutation_id: 21,
        target: submitted_target.clone(),
    });
    state.issues_state.inline_state = submitted_target;

    let state = state.apply(AppEvent::IssueCreated {
        scope_repo_id: RepositoryId("repo-1".to_string()),
        mutation_id: 21,
        issue: Box::new(Issue {
            title: "Fresh title".to_string(),
            body: "Fresh body".to_string(),
            ..make_test_issue(77)
        }),
    });

    assert!(state.issues_state.mutation_pending.is_none());
    assert_eq!(state.issues_state.inline_state, InlineState::None);
    assert_eq!(
        state.issues_state.draft_notice.as_deref(),
        Some("Created issue #77")
    );
    assert_eq!(
        state
            .issues_state
            .issues()
            .first()
            .map(|issue| issue.number),
        Some(77),
        "created issue must be visible in the list without a GitHub reload (issue #215)"
    );
    assert_eq!(state.issues_state.selected_issue_index(), Some(0));
    assert_eq!(
        state.issues_state.issue_focus,
        IssueFocus::IssueList,
        "create success returns focus to the issue list"
    );
}

/// Issue #215: create success must keep the create notice; a network detail
/// reload is not part of the create reducer path (late detail events must not
/// race the optimistic selection/notice).
#[test]
fn test_create_issue_success_preserves_notice_without_clearing_via_refocus() {
    let submitted_target = InlineState::Composer {
        target: ComposerTarget::NewIssue,
        text: "title".to_string(),
        cursor: 5,
    };
    let state = issues_mode_state_with_repo("repo-1");
    let mut state = state.apply(AppEvent::MutationSubmitted {
        scope_repo_id: RepositoryId("repo-1".to_string()),
        mutation_id: 24,
        target: submitted_target.clone(),
    });
    state.issues_state.inline_state = submitted_target;
    state.issues_state.issue_detail = Some(p15_detail(10));

    let state = state.apply(AppEvent::IssueCreated {
        scope_repo_id: RepositoryId("repo-1".to_string()),
        mutation_id: 24,
        issue: Box::new(make_test_issue(88)),
    });

    assert_eq!(
        state.issues_state.draft_notice.as_deref(),
        Some("Created issue #88")
    );
    // Reducer itself does not wipe detail; async detail/list reloads are
    // intentionally not dispatched from the create success path.
    assert!(state.issues_state.issue_detail.is_some());
    assert_eq!(state.issues_state.selected_issue_index(), Some(0));
}

/// Issue #215: create success prepends the new issue ahead of existing rows and
/// selects it, without waiting on a race-prone list reload.
#[test]
fn test_create_issue_success_prepends_and_selects_new_issue() {
    let submitted_target = InlineState::Composer {
        target: ComposerTarget::NewIssue,
        text: "title".to_string(),
        cursor: 5,
    };
    let state = issues_mode_state_with_repo("repo-1");
    let mut state = state.apply(AppEvent::MutationSubmitted {
        scope_repo_id: RepositoryId("repo-1".to_string()),
        mutation_id: 22,
        target: submitted_target.clone(),
    });
    state.issues_state.inline_state = submitted_target;
    state
        .issues_state
        .list
        .replace_items(vec![make_test_issue(10), make_test_issue(9)]);
    state.issues_state.list.set_selected_index(Some(1));

    let state = state.apply(AppEvent::IssueCreated {
        scope_repo_id: RepositoryId("repo-1".to_string()),
        mutation_id: 22,
        issue: Box::new(Issue {
            title: "Brand new".to_string(),
            ..make_test_issue(42)
        }),
    });

    let numbers: Vec<_> = state
        .issues_state
        .issues()
        .iter()
        .map(|issue| issue.number)
        .collect();
    assert_eq!(numbers, vec![42, 10, 9]);
    assert_eq!(state.issues_state.selected_issue_index(), Some(0));
    assert_eq!(state.issues_state.issues()[0].title.as_str(), "Brand new");
}

/// Issue #215: closed-only list filters must not receive an open created issue,
/// but the create notice still records success.
#[test]
fn test_create_issue_success_skips_list_insert_when_filter_is_closed_only() {
    use crate::domain::IssueFilterState;

    let submitted_target = InlineState::Composer {
        target: ComposerTarget::NewIssue,
        text: "title".to_string(),
        cursor: 5,
    };
    let state = issues_mode_state_with_repo("repo-1");
    let mut state = state.apply(AppEvent::MutationSubmitted {
        scope_repo_id: RepositoryId("repo-1".to_string()),
        mutation_id: 23,
        target: submitted_target.clone(),
    });
    state.issues_state.inline_state = submitted_target;
    state.issues_state.committed_filter.state = Some(IssueFilterState::Closed);
    state
        .issues_state
        .list
        .replace_items(vec![make_test_issue(10)]);
    state.issues_state.list.set_selected_index(Some(0));

    let state = state.apply(AppEvent::IssueCreated {
        scope_repo_id: RepositoryId("repo-1".to_string()),
        mutation_id: 23,
        issue: Box::new(make_test_issue(99)),
    });

    assert_eq!(
        state
            .issues_state
            .issues()
            .iter()
            .map(|issue| issue.number)
            .collect::<Vec<_>>(),
        vec![10],
        "closed-only filter must not show a newly created open issue"
    );
    assert_eq!(state.issues_state.selected_issue_index(), Some(0));
    assert_eq!(
        state.issues_state.draft_notice.as_deref(),
        Some("Created issue #99")
    );
}

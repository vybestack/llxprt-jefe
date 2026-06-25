//! Pull Requests Mode composer-focus tests (#56) — open composer sets
//! NewComment subfocus, comment created appends + follows viewport, agent
//! chooser open/navigate/confirm/cancel.
//!
//! @plan PLAN-20260624-PR-MODE.P04
//! @requirement REQ-PR-010
//! @requirement REQ-PR-011

use crate::domain::{
    IssueComment, PrCheckStatus, PrState, PullRequest, PullRequestDetail, Repository, RepositoryId,
};
use crate::state::AppState;
use crate::state::types::{
    AppEvent, ComposerTarget, InlineState, PrDetailSubfocus, PrFocus, ScreenMode,
};

/// Helper: PR-mode state with a loaded detail (1 PR selected).
fn prs_state_with_detail(repo_id: &str, pr_number: u64) -> AppState {
    let mut state = AppState {
        screen_mode: ScreenMode::DashboardPullRequests,
        ..AppState::default()
    };
    state.repositories.push(Repository::new(
        RepositoryId(repo_id.to_string()),
        "Test Repo".to_string(),
        repo_id.to_string(),
        std::path::PathBuf::from("/tmp/test"),
    ));
    state.selected_repository_index = Some(0);
    state.prs_state.active = true;
    state.prs_state.pr_focus = PrFocus::PrDetail;
    state.prs_state.pull_requests = vec![PullRequest {
        number: pr_number,
        title: format!("PR #{pr_number}"),
        state: PrState::Open,
        author_login: "testuser".to_string(),
        updated_at: "2024-01-01T00:00:00Z".to_string(),
        head_ref: "feature".to_string(),
        base_ref: "main".to_string(),
        is_draft: false,
        review_decision: None,
        checks_status: PrCheckStatus::None,
        assignee_summary: String::new(),
        labels_summary: String::new(),
        comment_count: 0,
    }];
    state.prs_state.selected_pr_index = Some(0);
    state.prs_state.pr_detail = Some(PullRequestDetail {
        repo_owner_name: "owner/repo".to_string(),
        number: pr_number,
        title: format!("PR #{pr_number}"),
        state: PrState::Open,
        is_draft: false,
        author_login: "octocat".to_string(),
        created_at: "2024-01-01T00:00:00Z".to_string(),
        updated_at: "2024-01-02T00:00:00Z".to_string(),
        head_ref: "feature".to_string(),
        base_ref: "main".to_string(),
        labels: vec![],
        assignees: vec![],
        milestone: None,
        body: "PR body".to_string(),
        external_url: format!("https://github.com/owner/repo/pull/{pr_number}"),
        review_decision: None,
        checks_status: PrCheckStatus::None,
        reviews: vec![],
        checks: vec![],
        comments: vec![],
        has_more_comments: false,
        comments_cursor: None,
    });
    state.prs_state.inline_state = InlineState::None;
    state
}

/// Helper: a test comment.
fn make_comment(id: u64, author: &str) -> IssueComment {
    IssueComment {
        comment_id: id,
        author_login: author.to_string(),
        created_at: "2024-01-03T00:00:00Z".to_string(),
        edited_at: None,
        body: format!("Comment {id}"),
    }
}

/// PrOpenNewCommentComposer must set inline_state to Composer(NewComment) AND
/// move detail_subfocus to NewComment (#56).
///
/// @plan PLAN-20260624-PR-MODE.P04
/// @requirement REQ-PR-010
/// @pseudocode component-001 lines 292-298
#[test]
fn test_open_comment_composer_sets_subfocus_newcomment() {
    let state = prs_state_with_detail("repo-1", 1);

    let new_state = state.apply(AppEvent::PrOpenNewCommentComposer);

    assert!(
        matches!(
            &new_state.prs_state.inline_state,
            InlineState::Composer {
                target: ComposerTarget::NewComment,
                ..
            }
        ),
        "inline_state must be Composer(NewComment), got {:?}",
        new_state.prs_state.inline_state
    );
    assert_eq!(
        new_state.prs_state.detail_subfocus,
        PrDetailSubfocus::NewComment,
        "detail_subfocus must move to NewComment (#56)"
    );
}

/// PrCommentCreated must append the comment, clear the composer, set subfocus
/// to the new comment, and follow the viewport to reveal it (#56).
///
/// @plan PLAN-20260624-PR-MODE.P04
/// @requirement REQ-PR-010
/// @pseudocode component-001 lines 316-322
#[test]
fn test_comment_created_appends_and_marks_follow_viewport() {
    let mut state = prs_state_with_detail("repo-1", 1);
    // Simulate an active composer (pending mutation).
    state.prs_state.inline_state = InlineState::Composer {
        target: ComposerTarget::NewComment,
        text: "draft text".to_string(),
        cursor: 10,
    };
    state.prs_state.mutation_pending = Some(crate::state::types::PrMutationPending {
        scope_repo_id: RepositoryId("repo-1".to_string()),
        mutation_id: 1,
        target: ComposerTarget::NewComment,
    });
    state.prs_state.next_mutation_id = 2;
    let existing = make_comment(100, "alice");
    state
        .prs_state
        .pr_detail
        .as_mut()
        .unwrap_or_else(|| panic!("detail should exist"))
        .comments = vec![existing];

    let new_state = state.apply(AppEvent::PrCommentCreated {
        scope_repo_id: RepositoryId("repo-1".to_string()),
        pr_number: 1,
        mutation_id: 1,
        comment: make_comment(101, "bob"),
    });

    let detail = new_state
        .prs_state
        .pr_detail
        .clone()
        .unwrap_or_else(|| panic!("detail should remain"));
    assert_eq!(
        detail.comments.len(),
        2,
        "comment must be appended to existing"
    );
    assert_eq!(detail.comments[1].comment_id, 101);
    // Composer cleared.
    assert_eq!(new_state.prs_state.inline_state, InlineState::None);
    assert!(
        new_state.prs_state.mutation_pending.is_none(),
        "mutation_pending must clear after success"
    );
    // Subfocus set to the new comment.
    assert_eq!(
        new_state.prs_state.detail_subfocus,
        PrDetailSubfocus::Comment(1),
        "subfocus must point at the newly-created comment (#56)"
    );
}

/// PrOpenAgentChooser must open the chooser (when agents available).
///
/// @plan PLAN-20260624-PR-MODE.P04
/// @requirement REQ-PR-011
/// @pseudocode component-001 lines 331-340
#[test]
fn test_agent_chooser_open_navigate_confirm_cancel() {
    let mut state = prs_state_with_detail("repo-1", 1);
    // Provide agents so the chooser opens.
    state.agents.push(crate::domain::Agent::new(
        crate::domain::AgentId("agent-1".to_string()),
        RepositoryId("repo-1".to_string()),
        "Agent 1".to_string(),
        std::path::PathBuf::from("/tmp/agent1"),
    ));
    state.agents.push(crate::domain::Agent::new(
        crate::domain::AgentId("agent-2".to_string()),
        RepositoryId("repo-1".to_string()),
        "Agent 2".to_string(),
        std::path::PathBuf::from("/tmp/agent2"),
    ));

    // Open the chooser.
    let state = state.apply(AppEvent::PrOpenAgentChooser);
    assert!(
        state.prs_state.agent_chooser.is_some(),
        "agent_chooser must open"
    );

    // Navigate down.
    let state = state.apply(AppEvent::PrAgentChooserNavigateDown);
    let chooser = state
        .prs_state
        .agent_chooser
        .clone()
        .unwrap_or_else(|| panic!("chooser should remain open after navigate"));
    assert_eq!(chooser.selected_index, 1);

    // Navigate up.
    let state = state.apply(AppEvent::PrAgentChooserNavigateUp);
    let chooser = state
        .prs_state
        .agent_chooser
        .clone()
        .unwrap_or_else(|| panic!("chooser should remain open after navigate"));
    assert_eq!(chooser.selected_index, 0);

    // Confirm closes the chooser (and dispatches the send — not asserted here).
    let state = state.apply(AppEvent::PrAgentChooserConfirm);
    assert!(
        state.prs_state.agent_chooser.is_none(),
        "agent_chooser must close on confirm"
    );

    // Re-open then cancel.
    let state = state.apply(AppEvent::PrOpenAgentChooser);
    assert!(state.prs_state.agent_chooser.is_some());
    let state = state.apply(AppEvent::PrAgentChooserCancel);
    assert!(
        state.prs_state.agent_chooser.is_none(),
        "agent_chooser must close on cancel"
    );
}

//! Shared fixtures for app-input Pull Requests Mode integration tests.

use jefe::domain::{
    CommentDetailIdentity, IssueComment, PageToken, PaginatedList, PrCheckStatus, PrState,
    PullRequestDetail, RepositoryId,
};

/// Build an empty comment list for a PR detail fixture.
fn empty_comments() -> PaginatedList<IssueComment, CommentDetailIdentity> {
    PaginatedList::from_loaded(
        CommentDetailIdentity {
            scope_repo_id: RepositoryId::default(),
            number: 0,
        },
        Vec::new(),
        PageToken::from_cursor(None, false),
    )
}

/// Build a PR detail fixture for integration tests.
pub(super) fn make_test_pr_detail(number: u64) -> PullRequestDetail {
    PullRequestDetail {
        repo_owner_name: "owner/repo".to_string(),
        number,
        title: format!("PR #{number}"),
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
        external_url: format!("https://github.com/owner/repo/pull/{number}"),
        review_decision: None,
        checks_status: PrCheckStatus::None,
        reviews: vec![],
        checks: vec![],
        comments: empty_comments(),
        mergeable: None,
        merge_state_status: None,
    }
}

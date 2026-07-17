//! Behavioral coverage for RFC 3339 timestamp sorting (issue #336).

use crate::domain::{
    Issue, IssueState, PrCheckStatus, PrReview, PrReviewState, PrState, PullRequest,
};
use crate::github::{sort_issues, sort_pr_reviews, sort_pull_requests};

fn issue(number: u64, updated_at: &str) -> Issue {
    Issue {
        number,
        node_id: String::new(),
        title: format!("issue {number}"),
        state: IssueState::Open,
        author_login: String::new(),
        updated_at: updated_at.to_string(),
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

fn pull_request(number: u64, updated_at: &str) -> PullRequest {
    PullRequest {
        number,
        title: format!("pull request {number}"),
        state: PrState::Open,
        author_login: String::new(),
        updated_at: updated_at.to_string(),
        head_ref: String::new(),
        head_sha: String::new(),
        base_ref: String::new(),
        is_draft: false,
        review_decision: None,
        checks_status: PrCheckStatus::None,
        assignee_summary: String::new(),
        labels_summary: String::new(),
        comment_count: 0,
        mergeable: None,
    }
}

fn review(review_id: &str, submitted_at: &str) -> PrReview {
    PrReview {
        review_id: Some(review_id.to_string()),
        author_login: String::new(),
        state: PrReviewState::Commented,
        submitted_at: submitted_at.to_string(),
        body: None,
        review_threads: Vec::new(),
    }
}

#[test]
fn issues_sort_mixed_precision_by_instant_then_number() {
    let mut issues = vec![
        issue(4, "2026-07-02T10:00:00Z"),
        issue(3, "2026-07-02T10:00:00.123Z"),
        issue(2, "2026-07-02T09:00:00-01:00"),
        issue(1, "2026-07-02T10:00:00+00:00"),
    ];

    sort_issues(&mut issues);

    assert_eq!(
        issues.iter().map(|item| item.number).collect::<Vec<_>>(),
        vec![3, 1, 2, 4]
    );
}

#[test]
fn pull_requests_sort_mixed_offsets_by_instant_then_number() {
    let mut pull_requests = vec![
        pull_request(3, "2026-07-02T10:00:00+05:00"),
        pull_request(2, "2026-07-02T06:00:00Z"),
        pull_request(1, "2026-07-02T06:00:00+00:00"),
    ];

    sort_pull_requests(&mut pull_requests);

    assert_eq!(
        pull_requests
            .iter()
            .map(|item| item.number)
            .collect::<Vec<_>>(),
        vec![1, 2, 3]
    );
}

#[test]
fn pr_reviews_sort_mixed_forms_by_instant_then_review_id() {
    let mut reviews = vec![
        review("PRR_2", "2026-07-02T10:00:00Z"),
        review("PRR_3", "2026-07-02T15:00:00+05:00"),
        review("PRR_4", "2026-07-02T10:00:00.001Z"),
    ];

    sort_pr_reviews(&mut reviews);

    assert_eq!(
        reviews
            .iter()
            .map(|item| item.review_id.as_deref())
            .collect::<Vec<_>>(),
        vec![Some("PRR_4"), Some("PRR_3"), Some("PRR_2")]
    );
}

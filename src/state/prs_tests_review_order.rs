//! PR review newest-first ordering tests (issue #238).
//!
//! After `sort_pr_reviews`, rendered review headers and keyboard navigation
//! must share the same newest → oldest document order, bodyless reviews stay
//! non-focusable, and reply/resolve targets remain identified by thread id.

use crate::domain::{
    IssueComment, PrCheckStatus, PrReview, PrReviewState, PrReviewThread, PrState,
    PullRequestDetail,
};
use crate::github::sort_pr_reviews;
use crate::pr_detail_content::build_pr_detail_content;
use crate::state::events::AppEvent;
use crate::state::types::{InlineState, PrDetailSubfocus};

use super::prs_nav_ops::pr_detail_subfocus_order;
use super::prs_tests_detail::prs_mode_state;

fn make_test_pr_detail(number: u64) -> PullRequestDetail {
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
        head_sha: "sha123".to_string(),
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
        comments: crate::domain::PaginatedList::from_loaded(
            crate::domain::CommentDetailIdentity {
                scope_repo_id: crate::domain::RepositoryId::default(),
                number,
            },
            vec![],
            crate::domain::PageToken::from_cursor(None, false),
        ),
        mergeable: None,
        merge_state_status: None,
    }
}

fn review_with_thread(
    review_id: &str,
    author: &str,
    submitted_at: &str,
    body: Option<&str>,
    thread_id: &str,
) -> PrReview {
    PrReview {
        review_id: Some(review_id.to_string()),
        author_login: author.to_string(),
        state: PrReviewState::Commented,
        submitted_at: submitted_at.to_string(),
        body: body.map(str::to_string),
        review_threads: vec![PrReviewThread {
            thread_id: thread_id.to_string(),
            is_resolved: false,
            is_outdated: false,
            review_id: Some(review_id.to_string()),
            path: Some("src/main.rs".to_string()),
            line: Some(10),
            comments: vec![IssueComment {
                comment_id: 1,
                author_login: author.to_string(),
                created_at: submitted_at.to_string(),
                edited_at: None,
                body: format!("{author} thread"),
            }],
        }],
    }
}

/// Newest review (with body) is Review(0); its thread is ReviewThread(0).
#[test]
fn newest_review_is_first_nav_stop_after_sort() {
    let mut detail = make_test_pr_detail(238);
    detail.reviews = vec![
        review_with_thread(
            "PRR_OLD",
            "old",
            "2026-07-01T10:00:00Z",
            Some("old body"),
            "t_old",
        ),
        review_with_thread(
            "PRR_NEW",
            "new",
            "2026-07-03T10:00:00Z",
            Some("new body"),
            "t_new",
        ),
        review_with_thread(
            "PRR_MID",
            "mid",
            "2026-07-02T10:00:00Z",
            Some("mid body"),
            "t_mid",
        ),
    ];
    sort_pr_reviews(&mut detail.reviews);

    let order = pr_detail_subfocus_order(&detail);
    assert_eq!(
        order,
        vec![
            PrDetailSubfocus::Body,
            PrDetailSubfocus::Review(0),
            PrDetailSubfocus::ReviewThread(0),
            PrDetailSubfocus::Review(1),
            PrDetailSubfocus::ReviewThread(1),
            PrDetailSubfocus::Review(2),
            PrDetailSubfocus::ReviewThread(2),
            PrDetailSubfocus::NewComment,
        ]
    );
    assert_eq!(detail.reviews[0].author_login, "new");
    assert_eq!(detail.reviews[0].review_threads[0].thread_id, "t_new");
}

/// Rendered review headers follow the same newest-first order as navigation.
#[test]
fn render_order_matches_nav_order_after_sort() {
    let mut detail = make_test_pr_detail(238);
    detail.reviews = vec![
        review_with_thread(
            "PRR_OLD",
            "old_author",
            "2026-07-01T10:00:00Z",
            Some("old body"),
            "t_old",
        ),
        review_with_thread(
            "PRR_NEW",
            "new_author",
            "2026-07-03T10:00:00Z",
            Some("new body"),
            "t_new",
        ),
    ];
    sort_pr_reviews(&mut detail.reviews);

    let content = build_pr_detail_content(
        &detail,
        PrDetailSubfocus::Body,
        &InlineState::None,
        false,
        false,
    );
    let new_pos = content
        .text
        .find("new_author")
        .unwrap_or_else(|| panic!("newest author should render"));
    let old_pos = content
        .text
        .find("old_author")
        .unwrap_or_else(|| panic!("oldest author should render"));
    assert!(
        new_pos < old_pos,
        "newest review header must render before older ones"
    );

    let order = pr_detail_subfocus_order(&detail);
    let first_review = order
        .iter()
        .find_map(|item| match item {
            PrDetailSubfocus::Review(i) => Some(*i),
            _ => None,
        })
        .unwrap_or_else(|| panic!("expected a review focus stop"));
    assert_eq!(first_review, 0);
    assert_eq!(detail.reviews[first_review].author_login, "new_author");
}

/// Bodyless reviews remain non-focusable; their threads stay reachable and
/// follow the newest-first parent order.
#[test]
fn bodyless_newest_first_still_skips_review_headers_in_nav() {
    let mut detail = make_test_pr_detail(238);
    detail.reviews = vec![
        review_with_thread("PRR_OLD", "old", "2026-07-01T10:00:00Z", None, "t_old"),
        review_with_thread("PRR_NEW", "new", "2026-07-03T10:00:00Z", None, "t_new"),
    ];
    sort_pr_reviews(&mut detail.reviews);

    let order = pr_detail_subfocus_order(&detail);
    assert!(
        order
            .iter()
            .all(|item| !matches!(item, PrDetailSubfocus::Review(_))),
        "bodyless reviews must not be focus stops after reorder, got {order:?}"
    );
    assert_eq!(
        order
            .iter()
            .filter_map(|item| match item {
                PrDetailSubfocus::ReviewThread(i) => Some(*i),
                _ => None,
            })
            .collect::<Vec<_>>(),
        vec![0, 1]
    );
    assert_eq!(detail.reviews[0].review_threads[0].thread_id, "t_new");
}

/// Resolve still targets the correct flat thread index after newest-first reorder.
#[test]
fn resolve_targets_flat_thread_index_after_newest_first_reorder() {
    use crate::domain::RepositoryId;
    use crate::state::types::PrThreadResolvePending;

    let mut state = prs_mode_state("repo-1");
    let mut detail = make_test_pr_detail(238);
    detail.reviews = vec![
        review_with_thread(
            "PRR_OLD",
            "old",
            "2026-07-01T10:00:00Z",
            Some("old body"),
            "t_old",
        ),
        review_with_thread(
            "PRR_NEW",
            "new",
            "2026-07-03T10:00:00Z",
            Some("new body"),
            "t_new",
        ),
    ];
    sort_pr_reviews(&mut detail.reviews);
    state.prs_state.pr_detail = Some(detail);
    // Flat thread 0 is the newest review's thread after sort.
    state.prs_state.thread_resolve_pending = Some(PrThreadResolvePending {
        scope_repo_id: RepositoryId("repo-1".to_string()),
        thread_index: 0,
        thread_id: "t_new".to_string(),
        resolve: true,
        request_id: 1,
    });

    let state = state.apply(AppEvent::PrThreadResolveSucceeded {
        scope_repo_id: RepositoryId("repo-1".to_string()),
        thread_index: 0,
        is_resolved: true,
        request_id: 1,
    });
    let detail = state
        .prs_state
        .pr_detail
        .as_ref()
        .unwrap_or_else(|| panic!("detail present"));
    assert_eq!(detail.reviews[0].review_threads[0].thread_id, "t_new");
    assert!(
        detail.reviews[0].review_threads[0].is_resolved,
        "resolve by flat index 0 must hit the newest review's thread after reorder"
    );
    assert!(
        !detail.reviews[1].review_threads[0].is_resolved,
        "older thread must stay unresolved"
    );
}

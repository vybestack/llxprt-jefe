//! Behavior tests for shared Issue and PR detail comment pagination.

use crate::domain::{
    CommentDetailIdentity, IssueComment, ListRequestId, PageToken, PaginatedList, RepositoryId,
};
use crate::state::pagination::{AcceptOutcome, BeginOutcome, LoadCorrelation, PageResult};

fn identity() -> CommentDetailIdentity {
    CommentDetailIdentity {
        scope_repo_id: RepositoryId("repo-1".to_string()),
        number: 42,
    }
}

fn comment(comment_id: u64) -> IssueComment {
    IssueComment {
        comment_id,
        author_login: "octocat".to_string(),
        created_at: "2026-07-13T00:00:00Z".to_string(),
        edited_at: None,
        body: format!("comment {comment_id}"),
    }
}

fn seeded_comments() -> PaginatedList<IssueComment, CommentDetailIdentity> {
    PaginatedList::from_loaded(
        identity(),
        vec![comment(1)],
        PageToken::Cursor("cursor-1".to_string()),
    )
}

fn begin_comment_page(
    comments: &mut PaginatedList<IssueComment, CommentDetailIdentity>,
) -> ListRequestId {
    let Ok(request_id) = comments.next_request_id() else {
        panic!("comment request id must be available");
    };
    assert_eq!(
        comments.begin_page(PageToken::Cursor("cursor-1".to_string()), request_id),
        BeginOutcome::Started
    );
    request_id
}

#[test]
fn comment_page_accept_appends_items_and_advances_continuation() {
    let mut comments = seeded_comments();
    let request_id = begin_comment_page(&mut comments);

    let outcome = comments.accept_page(PageResult {
        identity: identity(),
        request_id,
        requested_token: PageToken::Cursor("cursor-1".to_string()),
        items: vec![comment(2)],
        next_page: PageToken::Cursor("cursor-2".to_string()),
    });

    assert_eq!(outcome, AcceptOutcome::Applied);
    assert_eq!(
        comments
            .items()
            .iter()
            .map(|item| item.comment_id)
            .collect::<Vec<_>>(),
        vec![1, 2]
    );
    assert_eq!(
        comments.next_page(),
        &PageToken::Cursor("cursor-2".to_string())
    );
    assert!(!comments.has_pending_request());
}

#[test]
fn comment_page_with_wrong_request_id_is_stale() {
    let mut comments = seeded_comments();
    begin_comment_page(&mut comments);

    let outcome = comments.accept_page(PageResult {
        identity: identity(),
        request_id: ListRequestId::from_raw(999),
        requested_token: PageToken::Cursor("cursor-1".to_string()),
        items: vec![comment(2)],
        next_page: PageToken::Done,
    });

    assert_eq!(outcome, AcceptOutcome::Stale);
    assert_eq!(comments.items().len(), 1);
    assert!(comments.has_pending_request());
}

#[test]
fn comment_page_with_wrong_token_is_stale() {
    let mut comments = seeded_comments();
    let request_id = begin_comment_page(&mut comments);

    let outcome = comments.accept_page(PageResult {
        identity: identity(),
        request_id,
        requested_token: PageToken::Cursor("wrong-cursor".to_string()),
        items: vec![comment(2)],
        next_page: PageToken::Done,
    });

    assert_eq!(outcome, AcceptOutcome::Stale);
    assert_eq!(comments.items().len(), 1);
    assert!(comments.has_pending_request());
}

#[test]
fn failed_comment_page_preserves_continuation_for_retry() {
    let mut comments = seeded_comments();
    let request_id = begin_comment_page(&mut comments);
    let correlation = LoadCorrelation::Page {
        identity: identity(),
        token: PageToken::Cursor("cursor-1".to_string()),
        request_id,
    };

    assert_eq!(
        comments.accept_failure(&correlation),
        AcceptOutcome::Applied
    );
    assert_eq!(
        comments.next_page(),
        &PageToken::Cursor("cursor-1".to_string())
    );
    assert!(!comments.has_pending_request());

    let retry_request_id = begin_comment_page(&mut comments);
    assert!(retry_request_id > request_id);
}

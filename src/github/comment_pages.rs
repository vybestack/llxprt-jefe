//! Comment pagination construction shared by GitHub boundary operations.

use crate::domain::{CommentDetailIdentity, IssueComment, PageToken, PaginatedList};

use super::CommentsResponse;

/// Build comments loaded before the application supplies a stable detail scope.
pub(super) fn loaded_comments(
    response: CommentsResponse,
) -> PaginatedList<IssueComment, CommentDetailIdentity> {
    PaginatedList::from_unbound(
        response.comments,
        PageToken::from_cursor(response.cursor, response.has_more),
    )
}

/// Build embedded detail comments as an exhausted, identity-free page.
pub(super) fn exhausted_comments(
    comments: Vec<IssueComment>,
) -> PaginatedList<IssueComment, CommentDetailIdentity> {
    PaginatedList::from_unbound(comments, PageToken::Done)
}

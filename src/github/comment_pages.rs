//! Comment pagination construction shared by GitHub boundary operations.

use crate::domain::{CommentDetailIdentity, IssueComment, PageToken, PaginatedList, RepositoryId};

use super::CommentsResponse;

/// Bind a paginated API response to its issue or pull-request detail.
pub(super) fn loaded_comments(
    repo_owner_name: String,
    number: u64,
    response: CommentsResponse,
) -> PaginatedList<IssueComment, CommentDetailIdentity> {
    PaginatedList::from_loaded(
        CommentDetailIdentity {
            scope_repo_id: RepositoryId(repo_owner_name),
            number,
        },
        response.comments,
        PageToken::from_cursor(response.cursor, response.has_more),
    )
}

/// Bind comments embedded in a detail response to an exhausted page.
pub(super) fn exhausted_comments(
    repo_owner_name: String,
    number: u64,
    comments: Vec<IssueComment>,
) -> PaginatedList<IssueComment, CommentDetailIdentity> {
    PaginatedList::from_loaded(
        CommentDetailIdentity {
            scope_repo_id: RepositoryId(repo_owner_name),
            number,
        },
        comments,
        PageToken::Done,
    )
}

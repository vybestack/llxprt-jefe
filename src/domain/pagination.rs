//! Pagination contracts shared across list state and boundary messages.
//!
//! These pure value types unify GraphQL cursor and REST page-number pagination
//! so the deterministic state container can derive whether another page exists
//! without storing a contradictory bool.

use super::RepositoryId;

/// Identity shared by Issue and PR detail comment pagination.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CommentDetailIdentity {
    /// Repository whose detail comments are loaded.
    pub scope_repo_id: RepositoryId,
    /// Issue or pull-request number within the repository.
    pub number: u64,
}

/// Correlation identifier for one list request.
///
/// Monotonically allocated by `PaginatedList::next_request_id`. The default
/// value (0) means "no ids allocated yet"; the first real request is 1.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
pub struct ListRequestId(u64);

impl ListRequestId {
    /// Construct from a raw counter value.
    #[must_use]
    pub const fn from_raw(value: u64) -> Self {
        Self(value)
    }

    /// Return the raw counter value.
    #[must_use]
    pub const fn get(self) -> u64 {
        self.0
    }

    /// Compute the next id, or `None` at `u64` exhaustion.
    #[must_use]
    pub const fn checked_next(self) -> Option<Self> {
        match self.0.checked_add(1) {
            Some(v) => Some(Self(v)),
            None => None,
        }
    }
}

/// Continuation required to fetch the next list page.
///
/// Unifies the two pagination models used by the `gh` backends:
/// - `Cursor` — GraphQL `pageInfo.endCursor` (Issues, PRs).
/// - `PageNumber` — REST next-page number (Actions). `PageNumber(n)` means the
///   NEXT page to request is `n`.
/// - `Done` — no more pages.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PageToken {
    /// GraphQL end-cursor for the next page.
    Cursor(String),
    /// REST next-page number (1-based).
    PageNumber(u32),
    /// No more pages remain.
    Done,
}

impl PageToken {
    /// Build the continuation from GraphQL page-info.
    ///
    /// `has_more = false` always yields `Done`. `has_more = true` yields
    /// `Cursor(c)` when a cursor is present, or `Done` when the cursor is
    /// missing (defensive: a backend claiming more pages with no cursor is
    /// treated as exhausted so the UI never wedges on a load-more that can't
    /// fire).
    #[must_use]
    pub fn from_cursor(cursor: Option<String>, has_more: bool) -> Self {
        if has_more {
            cursor.map_or(Self::Done, Self::Cursor)
        } else {
            Self::Done
        }
    }

    /// Build the continuation after a completed REST page.
    ///
    /// `page` is the page that just completed; `PageNumber` stores the NEXT
    /// page to request (`page + 1`). `has_more = false` yields `Done`.
    #[must_use]
    pub fn after_page(page: u32, has_more: bool) -> Self {
        if has_more {
            page.checked_add(1).map_or(Self::Done, Self::PageNumber)
        } else {
            Self::Done
        }
    }

    /// Whether more pages may be available (derived: `!Done`).
    #[must_use]
    pub const fn has_more(&self) -> bool {
        !matches!(self, Self::Done)
    }
}

//! Generic deterministic state container for one lazily-loaded list.
//!
//! `PaginatedList<T, I>` owns the full lifecycle of a single list: reload
//! (replace), page (append), failure, and stale rejection. It is pure — no
//! I/O, no side effects. Screen adapters (`state::*_load_ops`) construct
//! identity/result values and delegate here, then apply screen-specific
//! detail/error/scroll policy based on the returned [`AcceptOutcome`].
//!
//! Design invariants:
//! - Exactly one pending operation at a time ([`PendingLoad`] enum), so a
//!   reload and a page load can never disagree.
//! - `has_more` is derived from [`PageToken`] (`!Done`), never stored.
//! - Zero bool fields on the struct; loading visibility is derived from the
//!   pending kind + [`ReloadVisibility`].
//! - Stale rejection is unconditional (no `request_id == 0` special-case).

use crate::domain::{ListRequestId, PageToken};

/// Whether a reload shows a visible loading indicator or runs silently.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReloadVisibility {
    /// Show a loading indicator and reset selection to the first row.
    Visible,
    /// No loading indicator; preserve and clamp the existing selection.
    Silent,
}

/// The single in-flight operation, if any.
#[derive(Debug, Clone, PartialEq, Eq)]
enum PendingLoad<I> {
    /// A full reload (replace all items).
    Reload {
        identity: I,
        request_id: ListRequestId,
        visibility: ReloadVisibility,
    },
    /// A page append (fetch next page).
    Page {
        identity: I,
        token: PageToken,
        request_id: ListRequestId,
    },
}

/// Correlation key for matching a result (success or failure) to the pending
/// operation. Used by `accept_failure` and `is_stale`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LoadCorrelation<I> {
    /// Correlates a reload result/failure.
    Reload {
        identity: I,
        request_id: ListRequestId,
    },
    /// Correlates a page result/failure.
    Page {
        identity: I,
        token: PageToken,
        request_id: ListRequestId,
    },
}

/// Outcome of an accept method (success result or failure).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AcceptOutcome {
    /// The result was applied; items changed.
    Applied,
    /// The result was applied but the item set is empty.
    Empty,
    /// The result did not match the pending operation; state is unchanged.
    Stale,
}

/// Outcome of a `begin_*` method.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BeginOutcome {
    /// The operation was started.
    Started,
    /// A pending operation already exists; the caller should not start another.
    Busy,
    /// `next_page` is `Done`; there are no more pages to fetch.
    Exhausted,
    /// The provided token does not match `next_page`.
    TokenMismatch,
}

/// Error returned when `u64` request-id space is exhausted.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RequestIdExhausted;

/// Completed reload data ready to apply to the list.
#[derive(Debug, Clone)]
pub struct ReloadResult<T, I> {
    /// The identity this result belongs to.
    pub identity: I,
    /// The request id allocated by `next_request_id`.
    pub request_id: ListRequestId,
    /// The full replacement item set.
    pub items: Vec<T>,
    /// Continuation for the next page after this reload.
    pub next_page: PageToken,
}

/// Completed page data ready to append to the list.
#[derive(Debug, Clone)]
pub struct PageResult<T, I> {
    /// The identity this result belongs to.
    pub identity: I,
    /// The request id allocated by `next_request_id`.
    pub request_id: ListRequestId,
    /// The token that was used to request this page.
    pub requested_token: PageToken,
    /// Items to append.
    pub items: Vec<T>,
    /// Continuation for the next page.
    pub next_page: PageToken,
}

/// Generic deterministic state container for one lazily-loaded list.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PaginatedList<T, I> {
    items: Vec<T>,
    selected_index: Option<usize>,
    identity: Option<I>,
    next_page: PageToken,
    pending: Option<PendingLoad<I>>,
    last_request_id: ListRequestId,
}

impl<T, I> Default for PaginatedList<T, I> {
    fn default() -> Self {
        Self {
            items: Vec::new(),
            selected_index: None,
            identity: None,
            next_page: PageToken::Done,
            pending: None,
            last_request_id: ListRequestId::default(),
        }
    }
}

impl<T, I> PaginatedList<T, I> {
    /// Returns the loaded items.
    #[must_use]
    pub fn items(&self) -> &[T] {
        &self.items
    }

    /// Mutable access to the items vector (test-only).
    #[cfg(test)]
    pub(crate) fn items_mut(&mut self) -> &mut Vec<T> {
        &mut self.items
    }

    /// Returns the selected row index, if any.
    #[must_use]
    pub const fn selected_index(&self) -> Option<usize> {
        self.selected_index
    }

    /// Set the selected index, clamping to `len-1` or `None` when out of range.
    pub fn set_selected_index(&mut self, idx: Option<usize>) {
        self.selected_index = match (idx, self.items.len()) {
            (Some(i), n) if i < n => Some(i),
            (Some(_), n) if n > 0 => Some(n - 1),
            (Some(_) | None, _) => None,
        };
    }

    /// Returns the continuation token for the next page.
    #[must_use]
    pub const fn next_page(&self) -> &PageToken {
        &self.next_page
    }

    /// Whether more pages may be available (derived from `next_page`).
    #[must_use]
    pub fn has_more(&self) -> bool {
        self.next_page.has_more()
    }

    /// Whether any operation (reload or page) is pending.
    #[must_use]
    pub const fn has_pending_request(&self) -> bool {
        self.pending.is_some()
    }

    /// Whether a visible loading indicator should be shown.
    ///
    /// True for a `Visible` reload or any page load; false for a `Silent`
    /// reload and when nothing is pending.
    #[must_use]
    pub const fn is_loading(&self) -> bool {
        matches!(
            self.pending,
            Some(
                PendingLoad::Reload {
                    visibility: ReloadVisibility::Visible,
                    ..
                } | PendingLoad::Page { .. }
            )
        )
    }

    /// Returns the identity of the current pending operation, if any.
    #[must_use]
    pub const fn identity(&self) -> Option<&I> {
        self.identity.as_ref()
    }

    /// Clear items and selection, keeping request-id history and continuation.
    pub fn clear_items(&mut self) {
        self.items.clear();
        self.selected_index = None;
    }

    /// Full reset to default state (items, selection, identity, continuation,
    /// pending). Used on scope changes (e.g. repo switch).
    pub fn clear(&mut self) {
        self.items.clear();
        self.selected_index = None;
        self.identity = None;
        self.next_page = PageToken::Done;
        self.pending = None;
    }

    /// Allocate the next monotonic request id.
    ///
    /// Returns an error if the `u64` space is exhausted.
    pub fn next_request_id(&mut self) -> Result<ListRequestId, RequestIdExhausted> {
        match self.last_request_id.checked_next() {
            Some(next) => {
                self.last_request_id = next;
                Ok(next)
            }
            None => Err(RequestIdExhausted),
        }
    }

    /// Returns the last-allocated request id (for diagnostics).
    #[must_use]
    pub const fn last_request_id(&self) -> ListRequestId {
        self.last_request_id
    }

    /// Whether a load-more should fire: items non-empty, selection at last
    /// index, continuation available, and no pending operation.
    #[must_use]
    pub fn should_load_more(&self, selected_index: Option<usize>) -> bool {
        let Some(last) = self.items.len().checked_sub(1) else {
            return false;
        };
        Self::selected_index_is_last(selected_index, last)
            && self.next_page.has_more()
            && self.pending.is_none()
    }

    /// Check if the provided selection equals the last item index.
    fn selected_index_is_last(idx: Option<usize>, last: usize) -> bool {
        matches!(idx, Some(i) if i == last)
    }
}

// ── Reload / page begin operations (require I: Clone) ──────────────────────

impl<T, I: Clone> PaginatedList<T, I> {
    /// Begin a visible reload. Always wins over a pending page.
    pub fn begin_reload(&mut self, identity: I, request_id: ListRequestId) -> BeginOutcome {
        self.begin_reload_with_visibility(identity, request_id, ReloadVisibility::Visible)
    }

    /// Begin a silent reload (no visible loading indicator). Always wins.
    pub fn begin_silent_reload(&mut self, identity: I, request_id: ListRequestId) -> BeginOutcome {
        self.begin_reload_with_visibility(identity, request_id, ReloadVisibility::Silent)
    }

    /// Core reload begin: supersede any pending operation, reset continuation,
    /// store identity, and set the pending reload.
    pub fn begin_reload_with_visibility(
        &mut self,
        identity: I,
        request_id: ListRequestId,
        visibility: ReloadVisibility,
    ) -> BeginOutcome {
        self.next_page = PageToken::Done;
        self.identity = Some(identity.clone());
        self.pending = Some(PendingLoad::Reload {
            identity,
            request_id,
            visibility,
        });
        BeginOutcome::Started
    }

    /// Begin a page load. Returns `Busy` if pending, `Exhausted` if
    /// `next_page == Done`, `TokenMismatch` if the token doesn't match.
    pub fn begin_page(&mut self, token: PageToken, request_id: ListRequestId) -> BeginOutcome {
        if self.pending.is_some() {
            return BeginOutcome::Busy;
        }
        if matches!(self.next_page, PageToken::Done) {
            return BeginOutcome::Exhausted;
        }
        if token != self.next_page {
            return BeginOutcome::TokenMismatch;
        }
        let Some(identity) = &self.identity else {
            return BeginOutcome::TokenMismatch;
        };
        self.pending = Some(PendingLoad::Page {
            identity: identity.clone(),
            token,
            request_id,
        });
        BeginOutcome::Started
    }
}

// ── Accept operations (require I: PartialEq) ───────────────────────────────

impl<T, I: PartialEq> PaginatedList<T, I> {
    /// Apply a completed reload result.
    ///
    /// Stale (wrong identity or request id) → no change. On match: replace
    /// items, store identity + continuation, clear pending. For `Visible`
    /// select index 0 if non-empty else None; for `Silent` clamp existing
    /// selection to the new length.
    pub fn accept_loaded(&mut self, result: ReloadResult<T, I>) -> AcceptOutcome {
        let ReloadResult {
            identity,
            request_id,
            items,
            next_page,
        } = result;

        let visibility = match &self.pending {
            Some(PendingLoad::Reload {
                identity: pending_id,
                request_id: pending_req,
                visibility,
            }) if *pending_id == identity && *pending_req == request_id => *visibility,
            _ => return AcceptOutcome::Stale,
        };

        self.items = items;
        self.identity = Some(identity);
        self.next_page = next_page;
        self.pending = None;

        match visibility {
            ReloadVisibility::Visible => {
                self.selected_index = if self.items.is_empty() { None } else { Some(0) };
            }
            ReloadVisibility::Silent => {
                self.selected_index = match self.selected_index {
                    Some(i) if i < self.items.len() => Some(i),
                    Some(_) if self.items.is_empty() => None,
                    Some(_) => Some(self.items.len() - 1),
                    None => None,
                };
            }
        }

        if self.items.is_empty() {
            AcceptOutcome::Empty
        } else {
            AcceptOutcome::Applied
        }
    }

    /// Apply a completed page result (append).
    ///
    /// Stale unless pending is a `Page` with matching identity, request id,
    /// and requested token. On match: append items, store continuation, clear
    /// pending, preserve selection.
    pub fn accept_page(&mut self, result: PageResult<T, I>) -> AcceptOutcome {
        let PageResult {
            identity,
            request_id,
            requested_token,
            items,
            next_page,
        } = result;

        let matches = matches!(
            &self.pending,
            Some(PendingLoad::Page {
                identity: pending_id,
                token: pending_token,
                request_id: pending_req,
            }) if *pending_id == identity
                && *pending_token == requested_token
                && *pending_req == request_id
        );
        if !matches {
            return AcceptOutcome::Stale;
        }

        let was_empty = items.is_empty();
        self.items.extend(items);
        self.next_page = next_page;
        self.pending = None;

        if was_empty {
            AcceptOutcome::Empty
        } else {
            AcceptOutcome::Applied
        }
    }

    /// Apply a failure for the correlated operation.
    ///
    /// On match: clear pending, preserve rows/selection/identity/continuation.
    /// Always returns `Applied` on match (never `Empty`).
    pub fn accept_failure(&mut self, correlation: &LoadCorrelation<I>) -> AcceptOutcome {
        let matches = match (&self.pending, correlation) {
            (
                Some(PendingLoad::Reload {
                    identity: pid,
                    request_id: preq,
                    ..
                }),
                LoadCorrelation::Reload {
                    identity: cid,
                    request_id: creq,
                },
            ) => *pid == *cid && *preq == *creq,
            (
                Some(PendingLoad::Page {
                    identity: pid,
                    token: ptok,
                    request_id: preq,
                }),
                LoadCorrelation::Page {
                    identity: cid,
                    token: ctok,
                    request_id: creq,
                },
            ) => *pid == *cid && *ptok == *ctok && *preq == *creq,
            _ => false,
        };

        if matches {
            self.pending = None;
            AcceptOutcome::Applied
        } else {
            AcceptOutcome::Stale
        }
    }

    /// Whether the given correlation is stale (does not match the pending op).
    #[must_use]
    pub fn is_stale(&self, correlation: &LoadCorrelation<I>) -> bool {
        !matches!(
            self.accept_failure_proxy(correlation),
            AcceptOutcome::Applied
        )
    }

    /// Non-mutating proxy for `is_stale` — checks the same matching logic
    /// without clearing pending. We duplicate the match to avoid mutating in a
    /// predicate (accept_failure clears pending on match).
    fn accept_failure_proxy(&self, correlation: &LoadCorrelation<I>) -> AcceptOutcome {
        let matches = match (&self.pending, correlation) {
            (
                Some(PendingLoad::Reload {
                    identity: pid,
                    request_id: preq,
                    ..
                }),
                LoadCorrelation::Reload {
                    identity: cid,
                    request_id: creq,
                },
            ) => *pid == *cid && *preq == *creq,
            (
                Some(PendingLoad::Page {
                    identity: pid,
                    token: ptok,
                    request_id: preq,
                }),
                LoadCorrelation::Page {
                    identity: cid,
                    token: ctok,
                    request_id: creq,
                },
            ) => *pid == *cid && *ptok == *ctok && *preq == *creq,
            _ => false,
        };
        if matches {
            AcceptOutcome::Applied
        } else {
            AcceptOutcome::Stale
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Test identity type: a scope tag + a filter-equivalent value.
    type TestIdentity = (u32, String);

    fn ident(n: u32) -> TestIdentity {
        (n, "filter".to_string())
    }

    /// Extract a request id from `next_request_id`, panicking on exhaustion
    /// (acceptable in test setup where the state is known).
    fn alloc_request_id<T, I>(list: &mut PaginatedList<T, I>) -> ListRequestId {
        let Ok(id) = list.next_request_id() else {
            panic!("request id allocation must succeed in test setup");
        };
        id
    }

    // ── Request id allocation ───────────────────────────────────────────────

    #[test]
    fn first_request_id_is_one() {
        let mut list: PaginatedList<u32, TestIdentity> = PaginatedList::default();
        let id = alloc_request_id(&mut list);
        assert_eq!(id.get(), 1);
    }

    #[test]
    fn request_ids_increase_monotonically() {
        let mut list: PaginatedList<u32, TestIdentity> = PaginatedList::default();
        let a = alloc_request_id(&mut list);
        let b = alloc_request_id(&mut list);
        let c = alloc_request_id(&mut list);
        assert!(b.get() > a.get());
        assert!(c.get() > b.get());
    }

    #[test]
    fn request_id_exhaustion_returns_error() {
        let mut list: PaginatedList<u32, TestIdentity> = PaginatedList {
            last_request_id: ListRequestId::from_raw(u64::MAX),
            ..Default::default()
        };
        assert_eq!(list.next_request_id(), Err(RequestIdExhausted));
    }

    // ── Reload begin ─────────────────────────────────────────────────────────

    #[test]
    fn begin_reload_records_identity_and_visible_loading() {
        let mut list: PaginatedList<u32, TestIdentity> = PaginatedList::default();
        let req = alloc_request_id(&mut list);
        let outcome = list.begin_reload(ident(1), req);
        assert_eq!(outcome, BeginOutcome::Started);
        assert!(list.has_pending_request());
        assert!(list.is_loading());
        assert_eq!(list.identity(), Some(&ident(1)));
    }

    #[test]
    fn new_reload_supersedes_pending_page() {
        let mut list: PaginatedList<u32, TestIdentity> = PaginatedList::default();
        let req = alloc_request_id(&mut list);
        list.begin_reload(ident(1), req);
        // Simulate a reload completing to set up a continuation.
        let outcome = list.accept_loaded(ReloadResult {
            identity: ident(1),
            request_id: req,
            items: vec![10, 20],
            next_page: PageToken::PageNumber(2),
        });
        assert_eq!(outcome, AcceptOutcome::Applied);

        // Begin page 2.
        let req2 = alloc_request_id(&mut list);
        let page_outcome = list.begin_page(PageToken::PageNumber(2), req2);
        assert_eq!(page_outcome, BeginOutcome::Started);

        // A new reload supersedes the pending page.
        let req3 = alloc_request_id(&mut list);
        let reload_outcome = list.begin_reload(ident(1), req3);
        assert_eq!(reload_outcome, BeginOutcome::Started);
        // The pending is now a reload (identity may be same but kind changed).
        assert!(list.has_pending_request());
    }

    // ── Reload accept ─────────────────────────────────────────────────────────

    #[test]
    fn matching_reload_replaces_items_and_selects_first() {
        let mut list: PaginatedList<u32, TestIdentity> = PaginatedList::default();
        let req = alloc_request_id(&mut list);
        list.begin_reload(ident(1), req);

        let outcome = list.accept_loaded(ReloadResult {
            identity: ident(1),
            request_id: req,
            items: vec![10, 20, 30],
            next_page: PageToken::PageNumber(2),
        });
        assert_eq!(outcome, AcceptOutcome::Applied);
        assert_eq!(list.items(), &[10, 20, 30]);
        assert_eq!(list.selected_index(), Some(0));
        assert!(!list.has_pending_request());
        assert_eq!(list.next_page(), &PageToken::PageNumber(2));
    }

    #[test]
    fn matching_empty_reload_clears_items_and_selection() {
        let mut list: PaginatedList<u32, TestIdentity> = PaginatedList::default();
        let req = alloc_request_id(&mut list);
        list.begin_reload(ident(1), req);

        let outcome = list.accept_loaded(ReloadResult {
            identity: ident(1),
            request_id: req,
            items: Vec::new(),
            next_page: PageToken::Done,
        });
        assert_eq!(outcome, AcceptOutcome::Empty);
        assert!(list.items().is_empty());
        assert_eq!(list.selected_index(), None);
    }

    #[test]
    fn stale_reload_request_id_changes_nothing() {
        let mut list: PaginatedList<u32, TestIdentity> = PaginatedList::default();
        let req = alloc_request_id(&mut list);
        list.begin_reload(ident(1), req);

        let stale_req = ListRequestId::from_raw(999);
        let outcome = list.accept_loaded(ReloadResult {
            identity: ident(1),
            request_id: stale_req,
            items: vec![10],
            next_page: PageToken::Done,
        });
        assert_eq!(outcome, AcceptOutcome::Stale);
        assert!(list.items().is_empty());
        assert!(list.has_pending_request());
    }

    #[test]
    fn stale_reload_identity_changes_nothing() {
        let mut list: PaginatedList<u32, TestIdentity> = PaginatedList::default();
        let req = alloc_request_id(&mut list);
        list.begin_reload(ident(1), req);

        let outcome = list.accept_loaded(ReloadResult {
            identity: ident(2),
            request_id: req,
            items: vec![10],
            next_page: PageToken::Done,
        });
        assert_eq!(outcome, AcceptOutcome::Stale);
        assert!(list.items().is_empty());
    }

    // ── Silent reload ─────────────────────────────────────────────────────────

    #[test]
    fn silent_reload_is_pending_without_visible_loading() {
        let mut list: PaginatedList<u32, TestIdentity> = PaginatedList::default();
        let req = alloc_request_id(&mut list);
        list.begin_silent_reload(ident(1), req);
        assert!(list.has_pending_request());
        assert!(
            !list.is_loading(),
            "silent reload must not show a visible loading indicator"
        );
    }

    #[test]
    fn silent_reload_preserves_and_clamps_numeric_selection() {
        let mut list: PaginatedList<u32, TestIdentity> = PaginatedList {
            items: vec![10, 20, 30, 40, 50],
            selected_index: Some(3),
            identity: Some(ident(1)),
            ..Default::default()
        };

        let req = alloc_request_id(&mut list);
        list.begin_silent_reload(ident(1), req);

        // Silent reload completes with only 2 items — selection must clamp.
        let outcome = list.accept_loaded(ReloadResult {
            identity: ident(1),
            request_id: req,
            items: vec![100, 200],
            next_page: PageToken::Done,
        });
        assert_eq!(outcome, AcceptOutcome::Applied);
        assert_eq!(list.items(), &[100, 200]);
        assert_eq!(
            list.selected_index(),
            Some(1),
            "selection must clamp to last index of the new shorter list"
        );
    }

    // ── Page begin ───────────────────────────────────────────────────────────

    #[test]
    fn begin_page_requires_current_continuation() {
        let mut list: PaginatedList<u32, TestIdentity> = PaginatedList::default();
        let req = alloc_request_id(&mut list);
        list.begin_reload(ident(1), req);
        list.accept_loaded(ReloadResult {
            identity: ident(1),
            request_id: req,
            items: vec![10],
            next_page: PageToken::PageNumber(2),
        });

        let req2 = alloc_request_id(&mut list);
        // Wrong token (3 != 2).
        let outcome = list.begin_page(PageToken::PageNumber(3), req2);
        assert_eq!(outcome, BeginOutcome::TokenMismatch);

        // Correct token.
        let outcome2 = list.begin_page(PageToken::PageNumber(2), req2);
        assert_eq!(outcome2, BeginOutcome::Started);
    }

    #[test]
    fn begin_page_when_done_returns_exhausted() {
        let mut list: PaginatedList<u32, TestIdentity> = PaginatedList::default();
        let req = alloc_request_id(&mut list);
        // next_page is Done by default.
        let outcome = list.begin_page(PageToken::Done, req);
        assert_eq!(outcome, BeginOutcome::Exhausted);
    }

    #[test]
    fn begin_page_while_pending_returns_busy() {
        let mut list: PaginatedList<u32, TestIdentity> = PaginatedList::default();
        let req = alloc_request_id(&mut list);
        list.begin_reload(ident(1), req);

        let req2 = alloc_request_id(&mut list);
        // Even with a matching token, pending reload blocks page begin.
        let outcome = list.begin_page(PageToken::PageNumber(2), req2);
        assert_eq!(outcome, BeginOutcome::Busy);
    }

    // ── Page accept ───────────────────────────────────────────────────────────

    fn setup_list_with_page_pending() -> PaginatedList<u32, TestIdentity> {
        let mut list: PaginatedList<u32, TestIdentity> = PaginatedList::default();
        let req = alloc_request_id(&mut list);
        list.begin_reload(ident(1), req);
        list.accept_loaded(ReloadResult {
            identity: ident(1),
            request_id: req,
            items: vec![10, 20],
            next_page: PageToken::PageNumber(2),
        });
        let req2 = alloc_request_id(&mut list);
        let outcome = list.begin_page(PageToken::PageNumber(2), req2);
        assert_eq!(outcome, BeginOutcome::Started);
        list
    }

    #[test]
    fn matching_page_appends_items() {
        let mut list = setup_list_with_page_pending();
        let req2 = list.last_request_id();
        let outcome = list.accept_page(PageResult {
            identity: ident(1),
            request_id: req2,
            requested_token: PageToken::PageNumber(2),
            items: vec![30, 40],
            next_page: PageToken::Done,
        });
        assert_eq!(outcome, AcceptOutcome::Applied);
        assert_eq!(list.items(), &[10, 20, 30, 40]);
        assert!(!list.has_pending_request());
        assert_eq!(list.selected_index(), Some(0), "selection preserved");
    }

    #[test]
    fn page_with_wrong_request_id_is_stale() {
        let mut list = setup_list_with_page_pending();
        let outcome = list.accept_page(PageResult {
            identity: ident(1),
            request_id: ListRequestId::from_raw(999),
            requested_token: PageToken::PageNumber(2),
            items: vec![30],
            next_page: PageToken::Done,
        });
        assert_eq!(outcome, AcceptOutcome::Stale);
        assert_eq!(list.items(), &[10, 20]);
    }

    #[test]
    fn page_with_wrong_identity_is_stale() {
        let mut list = setup_list_with_page_pending();
        let req2 = list.last_request_id();
        let outcome = list.accept_page(PageResult {
            identity: ident(2),
            request_id: req2,
            requested_token: PageToken::PageNumber(2),
            items: vec![30],
            next_page: PageToken::Done,
        });
        assert_eq!(outcome, AcceptOutcome::Stale);
    }

    #[test]
    fn page_with_wrong_requested_token_is_stale() {
        let mut list = setup_list_with_page_pending();
        let req2 = list.last_request_id();
        let outcome = list.accept_page(PageResult {
            identity: ident(1),
            request_id: req2,
            requested_token: PageToken::PageNumber(3),
            items: vec![30],
            next_page: PageToken::Done,
        });
        assert_eq!(outcome, AcceptOutcome::Stale);
    }

    #[test]
    fn empty_page_applies_continuation_and_returns_empty() {
        let mut list = setup_list_with_page_pending();
        let req2 = list.last_request_id();
        let outcome = list.accept_page(PageResult {
            identity: ident(1),
            request_id: req2,
            requested_token: PageToken::PageNumber(2),
            items: Vec::new(),
            next_page: PageToken::Done,
        });
        assert_eq!(outcome, AcceptOutcome::Empty);
        assert_eq!(list.items(), &[10, 20], "existing items preserved");
        assert_eq!(list.next_page(), &PageToken::Done);
        assert!(!list.has_pending_request());
    }

    // ── Failure ───────────────────────────────────────────────────────────────

    #[test]
    fn matching_reload_failure_clears_pending_but_preserves_rows() {
        let mut list: PaginatedList<u32, TestIdentity> = PaginatedList {
            items: vec![10, 20],
            selected_index: Some(1),
            ..Default::default()
        };
        let req = alloc_request_id(&mut list);
        list.begin_reload(ident(1), req);

        let outcome = list.accept_failure(&LoadCorrelation::Reload {
            identity: ident(1),
            request_id: req,
        });
        assert_eq!(outcome, AcceptOutcome::Applied);
        assert!(!list.has_pending_request());
        assert_eq!(list.items(), &[10, 20], "rows preserved on failure");
        assert_eq!(list.selected_index(), Some(1));
    }

    #[test]
    fn matching_page_failure_preserves_continuation_for_retry() {
        let mut list = setup_list_with_page_pending();
        let req2 = list.last_request_id();
        let continuation_before = list.next_page().clone();

        let outcome = list.accept_failure(&LoadCorrelation::Page {
            identity: ident(1),
            token: PageToken::PageNumber(2),
            request_id: req2,
        });
        assert_eq!(outcome, AcceptOutcome::Applied);
        assert!(!list.has_pending_request());
        assert_eq!(
            list.next_page(),
            &continuation_before,
            "continuation preserved for retry"
        );
    }

    #[test]
    fn stale_failure_does_not_clear_current_pending_request() {
        let mut list: PaginatedList<u32, TestIdentity> = PaginatedList::default();
        let req = alloc_request_id(&mut list);
        list.begin_reload(ident(1), req);

        let outcome = list.accept_failure(&LoadCorrelation::Reload {
            identity: ident(1),
            request_id: ListRequestId::from_raw(999),
        });
        assert_eq!(outcome, AcceptOutcome::Stale);
        assert!(
            list.has_pending_request(),
            "stale failure must not clear pending"
        );
    }

    // ── should_load_more ───────────────────────────────────────────────────────

    #[test]
    fn load_more_is_true_at_last_row_with_continuation() {
        let list: PaginatedList<u32, TestIdentity> = PaginatedList {
            items: vec![10, 20, 30],
            selected_index: Some(2),
            next_page: PageToken::PageNumber(2),
            ..Default::default()
        };
        assert!(list.should_load_more(list.selected_index()));
    }

    #[test]
    fn load_more_is_false_before_last_row() {
        let list: PaginatedList<u32, TestIdentity> = PaginatedList {
            items: vec![10, 20, 30],
            selected_index: Some(1),
            next_page: PageToken::PageNumber(2),
            ..Default::default()
        };
        assert!(!list.should_load_more(list.selected_index()));
    }

    #[test]
    fn load_more_is_false_for_empty_list() {
        let list: PaginatedList<u32, TestIdentity> = PaginatedList::default();
        assert!(!list.should_load_more(None));
    }

    #[test]
    fn load_more_is_false_when_done() {
        let list: PaginatedList<u32, TestIdentity> = PaginatedList {
            items: vec![10],
            selected_index: Some(0),
            next_page: PageToken::Done,
            ..Default::default()
        };
        assert!(!list.should_load_more(list.selected_index()));
    }

    #[test]
    fn load_more_is_false_while_request_pending() {
        let mut list: PaginatedList<u32, TestIdentity> = PaginatedList {
            items: vec![10, 20],
            selected_index: Some(1),
            identity: Some(ident(1)),
            next_page: PageToken::PageNumber(2),
            ..Default::default()
        };
        let req = alloc_request_id(&mut list);
        let outcome = list.begin_page(PageToken::PageNumber(2), req);
        assert_eq!(outcome, BeginOutcome::Started);
        assert!(!list.should_load_more(list.selected_index()));
    }

    #[test]
    fn load_more_is_false_for_out_of_bounds_selection() {
        let list: PaginatedList<u32, TestIdentity> = PaginatedList {
            items: vec![10, 20],
            selected_index: Some(1),
            next_page: PageToken::PageNumber(2),
            ..Default::default()
        };
        // Pass an out-of-bounds selection (e.g. 99).
        assert!(!list.should_load_more(Some(99)));
    }
}

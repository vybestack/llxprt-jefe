//! Generic deterministic state container for one lazily-loaded list.
//!
//! `PaginatedList<T, I>` owns the full lifecycle of a single list: reload
//! (replace), page (append), failure, and stale rejection. It is pure — no
//! I/O, no side effects. State adapters construct identity/result values and
//! delegate here, then apply screen-specific detail/error/scroll policy based
//! on the returned [`AcceptOutcome`].
//!
//! Design invariants:
//! - Exactly one pending operation at a time ([`PendingLoad`] enum), so a
//!   reload and a page load can never disagree.
//! - `has_more` is derived from [`PageToken`] (`!Done`), never stored.
//! - Zero bool fields on the struct; loading visibility is derived from the
//!   pending kind + [`ReloadVisibility`].
//! - Stale rejection is unconditional (no `request_id == 0` special-case).

use super::{ListRequestId, PageToken};

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
    /// The result was applied but its replacement set or incoming page was empty.
    ///
    /// See [`accept_loaded`](Self::accept_loaded) and
    /// [`accept_page`](Self::accept_page) for the operation-specific meaning.
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

impl<T, I, Idx> std::ops::Index<Idx> for PaginatedList<T, I>
where
    Vec<T>: std::ops::Index<Idx>,
{
    type Output = <Vec<T> as std::ops::Index<Idx>>::Output;

    fn index(&self, index: Idx) -> &Self::Output {
        &self.items[index]
    }
}

impl<T, I> PaginatedList<T, I> {
    /// Build a settled list from boundary data that has no stable application
    /// identity yet.
    ///
    /// State reducers must call [`Self::rebind_identity`] with the selected
    /// repository and detail identity before beginning pagination.
    #[must_use]
    pub fn from_unbound(items: Vec<T>, next_page: PageToken) -> Self {
        Self {
            items,
            selected_index: None,
            identity: None,
            next_page,
            pending: None,
            last_request_id: ListRequestId::default(),
        }
    }

    /// Build a settled list from data loaded at a boundary.
    #[must_use]
    pub fn from_loaded(identity: I, items: Vec<T>, next_page: PageToken) -> Self {
        Self {
            items,
            selected_index: None,
            identity: Some(identity),
            next_page,
            pending: None,
            last_request_id: ListRequestId::default(),
        }
    }

    /// Returns the loaded items.
    #[must_use]
    pub fn items(&self) -> &[T] {
        &self.items
    }

    /// Returns the number of loaded items.
    #[must_use]
    pub fn len(&self) -> usize {
        self.items.len()
    }

    /// Returns whether no items are loaded.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.items.is_empty()
    }

    /// Returns the item at `index`, if present.
    #[must_use]
    pub fn get(&self, index: usize) -> Option<&T> {
        self.items.get(index)
    }

    /// Returns mutable access to the item at `index`, if present.
    pub fn get_mut(&mut self, index: usize) -> Option<&mut T> {
        self.items.get_mut(index)
    }

    /// Returns an iterator over loaded items.
    pub fn iter(&self) -> std::slice::Iter<'_, T> {
        self.items.iter()
    }

    /// Returns a mutable iterator over loaded items.
    pub fn iter_mut(&mut self) -> std::slice::IterMut<'_, T> {
        self.items.iter_mut()
    }

    /// Appends an item without changing pagination state.
    pub fn push(&mut self, item: T) {
        self.items.push(item);
    }

    /// Mutable access to the items vector (lib test-only).
    #[cfg(test)]
    pub(crate) fn items_mut(&mut self) -> &mut Vec<T> {
        &mut self.items
    }

    /// Replace the entire item set without touching selection or pagination.
    ///
    /// Production state adapters use this to splice server-side mutations back
    /// into an already-loaded list (e.g. reflecting a close/delete or filter
    /// result) without re-driving the reload lifecycle. Test setup and
    /// snapshot-restore also use it when the full item set is known but pending
    /// state should not be disturbed.
    pub fn replace_items(&mut self, items: Vec<T>) {
        self.items = items;
        self.set_selected_index(self.selected_index);
    }

    /// Sort loaded items in place without allocating a temporary list copy.
    ///
    /// Leaves selection index, identity, and pagination tokens unchanged —
    /// callers that need selection to follow an item identity must remap the
    /// selected index after sorting.
    pub(crate) fn sort_by<F>(&mut self, compare: F)
    where
        F: FnMut(&T, &T) -> std::cmp::Ordering,
    {
        self.items.sort_by(compare);
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

    /// Returns the identity of the loaded list, if any.
    #[must_use]
    pub const fn identity(&self) -> Option<&I> {
        self.identity.as_ref()
    }

    /// Rebind the loaded list to a boundary-provided identity and cancel any
    /// in-flight operation while preserving items and continuation.
    pub fn rebind_identity(&mut self, identity: I) {
        self.identity = Some(identity);
        self.pending = None;
    }

    /// Cancel any in-flight operation while preserving loaded data and the
    /// continuation token so the operation can be retried later.
    pub fn cancel_pending(&mut self) {
        self.pending = None;
    }

    /// Clear items and selection, keeping request-id history and continuation.
    pub fn clear_items(&mut self) {
        self.items.clear();
        self.selected_index = None;
    }

    /// Reset list content and pagination for a scope change (e.g. repo switch).
    ///
    /// Clears items, selection, identity, continuation, and any pending
    /// operation. The `last_request_id` counter is intentionally retained so
    /// request ids never recycle across scope changes (monotonic-id guarantee).
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

    /// Preserve a request-id high-water mark from a replaced container.
    ///
    /// Detail comment lists are replaceable snapshots. State adapters use this
    /// before allocating so callbacks from a retired snapshot cannot collide
    /// with a request started by its replacement.
    pub fn preserve_request_history(&mut self, last_request_id: ListRequestId) {
        self.last_request_id = self.last_request_id.max(last_request_id);
    }

    /// Returns the last-allocated request id (for diagnostics).
    #[must_use]
    pub const fn last_request_id(&self) -> ListRequestId {
        self.last_request_id
    }

    /// Whether a load-more should fire: items non-empty, selection at last
    /// index, continuation available, and no pending operation.
    ///
    /// `selected_index` is the caller's current selection (typically
    /// `self.selected_index()`), evaluated against the last item to decide
    /// whether to trigger load-more.
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

    /// Core reload begin: supersede any pending operation, preserve the current
    /// continuation until success, store identity, and set the pending reload.
    ///
    /// A reload always wins: any in-flight page request is abandoned (its
    /// result will be `Stale` when it arrives). The return value is always
    /// `Started`; callers track their request id to correlate the response.
    pub fn begin_reload_with_visibility(
        &mut self,
        identity: I,
        request_id: ListRequestId,
        visibility: ReloadVisibility,
    ) -> BeginOutcome {
        self.identity = Some(identity.clone());
        self.pending = Some(PendingLoad::Reload {
            identity,
            request_id,
            visibility,
        });
        BeginOutcome::Started
    }

    /// Begin a page load. Returns `Busy` if pending, `Exhausted` if
    /// `next_page == Done`, `TokenMismatch` if the token doesn't match or
    /// the list has no bound identity.
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
    /// pending, preserve selection. `Empty` is returned when the incoming page
    /// contributed zero items, which differs from `accept_loaded` where `Empty`
    /// means the resulting list is empty. This distinction is intentional: a
    /// page can arrive empty while the list still holds items from prior pages.
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
        if self.pending_matches(correlation) {
            self.pending = None;
            AcceptOutcome::Applied
        } else {
            AcceptOutcome::Stale
        }
    }

    /// Whether the given correlation is stale (does not match the pending op).
    #[must_use]
    pub fn is_stale(&self, correlation: &LoadCorrelation<I>) -> bool {
        !self.pending_matches(correlation)
    }

    /// Single source of truth for correlating a result with the pending op.
    ///
    /// Matches by operation kind, identity, request id, and (for pages) the
    /// requested continuation token. `accept_failure` and `is_stale` both
    /// delegate here so stale detection can never diverge from acceptance.
    fn pending_matches(&self, correlation: &LoadCorrelation<I>) -> bool {
        match (&self.pending, correlation) {
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
        }
    }
}

impl<'a, T, I> IntoIterator for &'a PaginatedList<T, I> {
    type Item = &'a T;
    type IntoIter = std::slice::Iter<'a, T>;
    fn into_iter(self) -> Self::IntoIter {
        self.items.iter()
    }
}

impl<'a, T, I> IntoIterator for &'a mut PaginatedList<T, I> {
    type Item = &'a mut T;
    type IntoIter = std::slice::IterMut<'a, T>;
    fn into_iter(self) -> Self::IntoIter {
        self.items.iter_mut()
    }
}

#[cfg(test)]
#[path = "paginated_list_tests.rs"]
mod tests;

//! Unit tests for the generic pagination state container.

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
fn visible_reload_failure_preserves_continuation() {
    let mut list = setup_list_with_page_pending();
    let page_request = list.last_request_id();
    let page_failure = LoadCorrelation::Page {
        identity: ident(1),
        token: PageToken::PageNumber(2),
        request_id: page_request,
    };
    assert_eq!(list.accept_failure(&page_failure), AcceptOutcome::Applied);

    let reload_request = alloc_request_id(&mut list);
    list.begin_reload(ident(1), reload_request);
    let outcome = list.accept_failure(&LoadCorrelation::Reload {
        identity: ident(1),
        request_id: reload_request,
    });

    assert_eq!(outcome, AcceptOutcome::Applied);
    assert!(!list.has_pending_request());
    assert_eq!(list.items(), &[10, 20]);
    assert_eq!(list.next_page(), &PageToken::PageNumber(2));
    assert!(list.has_more());
}

#[test]
fn silent_reload_failure_preserves_continuation() {
    let mut list = setup_list_with_page_pending();
    let page_request = list.last_request_id();
    let page_failure = LoadCorrelation::Page {
        identity: ident(1),
        token: PageToken::PageNumber(2),
        request_id: page_request,
    };
    assert_eq!(list.accept_failure(&page_failure), AcceptOutcome::Applied);

    let reload_request = alloc_request_id(&mut list);
    list.begin_silent_reload(ident(1), reload_request);
    let outcome = list.accept_failure(&LoadCorrelation::Reload {
        identity: ident(1),
        request_id: reload_request,
    });

    assert_eq!(outcome, AcceptOutcome::Applied);
    assert!(!list.has_pending_request());
    assert_eq!(list.items(), &[10, 20]);
    assert_eq!(list.next_page(), &PageToken::PageNumber(2));
    assert!(list.has_more());
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

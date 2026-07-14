//! Regression tests for coalesced post-mutation refresh scheduling.

use super::post_mutation_refresh::PostMutationRefresh;

fn requested_refresh() -> PostMutationRefresh {
    let mut refresh = PostMutationRefresh::default();
    refresh.request();
    refresh
}

#[test]
fn requested_refresh_waits_for_list_request_then_starts_once() {
    let mut refresh = requested_refresh();
    assert!(!refresh.is_ready(true, false));
    assert!(refresh.is_ready(false, false));
    refresh.started();
    assert!(!refresh.is_ready(false, false));
}

#[test]
fn requested_refresh_waits_for_detail_request_then_starts_once() {
    let mut refresh = requested_refresh();
    assert!(!refresh.is_ready(false, true));
    assert!(refresh.is_ready(false, false));
    refresh.started();
    assert!(!refresh.is_ready(false, false));
}

#[test]
fn requested_refresh_waits_for_both_requests_then_starts_once() {
    let mut refresh = requested_refresh();
    assert!(!refresh.is_ready(true, true));
    assert!(!refresh.is_ready(false, true));
    assert!(!refresh.is_ready(true, false));
    assert!(refresh.is_ready(false, false));
    refresh.started();
    assert!(!refresh.is_ready(false, false));
}

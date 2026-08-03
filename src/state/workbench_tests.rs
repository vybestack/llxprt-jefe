//! Workbench state tests (issue #626).
//!
//! Reducer-level tests proving:
//! - The default status filter is all-on (not the projection's all-off).
//! - Toggling a bucket flips exactly that bucket and resets the page to 0.
//! - Next/prev page are clamped at both ends (no wrap).

use super::{AppEvent, AppState};
use crate::state::transition::TransitionExt;
use crate::workbench_view::{StatusBucket, StatusFilterMask};

#[test]
fn default_status_filter_is_all_on() {
    let state = AppState::default();
    let mask = state.workbench_status_filter.mask();
    assert!(mask.allows(StatusBucket::NeedsYou));
    assert!(mask.allows(StatusBucket::Working));
    assert!(mask.allows(StatusBucket::Ready));
    assert!(mask.allows(StatusBucket::Stale));
    assert!(!mask.all_off(), "default mask must not be all-off");
}

#[test]
fn toggling_a_bucket_flips_only_that_bucket() {
    let state = AppState::default();
    let after = state
        .apply(AppEvent::ToggleWorkbenchStatusBucket(StatusBucket::Ready))
        .committed_pure();

    let mask = after.workbench_status_filter.mask();
    assert!(
        mask.allows(StatusBucket::NeedsYou),
        "unrelated buckets stay on"
    );
    assert!(mask.allows(StatusBucket::Working));
    assert!(
        !mask.allows(StatusBucket::Ready),
        "toggled bucket flips off"
    );
    assert!(mask.allows(StatusBucket::Stale));
}

#[test]
fn toggling_a_bucket_resets_page_to_zero() {
    let state = AppState {
        workbench_page: 5,
        ..AppState::default()
    };
    let after = state
        .apply(AppEvent::ToggleWorkbenchStatusBucket(StatusBucket::Working))
        .committed_pure();
    assert_eq!(
        after.workbench_page, 0,
        "toggling a filter must reset the page"
    );
}

#[test]
fn next_page_advances_page() {
    let state = AppState::default();
    let after = state.apply(AppEvent::WorkbenchNextPage).committed_pure();
    assert_eq!(after.workbench_page, 1);
}

#[test]
fn prev_page_clamps_at_zero() {
    let state = AppState::default();
    let after = state.apply(AppEvent::WorkbenchPrevPage).committed_pure();
    assert_eq!(
        after.workbench_page, 0,
        "prev page at 0 must clamp, not wrap"
    );
}

#[test]
fn prev_page_decrements_from_positive() {
    let state = AppState {
        workbench_page: 3,
        ..AppState::default()
    };
    let after = state.apply(AppEvent::WorkbenchPrevPage).committed_pure();
    assert_eq!(after.workbench_page, 2);
}

#[test]
fn next_page_then_prev_returns_to_start() {
    let state = AppState::default();
    let mid = state.apply(AppEvent::WorkbenchNextPage).committed_pure();
    let after = mid.apply(AppEvent::WorkbenchPrevPage).committed_pure();
    assert_eq!(after.workbench_page, 0);
}

#[test]
fn toggling_all_buckets_off_yields_all_off_mask() {
    let state = AppState::default();
    let mut after = state;
    for bucket in [
        StatusBucket::NeedsYou,
        StatusBucket::Working,
        StatusBucket::Ready,
        StatusBucket::Stale,
    ] {
        after = after
            .apply(AppEvent::ToggleWorkbenchStatusBucket(bucket))
            .committed_pure();
    }
    assert!(
        after.workbench_status_filter.mask().all_off(),
        "toggling every bucket off must yield all-off"
    );
}

#[test]
fn toggling_a_bucket_back_on_restores_all_on() {
    let state = AppState::default();
    let off = state
        .apply(AppEvent::ToggleWorkbenchStatusBucket(StatusBucket::Stale))
        .committed_pure();
    let on = off
        .apply(AppEvent::ToggleWorkbenchStatusBucket(StatusBucket::Stale))
        .committed_pure();
    assert_eq!(
        on.workbench_status_filter.mask(),
        StatusFilterMask::all_on(),
        "toggling twice must restore the original mask"
    );
}

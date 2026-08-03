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

/// The reducer deliberately does not know the page count. It cannot: the number
/// of pages depends on terminal size, which is a render-time fact and is not
/// part of `AppState`. So `WorkbenchNextPage` only ever increments (saturating
/// at the integer bound), and the projection clamps the requested page against
/// the real page count when it builds the view. This test pins that split so a
/// future reader does not mistake the missing upper bound for an oversight.
#[test]
fn next_page_increments_without_an_upper_bound_and_saturates() {
    let state = AppState {
        workbench_page: usize::MAX,
        ..AppState::default()
    };
    let after = state.apply(AppEvent::WorkbenchNextPage).committed_pure();
    assert_eq!(
        after.workbench_page,
        usize::MAX,
        "next page must saturate rather than overflow"
    );
}

/// An out-of-range page is clamped by the projection, not the reducer.
#[test]
fn projection_clamps_a_page_beyond_the_last() {
    let view = crate::workbench_view::build_workbench_view_ref(
        &[],
        crate::workbench_view::StatusFilterMask::all_on(),
        None,
        200,
        40,
        99,
    );
    assert_eq!(
        view.layout.page, 0,
        "a page past the end must clamp to the last page"
    );
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

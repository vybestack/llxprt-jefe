//! Behavioral coverage for issue #435.
//!
//! Launch failures (and startup session-reclaim reports) must not dead-end in a
//! truncated status-bar line, and must not steal the screen with a modal. The
//! status bar carries only a persistent *count* of outstanding errors; the full
//! text lives on the Errors screen where it is already selectable and copyable.

#[path = "common/app_state.rs"]
mod common_app_state;

use jefe::domain::ErrorSource;
use jefe::state::capture_reclaim_report;
use jefe::ui::components::status_bar_stats;

/// A realistic reclaim report: long, and enumerating far more sessions than a
/// single status-bar line could ever hold.
const RECLAIM_REPORT: &str = "21 live jefe session(s) match no agent and were left running: \
jefe-a1, jefe-a2, jefe-a3, jefe-a4, jefe-a5, jefe-a6, jefe-a7, jefe-a8, jefe-a9, jefe-a10, \
jefe-a11, jefe-a12, jefe-a13, jefe-a14, jefe-a15, jefe-a16, jefe-a17, jefe-a18, jefe-a19, \
jefe-a20, jefe-a21";

#[test]
fn a_clean_status_bar_shows_only_the_running_summary() {
    let stats = status_bar_stats(None, 4, 4, 9, 0);
    assert_eq!(stats, "4 repos | 4/9 running");
}

#[test]
fn outstanding_errors_appear_as_a_count_beside_the_summary() {
    let stats = status_bar_stats(None, 4, 4, 9, 3);
    assert_eq!(stats, "4 repos | 4/9 running | 3 errors");
}

#[test]
fn a_single_outstanding_error_is_not_pluralised() {
    let stats = status_bar_stats(None, 4, 4, 9, 1);
    assert_eq!(stats, "4 repos | 4/9 running | 1 error");
}

/// The whole point of the change: the bar is a *clue*, never a transcript. No
/// amount of error text may reach it, so there is nothing left to truncate.
#[test]
fn the_status_bar_never_carries_error_text() {
    let stats = status_bar_stats(None, 4, 4, 9, 21);
    assert!(
        !stats.contains("ERR:"),
        "status bar must not render error text: {stats}"
    );
    assert!(
        !stats.contains("jefe-a1"),
        "status bar must not render error detail: {stats}"
    );
    assert!(stats.ends_with("| 21 errors"), "{stats}");
}

/// Transient warnings (shell closed, theme resolve failed) are short, actionable
/// and still belong in the bar, but they must not hide the error clue.
#[test]
fn a_transient_warning_still_shows_the_error_count() {
    let stats = status_bar_stats(Some("Selected shell no longer exists."), 4, 4, 9, 2);
    assert_eq!(stats, "WARN: Selected shell no longer exists. | 2 errors");
}

#[test]
fn a_transient_warning_alone_shows_no_count() {
    let stats = status_bar_stats(Some("Selected shell no longer exists."), 4, 4, 9, 0);
    assert_eq!(stats, "WARN: Selected shell no longer exists.");
}

/// The reclaim report is the case that prompted this: it fired on startup,
/// listed 21 sessions in a 50-character slot, and buried everything else.
#[test]
fn the_reclaim_report_goes_to_the_errors_screen_not_the_status_bar() {
    let mut state = crate::common_app_state::app_state();
    capture_reclaim_report(&mut state, RECLAIM_REPORT);

    assert_eq!(
        state.warning_message, None,
        "the reclaim report must not occupy the status-bar warning slot"
    );
    assert_eq!(
        state
            .errors_state
            .last_error()
            .map(|entry| entry.detail.as_str()),
        Some(RECLAIM_REPORT),
        "the full report must be recorded in the errors ring"
    );
    assert!(
        state
            .errors_state
            .last_error()
            .is_some_and(|entry| entry.source == ErrorSource::Other),
        "the reclaim report is not attributable to a single subsystem"
    );
}

/// It is recorded *silently*: it must count toward the clue without hijacking
/// the user's current selection on the Errors screen.
#[test]
fn the_reclaim_report_counts_without_stealing_the_errors_selection() {
    let mut state = crate::common_app_state::app_state();
    capture_reclaim_report(&mut state, RECLAIM_REPORT);

    assert_eq!(state.errors_state.count(), 1);
    assert_eq!(
        state.last_error_title(),
        None,
        "a silent entry must not drive the status-bar error projection"
    );
}

/// The count is what the bar renders, so a reclaim report must move it.
#[test]
fn a_recorded_reclaim_report_drives_the_status_bar_count() {
    let mut state = crate::common_app_state::app_state();
    capture_reclaim_report(&mut state, RECLAIM_REPORT);

    let rendered = status_bar_stats(None, 4, 4, 9, state.errors_state.count());
    assert_eq!(rendered, "4 repos | 4/9 running | 1 error");
}

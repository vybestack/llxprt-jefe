//! Tests for Enter separation in the PTY input path (issue #627).

use super::{ENTER_INPUT_GAP, KeyWritePacing, PtyInputKind};

use std::time::{Duration, Instant};

/// A10: the worst case this exists for — a batch of queued key events drained
/// in one go, so the Enter would otherwise be written in the same instant as
/// the character before it. The whole guard interval is waited out.
#[test]
fn enter_written_in_the_same_instant_waits_the_whole_gap() {
    let now = Instant::now();
    let mut pacing = KeyWritePacing::new();
    pacing.record(now);

    assert_eq!(
        pacing.delay_before(PtyInputKind::Enter, now),
        ENTER_INPUT_GAP
    );
}

/// A10: part-way through the window, only the remainder is waited out.
#[test]
fn enter_part_way_through_the_window_waits_only_the_remainder() {
    let now = Instant::now();
    let mut pacing = KeyWritePacing::new();
    pacing.record(now);

    let elapsed = Duration::from_millis(20);
    assert_eq!(
        pacing.delay_before(PtyInputKind::Enter, now + elapsed),
        ENTER_INPUT_GAP.saturating_sub(elapsed)
    );
}

/// A11: typing at human speed costs nothing. The guard only ever repairs
/// separation jefe itself removed.
#[test]
fn enter_after_the_window_is_not_delayed() {
    let now = Instant::now();
    let mut pacing = KeyWritePacing::new();
    pacing.record(now);

    assert_eq!(
        pacing.delay_before(PtyInputKind::Enter, now + ENTER_INPUT_GAP),
        Duration::ZERO
    );
    assert_eq!(
        pacing.delay_before(
            PtyInputKind::Enter,
            now + ENTER_INPUT_GAP + Duration::from_millis(1)
        ),
        Duration::ZERO
    );
}

/// A11: the first thing ever written to a child has nothing to be separated
/// from.
#[test]
fn the_first_write_is_never_delayed() {
    let pacing = KeyWritePacing::new();

    assert_eq!(
        pacing.delay_before(PtyInputKind::Enter, Instant::now()),
        Duration::ZERO
    );
}

/// A12: nothing but Enter is ever held back, however recent the last write.
#[test]
fn non_enter_input_is_never_delayed() {
    let now = Instant::now();
    let mut pacing = KeyWritePacing::new();
    pacing.record(now);

    assert_eq!(
        pacing.delay_before(PtyInputKind::Other, now),
        Duration::ZERO
    );
}

/// A12: every write moves the mark, so a second Enter is separated from the
/// first one rather than from some older keystroke.
#[test]
fn each_write_resets_the_separation_mark() {
    let now = Instant::now();
    let mut pacing = KeyWritePacing::new();

    pacing.record(now);
    let later = now + Duration::from_millis(30);
    pacing.record(later);

    assert_eq!(
        pacing.delay_before(PtyInputKind::Enter, later),
        ENTER_INPUT_GAP,
        "the gap is measured from the most recent write, not the first"
    );
}

/// A12: a clock that appears to go backwards must not produce a negative or
/// wrapped wait.
#[test]
fn a_write_mark_in_the_future_still_bounds_the_wait() {
    let now = Instant::now();
    let mut pacing = KeyWritePacing::new();
    pacing.record(now + Duration::from_millis(100));

    assert_eq!(
        pacing.delay_before(PtyInputKind::Enter, now),
        ENTER_INPUT_GAP,
        "the wait is bounded by the guard interval"
    );
}

/// The default classification is the one that is never delayed, so a caller
/// that forgets to classify cannot accidentally add latency.
#[test]
fn the_default_input_kind_is_not_delayed() {
    let now = Instant::now();
    let mut pacing = KeyWritePacing::new();
    pacing.record(now);

    assert_eq!(PtyInputKind::default(), PtyInputKind::Other);
    assert_eq!(
        pacing.delay_before(PtyInputKind::default(), now),
        Duration::ZERO
    );
}

//! Behavioral tests for pure capture models and literal matchers.
//!
//! @plan PLAN-20260629-TMUX-HARNESS.P02
//! @requirement REQ-TMUX-HARNESS-002

use super::*;

/// Test-only helper: unwrap a `Result::Ok` or panic with context.
trait TestResultExt<T> {
    fn value_or_panic(self, context: &str) -> T;
}

impl<T, E: std::fmt::Debug> TestResultExt<T> for Result<T, E> {
    fn value_or_panic(self, context: &str) -> T {
        match self {
            Ok(value) => value,
            Err(error) => panic!("{context}: {error:?}"),
        }
    }
}

/// Test-only helper: assert a `Result::Err` or panic.
fn error_or_panic<T: std::fmt::Debug, E>(result: Result<T, E>, context: &str) -> E {
    match result {
        Err(error) => error,
        Ok(value) => panic!("{context}: unexpectedly succeeded with {value:?}"),
    }
}

/// Build a screen capture for matcher tests.
fn screen(lines: &[&str]) -> ScreenCapture {
    ScreenCapture::new(
        u16::try_from(lines.len()).unwrap_or_else(|_| panic!("too many test lines")),
        80,
        lines.iter().map(|line| (*line).to_string()).collect(),
    )
}

/// Build a scrollback sample for matcher tests.
fn history(size: u64, lines: &[&str]) -> ScrollbackSample {
    ScrollbackSample::new(size, lines.iter().map(|line| (*line).to_string()).collect())
}

/// Contains succeeds with the first zero-based line containing the literal.
///
/// @plan PLAN-20260629-TMUX-HARNESS.P02
/// @requirement REQ-TMUX-HARNESS-002
#[test]
fn screen_contains_literal_reports_first_matching_line() {
    let capture = screen(&["ready", "prompt> run", "prompt> done"]);

    let outcome = screen_contains(&capture, MatchPattern::literal("prompt>"));

    assert!(outcome.matched);
    assert_eq!(outcome.matched_line, Some(1));
    assert_eq!(outcome.pattern.text(), "prompt>");
}

/// Contains fails cleanly for absent literals, including empty captures.
///
/// @plan PLAN-20260629-TMUX-HARNESS.P02
/// @requirement REQ-TMUX-HARNESS-002
#[test]
fn screen_contains_literal_fails_when_absent() {
    let empty = screen(&[]);
    let nonempty = screen(&["alpha", "beta"]);

    let empty_outcome = screen_contains(&empty, MatchPattern::literal("alpha"));
    let absent_outcome = screen_contains(&nonempty, MatchPattern::literal("gamma"));

    assert!(!empty_outcome.matched);
    assert_eq!(empty_outcome.matched_line, None);
    assert!(!absent_outcome.matched);
    assert_eq!(absent_outcome.matched_line, None);
}

/// Absent is the complement used by waitForNot: it matches when a literal is gone.
///
/// @plan PLAN-20260629-TMUX-HARNESS.P02
/// @requirement REQ-TMUX-HARNESS-002
#[test]
fn screen_absent_literal_is_contains_complement() {
    let capture = screen(&["loading", "prompt"]);

    let missing = screen_absent(&capture, MatchPattern::literal("done"));
    let present = screen_absent(&capture, MatchPattern::literal("loading"));

    assert!(missing.matched);
    assert_eq!(missing.matched_line, None);
    assert!(!present.matched);
    assert_eq!(present.matched_line, Some(0));
}

/// Exact-count predicates report zero, multiple, and mismatch counts.
///
/// @plan PLAN-20260629-TMUX-HARNESS.P02
/// @requirement REQ-TMUX-HARNESS-002
#[test]
fn screen_count_reports_exact_literal_occurrences() {
    let capture = screen(&["xx", "x y x", "none"]);

    let zero = screen_count(&capture, MatchPattern::literal("z"), 0);
    let four = screen_count(&capture, MatchPattern::literal("x"), 4);
    let mismatch = screen_count(&capture, MatchPattern::literal("x"), 3);

    assert!(zero.matched);
    assert_eq!(zero.actual, 0);
    assert!(four.matched);
    assert_eq!(four.actual, 4);
    assert!(!mismatch.matched);
    assert_eq!(mismatch.actual, 4);
    assert_eq!(mismatch.expected, 3);
}

/// Empty literals are treated as no-match and zero-count for public matcher safety.
///
/// @plan PLAN-20260629-TMUX-HARNESS.P02
/// @requirement REQ-TMUX-HARNESS-002
#[test]
fn empty_literal_is_no_match_and_zero_count() {
    let capture = screen(&["abc", ""]);
    let contains = screen_contains(&capture, MatchPattern::literal(""));
    let absent = screen_absent(&capture, MatchPattern::literal(""));
    let count = screen_count(&capture, MatchPattern::literal(""), 0);

    assert!(!contains.matched);
    assert_eq!(contains.matched_line, None);
    assert!(absent.matched);
    assert!(count.matched);
    assert_eq!(count.actual, 0);
}

/// Scrollback matchers operate on sampled history text instead of current screen rows.
///
/// @plan PLAN-20260629-TMUX-HARNESS.P02
/// @requirement REQ-TMUX-HARNESS-002
#[test]
fn scrollback_contains_absent_and_count_scan_history_lines() {
    let sample = history(30, &["old prompt", "command", "old prompt"]);

    let contains = scrollback_contains(&sample, MatchPattern::literal("command"));
    let missing = scrollback_absent(&sample, MatchPattern::literal("new prompt"));
    let present = scrollback_absent(&sample, MatchPattern::literal("command"));
    let count = scrollback_count(&sample, MatchPattern::literal("old"), 2);

    assert!(contains.matched);
    assert_eq!(contains.matched_line, Some(1));
    assert!(missing.matched);
    assert_eq!(missing.matched_line, None);
    assert!(!present.matched);
    assert_eq!(present.matched_line, Some(1));
    assert!(count.matched);
    assert_eq!(count.actual, 2);
}

/// History delta succeeds when history grew by the requested minimum.
///
/// @plan PLAN-20260629-TMUX-HARNESS.P02
/// @requirement REQ-TMUX-HARNESS-002
#[test]
fn history_delta_accepts_in_range_growth() {
    let before = history(10, &[]);
    let after = history(14, &["a", "b"]);

    let outcome = history_delta(&before, &after, 4);

    assert!(outcome.matched);
    assert_eq!(outcome.actual_delta, Some(4));
    assert_eq!(outcome.previous_size, 10);
    assert_eq!(outcome.current_size, 14);
}

/// History delta fails for too-small growth and backwards history sizes.
///
/// @plan PLAN-20260629-TMUX-HARNESS.P02
/// @requirement REQ-TMUX-HARNESS-002
#[test]
fn history_delta_rejects_out_of_range_or_backwards_growth() {
    let before = history(10, &[]);
    let small = history(12, &[]);
    let backwards = history(8, &[]);

    let too_small = history_delta(&before, &small, 3);
    let impossible = history_delta(&before, &backwards, 1);

    assert!(!too_small.matched);
    assert_eq!(too_small.actual_delta, Some(2));
    assert!(!impossible.matched);
    assert_eq!(impossible.actual_delta, None);
}

/// tmux pane_dead output parses deterministically for live and dead panes.
///
/// @plan PLAN-20260629-TMUX-HARNESS.P02
/// @requirement REQ-TMUX-HARNESS-002
#[test]
fn pane_status_parses_tmux_dead_values() {
    let live = PaneStatus::parse_tmux_pane_dead("0\n").value_or_panic("live pane parses");
    let dead = PaneStatus::parse_tmux_pane_dead(" 1 ").value_or_panic("dead pane parses");

    assert!(!live.dead);
    assert!(dead.dead);
}

/// Unexpected pane_dead output is a typed parse error.
///
/// @plan PLAN-20260629-TMUX-HARNESS.P02
/// @requirement REQ-TMUX-HARNESS-002
#[test]
fn pane_status_rejects_unexpected_tmux_output() {
    let err = error_or_panic(PaneStatus::parse_tmux_pane_dead("maybe"), "bad pane value");

    assert_eq!(err.value, "maybe");
    assert_eq!(err.to_string(), "invalid tmux pane_dead value: 'maybe'");
}

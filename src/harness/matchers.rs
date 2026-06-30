//! Pure literal matchers and predicate outcomes for captured TUI text.
//!
//! @plan PLAN-20260629-TMUX-HARNESS.P02
//! @requirement REQ-TMUX-HARNESS-002

use super::capture::{ScreenCapture, ScrollbackSample};

/// Pattern used by harness predicates.
///
/// The first harness milestone intentionally supports literal text only. Regex
/// support can be added later behind another typed variant if it is justified.
///
/// @plan PLAN-20260629-TMUX-HARNESS.P02
/// @requirement REQ-TMUX-HARNESS-002
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MatchPattern {
    Literal(String),
}

impl MatchPattern {
    /// Construct a literal pattern.
    ///
    /// @plan PLAN-20260629-TMUX-HARNESS.P02
    /// @requirement REQ-TMUX-HARNESS-002
    #[must_use]
    pub fn literal(text: impl Into<String>) -> Self {
        Self::Literal(text.into())
    }

    /// Text representation used by literal scanning.
    #[must_use]
    pub fn text(&self) -> &str {
        match self {
            Self::Literal(text) => text,
        }
    }
}

/// Result of a contains/absent predicate.
///
/// @plan PLAN-20260629-TMUX-HARNESS.P02
/// @requirement REQ-TMUX-HARNESS-002
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PredicateOutcome {
    pub matched: bool,
    pub pattern: MatchPattern,
    pub matched_line: Option<usize>,
}

/// Result of an exact-count predicate.
///
/// @plan PLAN-20260629-TMUX-HARNESS.P02
/// @requirement REQ-TMUX-HARNESS-002
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CountOutcome {
    pub matched: bool,
    pub pattern: MatchPattern,
    pub expected: usize,
    pub actual: usize,
}

/// Result of comparing two scrollback history sizes.
///
/// @plan PLAN-20260629-TMUX-HARNESS.P02
/// @requirement REQ-TMUX-HARNESS-002
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HistoryDeltaOutcome {
    pub matched: bool,
    pub previous_size: u64,
    pub current_size: u64,
    pub min_delta: u64,
    pub actual_delta: Option<u64>,
}

/// Evaluate whether a screen capture contains `pattern`.
///
/// @plan PLAN-20260629-TMUX-HARNESS.P02
/// @requirement REQ-TMUX-HARNESS-002
#[must_use]
pub fn screen_contains(capture: &ScreenCapture, pattern: MatchPattern) -> PredicateOutcome {
    contains_lines(capture.lines(), pattern)
}

/// Evaluate whether a screen capture does not contain `pattern`.
///
/// @plan PLAN-20260629-TMUX-HARNESS.P02
/// @requirement REQ-TMUX-HARNESS-002
#[must_use]
pub fn screen_absent(capture: &ScreenCapture, pattern: MatchPattern) -> PredicateOutcome {
    absent_lines(capture.lines(), pattern)
}

/// Count exact literal occurrences in a screen capture.
///
/// @plan PLAN-20260629-TMUX-HARNESS.P02
/// @requirement REQ-TMUX-HARNESS-002
#[must_use]
pub fn screen_count(
    capture: &ScreenCapture,
    pattern: MatchPattern,
    expected: usize,
) -> CountOutcome {
    count_lines(capture.lines(), pattern, expected)
}

/// Evaluate whether a scrollback sample contains `pattern`.
///
/// @plan PLAN-20260629-TMUX-HARNESS.P02
/// @requirement REQ-TMUX-HARNESS-002
#[must_use]
pub fn scrollback_contains(sample: &ScrollbackSample, pattern: MatchPattern) -> PredicateOutcome {
    contains_lines(sample.lines(), pattern)
}

/// Evaluate whether a scrollback sample does not contain `pattern`.
///
/// @plan PLAN-20260629-TMUX-HARNESS.P02
/// @requirement REQ-TMUX-HARNESS-002
#[must_use]
pub fn scrollback_absent(sample: &ScrollbackSample, pattern: MatchPattern) -> PredicateOutcome {
    absent_lines(sample.lines(), pattern)
}

/// Count exact literal occurrences in a scrollback sample.
///
/// @plan PLAN-20260629-TMUX-HARNESS.P02
/// @requirement REQ-TMUX-HARNESS-002
#[must_use]
pub fn scrollback_count(
    sample: &ScrollbackSample,
    pattern: MatchPattern,
    expected: usize,
) -> CountOutcome {
    count_lines(sample.lines(), pattern, expected)
}

/// Evaluate whether scrollback history grew by at least `min_delta`.
///
/// @plan PLAN-20260629-TMUX-HARNESS.P02
/// @requirement REQ-TMUX-HARNESS-002
#[must_use]
pub fn history_delta(
    before: &ScrollbackSample,
    after: &ScrollbackSample,
    min_delta: u64,
) -> HistoryDeltaOutcome {
    let actual_delta = after.history_size.checked_sub(before.history_size);
    let matched = actual_delta.is_some_and(|delta| delta >= min_delta);
    HistoryDeltaOutcome {
        matched,
        previous_size: before.history_size,
        current_size: after.history_size,
        min_delta,
        actual_delta,
    }
}

/// Evaluate literal containment over a line slice.
fn contains_lines(lines: &[String], pattern: MatchPattern) -> PredicateOutcome {
    let matched_line = first_matching_line(lines, pattern.text());
    PredicateOutcome {
        matched: matched_line.is_some(),
        pattern,
        matched_line,
    }
}

/// Evaluate literal absence over a line slice.
fn absent_lines(lines: &[String], pattern: MatchPattern) -> PredicateOutcome {
    let present = first_matching_line(lines, pattern.text());
    PredicateOutcome {
        matched: present.is_none(),
        pattern,
        matched_line: present,
    }
}

/// Count literal occurrences over a line slice.
fn count_lines(lines: &[String], pattern: MatchPattern, expected: usize) -> CountOutcome {
    let actual = count_occurrences(lines, pattern.text());
    CountOutcome {
        matched: actual == expected,
        pattern,
        expected,
        actual,
    }
}

/// Return the first zero-based line index containing a literal.
fn first_matching_line(lines: &[String], needle: &str) -> Option<usize> {
    if needle.is_empty() {
        return None;
    }
    lines.iter().position(|line| line.contains(needle))
}

/// Count all non-overlapping literal occurrences across all lines.
fn count_occurrences(lines: &[String], needle: &str) -> usize {
    if needle.is_empty() {
        return 0;
    }
    lines.iter().map(|line| line.matches(needle).count()).sum()
}

//! Shared UI utility functions.
//!
//! Pure, iocraft-free helpers used across multiple UI components. These
//! functions contain no side effects and are unit-testable without a terminal.

use unicode_width::{UnicodeWidthChar, UnicodeWidthStr};

/// Ellipsis character appended when text is truncated to fit a width budget.
pub const ELLIPSIS: char = '…';

/// Truncate `text` to fit within `max_width` terminal columns, appending an
/// ellipsis (`…`) when truncation occurs.
///
/// Uses character boundaries and Unicode display width so multi-byte characters
/// are never split and wide characters (e.g. CJK) are accounted for.
///
/// # Edge cases
///
/// - `max_width == 0` → returns an empty string.
/// - Text that already fits → returned unchanged.
/// - `max_width` too small for any content (≤ ellipsis width) → returns just `…`.
#[must_use]
pub fn truncate_with_ellipsis(text: &str, max_width: usize) -> String {
    if max_width == 0 {
        return String::new();
    }
    if UnicodeWidthStr::width(text) <= max_width {
        return text.to_string();
    }

    let ellipsis_width = ELLIPSIS.width().unwrap_or(1);
    if max_width <= ellipsis_width {
        return ELLIPSIS.to_string();
    }

    let content_width = max_width - ellipsis_width;
    let mut used = 0usize;
    // Pre-allocate for the worst-case content plus ellipsis to avoid
    // per-character reallocations in the hot UI render path.
    let mut result = String::with_capacity(max_width);
    for ch in text.chars() {
        let width = ch.width().unwrap_or(0);
        if used + width > content_width {
            break;
        }
        used += width;
        result.push(ch);
    }
    result.push(ELLIPSIS);
    result
}

#[cfg(test)]
mod tests {
    use super::truncate_with_ellipsis;
    use unicode_width::UnicodeWidthStr;

    #[test]
    fn text_shorter_than_budget_returned_unchanged() {
        assert_eq!(truncate_with_ellipsis("hello", 10), "hello");
    }

    #[test]
    fn text_exceeding_budget_truncated_with_ellipsis() {
        let result = truncate_with_ellipsis("a very long title that exceeds the budget", 10);
        assert!(result.ends_with('\u{2026}'));
        assert_eq!(
            UnicodeWidthStr::width(result.as_str()),
            10,
            "truncated result must exactly fill the width budget"
        );
    }

    #[test]
    fn text_exactly_at_budget_not_truncated() {
        assert_eq!(truncate_with_ellipsis("exact", 5), "exact");
    }

    #[test]
    fn empty_string_returned_unchanged() {
        assert_eq!(truncate_with_ellipsis("", 10), "");
    }

    #[test]
    fn zero_budget_returns_empty_string() {
        assert_eq!(truncate_with_ellipsis("hello", 0), "");
    }

    #[test]
    fn one_column_budget_returns_just_ellipsis() {
        assert_eq!(truncate_with_ellipsis("abcdef", 1), "…");
    }

    #[test]
    fn two_column_budget_returns_one_char_plus_ellipsis() {
        let result = truncate_with_ellipsis("abcdef", 2);
        assert_eq!(result, "a…");
        assert_eq!(UnicodeWidthStr::width(result.as_str()), 2);
    }

    #[test]
    fn multi_byte_cjk_wide_chars_truncate_on_character_boundary() {
        // Each CJK char has display width 2.
        let text = "日本語テスト";
        let result = truncate_with_ellipsis(text, 5);
        assert!(
            UnicodeWidthStr::width(result.as_str()) <= 5,
            "truncated CJK result must not exceed the width budget: {result}"
        );
        assert!(result.ends_with('\u{2026}'));
        // First char must survive truncation (not split mid-code-point).
        assert_eq!(result.chars().next(), Some('日'));
    }

    #[test]
    fn unicode_emoji_truncates_on_character_boundary() {
        let title = "\u{1F600}\u{1F601}\u{1F602}\u{1F603}\u{1F604}\u{1F605}\u{1F606}\u{1F607}\u{1F608}\u{1F609}";
        let result = truncate_with_ellipsis(title, 5);
        assert!(
            UnicodeWidthStr::width(result.as_str()) <= 5,
            "truncated emoji result must not exceed the width budget: {result}"
        );
        assert!(result.ends_with('\u{2026}'));
        assert!(result.chars().next().is_some());
    }

    #[test]
    fn full_width_chars_at_exact_budget_returned_unchanged() {
        // Fullwidth digits: each has display width 2, so two = width 4.
        let text = "１２";
        assert_eq!(truncate_with_ellipsis(text, 4), "１２");
    }

    #[test]
    fn full_width_chars_exceeding_budget_are_truncated() {
        // Width 4 > 3 budget: only one wide char (width 2) fits before ellipsis.
        let text = "１２";
        let result = truncate_with_ellipsis(text, 3);
        assert!(result.ends_with('\u{2026}'));
        assert!(
            UnicodeWidthStr::width(result.as_str()) <= 3,
            "truncated result must not exceed budget 3: {result}"
        );
        assert_eq!(result.chars().next(), Some('１'));
    }
}

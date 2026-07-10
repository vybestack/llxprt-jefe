//! Shared UI utility functions.
//!
//! Pure, iocraft-free helpers used across multiple UI components. These
//! functions contain no side effects and are unit-testable without a terminal.

use unicode_width::{UnicodeWidthChar, UnicodeWidthStr};

/// Ellipsis character appended when text is truncated to fit a width budget.
pub const ELLIPSIS: char = '…';

/// Em-dash used as the placeholder for empty detail fields (issue #155):
/// replaces the leaky `-` / `None` run-on so empty metadata reads cleanly.
pub const EMPTY_FIELD: &str = "—";

/// English month abbreviations, indexed by month number (1 = Jan).
const MONTH_ABBR: [&str; 13] = [
    "", "Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec",
];

/// Format a GitHub ISO-8601 timestamp into a compact human date.
///
/// Accepts the forms `gh` returns (`2026-07-06T15:26:53Z` and the date-only
/// `2026-07-06`) and renders `Jul 6, 2026` (or `Jul 6, 2026 15:26` when a time
/// component is present). Anything that does not parse is returned unchanged so
/// a surprising timestamp never blanks out the header — the raw value is the
/// fallback (issue #155: raw ISO timestamps are the defect being fixed, but a
/// parse failure must degrade to the original, not to an empty field).
///
/// This is dependency-free (no `chrono`/`time` crate) to keep comrak the only
/// new dependency introduced by the detail redesign.
#[must_use]
pub fn format_iso_date(iso: &str) -> String {
    let trimmed = iso.trim();
    if trimmed.is_empty() {
        return String::new();
    }
    // Split date from optional time at 'T' or a space.
    let (date_part, time_part) = trimmed
        .split_once('T')
        .or_else(|| trimmed.split_once(' '))
        .map_or((trimmed, None), |(d, t)| (d, Some(t)));

    let Some((year, month, day)) = parse_date(date_part) else {
        return trimmed.to_string();
    };
    let Some(month_abbr) = MONTH_ABBR.get(month) else {
        return trimmed.to_string();
    };
    let base = format!("{month_abbr} {day}, {year}");

    if let Some(time) = time_part
        && let Some(hhmm) = parse_hhmm(time)
    {
        return format!("{base} {hhmm}");
    }
    base
}

/// Parse a `YYYY-MM-DD` date into `(year, month, day)` with month in `1..=12`.
fn parse_date(s: &str) -> Option<(i32, usize, u32)> {
    let s = s.trim();
    let mut parts = s.split('-');
    let year: i32 = parts.next()?.trim().parse().ok()?;
    let month: usize = parts.next()?.trim().parse().ok()?;
    let day: u32 = parts.next()?.trim().parse().ok()?;
    if !(1..=12).contains(&month) {
        return None;
    }
    if day == 0 {
        return None;
    }
    Some((year, month, day))
}

/// Parse an `HH:MM` prefix out of a time component like `15:26:53Z`,
/// returning the `HH:MM` string. Seconds and the trailing `Z`/offset are
/// dropped so only the hour/minute is shown.
fn parse_hhmm(time: &str) -> Option<String> {
    let t = time.trim().trim_end_matches('Z');
    let mut parts = t.split(':');
    let hh: u32 = parts.next()?.trim().parse().ok()?;
    let mm: u32 = parts.next()?.trim().parse().ok()?;
    if hh > 23 || mm > 59 {
        return None;
    }
    Some(format!("{hh:02}:{mm:02}"))
}

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
    use super::{format_iso_date, truncate_with_ellipsis};
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

    // ── format_iso_date (issue #155) ───────────────────────────────────────

    #[test]
    fn format_iso_date_full_timestamp() {
        assert_eq!(format_iso_date("2026-07-06T15:26:53Z"), "Jul 6, 2026 15:26");
    }

    #[test]
    fn format_iso_date_date_only() {
        assert_eq!(format_iso_date("2026-07-06"), "Jul 6, 2026");
    }

    #[test]
    fn format_iso_date_space_separator() {
        assert_eq!(format_iso_date("2026-07-06 23:57:04"), "Jul 6, 2026 23:57");
    }

    #[test]
    fn format_iso_date_empty_returns_empty() {
        assert_eq!(format_iso_date(""), "");
    }

    #[test]
    fn format_iso_date_unparseable_returned_unchanged() {
        // A surprising value must degrade to the original, not an empty field.
        assert_eq!(format_iso_date("whenever"), "whenever");
    }

    #[test]
    fn format_iso_date_invalid_month_returned_unchanged() {
        assert_eq!(format_iso_date("2026-13-06"), "2026-13-06");
    }

    #[test]
    fn format_iso_date_invalid_day_returned_unchanged() {
        assert_eq!(format_iso_date("2026-07-00"), "2026-07-00");
    }

    #[test]
    fn format_iso_date_round_trips_all_months() {
        for (m, abbr) in [(1, "Jan"), (4, "Apr"), (12, "Dec")] {
            let iso = format!("2026-{m:02}-15");
            assert_eq!(format_iso_date(&iso), format!("{abbr} 15, 2026"));
        }
    }
}

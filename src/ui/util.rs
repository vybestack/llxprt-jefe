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

/// Join field values for display, or [`EMPTY_FIELD`] when empty.
///
/// Issue #155: replaces the leaky `-` placeholder with a clean em-dash.
/// Blank/whitespace-only entries are dropped (matching [`field_opt`]'s
/// normalization) so a stray empty label never renders as `a, , b`.
/// Shared by the Issue and PR detail header projections.
#[must_use]
pub fn field_list(values: &[String]) -> String {
    let joined = values
        .iter()
        .map(|v| v.trim())
        .filter(|v| !v.is_empty())
        .collect::<Vec<_>>()
        .join(", ");
    if joined.is_empty() {
        return EMPTY_FIELD.to_string();
    }
    joined
}

/// An optional field value for display, or [`EMPTY_FIELD`] when absent or
/// whitespace-only (a blank milestone must render the placeholder, not a
/// gap). Shared by the Issue and PR detail header projections.
#[must_use]
pub fn field_opt(value: Option<&str>) -> String {
    // Trim the returned value too (not just the emptiness check) so a
    // padded value like " v1.0 " renders without stray whitespace,
    // consistent with `field_list`'s per-item trimming.
    value
        .map(str::trim)
        .filter(|v| !v.is_empty())
        .map_or_else(|| EMPTY_FIELD.to_string(), str::to_string)
}

/// Format a GitHub ISO-8601 timestamp into a compact human date.
///
/// Accepts the forms `gh` returns (`2026-07-06T15:26:53Z` and the date-only
/// `2026-07-06`) and renders `Jul 6, 2026` (or `Jul 6, 2026 15:26` when a time
/// component is present). When the DATE does not parse the function falls
/// back to the trimmed input so a surprising timestamp never blanks out the
/// header (issue #155: raw ISO timestamps are the defect being fixed, but a
/// DATE parse failure must degrade to the original text, not to an empty
/// field). When the date parses but the TIME component is malformed or
/// out-of-range, the time is silently dropped and a date-only string is
/// returned (e.g. `"Jul 6, 2026"`) rather than falling back to the raw input.
///
/// This is dependency-free (no `chrono`/`time` crate) to keep comrak the only
/// new dependency introduced by the detail redesign.
#[must_use]
pub fn format_iso_date(iso: &str) -> String {
    let trimmed = iso.trim();
    if trimmed.is_empty() {
        // Blank timestamps render the standard empty-field placeholder so the
        // header reads `created: —` instead of leaving a silent gap.
        return EMPTY_FIELD.to_string();
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

/// Parse a strict `YYYY-MM-DD` date into `(year, month, day)` with a valid
/// month and a day that fits the month (including leap-year February). Each
/// component must have the expected zero-padded width (4-2-2) and no trailing
/// junk, so non-standard forms like `26-7-6` or `2026-07-06-extra` fall through
/// to the raw-string fallback rather than producing a misleading date.
fn parse_date(s: &str) -> Option<(i32, usize, u32)> {
    let s = s.trim();
    let mut parts = s.split('-');
    // Components are NOT trimmed: internal whitespace (`2026 - 07 - 06`)
    // must fail the width check below, matching the strict 4-2-2 contract.
    let year_s = parts.next()?;
    let month_s = parts.next()?;
    let day_s = parts.next()?;
    // No trailing components allowed.
    if parts.next().is_some() {
        return None;
    }
    // Enforce zero-padded widths (4-2-2) so non-standard forms are rejected.
    if year_s.len() != 4 || month_s.len() != 2 || day_s.len() != 2 {
        return None;
    }
    let year: i32 = year_s.parse().ok()?;
    let month: usize = month_s.parse().ok()?;
    let day: u32 = day_s.parse().ok()?;
    if !(1..=12).contains(&month) {
        return None;
    }
    if day == 0 || day > days_in_month(year, month) {
        return None;
    }
    Some((year, month, day))
}

/// Number of days in a month, accounting for leap-year February.
fn days_in_month(year: i32, month: usize) -> u32 {
    match month {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        4 | 6 | 9 | 11 => 30,
        2 if is_leap_year(year) => 29,
        _ => 28,
    }
}

/// Proleptic Gregorian leap-year test.
fn is_leap_year(year: i32) -> bool {
    (year % 4 == 0 && year % 100 != 0) || (year % 400 == 0)
}

/// Parse an `HH:MM` prefix out of a time component like `15:26:53Z`,
/// returning the `HH:MM` string. Seconds and the trailing `Z`/offset are
/// dropped so only the hour/minute is shown.
fn parse_hhmm(time: &str) -> Option<String> {
    // Strip a trailing UTC indicator or a timezone offset (e.g. `+02:00`,
    // `-07:00`, `+0530`) so offset timestamps parse to the same HH:MM as a
    // `Z` timestamp. Offset signs only appear after the seconds, so split at
    // the first `+`/`-` following the start of the time string.
    let mut t = time.trim();
    if let Some(pos) = t
        .char_indices()
        .skip(1)
        .find(|(_, c)| *c == '+' || *c == '-')
        .map(|(p, _)| p)
    {
        t = &t[..pos];
    }
    // Accept lowercase `z` too — ISO-8601 permits it even though GitHub
    // emits uppercase.
    t = t.trim_end_matches(['Z', 'z']);
    let mut parts = t.split(':');
    // Components are NOT trimmed: internal whitespace (`15 : 26`) must fail
    // the strict 2-2 width check below, mirroring parse_date's strictness.
    let hh_s = parts.next()?;
    let mm_s = parts.next()?;
    // An optional seconds component is allowed and dropped, but it must still
    // be a valid zero-padded 00-60 value (60 = leap second) — a malformed or
    // out-of-range seconds field means the time component is suspect, so the
    // whole time is dropped (parse_hhmm returns None), causing format_iso_date
    // to yield a date-only result. More than one extra component is malformed
    // outright.
    match (parts.next(), parts.next()) {
        (None, _) => {}
        (Some(ss_s), None) => {
            // Fractional seconds (`53.123`) are valid ISO-8601; validate the
            // whole field (all-digit, non-empty fraction) and drop the
            // fraction like the seconds themselves — `53.foo`/`53.` mean the
            // timestamp is suspect, so the time component drops. No trimming
            // here either: `: 53` fails the width check like the others.
            let seconds = ss_s;
            let ss_s = match seconds.split_once('.') {
                Some((integer, fraction))
                    if !fraction.is_empty()
                        && fraction.bytes().all(|byte| byte.is_ascii_digit()) =>
                {
                    integer
                }
                Some(_) => return None,
                None => seconds,
            };
            if ss_s.len() != 2 {
                return None;
            }
            let ss: u32 = ss_s.parse().ok()?;
            if ss > 60 {
                return None;
            }
        }
        _ => return None,
    }
    // Enforce zero-padded 2-2 width, mirroring parse_date, so non-standard
    // time forms fall through to the raw fallback.
    if hh_s.len() != 2 || mm_s.len() != 2 {
        return None;
    }
    let hh: u32 = hh_s.parse().ok()?;
    let mm: u32 = mm_s.parse().ok()?;
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
    use super::{EMPTY_FIELD, field_list, field_opt, format_iso_date, truncate_with_ellipsis};
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
    fn format_iso_date_empty_returns_placeholder() {
        // Blank timestamps render the standard empty-field placeholder so
        // headers read `created: —` instead of a silent gap (issue #155).
        assert_eq!(format_iso_date(""), EMPTY_FIELD);
        assert_eq!(format_iso_date("   "), EMPTY_FIELD);
    }

    #[test]
    fn field_list_joins_or_placeholder() {
        assert_eq!(field_list(&[]), EMPTY_FIELD);
        let values = vec!["a".to_string(), "b".to_string()];
        assert_eq!(field_list(&values), "a, b");
    }

    /// A missing OR whitespace-only optional field renders the placeholder,
    /// never a blank gap (e.g. a milestone of `"  "`).
    #[test]
    fn field_opt_placeholder_for_absent_or_blank() {
        assert_eq!(field_opt(None), EMPTY_FIELD);
        assert_eq!(field_opt(Some("")), EMPTY_FIELD);
        assert_eq!(field_opt(Some("   ")), EMPTY_FIELD);
        assert_eq!(field_opt(Some("v1.0")), "v1.0");
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

    /// An impossible day-of-month (e.g. Feb 31) must fall through to the raw
    /// string, not be rendered as a plausible-but-invalid date.
    #[test]
    fn format_iso_date_impossible_day_returned_unchanged() {
        assert_eq!(format_iso_date("2026-02-31"), "2026-02-31");
        assert_eq!(format_iso_date("2026-04-31"), "2026-04-31");
    }

    /// Leap-year February accepts the 29th; a common year does not.
    #[test]
    fn format_iso_date_leap_year_february() {
        assert_eq!(format_iso_date("2024-02-29"), "Feb 29, 2024");
        assert_eq!(format_iso_date("2025-02-29"), "2025-02-29");
    }

    #[test]
    fn format_iso_date_round_trips_all_months() {
        for (m, abbr) in [
            (1, "Jan"),
            (2, "Feb"),
            (3, "Mar"),
            (4, "Apr"),
            (5, "May"),
            (6, "Jun"),
            (7, "Jul"),
            (8, "Aug"),
            (9, "Sep"),
            (10, "Oct"),
            (11, "Nov"),
            (12, "Dec"),
        ] {
            let iso = format!("2026-{m:02}-15");
            assert_eq!(format_iso_date(&iso), format!("{abbr} 15, 2026"));
        }
    }

    /// A timezone offset directly after HH:MM (no seconds component) parses
    /// the same as the seconds-bearing form.
    #[test]
    fn format_iso_date_offset_without_seconds() {
        assert_eq!(
            format_iso_date("2026-07-06T15:26+02:00"),
            "Jul 6, 2026 15:26"
        );
        assert_eq!(
            format_iso_date("2026-07-06T15:26-07:00"),
            "Jul 6, 2026 15:26"
        );
    }

    /// A timezone offset (e.g. `+02:00`) must be stripped so the HH:MM is the
    /// same as a `Z` timestamp.
    #[test]
    fn format_iso_date_strips_timezone_offset() {
        assert_eq!(
            format_iso_date("2026-07-06T15:26:53+02:00"),
            "Jul 6, 2026 15:26"
        );
        assert_eq!(
            format_iso_date("2026-07-06T15:26:53-07:00"),
            "Jul 6, 2026 15:26"
        );
    }

    /// Trailing junk after the date (e.g. `-extra`) must fall through to the
    /// raw string rather than producing a misleading date.
    #[test]
    fn format_iso_date_rejects_trailing_components() {
        assert_eq!(format_iso_date("2026-07-06-extra"), "2026-07-06-extra");
    }

    /// Non-zero-padded components must fall through to the raw string.
    #[test]
    fn format_iso_date_rejects_non_zero_padded() {
        assert_eq!(format_iso_date("26-7-6"), "26-7-6");
    }

    /// An optional seconds component is accepted and dropped, but malformed
    /// extra components fall through to the raw string (mirrors the date-side
    /// trailing-component guard).
    #[test]
    fn format_iso_date_accepts_seconds_rejects_extra_components() {
        // Seconds present, with offset stripped.
        assert_eq!(format_iso_date("2026-07-06T15:26:53Z"), "Jul 6, 2026 15:26");
        // Too many components after HH:MM is malformed: the time is rejected,
        // leaving a date-only result.
        assert_eq!(format_iso_date("2026-07-06T15:26:99:99"), "Jul 6, 2026");
    }

    /// The dropped seconds component is still validated: out-of-range or
    /// non-zero-padded seconds mark the whole time as suspect (date-only
    /// result), matching parse_date's strictness. A leap second (60) passes.
    #[test]
    fn format_iso_date_validates_dropped_seconds() {
        assert_eq!(format_iso_date("2026-07-06T15:26:99Z"), "Jul 6, 2026");
        assert_eq!(format_iso_date("2026-07-06T15:26:5Z"), "Jul 6, 2026");
        assert_eq!(format_iso_date("2026-07-06T15:26:60Z"), "Jul 6, 2026 15:26");
    }

    /// Fractional seconds are valid ISO-8601 and must not defeat the HH:MM
    /// extraction (the integer part validates; the fraction drops). Malformed
    /// fractions (`53.foo`, `53.`) mean the timestamp is suspect, so the time
    /// component drops to a date-only render.
    #[test]
    fn format_iso_date_fractional_seconds() {
        assert_eq!(
            format_iso_date("2026-07-06T15:26:53.123Z"),
            "Jul 6, 2026 15:26"
        );
        assert_eq!(
            format_iso_date("2026-07-06T15:26:53.123+02:00"),
            "Jul 6, 2026 15:26"
        );
        assert_eq!(format_iso_date("2026-07-06T15:26:53.fooZ"), "Jul 6, 2026");
        assert_eq!(format_iso_date("2026-07-06T15:26:53.Z"), "Jul 6, 2026");
    }
}

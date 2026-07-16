//! RFC 3339 instant comparison for timestamp-bearing GitHub responses.
//!
//! The comparator is dependency-free, side-effect-free, and shared by every
//! GitHub list sorter so offset and fractional-second forms cannot drift.

use std::cmp::Ordering;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct Rfc3339Instant<'a> {
    whole_seconds: i64,
    fraction: &'a [u8],
}

impl Ord for Rfc3339Instant<'_> {
    fn cmp(&self, other: &Self) -> Ordering {
        self.whole_seconds
            .cmp(&other.whole_seconds)
            .then_with(|| self.fraction.cmp(other.fraction))
    }
}

impl PartialOrd for Rfc3339Instant<'_> {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
enum TimestampSortKey<'a> {
    Invalid(&'a str),
    Valid(Rfc3339Instant<'a>),
}

/// Compare two RFC 3339 timestamps newest-first.
///
/// Valid timestamps precede malformed values. Malformed values retain their
/// reverse-lexicographic order, which also keeps an empty timestamp last.
pub(super) fn cmp_rfc3339_newest_first(a: &str, b: &str) -> Ordering {
    timestamp_sort_key(b).cmp(&timestamp_sort_key(a))
}

fn timestamp_sort_key(value: &str) -> TimestampSortKey<'_> {
    parse_rfc3339(value).map_or(TimestampSortKey::Invalid(value), TimestampSortKey::Valid)
}

fn parse_rfc3339(value: &str) -> Option<Rfc3339Instant<'_>> {
    let bytes = value.as_bytes();
    if !fixed_separators_are_valid(bytes) {
        return None;
    }

    let year = i64::from(parse_digits(bytes, 0, 4)?);
    let month = parse_digits(bytes, 5, 2)?;
    let day = parse_digits(bytes, 8, 2)?;
    let hour = parse_digits(bytes, 11, 2)?;
    let minute = parse_digits(bytes, 14, 2)?;
    let second = parse_digits(bytes, 17, 2)?;
    if !date_time_fields_are_valid(year, month, day, hour, minute, second) {
        return None;
    }

    let (fraction, timezone_position) = parse_fraction(bytes)?;
    let offset_seconds = parse_utc_offset(bytes, timezone_position)?;
    let whole_seconds = days_from_civil(year, month, day) * 86_400
        + i64::from(hour) * 3_600
        + i64::from(minute) * 60
        + i64::from(second.min(59))
        + i64::from(second == 60)
        - offset_seconds;

    Some(Rfc3339Instant {
        whole_seconds,
        fraction,
    })
}

fn fixed_separators_are_valid(bytes: &[u8]) -> bool {
    bytes.len() >= 20
        && bytes.get(4) == Some(&b'-')
        && bytes.get(7) == Some(&b'-')
        && matches!(bytes.get(10), Some(b'T' | b't'))
        && bytes.get(13) == Some(&b':')
        && bytes.get(16) == Some(&b':')
}

fn parse_fraction(bytes: &[u8]) -> Option<(&[u8], usize)> {
    if bytes.get(19) != Some(&b'.') {
        return Some((&bytes[19..19], 19));
    }

    let start = 20;
    let mut end = start;
    while bytes.get(end).is_some_and(u8::is_ascii_digit) {
        end += 1;
    }
    if end == start {
        return None;
    }
    while end > start && bytes[end - 1] == b'0' {
        end -= 1;
    }
    let timezone_position = bytes[20..].iter().position(|byte| !byte.is_ascii_digit())? + 20;
    Some((&bytes[start..end], timezone_position))
}

fn parse_utc_offset(bytes: &[u8], position: usize) -> Option<i64> {
    match bytes.get(position) {
        Some(b'Z' | b'z') if position + 1 == bytes.len() => Some(0),
        Some(sign @ (b'+' | b'-')) if position + 6 == bytes.len() => {
            if bytes.get(position + 3) != Some(&b':') {
                return None;
            }
            let hours = parse_digits(bytes, position + 1, 2)?;
            let minutes = parse_digits(bytes, position + 4, 2)?;
            if hours > 23 || minutes > 59 {
                return None;
            }
            let magnitude = i64::from(hours * 3_600 + minutes * 60);
            Some(if *sign == b'+' { magnitude } else { -magnitude })
        }
        _ => None,
    }
}

fn parse_digits(bytes: &[u8], start: usize, length: usize) -> Option<u32> {
    let mut value = 0_u32;
    for byte in bytes.get(start..start + length)? {
        if !byte.is_ascii_digit() {
            return None;
        }
        value = value * 10 + u32::from(byte - b'0');
    }
    Some(value)
}

fn date_time_fields_are_valid(
    year: i64,
    month: u32,
    day: u32,
    hour: u32,
    minute: u32,
    second: u32,
) -> bool {
    (1..=12).contains(&month)
        && (1..=days_in_month(year, month)).contains(&day)
        && hour <= 23
        && minute <= 59
        && second <= 60
}

fn days_in_month(year: i64, month: u32) -> u32 {
    match month {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        4 | 6 | 9 | 11 => 30,
        2 if is_leap_year(year) => 29,
        _ => 28,
    }
}

fn is_leap_year(year: i64) -> bool {
    (year % 4 == 0 && year % 100 != 0) || year % 400 == 0
}

fn days_from_civil(year: i64, month: u32, day: u32) -> i64 {
    let adjusted_year = year - i64::from(month <= 2);
    let era = adjusted_year.div_euclid(400);
    let year_of_era = adjusted_year - era * 400;
    let shifted_month = i64::from(month) + if month > 2 { -3 } else { 9 };
    let day_of_year = (153 * shifted_month + 2) / 5 + i64::from(day) - 1;
    let day_of_era = year_of_era * 365 + year_of_era / 4 - year_of_era / 100 + day_of_year;
    era * 146_097 + day_of_era
}

#[cfg(test)]
mod tests {
    use super::{cmp_rfc3339_newest_first, parse_rfc3339};
    use std::cmp::Ordering;

    #[test]
    fn equivalent_offsets_and_fraction_precision_compare_equal() {
        assert_eq!(
            parse_rfc3339("2026-07-02T10:00:00.1000Z"),
            parse_rfc3339("2026-07-02t11:00:00.1+01:00")
        );
    }

    #[test]
    fn arbitrary_fraction_precision_compares_chronologically() {
        assert!(
            parse_rfc3339("2026-07-02T10:00:00.00000000001Z")
                > parse_rfc3339("2026-07-02T10:00:00.000000000001Z")
        );
    }

    #[test]
    fn leap_second_matches_the_next_whole_second() {
        assert_eq!(
            parse_rfc3339("2016-12-31T23:59:60Z"),
            parse_rfc3339("2017-01-01T00:00:00Z")
        );
    }

    #[test]
    fn invalid_fields_are_rejected() {
        for value in [
            "2025-02-29T00:00:00Z",
            "2026-01-01T24:00:00Z",
            "2026-01-01T00:00:00+24:00",
            "2026-01-01T00:00:00. Z",
        ] {
            assert_eq!(parse_rfc3339(value), None, "accepted {value}");
        }
    }

    #[test]
    fn valid_values_precede_malformed_and_empty_values() {
        let mut values = vec!["", "bad", "2026-01-01T00:00:00Z"];
        values.sort_by(|a, b| cmp_rfc3339_newest_first(a, b));
        assert_eq!(values, vec!["2026-01-01T00:00:00Z", "bad", ""]);
        assert_eq!(cmp_rfc3339_newest_first("bad", "bad"), Ordering::Equal);
    }
}

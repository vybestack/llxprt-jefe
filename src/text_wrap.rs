//! Pure, iocraft-free word-wrap projection.
//!
//! This is the single shared wrapping primitive for the app. Row capacity is
//! measured in terminal display cells while source ranges remain Unicode-scalar
//! offsets, so renderers can share one physical-grid contract without changing
//! the editor or selection coordinate models.
//!
//! Semantics:
//! - Rows break at whitespace when the next word would exceed `width` cells.
//! - An overlong word breaks at the largest scalar boundary that fits.
//! - Zero-cell combining marks remain attached to a fitting base scalar.
//! - A glyph wider than an empty nonzero row renders as a one-cell ellipsis and
//!   retains its source range.
//! - Explicit newlines always start a row; `width == 0` returns one empty row.
//!
//! @requirement REQ-TEXT-WRAP

use unicode_width::UnicodeWidthChar;

const OVERWIDE_GLYPH_PLACEHOLDER: &str = "…";

/// One wrapped row: the display text plus the half-open `[start, end)`
/// char-column range it covers within the source text.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WrapRow {
    /// The wrapped text for this row (no trailing newline).
    pub text: String,
    /// Inclusive start char column within the source text.
    pub start: usize,
    /// Exclusive end char column within the source text.
    pub end: usize,
}

/// Wrap `text` into rows of at most `width` terminal display cells while
/// retaining Unicode-scalar source ranges.
///
/// `width == 0` yields a single empty row. The result is never empty: even
/// empty input produces one row.
#[must_use]
pub fn wrap_text(text: &str, width: usize) -> Vec<WrapRow> {
    if width == 0 {
        return vec![WrapRow {
            text: String::new(),
            start: 0,
            end: 0,
        }];
    }
    let mut rows = Vec::new();
    let mut base = 0usize; // cumulative global char offset, incl. newlines
    for line in split_lines(text) {
        wrap_single_line(line, width, base, &mut rows);
        // +1 accounts for the newline delimiter that split() removed.
        base += line.chars().count() + 1;
    }
    if rows.is_empty() {
        rows.push(WrapRow {
            text: String::new(),
            start: 0,
            end: 0,
        });
    }
    rows
}

/// Split `text` on `'\n'`, preserving a trailing empty line after a final
/// newline (mirrors the composer line semantics). Empty input yields one
/// empty slice so the caller always produces at least one row.
fn split_lines(text: &str) -> Vec<&str> {
    if text.is_empty() {
        return vec![""];
    }
    text.split('\n').collect()
}

#[derive(Clone, Copy)]
struct RowEnd {
    display_end: usize,
    source_end: usize,
    overwide: bool,
}

impl RowEnd {
    fn normal(display_end: usize, source_end: usize) -> Self {
        Self {
            display_end,
            source_end,
            overwide: false,
        }
    }

    fn overwide(source_end: usize) -> Self {
        Self {
            display_end: 0,
            source_end,
            overwide: true,
        }
    }
}

fn wrap_single_line(line: &str, width: usize, base: usize, rows: &mut Vec<WrapRow>) {
    if line.is_empty() {
        rows.push(WrapRow {
            text: String::new(),
            start: base,
            end: base,
        });
        return;
    }
    let chars = line.chars().collect::<Vec<_>>();
    let mut start = 0;
    while start < chars.len() {
        let end = display_row_end(&chars, start, width);
        rows.push(WrapRow {
            text: row_text(&chars, start, end),
            start: base + start,
            end: base + end.source_end,
        });
        start = end.source_end;
    }
}

fn display_row_end(chars: &[char], start: usize, width: usize) -> RowEnd {
    let mut used: usize = 0;
    let mut cursor = start;
    let mut last_whitespace_end = None;
    while cursor < chars.len() {
        let char_width = UnicodeWidthChar::width(chars[cursor]).unwrap_or(0);
        if used.saturating_add(char_width) > width {
            return overflow_row_end(chars, start, cursor, last_whitespace_end);
        }
        used = used.saturating_add(char_width);
        cursor += 1;
        if chars[cursor - 1].is_whitespace() {
            last_whitespace_end = Some(cursor);
        }
    }
    RowEnd::normal(cursor, cursor)
}

fn overflow_row_end(
    chars: &[char],
    start: usize,
    cursor: usize,
    last_whitespace_end: Option<usize>,
) -> RowEnd {
    if chars[cursor].is_whitespace() {
        let source_end = chars[cursor..]
            .iter()
            .take_while(|character| character.is_whitespace())
            .count()
            + cursor;
        return RowEnd::normal(cursor, source_end);
    }
    if let Some(break_end) = last_whitespace_end.filter(|end| *end > start) {
        return RowEnd::normal(break_end, break_end);
    }
    if cursor > start {
        return RowEnd::normal(cursor, cursor);
    }
    let source_end = chars[cursor + 1..]
        .iter()
        .take_while(|character| UnicodeWidthChar::width(**character).unwrap_or(0) == 0)
        .count()
        + cursor
        + 1;
    RowEnd::overwide(source_end)
}

fn row_text(chars: &[char], start: usize, end: RowEnd) -> String {
    if end.overwide {
        let mut text = OVERWIDE_GLYPH_PLACEHOLDER.to_string();
        text.extend(chars[start + 1..end.source_end].iter());
        return text;
    }
    let mut text = chars[start..end.display_end].iter().collect::<String>();
    while text.ends_with(' ') {
        text.pop();
    }
    text
}

/// Find the row index that contains the given source char column, and the
/// column relative to that row's start. Columns outside the rows clamp to the
/// last row's source end.
#[must_use]
pub fn row_for_column(rows: &[WrapRow], col: usize) -> Option<(usize, usize)> {
    let mut last_idx = 0usize;
    let mut last_rel = 0usize;
    for (idx, row) in rows.iter().enumerate() {
        if col < row.end {
            let rel = col.saturating_sub(row.start);
            return Some((idx, rel));
        }
        last_idx = idx;
        last_rel = row.end.saturating_sub(row.start);
    }
    Some((last_idx, last_rel))
}

#[cfg(test)]
mod tests {
    use super::*;
    use unicode_width::UnicodeWidthStr;

    #[test]
    fn empty_text_one_empty_row() {
        let rows = wrap_text("", 10);
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].text, "");
        assert_eq!((rows[0].start, rows[0].end), (0, 0));
    }

    #[test]
    fn short_text_fits_one_row() {
        let rows = wrap_text("hello world", 50);
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].text, "hello world");
        assert_eq!((rows[0].start, rows[0].end), (0, 11));
    }

    #[test]
    fn wraps_at_word_boundary() {
        // width 11: "the quick" (9) + " brown" would be 15 -> wrap before brown.
        let rows = wrap_text("the quick brown fox", 11);
        let texts: Vec<&str> = rows.iter().map(|r| r.text.as_str()).collect();
        assert_eq!(texts, vec!["the quick", "brown fox"]);
        // No word is split.
        for t in &texts {
            assert!(!t.starts_with(' '), "row must not start with space: {t:?}");
        }
    }

    #[test]
    fn long_word_breaks_at_width() {
        // "abcdefghij" repeated = 20 chars, no spaces (one long word). At
        // width 8 it hard-breaks into 8 + 8 + 4 = "abcdefgh" | "ijabcdef" |
        // "ghij" (chars[0..8], chars[8..16], chars[16..20]).
        let rows = wrap_text("abcdefghijabcdefghij", 8);
        let texts: Vec<&str> = rows.iter().map(|r| r.text.as_str()).collect();
        assert_eq!(texts, vec!["abcdefgh", "ijabcdef", "ghij"]);
        // No row exceeds the width.
        for r in &rows {
            assert!(r.text.chars().count() <= 8);
        }
    }

    #[test]
    fn char_column_ranges_are_contiguous() {
        let src = "alpha bravo charlie delta echo foxtrot";
        let rows = wrap_text(src, 12);
        assert_eq!(rows[0].start, 0);
        // Row starts are strictly increasing and each row's end >= its start.
        for w in rows.windows(2) {
            assert!(w[1].start > w[0].start, "row starts must increase");
            assert!(w[0].end >= w[0].start, "end >= start");
        }
        // Within a single logical line, ranges are STRICTLY contiguous: each
        // row's end equals the next row's start (no gaps, no overlaps).
        for w in rows.windows(2) {
            assert_eq!(
                w[0].end, w[1].start,
                "rows must be strictly contiguous within a line"
            );
        }
        // Every non-whitespace source char is covered by exactly one row range.
        for (global, ch) in src.chars().enumerate() {
            if ch.is_whitespace() {
                continue;
            }
            let count = rows
                .iter()
                .filter(|r| r.start <= global && global < r.end)
                .count();
            assert_eq!(count, 1, "source col {global} ('{ch}') covered {count}x");
        }
        assert!(
            rows.last().is_some_and(|r| r.end > 0),
            "the final row must cover the tail"
        );
    }

    /// A source column at a newline position (in the gap between two logical
    /// lines) must NOT cause an underflow panic in `row_for_column`; it clamps
    /// safely to the next row.
    #[test]
    fn row_for_column_newline_gap_no_underflow() {
        // "alpha<NL>beta": newline is at global col 5 (between [0,5) and [6,10)).
        let rows = wrap_text(concat!("alpha", "\n", "beta"), 40);
        assert_eq!(rows[0].start, 0);
        assert_eq!(rows[0].end, 5);
        assert_eq!(rows[1].start, 6);
        // The newline column 5 is in the gap; must not panic.
        let Some((idx, rel)) = row_for_column(&rows, 5) else {
            panic!("gap col must resolve");
        };
        assert!(idx == 1, "gap col resolves to or past the next row");
        let _ = rel; // rel is a valid (saturated) offset
    }

    /// Source ranges stay STRICTLY contiguous even when inter-word spaces are
    /// dropped at a wrap boundary (a word that fills the width leaves no room
    /// for the following spaces). Every source char column must map to a row,
    /// and consecutive rows must satisfy `rows[i].end == rows[i+1].start`.
    #[test]
    fn dropped_wrap_spaces_keep_ranges_contiguous() {
        // width 4: "abcd ef" -> row0 covers "abcd" + the dropped space at col 4
        // (source [0,5)); row1 covers "ef" (source [5,7)). 'e' (col 5) must map
        // to row1 at rel 0, and the dropped-space col 4 must map to row0.
        let rows = wrap_text("abcd ef", 4);
        assert_eq!(rows.len(), 2);
        assert_eq!(rows[0].text, "abcd");
        assert_eq!(rows[1].text, "ef");
        // Strict contiguity across the wrap.
        assert_eq!(rows[0].end, rows[1].start);
        // The dropped space column (4) maps to row 0; 'e' (col 5) maps to row 1.
        assert_eq!(row_for_column(&rows, 4).map(|(i, _)| i), Some(0));
        assert_eq!(row_for_column(&rows, 5), Some((1, 0)));
    }

    #[test]
    fn no_row_exceeds_width() {
        let text = "supercalifragilisticexpialidocious and some normal words here";
        let width = 10;
        for r in wrap_text(text, width) {
            assert!(
                r.text.chars().count() <= width,
                "row exceeds width {width}: {:?} ({})",
                r.text,
                r.text.chars().count()
            );
        }
    }

    #[test]
    fn explicit_newline_starts_new_row() {
        let rows = wrap_text("line one\nline two continues", 40);
        let texts: Vec<&str> = rows.iter().map(|r| r.text.as_str()).collect();
        assert_eq!(texts, vec!["line one", "line two continues"]);
    }

    /// WrapRow ranges are GLOBAL char offsets across the whole source text
    /// (including the newline that separates logical lines), so a consumer can
    /// map any source char position to a single row.
    #[test]
    fn row_ranges_are_global_across_newlines() {
        // "alpha\nbeta": 'a' of alpha is global col 0; 'b' of beta is global
        // col 6 (after "alpha\n" = 5 letters + 1 newline).
        let rows = wrap_text("alpha\nbeta", 40);
        assert_eq!(rows[0].start, 0);
        assert_eq!(rows[0].end, 5);
        assert_eq!(rows[1].start, 6);
        assert_eq!(rows[1].end, 10);
        // row_for_column must map a global col in the second line correctly.
        assert_eq!(row_for_column(&rows, 7), Some((1, 1)));
    }

    /// Across BOTH newlines and word-wrap boundaries, every source char
    /// column maps to exactly one row (ranges are contiguous and global).
    #[test]
    fn row_ranges_contiguous_across_wrap_and_newlines() {
        // "aaaa bbbb" wraps at width 5, then a newline, then "cccc".
        let rows = wrap_text(
            "aaaa bbbb
cccc",
            5,
        );
        // Ranges must be non-decreasing and each row's end >= its start.
        for w in rows.windows(2) {
            assert!(
                w[0].start <= w[1].start,
                "row starts must be non-decreasing"
            );
        }
        // Every non-whitespace source char is inside some row's range.
        let src = "aaaa bbbb
cccc";
        for (global, ch) in src.chars().enumerate() {
            if ch.is_whitespace() {
                continue;
            }
            assert!(
                rows.iter().any(|r| r.start <= global && global < r.end),
                "source col {global} ('{ch}') is not in any row range: {rows:?}"
            );
        }
    }

    #[test]
    fn trailing_newline_yields_empty_row() {
        let rows = wrap_text("abc\n", 40);
        let texts: Vec<&str> = rows.iter().map(|r| r.text.as_str()).collect();
        assert_eq!(texts, vec!["abc", ""]);
    }

    #[test]
    fn multibyte_not_split() {
        // "héllo wörld" — multibyte chars must not be split.
        let rows = wrap_text("héllo wörld", 4);
        for r in &rows {
            assert!(!r.text.is_empty(), "no empty rows mid-word: {:?}", r.text);
            assert!(
                r.text.chars().count() <= 4,
                "row exceeds width: {:?}",
                r.text
            );
        }
        // The text reconstructs (ignoring where spaces were dropped at wrap).
        let joined: String = rows
            .iter()
            .flat_map(|r| r.text.chars())
            .filter(|c| !c.is_whitespace())
            .collect();
        assert_eq!(joined, "héllowörld");
    }

    #[test]
    fn cjk_wraps_by_terminal_cells_and_preserves_source_columns() {
        let rows = wrap_text("甲乙丙", 4);

        assert_eq!(
            rows,
            vec![
                WrapRow {
                    text: "甲乙".to_string(),
                    start: 0,
                    end: 2,
                },
                WrapRow {
                    text: "丙".to_string(),
                    start: 2,
                    end: 3,
                },
            ]
        );
        assert_eq!(row_for_column(&rows, 2), Some((1, 0)));
    }

    #[test]
    fn combining_marks_share_their_base_row_without_consuming_cells() {
        let rows = wrap_text("e\u{301}x", 1);

        assert_eq!(rows[0].text, "e\u{301}");
        assert_eq!((rows[0].start, rows[0].end), (0, 2));
        assert_eq!(rows[1].text, "x");
        assert_eq!((rows[1].start, rows[1].end), (2, 3));
        assert!(
            rows.iter()
                .all(|row| UnicodeWidthStr::width(row.text.as_str()) <= 1)
        );
    }

    #[test]
    fn overwide_glyph_uses_bounded_placeholder_and_retains_source_range() {
        let rows = wrap_text("甲", 1);

        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].text, "…");
        assert_eq!((rows[0].start, rows[0].end), (0, 1));
        assert_eq!(UnicodeWidthStr::width(rows[0].text.as_str()), 1);
    }

    #[test]
    fn zero_width_one_empty_row() {
        let rows = wrap_text("anything", 0);
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].text, "");
    }

    #[test]
    fn row_for_column_finds_correct_row() {
        // "the quick" + space (wrap point) | "brown fox" at width 11. Row 0
        // covers source cols [0,10) (the trailing space at col 9 is the wrap
        // point but still belongs to row 0 so ranges stay contiguous); row 1
        // starts at col 10.
        let rows = wrap_text("the quick brown fox", 11);
        // col 5 ('u') -> row 0, rel 5.
        assert_eq!(row_for_column(&rows, 5), Some((0, 5)));
        // col 10 ('b' of brown) -> row 1, rel 0.
        assert_eq!(row_for_column(&rows, 10), Some((1, 0)));
    }

    #[test]
    fn row_for_column_at_end_of_last_row() {
        let rows = wrap_text("hello", 10);
        // col 5 is past the last char -> clamp to row 0, rel 5.
        assert_eq!(row_for_column(&rows, 5), Some((0, 5)));
        // col 100 clamps to the last row.
        assert_eq!(row_for_column(&rows, 100), Some((0, 5)));
    }
}

//! Pure, iocraft-free word-wrap projection.
//!
//! Splits text into wrapped rows of at most `width` display columns, breaking
//! at word boundaries (whitespace) so words are never split mid-character.
//! Each row records the half-open `[start, end)` char-column range it covers
//! within the source text, so consumers (the editor caret, the displayer
//! selection) can map a source position onto the wrapped row that contains it.
//!
//! This is the single shared wrapping primitive for the app: the editor
//! (`TextBox`) and the read-only displayer (`ScrollableText`) both consume it
//! so wrapping behavior cannot drift between them.
//!
//! Semantics:
//! - Words are runs of non-whitespace characters. A break happens at a
//!   whitespace boundary when the next word would overflow `width`.
//! - A single word longer than `width` is broken at `width` columns (it cannot
//!   fit any other way); the remainder continues on the next row.
//! - A run of spaces between words is preserved up to the wrap point.
//! - Explicit newlines (`'\n'`) always start a new row.
//! - `width == 0` returns one empty row (callers suppress the caret).
//!
//! @requirement REQ-TEXT-WRAP

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

/// Wrap `text` into rows of at most `width` display columns, breaking at word
/// boundaries. See the module docs for the full semantics.
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
    for line in split_lines(text) {
        wrap_single_line(line, width, &mut rows);
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
    let lines: Vec<&str> = text.split('\n').collect();
    // `split('\n')` already emits a trailing "" for a final newline, so no
    // extra push is needed.
    lines
}

/// Wrap a single (newline-free) line into rows, appending to `rows`.
fn wrap_single_line(line: &str, width: usize, rows: &mut Vec<WrapRow>) {
    let chars: Vec<char> = line.chars().collect();
    let mut row_start = 0usize;
    let mut col = 0usize; // char columns consumed on the current row
    let mut row_chars: String = String::with_capacity(width);

    let mut i = 0usize;
    while i < chars.len() {
        let word_start = i;
        // A "word" is a run of non-whitespace.
        let mut j = i;
        while j < chars.len() && !chars[j].is_whitespace() {
            j += 1;
        }
        let word_end = j;
        let word_len = word_end - word_start;
        // The trailing whitespace belongs with this word up to the wrap point.
        let ws_start = j;
        while j < chars.len() && chars[j].is_whitespace() {
            j += 1;
        }
        let ws_end = j;
        let ws_len = ws_end - ws_start;

        if word_len >= width {
            // Over-long word: it must be broken across rows. First flush the
            // current row if it has content so the word starts fresh.
            if col > 0 {
                flush_row(rows, &mut row_chars, &mut row_start, &mut col);
            }
            let mut k = word_start;
            while k < word_end {
                let take = width.min(word_end - k);
                push_chars(&mut row_chars, &chars[k..k + take]);
                col = take;
                k += take;
                if k < word_end {
                    flush_row(rows, &mut row_chars, &mut row_start, &mut col);
                }
            }
            // Attach trailing spaces that fit on the final word-row.
            let space_budget = width.saturating_sub(col).min(ws_len);
            push_chars(&mut row_chars, &chars[ws_start..ws_start + space_budget]);
            col += space_budget;
            i = ws_start + space_budget;
        } else if col + word_len <= width {
            // Word fits on the current row; place it with as many trailing
            // spaces as fit.
            push_chars(&mut row_chars, &chars[word_start..word_end]);
            col += word_len;
            let space_budget = width.saturating_sub(col).min(ws_len);
            push_chars(&mut row_chars, &chars[ws_start..ws_start + space_budget]);
            col += space_budget;
            i = ws_end;
        } else {
            // Word fits within `width` but not on the current row: flush, then
            // start a new row with this word (dropping leading spaces).
            flush_row(rows, &mut row_chars, &mut row_start, &mut col);
            push_chars(&mut row_chars, &chars[word_start..word_end]);
            col = word_len;
            let space_budget = width.saturating_sub(col).min(ws_len);
            push_chars(&mut row_chars, &chars[ws_start..ws_start + space_budget]);
            col += space_budget;
            i = ws_end;
        }
    }

    // Flush the final partial row (always, so the line produces at least one
    // row even when empty).
    rows.push(WrapRow {
        text: row_chars,
        start: row_start,
        end: row_start + col,
    });
}

/// Push the final partial row accumulated in `row_chars`, resetting the
/// accumulator and updating `row_start`/`col`. Trailing spaces are trimmed
/// from the *displayed* row text, but the source `[start, end)` range still
/// spans them so ranges stay contiguous and every source column maps to
/// exactly one row.
fn flush_row(
    rows: &mut Vec<WrapRow>,
    row_chars: &mut String,
    row_start: &mut usize,
    col: &mut usize,
) {
    let end = *row_start + *col;
    // Trim trailing spaces from the rendered text only.
    let trailing = row_chars.chars().rev().take_while(|c| *c == ' ').count();
    if trailing > 0 {
        let keep_cols = row_chars.chars().count() - trailing;
        row_chars.truncate(byte_len_of_chars(row_chars, keep_cols));
    }
    rows.push(WrapRow {
        text: std::mem::take(row_chars),
        start: *row_start,
        end,
    });
    *row_start = end;
    *col = 0;
}

/// Byte length of the first `n` chars of `s`.
fn byte_len_of_chars(s: &str, n: usize) -> usize {
    s.char_indices().nth(n).map_or(s.len(), |(i, _)| i)
}

/// Append a slice of chars to a string.
fn push_chars(buf: &mut String, chars: &[char]) {
    for ch in chars {
        buf.push(*ch);
    }
}

/// Find the row index that contains the given source char column, and the
/// column relative to that row's start. Returns `(row_index, relative_col)`.
///
/// A column exactly at a row's `end` belongs to the next row (so a caret at a
/// wrap boundary lands on the continuation), except for the final row where it
/// clamps to the end. A column past the last row clamps to the last row.
#[must_use]
pub fn row_for_column(rows: &[WrapRow], col: usize) -> Option<(usize, usize)> {
    for (idx, row) in rows.iter().enumerate() {
        if col < row.end {
            return Some((idx, col - row.start));
        }
    }
    // col is at or past the final row's end.
    rows.last().map(|row| {
        let last_idx = rows.len() - 1;
        let rel = row.end.saturating_sub(row.start);
        (last_idx, rel)
    })
}

#[cfg(test)]
mod tests {
    use super::*;

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
        let rows = wrap_text("alpha bravo charlie delta echo foxtrot", 12);
        assert_eq!(rows[0].start, 0);
        for w in rows.windows(2) {
            assert_eq!(w[0].end, w[1].start, "rows must be contiguous");
        }
        assert!(
            rows.last().is_some_and(|r| r.end > 0),
            "the final row must cover the tail"
        );
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

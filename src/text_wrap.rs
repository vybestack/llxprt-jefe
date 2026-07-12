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
    let lines: Vec<&str> = text.split('\n').collect();
    // `split('\n')` already emits a trailing "" for a final newline, so no
    // extra push is needed.
    lines
}

/// Wrap a single (newline-free) line into rows, appending to `rows`. `base`
/// is the global source char offset where this line begins (so the emitted
/// `WrapRow.start`/`end` are global across the whole source text, not just
/// this line).
fn wrap_single_line(line: &str, width: usize, base: usize, rows: &mut Vec<WrapRow>) {
    let chars: Vec<char> = line.chars().collect();
    // `row_src_start` is the GLOBAL source offset of the first char in the
    // current (in-progress) row. It stays synced to `base + (local i)` at the
    // moment a row begins, so dropped wrap-spaces never desync the ranges.
    let mut row_src_start = base;
    let mut col = 0usize; // char columns consumed on the current row (local)
    let mut row_chars: String = String::with_capacity(width);

    let mut i = 0usize;
    while i < chars.len() {
        let (word_start, word_end, ws_start, ws_end) = scan_word(&chars, i);
        let word_len = word_end - word_start;
        let ws_len = ws_end - ws_start;

        if word_len >= width {
            // Over-long word: flush any in-progress row first so it starts fresh.
            if col > 0 {
                flush_row_at(rows, &mut row_chars, &mut row_src_start, col);
            }
            let ctx = WordPlaceCtx {
                chars: &chars,
                rows,
                row_chars: &mut row_chars,
                row_src_start: &mut row_src_start,
            };
            col = place_overlong_word(ctx, word_start, word_end, ws_start, ws_len, width);
            // Advance past the word plus the spaces that fit on the final chunk.
            let placed = if word_len % width == 0 { width } else { word_len % width };
            i = ws_start + width.saturating_sub(placed).min(ws_len);
        } else if col + word_len <= width {
            // Word fits on the current row; place it with trailing spaces.
            place_word_on_row(
                &chars[word_start..word_end],
                &chars[ws_start..ws_end],
                &mut row_chars,
                &mut col,
                width,
            );
            i = ws_end;
        } else {
            // Word fits within `width` but not on the current row: flush, then
            // start a new row at the word (dropping the leading spaces).
            flush_row_at(rows, &mut row_chars, &mut row_src_start, col);
            row_src_start = base + word_start;
            col = 0;
            place_word_on_row(
                &chars[word_start..word_end],
                &chars[ws_start..ws_end],
                &mut row_chars,
                &mut col,
                width,
            );
            i = ws_end;
        }
    }

    // Flush the final partial row, trimming trailing spaces from the text.
    flush_row_at(rows, &mut row_chars, &mut row_src_start, col);
    trim_final_row(rows);
}

/// Scan one word + its trailing whitespace run starting at `i` in `chars`.
/// Returns `(word_start, word_end, ws_start, ws_end)`.
fn scan_word(chars: &[char], i: usize) -> (usize, usize, usize, usize) {
    let word_start = i;
    let mut j = i;
    while j < chars.len() && !chars[j].is_whitespace() {
        j += 1;
    }
    let word_end = j;
    let ws_start = j;
    while j < chars.len() && chars[j].is_whitespace() {
        j += 1;
    }
    (word_start, word_end, ws_start, j)
}

/// Place a word (known to fit within `width`) onto the current row. Appends
/// the word plus as many trailing spaces as fit, updating `col`.
fn place_word_on_row(
    word: &[char],
    ws: &[char],
    row_chars: &mut String,
    col: &mut usize,
    width: usize,
) {
    push_chars(row_chars, word);
    *col += word.len();
    let space_budget = width.saturating_sub(*col).min(ws.len());
    push_chars(row_chars, &ws[..space_budget]);
    *col += space_budget;
}

/// Bundle the mutable references needed by [`place_overlong_word`] so its
/// signature stays under the argument-count limit.
struct WordPlaceCtx<'a, 'b, 'c> {
    chars: &'a [char],
    rows: &'b mut Vec<WrapRow>,
    row_chars: &'c mut String,
    row_src_start: &'c mut usize,
}

/// Place a word longer than `width` by emitting width-sized chunks (flushing
/// between them), then attaching trailing spaces that fit on the final chunk.
/// Returns the local `col` after the final chunk.
fn place_overlong_word(
    ctx: WordPlaceCtx<'_, '_, '_>,
    word_start: usize,
    word_end: usize,
    ws_start: usize,
    ws_len: usize,
    width: usize,
) -> usize {
    let WordPlaceCtx {
        chars,
        rows,
        row_chars,
        row_src_start,
    } = ctx;
    let mut k = word_start;
    let mut col = 0usize;
    while k < word_end {
        let take = width.min(word_end - k);
        push_chars(row_chars, &chars[k..k + take]);
        col = take;
        k += take;
        if k < word_end {
            flush_row_at(rows, row_chars, row_src_start, col);
        }
    }
    let space_budget = width.saturating_sub(col).min(ws_len);
    push_chars(row_chars, &chars[ws_start..ws_start + space_budget]);
    col += space_budget;
    col
}

/// Trim trailing spaces from the last emitted row's DISPLAYED text only,
/// leaving `end` (the source extent) intact so a position in the trailing
/// spaces still maps to this row.
fn trim_final_row(rows: &mut [WrapRow]) {
    let Some(last) = rows.last_mut() else {
        return;
    };
    let trailing = last.text.chars().rev().take_while(|c| *c == ' ').count();
    if trailing > 0 {
        let keep = last.text.chars().count() - trailing;
        last.text.truncate(byte_len_of_chars(&last.text, keep));
    }
}

/// Flush the in-progress row: push it with a GLOBAL `start` of
/// `row_src_start`, trim trailing spaces from the DISPLAYED text, but set
/// `end` to `row_src_start + consumed` (the full SOURCE extent, including
/// any trimmed/dropped trailing spaces) so source ranges stay contiguous and
/// a position in trailing spaces still maps to this row.
fn flush_row_at(
    rows: &mut Vec<WrapRow>,
    row_chars: &mut String,
    row_src_start: &mut usize,
    consumed: usize,
) {
    if consumed == 0 && row_chars.is_empty() {
        // An empty in-progress row (e.g. blank line): emit one empty row.
        rows.push(WrapRow {
            text: String::new(),
            start: *row_src_start,
            end: *row_src_start,
        });
        return;
    }
    let trailing = row_chars.chars().rev().take_while(|c| *c == ' ').count();
    let trimmed_text = if trailing > 0 {
        let keep = row_chars.chars().count() - trailing;
        let byte_len = byte_len_of_chars(row_chars, keep);
        row_chars[..byte_len].to_string()
    } else {
        row_chars.clone()
    };
    // `end` is the SOURCE extent (incl. trimmed spaces); the displayed text
    // is shorter. Consumers map caret/click columns via [start, end].
    rows.push(WrapRow {
        text: trimmed_text,
        start: *row_src_start,
        end: *row_src_start + consumed,
    });
    *row_src_start += consumed;
    row_chars.clear();
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
        let src = "alpha bravo charlie delta echo foxtrot";
        let rows = wrap_text(src, 12);
        assert_eq!(rows[0].start, 0);
        // Row starts are strictly increasing and each row's end >= its start.
        for w in rows.windows(2) {
            assert!(w[1].start > w[0].start, "row starts must increase");
            assert!(w[0].end >= w[0].start, "end >= start");
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

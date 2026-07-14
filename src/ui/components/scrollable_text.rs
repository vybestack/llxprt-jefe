//! Scrollable text viewport with scrollbar.
//!
//! Renders a FIXED number of display rows (`viewport_rows`) regardless of
//! content length. Content lines that exceed the pane width WORD-WRAP onto
//! multiple display rows (via the shared [`super::doc_wrap`] projection built
//! on [`crate::text_wrap`]) instead of being truncated, so long bodies and
//! comments fold onto the next visible row.
//!
//! Content is windowed by `scroll_offset`, which stays in CONTENT-LINE units
//! (the space the selection model and scroll bounds use); this component
//! converts it to a display-row window start via [`super::doc_wrap::line_first_row`].
//! Empty rows are padded so the component always occupies the same layout
//! space — preventing layout shifts.
//!
//! When a [`TextSelection`] is supplied (and its pane matches), the selected
//! cells are painted in inverse video so the user sees live drag-selection
//! feedback. The selection lives in *content* coordinates (line + char column);
//! each wrapped display row clips the selection range to its own
//! `[line_char_start, line_char_end)` char range within its content line.

use iocraft::prelude::*;

use crate::selection::{HighlightRange, TextSelection, row_highlight_range};
use crate::theme::SelectionColors;

use super::doc_wrap::{DocDisplayRow, caret_row_for_line_col, line_first_row, wrap_document};

/// Background color used for the selection highlight when no explicit color is
/// supplied. Swapped against the foreground for an inverse-video effect.
const DEFAULT_SEL_BG: Color = Color::White;
/// Foreground color used for the selection highlight when no explicit color is
/// supplied.
const DEFAULT_SEL_FG: Color = Color::Black;

/// Props for the scrollable text viewport.
#[derive(Default, Props)]
pub struct ScrollableTextProps {
    /// The full text content to display (may contain newlines).
    pub content: String,
    /// Current scroll offset in lines (0 = top).
    pub scroll_offset: usize,
    /// Fixed number of rows this viewport occupies. Must be set by the parent.
    /// The component always renders exactly this many `Box(height: 1u32)` elements.
    pub viewport_rows: usize,
    /// Max display width in characters. Lines exceeding this WORD-WRAP onto
    /// additional display rows at word boundaries (a single over-long word
    /// hard-breaks at the width). When 0, the width falls back to terminal
    /// width minus a safety margin.
    pub max_line_width: usize,
    /// Cursor line index (0-based, relative to content lines). When set, shows a
    /// reverse-video cursor at `(cursor_line, cursor_col)`.
    pub cursor_line: Option<usize>,
    /// Cursor column index (0-based, char offset within the display line).
    pub cursor_col: Option<usize>,
    /// Text color.
    pub color: Option<Color>,
    /// Text color for the character under the cursor.
    pub cursor_color: Option<Color>,
    /// Background color for the character under the cursor.
    pub cursor_bg: Option<Color>,
    /// Scrollbar track color (dimmed).
    pub track_color: Option<Color>,
    /// Scrollbar thumb color (bright).
    pub thumb_color: Option<Color>,
    /// Active text selection, if any. The selection's content coordinates are
    /// mapped to display rows via `scroll_offset`; selected cells are painted
    /// in inverse video.
    pub selection: Option<TextSelection>,
    /// Selection highlight background color.
    pub selection_bg: Option<Color>,
    /// Selection highlight foreground color.
    pub selection_fg: Option<Color>,
    /// Background color painted on plain (non-highlighted, non-cursor) rows.
    /// When set, avoids `Color::Reset` (terminal default) haze on the themed
    /// background. Detail panes pass `Some(rc.bg)`.
    pub bg: Option<Color>,
    /// Number of content lines preceding the scrollable region (e.g. detail
    /// header rows rendered above the viewport). The highlight line-mapping
    /// adds this offset so a content line that lives in the header is detected
    /// correctly. Defaults to 0 (no preceding header).
    pub content_line_offset: usize,
}

/// Compute scrollbar thumb position and size using integer math.
fn scrollbar_geometry(total: usize, visible: usize, offset: usize) -> (usize, usize) {
    if total <= visible || visible == 0 {
        return (0, visible);
    }
    let thumb_size = (visible * visible / total).max(1).min(visible);
    let max_offset = total.saturating_sub(visible);
    let scrollable_rows = visible.saturating_sub(thumb_size);
    let thumb_pos = (offset * scrollable_rows)
        .checked_div(max_offset)
        .map_or(0, |pos| pos.min(scrollable_rows));
    (thumb_pos, thumb_size)
}

/// Split a display line into `(before, selected, after)` segments given a
/// highlight range. The `end` column is clamped to the line length.
fn split_for_highlight(line: &str, range: HighlightRange) -> (String, String, String) {
    let chars: Vec<char> = line.chars().collect();
    let len = chars.len();
    let start = range.start.min(len);
    let end = if range.end == usize::MAX {
        len
    } else {
        range.end.min(len)
    };
    let before: String = chars.iter().take(start).collect();
    let selected: String = if start < end {
        chars.iter().skip(start).take(end - start).collect()
    } else {
        String::new()
    };
    let after: String = chars.iter().skip(end).collect();
    (before, selected, after)
}

/// Scrollable text viewport — renders exactly `viewport_rows` display boxes.
///
/// Content lines WORD-WRAP at `max_line_width`; the viewport windows wrapped
/// display rows starting from the row where content line `scroll_offset`
/// begins. Rows beyond content are blank. A scrollbar is drawn on the right
/// when the wrapped-row count exceeds the viewport. When a selection is
/// active, selected cells are painted in inverse video, clipped per wrapped
/// row to that row's char range within its content line.
#[component]
pub fn ScrollableText(props: &ScrollableTextProps) -> impl Into<AnyElement<'static>> {
    let fg = props.color.unwrap_or(Color::Reset);
    let track_color = props.track_color.unwrap_or(Color::DarkGrey);
    let thumb_color = props.thumb_color.unwrap_or(Color::White);
    let sel_colors = SelectionColors {
        fg: props.selection_fg.unwrap_or(DEFAULT_SEL_FG),
        bg: props.selection_bg.unwrap_or(DEFAULT_SEL_BG),
    };
    let vp = props.viewport_rows.max(1);

    // Sidebar (22) + borders/padding (~5) + scrollbar (1) = ~28 chars of chrome
    let max_w = if props.max_line_width > 0 {
        props.max_line_width
    } else {
        let term_cols = crossterm::terminal::size().map_or(120, |(w, _)| w as usize);
        term_cols.saturating_sub(28)
    };

    // Wrap the whole document into flat display rows. The scroll offset is in
    // CONTENT-LINE units; convert to a display-row window start so the viewport
    // begins at the top of the scrolled-to line.
    let all_rows: Vec<DocDisplayRow> = wrap_document(&props.content, max_w);
    let total = all_rows.len();
    let first_visible = line_first_row(&all_rows, props.scroll_offset);

    // Build exactly `vp` display rows. Past the content, render blank rows
    // (content line `usize::MAX` so they never match a selection or caret).
    let blank = DocDisplayRow {
        text: String::new(),
        line: usize::MAX,
        line_char_start: 0,
        line_char_end: 0,
    };
    let display_rows: Vec<DocDisplayRow> = (0..vp)
        .map(|row| {
            let idx = first_visible + row;
            if idx < total {
                all_rows[idx].clone()
            } else {
                blank.clone()
            }
        })
        .collect();

    // The scrollbar reflects DISPLAY-ROW count (wrapping grows the document).
    let show_scrollbar = total > vp;
    let (thumb_pos, thumb_size) = scrollbar_geometry(total, vp, first_visible);

    // Resolve the caret to its specific wrapped subrow. The cursor props are in
    // content-(line, col) space; map onto the wrapped row that carries that col
    // and compute the column relative to that row's start.
    let (cursor_vp_line, cursor_rel_col) = props
        .cursor_line
        .zip(props.cursor_col)
        .and_then(|(line, col)| caret_row_for_line_col(&all_rows, line, col))
        .map_or((None, 0), |(gr, rel)| (gr.checked_sub(first_visible), rel));
    let cursor_colors = CursorColors {
        fg: props.cursor_color.unwrap_or(Color::Black),
        bg: props.cursor_bg.unwrap_or(Color::White),
    };

    element! {
        Box(flex_direction: FlexDirection::Row, width: 100pct) {
            // Text content column — exactly `vp` rows
            Box(flex_direction: FlexDirection::Column, flex_grow: 1.0_f32) {
                #(display_rows.iter().enumerate().map(|(row_idx, drow)| {
                    render_display_row(
                        RowContext {
                            row_idx,
                            content_line_offset: props.content_line_offset,
                            content_line: drow.line,
                            line_char_start: drow.line_char_start,
                            line_char_end: drow.line_char_end,
                        },
                        &drow.text,
                        PlainRowColors {
                            fg,
                            bg: props.bg,
                        },
                        CursorPos {
                            vp_line: cursor_vp_line,
                            col: cursor_rel_col,
                        },
                        cursor_colors,
                        SelectionState {
                            selection: props.selection.as_ref(),
                            colors: sel_colors,
                        },
                    )
                }).collect::<Vec<_>>())
            }
            // Scrollbar column (1 char wide, same `vp` rows)
            #(if show_scrollbar {
                std::iter::once(element! {
                    Box(flex_direction: FlexDirection::Column, width: 1u32) {
                        #((0..vp).map(|row| {
                            let is_thumb = row >= thumb_pos && row < thumb_pos + thumb_size;
                            let ch = if is_thumb { "┃" } else { "│" };
                            let color = if is_thumb { thumb_color } else { track_color };
                            element! {
                                Box(height: 1u32) {
                                    Text(content: ch.to_string(), color: color)
                                }
                            }
                        }).collect::<Vec<_>>())
                    }
                }).collect::<Vec<_>>()
            } else {
                Vec::new()
            })
        }
    }
}

/// Bundled default (non-highlighted, non-cursor) foreground + background colors
/// for [`render_display_row`].
#[derive(Clone, Copy)]
struct PlainRowColors {
    /// Text color for plain rows.
    fg: Color,
    /// Background color for plain rows (`None` leaves the cell transparent).
    bg: Option<Color>,
}

/// Bundled cursor foreground + background colors.
///
/// Extracted to keep [`render_display_row`] under the clippy
/// `too_many_arguments` threshold (6).
#[derive(Clone, Copy)]
struct CursorColors {
    /// Text color for the character under the cursor.
    fg: Color,
    /// Background color for the character under the cursor.
    bg: Color,
}

/// Bundled cursor position info for [`render_display_row`].
#[derive(Clone, Copy)]
struct CursorPos {
    /// Viewport-relative row index of the caret (already mapped onto its
    /// wrapped subrow), or `None` when there is no caret in view.
    vp_line: Option<usize>,
    /// Column index within this wrapped row (relative to the row's start).
    col: usize,
}

/// Bundled row context for [`render_display_row`].
#[derive(Clone, Copy)]
struct RowContext {
    /// Viewport row index (0-based).
    row_idx: usize,
    /// Content lines preceding the scrollable region (header rows).
    content_line_offset: usize,
    /// Content-line index this wrapped row belongs to.
    content_line: usize,
    /// Inclusive start char column of this row within its content line.
    line_char_start: usize,
    /// Exclusive end char column of this row within its content line.
    line_char_end: usize,
}

/// Bundled selection state + colors for [`render_display_row`].
#[derive(Clone, Copy)]
struct SelectionState<'a> {
    /// Active text selection, if any.
    selection: Option<&'a TextSelection>,
    /// Selection highlight colors.
    colors: SelectionColors,
}

/// Returns one `AnyElement` (a `Box(height: 1u32)`).
fn render_display_row(
    ctx: RowContext,
    line: &str,
    plain_colors: PlainRowColors,
    cursor: CursorPos,
    cursor_colors: CursorColors,
    selection: SelectionState<'_>,
) -> AnyElement<'static> {
    let fg = plain_colors.fg;
    let has_cursor = Some(ctx.row_idx) == cursor.vp_line;

    let highlight = row_highlight(selection, ctx);

    if let Some(range) = highlight {
        return highlight_row_element(line, range, fg, selection.colors);
    }

    if has_cursor {
        return cursor_row_element(line, cursor.col, fg, cursor_colors);
    }

    element! {
        Box(height: 1u32, background_color: plain_colors.bg) {
            Text(content: line.to_string(), color: fg, wrap: TextWrap::NoWrap)
        }
    }
    .into_any()
}

/// Resolve the selection highlight range for one wrapped row, clipped to the
/// row's char window. Returns `None` when the row does not overlap the
/// selection (no highlight painted on this row).
fn row_highlight(selection: SelectionState<'_>, ctx: RowContext) -> Option<HighlightRange> {
    let sel_content_line = ctx.content_line.saturating_add(ctx.content_line_offset);
    selection
        .selection
        .and_then(|s| row_highlight_range(s, sel_content_line))
        .and_then(|range| {
            let clipped = clip_range_to_row(range, ctx.line_char_start, ctx.line_char_end);
            // An empty clipped range means this wrapped row does not overlap
            // the selection's column window — paint no highlight here.
            (clipped.start < clipped.end).then_some(clipped)
        })
}

/// Render a row with the selection highlight (inverse video on the selected span).
fn highlight_row_element(
    line: &str,
    range: HighlightRange,
    fg: Color,
    colors: SelectionColors,
) -> AnyElement<'static> {
    let (before, selected, after) = split_for_highlight(line, range);
    let sel_text = if selected.is_empty() {
        " ".to_string()
    } else {
        selected
    };
    element! {
        Box(height: 1u32) {
            Text(content: before, color: fg, wrap: TextWrap::NoWrap)
            Box(background_color: colors.bg) {
                Text(content: sel_text, color: colors.fg, wrap: TextWrap::NoWrap)
            }
            Text(content: after, color: fg, wrap: TextWrap::NoWrap)
        }
    }
    .into_any()
}

/// Render a row carrying the caret (inverse video on the caret cell).
fn cursor_row_element(
    line: &str,
    cursor_col: usize,
    fg: Color,
    cursor_colors: CursorColors,
) -> AnyElement<'static> {
    let chars: Vec<char> = line.chars().collect();
    let before: String = chars.iter().take(cursor_col).collect();
    let cursor_ch: String = chars.iter().skip(cursor_col).take(1).collect();
    let after: String = chars.iter().skip(cursor_col + 1).collect();
    let cursor_display = if cursor_ch.is_empty() {
        " ".to_string()
    } else {
        cursor_ch
    };
    element! {
        Box(height: 1u32) {
            Text(content: before, color: fg, wrap: TextWrap::NoWrap)
            Box(background_color: cursor_colors.bg) {
                Text(content: cursor_display, color: cursor_colors.fg, wrap: TextWrap::NoWrap)
            }
            Text(content: after, color: fg, wrap: TextWrap::NoWrap)
        }
    }
    .into_any()
}

/// Clip a content-line highlight range to the `[row_start, row_end)` char
/// window a single wrapped row covers, shifting it to be relative to `row_start`.
///
/// Returns a range in row-relative columns. When the selection does not
/// overlap this row's window, the result is an empty range (start == end),
/// which the caller renders as no highlight on this row.
fn clip_range_to_row(range: HighlightRange, row_start: usize, row_end: usize) -> HighlightRange {
    let sel_start = range.start;
    // `usize::MAX` means "to end of line": treat it as the row's end so the
    // tail of a multi-line selection paints the full row.
    let sel_end = if range.end == usize::MAX {
        row_end
    } else {
        range.end
    };
    let lo = sel_start.max(row_start);
    let hi = sel_end.min(row_end);
    if lo >= hi {
        return HighlightRange { start: 0, end: 0 };
    }
    HighlightRange {
        start: lo - row_start,
        end: hi - row_start,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_scrollbar_geometry_no_scroll() {
        let (pos, size) = scrollbar_geometry(5, 10, 0);
        assert_eq!(pos, 0);
        assert_eq!(size, 10);
    }

    #[test]
    fn test_scrollbar_geometry_at_top() {
        let (pos, size) = scrollbar_geometry(100, 20, 0);
        assert_eq!(pos, 0);
        assert!(size >= 1);
        assert!(size <= 20);
    }

    #[test]
    fn test_scrollbar_geometry_at_bottom() {
        let (pos, size) = scrollbar_geometry(100, 20, 80);
        assert!(pos + size <= 20);
    }

    #[test]
    fn split_for_highlight_middle_substring() {
        let (before, sel, after) =
            split_for_highlight("hello world", HighlightRange { start: 2, end: 7 });
        assert_eq!(before, "he");
        assert_eq!(sel, "llo w");
        assert_eq!(after, "orld");
    }

    #[test]
    fn split_for_highlight_to_end_of_line() {
        let (before, sel, after) = split_for_highlight(
            "hello",
            HighlightRange {
                start: 2,
                end: usize::MAX,
            },
        );
        assert_eq!(before, "he");
        assert_eq!(sel, "llo");
        assert!(after.is_empty());
    }

    #[test]
    fn split_for_highlight_from_zero() {
        let (before, sel, after) =
            split_for_highlight("hello", HighlightRange { start: 0, end: 3 });
        assert!(before.is_empty());
        assert_eq!(sel, "hel");
        assert_eq!(after, "lo");
    }

    /// Render `ScrollableText` into a plain-text canvas string at a fixed size.
    fn render_text(content: &str, viewport_rows: u16, max_w: usize, cols: u16) -> String {
        let mut elem = element! {
            Box(width: u32::from(cols), height: u32::from(viewport_rows)) {
                ScrollableText(
                    content: content.to_string(),
                    scroll_offset: 0usize,
                    viewport_rows: usize::from(viewport_rows),
                    max_line_width: max_w,
                    color: Some(Color::Reset),
                    bg: None,
                )
            }
        };
        let canvas = elem.render(Some(usize::from(cols)));
        let mut buf = Vec::new();
        canvas
            .write_ansi(&mut buf)
            .unwrap_or_else(|e| panic!("write_ansi failed: {e}"));
        String::from_utf8_lossy(&buf).to_string()
    }

    /// A long line must WORD-WRAP: both the start and the tail of a long body
    /// must be visible (truncation would drop the tail). This is the core
    /// regression guard for issue #212's read-only comment display follow-up.
    #[test]
    fn long_line_wraps_so_full_text_is_visible() {
        let body = "alpha bravo charlie delta echo foxtrot golf hotel india juliett";
        // viewport tall enough to hold every wrapped row at width 10.
        let rendered = render_text(body, 12, 10, 12);
        // The start AND the end of the body must both be visible — only
        // possible if the line wrapped onto several rows.
        assert!(
            rendered.contains("alpha"),
            "wrap must show the start: {rendered}"
        );
        assert!(
            rendered.contains("juliett"),
            "wrap must show the tail (truncation would drop it): {rendered}"
        );
    }

    /// Strip ANSI CSI escape sequences from a rendered string so column-width
    /// assertions measure visible glyphs, not SGR codes.
    fn strip_ansi(s: &str) -> String {
        let mut out = String::with_capacity(s.len());
        let bytes = s.as_bytes();
        let mut i = 0usize;
        while i < bytes.len() {
            if bytes[i] == 0x1b && i + 1 < bytes.len() && bytes[i + 1] == b'[' {
                // Skip to the terminating letter of the CSI sequence.
                i += 2;
                while i < bytes.len() && !bytes[i].is_ascii_alphabetic() {
                    i += 1;
                }
            } else {
                out.push(bytes[i] as char);
            }
            i += 1;
        }
        out
    }

    /// No wrapped row may exceed the pane column width (text + scrollbar).
    #[test]
    fn no_wrapped_row_exceeds_width() {
        let body = "supercalifragilisticexpialidocious and some normal words here";
        let cols = 16u16;
        let rendered = render_text(body, 10, 12, cols);
        let clean = strip_ansi(&rendered);
        for (i, line) in clean.lines().enumerate() {
            assert!(
                line.chars().count() <= usize::from(cols),
                "wrapped row {i} exceeds {cols} cols: {:?} ({} chars)",
                line,
                line.chars().count()
            );
        }
    }

    /// A short line that fits within the width stays on a single row (no
    /// spurious wrapping).
    #[test]
    fn short_line_does_not_wrap() {
        let rendered = render_text("hello", 2, 50, 60);
        // Exactly one non-blank row carrying "hello".
        let non_blank: Vec<&str> = rendered.lines().filter(|l| !l.trim().is_empty()).collect();
        assert!(
            non_blank.iter().any(|l| l.contains("hello")),
            "short line must render on one row: {rendered}"
        );
    }

    /// A selection whose column window does NOT overlap a wrapped row must
    /// paint NO highlight on that row (regression: the clipped range is empty,
    /// which previously rendered a spurious inverse-space cell).
    #[test]
    fn non_overlapping_selection_paints_no_highlight_on_row() {
        use crate::selection::{SelectablePane, SelectionPoint, TextSelection};
        // At width 10, "alpha" fits on row 0 and "bravo charlie ..." continues
        // on row 1. A selection covering only cols 0..5 (the "alpha" part,
        // row 0) must NOT paint any highlight cell on row 1.
        let content = "alpha bravo charlie delta";
        let selection = TextSelection {
            anchor: SelectionPoint::new(SelectablePane::IssueDetail, 0, 0),
            focus: SelectionPoint::new(SelectablePane::IssueDetail, 0, 5),
        };
        let mut elem = element! {
            Box(width: 12u32, height: 3u32) {
                ScrollableText(
                    content: content.to_string(),
                    scroll_offset: 0usize,
                    viewport_rows: 3usize,
                    max_line_width: 10usize,
                    color: Some(Color::Reset),
                    bg: None,
                    selection: Some(selection),
                    selection_bg: Some(Color::White),
                    selection_fg: Some(Color::Black),
                )
            }
        };
        let canvas = elem.render(Some(12));
        let mut buf = Vec::new();
        canvas
            .write_ansi(&mut buf)
            .unwrap_or_else(|e| panic!("write_ansi failed: {e}"));
        let ansi = String::from_utf8_lossy(&buf);
        // The "alpha" row carries a selection background SGR; the "charlie"
        // row must NOT carry one (it is outside the 0..5 selection window).
        let rows: Vec<&str> = ansi.lines().collect();
        let non_blank: Vec<&&str> = rows.iter().filter(|l| !l.trim().is_empty()).collect();
        assert!(
            non_blank.len() >= 2,
            "expected at least 2 wrapped rows, got {non_blank:?}: {ansi}"
        );
        // Row with selection (alpha) has a background SGR; the next row
        // (charlie) must have NONE.
        assert!(
            non_blank[0].contains("\u{1b}[48") || non_blank[0].contains("\u{1b}[7m"),
            "the alpha row must carry a selection highlight SGR: {ansi}"
        );
        assert!(
            !non_blank[1].contains("\u{1b}[48") && !non_blank[1].contains("\u{1b}[7m"),
            "the charlie row must carry NO selection highlight (selection does not overlap it): {ansi}"
        );
    }
}

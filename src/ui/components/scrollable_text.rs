//! Scrollable text viewport with scrollbar.
//!
//! Renders a FIXED number of line rows (`viewport_rows`) regardless of content length.
//! Content is windowed by `scroll_offset`. Empty rows are padded so the component
//! always occupies the same layout space — preventing layout shifts.
//!
//! When a [`TextSelection`] is supplied (and its pane matches), the selected
//! cells are painted in inverse video so the user sees live drag-selection
//! feedback. The selection lives in *content* coordinates; this component maps
//! each display row back to its content line via `scroll_offset`.

use iocraft::prelude::*;

use crate::selection::{HighlightRange, TextSelection, row_highlight_range};
use crate::theme::SelectionColors;

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
    /// Max display width in characters. Lines exceeding this are truncated with "…".
    /// When 0, lines are truncated to terminal width minus a safety margin.
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

/// Scrollable text viewport — renders exactly `viewport_rows` line boxes.
///
/// Content is windowed from `scroll_offset`. Lines beyond content are blank.
/// A scrollbar is drawn on the right when content exceeds the viewport. When a
/// selection is active, selected cells are painted in inverse video.
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

    let all_lines: Vec<&str> = if props.content.is_empty() {
        Vec::new()
    } else {
        props.content.lines().collect()
    };
    let total = all_lines.len();
    let max_offset = total.saturating_sub(vp);
    let offset = props.scroll_offset.min(max_offset);

    // Build exactly `vp` display lines — pad with empty if content is short
    let display_lines: Vec<String> = (0..vp)
        .map(|row| {
            let line_idx = offset + row;
            if line_idx < total {
                crate::ui::util::truncate_with_ellipsis(all_lines[line_idx], max_w)
            } else {
                String::new()
            }
        })
        .collect();

    let show_scrollbar = total > vp;
    let (thumb_pos, thumb_size) = scrollbar_geometry(total, vp, offset);

    // Cursor position relative to the viewport (adjusted for scroll offset)
    let cursor_vp_line = props.cursor_line.map(|l| l.saturating_sub(offset));
    let cursor_col = props.cursor_col.unwrap_or(0);
    let cursor_colors = CursorColors {
        fg: props.cursor_color.unwrap_or(Color::Black),
        bg: props.cursor_bg.unwrap_or(Color::White),
    };

    element! {
        Box(flex_direction: FlexDirection::Row, width: 100pct) {
            // Text content column — exactly `vp` rows
            Box(flex_direction: FlexDirection::Column, flex_grow: 1.0) {
                #(display_lines.iter().enumerate().map(|(row_idx, line)| {
                    render_display_row(
                        RowContext {
                            row_idx,
                            offset,
                            content_line_offset: props.content_line_offset,
                        },
                        line,
                        PlainRowColors {
                            fg,
                            bg: props.bg,
                        },
                        CursorPos {
                            line: props.cursor_line,
                            vp_line: cursor_vp_line,
                            col: cursor_col,
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
                vec![element! {
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
                }]
            } else {
                vec![]
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
    /// Content line index of the cursor (0-based), or `None` if no cursor.
    line: Option<usize>,
    /// Viewport-relative line index (already adjusted for scroll offset).
    vp_line: Option<usize>,
    /// Column index within the display line.
    col: usize,
}

/// Bundled row context for [`render_display_row`].
#[derive(Clone, Copy)]
struct RowContext {
    /// Viewport row index (0-based).
    row_idx: usize,
    /// Scroll offset of the viewport.
    offset: usize,
    /// Content lines preceding the scrollable region (header rows).
    content_line_offset: usize,
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
    let row_idx = ctx.row_idx;
    let offset = ctx.offset;
    let fg = plain_colors.fg;
    let cursor_line = cursor.line;
    let cursor_vp_line = cursor.vp_line;
    let cursor_col = cursor.col;
    let has_cursor = cursor_line.is_some()
        && Some(row_idx) == cursor_vp_line
        && (offset + row_idx) == cursor_line.unwrap_or(usize::MAX);

    let content_line = ctx.content_line_offset + offset + row_idx;
    let highlight = selection
        .selection
        .and_then(|s| row_highlight_range(s, content_line));

    if let Some(range) = highlight {
        let (before, selected, after) = split_for_highlight(line, range);
        let sel_text = if selected.is_empty() {
            " ".to_string()
        } else {
            selected
        };
        return element! {
            Box(height: 1u32) {
                Text(content: before, color: fg, wrap: TextWrap::NoWrap)
                Box(background_color: selection.colors.bg) {
                    Text(content: sel_text, color: selection.colors.fg, wrap: TextWrap::NoWrap)
                }
                Text(content: after, color: fg, wrap: TextWrap::NoWrap)
            }
        }
        .into_any();
    }

    if has_cursor {
        let chars: Vec<char> = line.chars().collect();
        let before: String = chars.iter().take(cursor_col).collect();
        let cursor_ch: String = chars.iter().skip(cursor_col).take(1).collect();
        let after: String = chars.iter().skip(cursor_col + 1).collect();
        let cursor_display = if cursor_ch.is_empty() {
            " ".to_string()
        } else {
            cursor_ch
        };
        return element! {
            Box(height: 1u32) {
                Text(content: before, color: fg, wrap: TextWrap::NoWrap)
                Box(background_color: cursor_colors.bg) {
                    Text(content: cursor_display, color: cursor_colors.fg, wrap: TextWrap::NoWrap)
                }
                Text(content: after, color: fg, wrap: TextWrap::NoWrap)
            }
        }
        .into_any();
    }

    element! {
        Box(height: 1u32, background_color: plain_colors.bg) {
            Text(content: line.to_string(), color: fg, wrap: TextWrap::NoWrap)
        }
    }
    .into_any()
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
}

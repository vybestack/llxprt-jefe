//! Scrollable text viewport with scrollbar.
//!
//! Renders a FIXED number of line rows (`viewport_rows`) regardless of content length.
//! Content is windowed by `scroll_offset`. Empty rows are padded so the component
//! always occupies the same layout space — preventing layout shifts.

use iocraft::prelude::*;

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
}

/// Truncate a string to at most `max_chars` display characters.
fn truncate_line(line: &str, max_chars: usize) -> String {
    if max_chars == 0 || line.chars().count() <= max_chars {
        return line.to_string();
    }
    let truncated: String = line.chars().take(max_chars.saturating_sub(1)).collect();
    format!("{truncated}…")
}

/// Compute scrollbar thumb position and size using integer math.
fn scrollbar_geometry(total: usize, visible: usize, offset: usize) -> (usize, usize) {
    if total <= visible || visible == 0 {
        return (0, visible);
    }
    let thumb_size = (visible * visible / total).max(1).min(visible);
    let max_offset = total.saturating_sub(visible);
    let scrollable_rows = visible.saturating_sub(thumb_size);
    let thumb_pos = if max_offset > 0 {
        (offset * scrollable_rows / max_offset).min(scrollable_rows)
    } else {
        0
    };
    (thumb_pos, thumb_size)
}

/// Scrollable text viewport — renders exactly `viewport_rows` line boxes.
///
/// Content is windowed from `scroll_offset`. Lines beyond content are blank.
/// A scrollbar is drawn on the right when content exceeds the viewport.
#[component]
pub fn ScrollableText(props: &ScrollableTextProps) -> impl Into<AnyElement<'static>> {
    let fg = props.color.unwrap_or(Color::Reset);
    let track_color = props.track_color.unwrap_or(Color::DarkGrey);
    let thumb_color = props.thumb_color.unwrap_or(Color::White);
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
                truncate_line(all_lines[line_idx], max_w)
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
    let cursor_fg = props.cursor_color.unwrap_or(Color::Black);
    let cursor_bg_color = props.cursor_bg.unwrap_or(Color::White);

    element! {
        Box(flex_direction: FlexDirection::Row, width: 100pct) {
            // Text content column — exactly `vp` rows
            Box(flex_direction: FlexDirection::Column, flex_grow: 1.0) {
                #(display_lines.iter().enumerate().map(|(row_idx, line)| {
                    // Check if this row has the cursor
                    let has_cursor = props.cursor_line.is_some()
                        && Some(row_idx) == cursor_vp_line
                        // Only show cursor if the content line is actually within the viewport
                        && (offset + row_idx) == props.cursor_line.unwrap_or(usize::MAX);

                    if has_cursor {
                        // Split line into: before_cursor | cursor_char | after_cursor
                        let chars: Vec<char> = line.chars().collect();
                        let before: String = chars.iter().take(cursor_col).collect();
                        let cursor_ch: String = chars.iter().skip(cursor_col).take(1).collect();
                        let after: String = chars.iter().skip(cursor_col + 1).collect();
                        let cursor_display = if cursor_ch.is_empty() { " ".to_string() } else { cursor_ch };
                        element! {
                            Box(height: 1u32) {
                                Text(content: before, color: fg, wrap: TextWrap::NoWrap)
                                Box(background_color: cursor_bg_color) {
                                    Text(content: cursor_display, color: cursor_fg, wrap: TextWrap::NoWrap)
                                }
                                Text(content: after, color: fg, wrap: TextWrap::NoWrap)
                            }
                        }
                    } else {
                        element! {
                            Box(height: 1u32) {
                                Text(content: line.clone(), color: fg, wrap: TextWrap::NoWrap)
                            }
                        }
                    }
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
}

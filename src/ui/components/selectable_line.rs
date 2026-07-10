//! Shared helper for rendering a single text line with optional drag-selection
//! highlight (inverse video).
//!
//! Used by form, chooser, and confirm-modal components that render their own
//! lines as `Box(height: 1u32)` elements and need to paint selection feedback
//! without duplicating the split/before/selected/after logic.

use iocraft::prelude::*;

use crate::selection::{HighlightRange, SelectablePane, TextSelection, row_highlight_range};
use crate::theme::SelectionColors;

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

/// Render a single text line (`Box(height: 1u32)`) with optional selection
/// highlight.
///
/// When `selection` is active on `pane` and covers `content_line`, the
/// selected segment is painted in inverse video (`sel_colors`). The rest of
/// the line uses `plain_fg`.
#[must_use]
pub fn selectable_line(
    text: &str,
    content_line: usize,
    selection: Option<TextSelection>,
    pane: SelectablePane,
    plain_fg: Color,
    sel_colors: SelectionColors,
) -> AnyElement<'static> {
    let on_pane = selection.is_some_and(|s| s.pane() == pane);
    let highlight = if on_pane {
        selection.and_then(|s| row_highlight_range(&s, content_line))
    } else {
        None
    };

    match highlight {
        Some(range) => {
            let (before, selected, after) = split_for_highlight(text, range);
            let sel_text = if selected.is_empty() {
                " ".to_string()
            } else {
                selected
            };
            element! {
                Box(height: 1u32) {
                    Text(content: before, color: plain_fg, wrap: TextWrap::NoWrap)
                    Box(background_color: sel_colors.bg) {
                        Text(content: sel_text, color: sel_colors.fg, wrap: TextWrap::NoWrap)
                    }
                    Text(content: after, color: plain_fg, wrap: TextWrap::NoWrap)
                }
            }
            .into_any()
        }
        None => element! {
            Box(height: 1u32) {
                Text(content: text.to_owned(), color: plain_fg)
            }
        }
        .into_any(),
    }
}

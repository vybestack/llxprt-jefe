//! Fixed-size multiline text-box component with an inline caret.
//!
//! Renders EXACTLY `viewport_rows` rows derived from the pure
//! [`build_text_box_view`] projection. The caret (when present) renders as a
//! reverse-video cell; an empty caret cell renders a visible space. The
//! component does NOT read the terminal size and does NOT mutate state.
//!
//! @plan PLAN-20260624-PR-MODE.P14
//! @requirement REQ-PR-009
//! @requirement REQ-PR-010
//! @pseudocode component-001 lines 169-176

use iocraft::prelude::*;

use crate::text_box_view::{TextBoxRow, build_text_box_view};

/// Props for the fixed-size text-box component.
///
/// @plan PLAN-20260624-PR-MODE.P14
/// @requirement REQ-PR-009
/// @pseudocode component-001 lines 169-176
#[derive(Default, Props)]
pub struct TextBoxProps {
    /// The full raw text (may contain newlines).
    pub text: String,
    /// Byte cursor within `text`.
    pub byte_cursor: usize,
    /// Fixed number of rows this component occupies.
    pub viewport_rows: usize,
    /// Max display width in characters for prefix + row text.
    pub content_width: usize,
    /// Prefix/gutter rendered before each row's text.
    pub prefix: String,
    /// Text color.
    pub color: Option<Color>,
    /// Text color for the caret cell.
    pub caret_color: Option<Color>,
    /// Background color for the caret cell.
    pub caret_bg: Option<Color>,
}

/// Color pair for the caret cell (foreground, background).
///
/// @plan PLAN-20260624-PR-MODE.P14
/// @requirement REQ-PR-009
/// @pseudocode component-001 lines 169-176
#[derive(Clone, Copy)]
struct CaretColors(Color, Color);

/// Return the byte index for a char column, clamping to `text.len()`.
///
/// @plan PLAN-20260624-PR-MODE.P14
/// @requirement REQ-PR-009
/// @pseudocode component-001 lines 169-176
fn byte_index_for_char_col(text: &str, col: usize) -> usize {
    if col == 0 {
        return 0;
    }
    text.char_indices()
        .nth(col)
        .map_or(text.len(), |(idx, _)| idx)
}

/// Split row text around the caret cell without collecting the whole line.
///
/// @plan PLAN-20260624-PR-MODE.P14
/// @requirement REQ-PR-009
/// @pseudocode component-001 lines 169-176
fn caret_parts(row: &TextBoxRow) -> (&str, &str, &str) {
    let caret_col = row.caret_col.unwrap_or(0);
    let cursor_start = byte_index_for_char_col(&row.text, caret_col);
    let cursor_end = byte_index_for_char_col(&row.text, caret_col.saturating_add(1));
    (
        &row.text[..cursor_start],
        &row.text[cursor_start..cursor_end],
        &row.text[cursor_end..],
    )
}

/// Render a single row: prefix + text, with the caret cell (if any) as
/// reverse-video.
///
/// @plan PLAN-20260624-PR-MODE.P14
/// @requirement REQ-PR-009
/// @pseudocode component-001 lines 169-176
fn row_element(
    row: &TextBoxRow,
    prefix: &str,
    fg: Color,
    caret: CaretColors,
) -> AnyElement<'static> {
    if row.caret_col.is_some() {
        let (before, cursor_ch, after) = caret_parts(row);
        // `cursor_ch` is intentionally empty when the caret is after the last
        // character on the line; render a visible space cell in that case.
        let cursor_display = if cursor_ch.is_empty() { " " } else { cursor_ch };
        element! {
            Box(height: 1u32) {
                Text(content: prefix.to_string(), color: fg, wrap: TextWrap::NoWrap)
                Text(content: before.to_string(), color: fg, wrap: TextWrap::NoWrap)
                Box(background_color: caret.1) {
                    Text(content: cursor_display.to_string(), color: caret.0, wrap: TextWrap::NoWrap)
                }
                Text(content: after.to_string(), color: fg, wrap: TextWrap::NoWrap)
            }
        }
        .into()
    } else {
        element! {
            Box(height: 1u32) {
                Text(content: prefix.to_string(), color: fg, wrap: TextWrap::NoWrap)
                Text(content: row.text.clone(), color: fg, wrap: TextWrap::NoWrap)
            }
        }
        .into()
    }
}

/// Fixed-size multiline text-box with an inline reverse-video caret.
///
/// @plan PLAN-20260624-PR-MODE.P14
/// @requirement REQ-PR-009
/// @requirement REQ-PR-010
/// @pseudocode component-001 lines 169-176
#[component]
pub fn TextBox(props: &TextBoxProps) -> impl Into<AnyElement<'static>> {
    let fg = props.color.unwrap_or(Color::Reset);
    let caret = CaretColors(
        props.caret_color.unwrap_or(Color::Black),
        props.caret_bg.unwrap_or(Color::White),
    );

    let prefix = props.prefix.as_str();
    let row_width = props.content_width.saturating_sub(prefix.chars().count());
    let view = build_text_box_view(
        &props.text,
        props.byte_cursor,
        props.viewport_rows,
        row_width,
    );

    let rows: Vec<AnyElement<'static>> = view
        .rows
        .iter()
        .map(|r| row_element(r, prefix, fg, caret))
        .collect();

    element! {
        Box(flex_direction: FlexDirection::Column, width: 100pct) {
            #(rows)
        }
    }
}

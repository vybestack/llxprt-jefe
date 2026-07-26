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
use unicode_width::UnicodeWidthStr;

use crate::text_box_view::{TextBoxRow, build_text_box_view};

const SCROLL_INDICATOR_WIDTH: usize = 2;

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

/// Render editable row content with the caret cell, if present, reversed.
fn editable_row_element(
    row: &TextBoxRow,
    prefix: &str,
    fg: Color,
    caret: CaretColors,
) -> AnyElement<'static> {
    if row.caret_col.is_some() {
        let (before, cursor_ch, after) = caret_parts(row);
        let cursor_display = if cursor_ch.is_empty() { " " } else { cursor_ch };
        element! {
            Box(height: 1u32, flex_grow: 1.0_f32) {
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
            Box(height: 1u32, flex_grow: 1.0_f32) {
                Text(content: prefix.to_string(), color: fg, wrap: TextWrap::NoWrap)
                Text(content: row.text.clone(), color: fg, wrap: TextWrap::NoWrap)
            }
        }
        .into()
    }
}

fn scroll_indicator(row: usize, row_count: usize, up: bool, down: bool) -> &'static str {
    match (row == 0 && up, row.saturating_add(1) == row_count && down) {
        (true, true) => "↑↓",
        (true, false) => "↑ ",
        (false, true) => " ↓",
        (false, false) => "  ",
    }
}

/// Render one editable row plus its fixed-width right-side indicator gutter.
fn row_element(
    row: &TextBoxRow,
    prefix: &str,
    indicator: &str,
    fg: Color,
    caret: CaretColors,
) -> AnyElement<'static> {
    let editable = editable_row_element(row, prefix, fg, caret);
    element! {
        Box(height: 1u32, width: 100pct) {
            #(vec![editable])
            Box(width: u32::try_from(SCROLL_INDICATOR_WIDTH).unwrap_or(u32::MAX)) {
                Text(content: indicator.to_string(), color: fg, wrap: TextWrap::NoWrap)
            }
        }
    }
    .into()
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
    let row_width = props
        .content_width
        .saturating_sub(UnicodeWidthStr::width(prefix))
        .saturating_sub(SCROLL_INDICATOR_WIDTH);
    let view = build_text_box_view(
        &props.text,
        props.byte_cursor,
        props.viewport_rows,
        row_width,
    );
    let can_scroll_up = view.can_scroll_up();
    let can_scroll_down = view.can_scroll_down();
    let row_count = view.rows.len();

    let rows: Vec<AnyElement<'static>> = view
        .rows
        .iter()
        .enumerate()
        .map(|(index, row)| {
            let indicator = scroll_indicator(index, row_count, can_scroll_up, can_scroll_down);
            row_element(row, prefix, indicator, fg, caret)
        })
        .collect();
    let width = u32::try_from(props.content_width).unwrap_or(u32::MAX);

    element! {
        Box(flex_direction: FlexDirection::Column, width: width) {
            #(rows)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::TextBox;
    use iocraft::prelude::*;
    use unicode_width::UnicodeWidthStr;

    fn render(text: &str, byte_cursor: usize, viewport_rows: usize, width: usize) -> String {
        render_with_prefix(text, byte_cursor, viewport_rows, width, "> ")
    }

    fn render_with_prefix(
        text: &str,
        byte_cursor: usize,
        viewport_rows: usize,
        width: usize,
        prefix: &str,
    ) -> String {
        let mut element = element! {
            Box(width: u32::try_from(width).unwrap_or(u32::MAX), height: u32::try_from(viewport_rows).unwrap_or(u32::MAX)) {
                TextBox(
                    text: text.to_string(),
                    byte_cursor: byte_cursor,
                    viewport_rows: viewport_rows,
                    content_width: width,
                    prefix: prefix.to_string(),
                    color: Color::White,
                    caret_color: Color::Black,
                    caret_bg: Color::White,
                )
            }
        };
        element.render(Some(width)).to_string()
    }

    #[test]
    fn scroll_arrows_are_right_aligned_within_parent_width() {
        let text = "one\ntwo\nthree\nfour\nfive\nsix\nseven\neight";
        let width = 12;

        let top = render(text, 0, 3, width);
        let Some(down_row) = top.lines().nth(2) else {
            panic!("expected the third viewport row: {top}");
        };
        assert_eq!(down_row.chars().nth(width - 1), Some('↓'), "{top}");
        assert!(
            !top.contains('↑'),
            "top viewport must not show an up arrow: {top}"
        );

        let bottom = render(text, text.len(), 3, width);
        let Some(up_row) = bottom.lines().next() else {
            panic!("expected the first viewport row: {bottom}");
        };
        assert_eq!(up_row.chars().nth(width - 2), Some('↑'), "{bottom}");
        assert!(
            !bottom.contains('↓'),
            "bottom viewport must not show a down arrow: {bottom}"
        );

        let middle_cursor = text
            .find("four")
            .unwrap_or_else(|| panic!("fixture line must exist"));
        let middle = render(text, middle_cursor, 3, width);
        let Some(first_row) = middle.lines().next() else {
            panic!("expected the first middle row: {middle}");
        };
        let Some(last_row) = middle.lines().nth(2) else {
            panic!("expected the last middle row: {middle}");
        };
        assert_eq!(first_row.chars().nth(width - 2), Some('↑'), "{middle}");
        assert_eq!(last_row.chars().nth(width - 1), Some('↓'), "{middle}");

        let single_row = render(text, middle_cursor, 1, width);
        let Some(only_row) = single_row.lines().next() else {
            panic!("expected the single viewport row: {single_row}");
        };
        assert_eq!(
            only_row.chars().skip(width - 2).collect::<String>(),
            "↑↓",
            "{single_row}"
        );

        let fitted = render("one\ntwo", 0, 3, width);
        assert!(
            !fitted.contains('↑'),
            "fitted content must not show up: {fitted}"
        );
        assert!(
            !fitted.contains('↓'),
            "fitted content must not show down: {fitted}"
        );
    }

    #[test]
    fn wide_body_wraps_before_the_fixed_indicator_gutter() {
        let rendered = render("甲乙丙", 0, 2, 8);
        let lines = rendered.lines().collect::<Vec<_>>();

        assert_eq!(lines.len(), 2, "{rendered}");
        assert!(lines[0].contains("甲乙"), "{rendered}");
        assert!(!lines[0].contains('丙'), "{rendered}");
        assert!(lines[1].contains('丙'), "{rendered}");
        assert!(
            lines.iter().all(|line| UnicodeWidthStr::width(*line) <= 8),
            "wide body rows must leave the right gutter inside width 8: {rendered}"
        );
    }

    #[test]
    fn wide_prefix_uses_terminal_cell_width_in_text_budget() {
        let rendered = render_with_prefix("abcde", 0, 2, 8, "界");
        let Some(first_row) = rendered.lines().next() else {
            panic!("expected the first rendered row: {rendered}");
        };
        assert!(
            first_row.contains("界abcd"),
            "two-cell prefix should leave four text cells before the gutter: {rendered}"
        );
        assert!(
            !first_row.contains("abcde"),
            "the fifth character should wrap instead of exceeding the width: {rendered}"
        );
    }
}

//! Pure viewport projection for the property editor overlay (issue #175).
//!
//! iocraft-free and side-effect-free: given the full option list, the cursor
//! index, and the available viewport row count, computes a cursor-following
//! visible window. Extracted so the windowing logic is unit-testable and so
//! the iocraft component stays a thin view.

/// A computed visible window of property-editor options.
#[derive(Debug, Clone, PartialEq, Eq)]
#[must_use]
pub struct PropertyEditorView {
    /// Index of the first visible option within the full option list.
    pub window_offset: usize,
    /// Number of visible options (slice length).
    pub visible_count: usize,
}

impl PropertyEditorView {
    /// Iterate over the full-list indices that fall within the visible window.
    pub fn iter_visible(&self) -> impl Iterator<Item = usize> {
        self.window_offset..self.window_offset + self.visible_count
    }

    /// Returns `true` if the option at `full_index` is within the visible
    /// window. Used by the iocraft component to decide cursor highlighting.
    #[must_use]
    pub fn contains(&self, full_index: usize) -> bool {
        full_index >= self.window_offset && full_index < self.window_offset + self.visible_count
    }
}

/// Compute a cursor-following visible window over the option list.
///
/// - `option_count`: total number of options.
/// - `selected_index`: the current cursor position (clamped to the last option).
/// - `viewport_rows`: maximum number of option rows that fit. `0` yields an
///   empty window.
///
/// The window always contains the cursor: when the cursor is near the start
/// the window begins at 0; when near the end the window ends at the last
/// option; otherwise the cursor is kept stable as the user scrolls.
pub fn build_property_editor_view(
    option_count: usize,
    selected_index: usize,
    viewport_rows: usize,
) -> PropertyEditorView {
    if option_count == 0 || viewport_rows == 0 {
        return PropertyEditorView {
            window_offset: 0,
            visible_count: 0,
        };
    }
    let cursor = selected_index.min(option_count - 1);
    // If everything fits, start at 0.
    if option_count <= viewport_rows {
        return PropertyEditorView {
            window_offset: 0,
            visible_count: option_count,
        };
    }
    // Window that ends at the last option when the cursor is in the final page.
    let max_offset = option_count - viewport_rows;
    // Keep the cursor away from the edges: when it would fall outside the
    // current window, shift so it sits at the matching edge. Start the window
    // so the cursor is visible, preferring to keep earlier rows stable.
    let start = if cursor < viewport_rows {
        0
    } else if cursor + viewport_rows > option_count {
        // Cursor is in the last viewport-sized chunk: pin window to the end.
        option_count - viewport_rows
    } else {
        // Cursor is past the first page but not yet in the last page:
        // advance the window so the cursor is the first visible row only when
        // it leaves the current window. Simplest stable behavior: center-ish
        // by starting one page in once the cursor exceeds the first page.
        cursor.saturating_sub(viewport_rows - 1).min(max_offset)
    };
    PropertyEditorView {
        window_offset: start.min(max_offset),
        visible_count: viewport_rows,
    }
}

/// Exact outer bounds of the rendered property-editor overlay.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct PropertyEditorOverlayBounds {
    /// Width including border and horizontal padding.
    pub outer_width: u16,
    /// Height including border rows.
    pub outer_height: u16,
}

/// Semantic role of a projected property-editor line.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PropertyEditorProjectionLineKind {
    /// Header row.
    Header,
    /// Divider row.
    Separator,
    /// Option row and its full-list index.
    Option { full_index: usize, cursor: bool },
    /// Editable title row.
    Title,
    /// Footer or validation-error row.
    Footer { error: bool },
}

/// One rendered and selectable line in the property-editor overlay.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PropertyEditorProjectionLine {
    /// Exact visible/copyable text.
    pub text: String,
    /// Semantic line role used by the thin renderer and hit routing.
    pub kind: PropertyEditorProjectionLineKind,
    /// Visible caret column for title editing.
    pub caret_col: Option<usize>,
}

/// Complete pure projection shared by rendering, selection, and geometry.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct PropertyEditorProjection {
    /// Exact rows rendered inside the overlay border.
    pub lines: Vec<PropertyEditorProjectionLine>,
    /// Exact outer widget bounds.
    pub bounds: PropertyEditorOverlayBounds,
}

/// Inputs to [`build_property_editor_projection`].
pub struct PropertyEditorProjectionInput<'a> {
    pub header: &'a str,
    pub options: &'a [(String, bool)],
    pub selected_index: usize,
    pub multi_select: bool,
    pub title_text: &'a str,
    pub title_cursor: usize,
    pub is_title: bool,
    pub error: Option<&'a str>,
    pub terminal_cols: u16,
    pub terminal_rows: u16,
}

const PROPERTY_SEPARATOR: &str = "─────────────────────────────────────────";
const OVERLAY_ORIGIN_COL: u16 = crate::layout::LEFT_COL_WIDTH + 4;
const OVERLAY_ORIGIN_ROW: u16 = 3;
const OVERLAY_HORIZONTAL_CHROME: u16 = 4;
const OVERLAY_NON_OPTION_ROWS: u16 = 6;

/// Build the exact property-editor rows, viewport, caret, footer, and bounds.
#[must_use]
pub fn build_property_editor_projection(
    input: PropertyEditorProjectionInput<'_>,
) -> PropertyEditorProjection {
    let available_width = input.terminal_cols.saturating_sub(OVERLAY_ORIGIN_COL);
    let content_width = available_width.saturating_sub(OVERLAY_HORIZONTAL_CHROME);
    let title_width = usize::from(content_width.clamp(1, 78));
    let available_height = input.terminal_rows.saturating_sub(OVERLAY_ORIGIN_ROW);
    let viewport_rows = usize::from(available_height.saturating_sub(OVERLAY_NON_OPTION_ROWS));
    let view = build_property_editor_view(input.options.len(), input.selected_index, viewport_rows);
    let windowed = view.visible_count < input.options.len();
    let mut lines = vec![
        projection_line(input.header, PropertyEditorProjectionLineKind::Header),
        projection_line(
            PROPERTY_SEPARATOR,
            PropertyEditorProjectionLineKind::Separator,
        ),
    ];
    if input.is_title {
        lines.push(project_title_line(
            input.title_text,
            input.title_cursor,
            title_width,
        ));
    } else {
        project_option_lines(&mut lines, &input, &view);
    }
    lines.push(projection_line(
        PROPERTY_SEPARATOR,
        PropertyEditorProjectionLineKind::Separator,
    ));
    lines.push(project_footer(&input, windowed));
    let max_line_width = lines
        .iter()
        .map(|line| line.text.chars().count())
        .max()
        .unwrap_or_default();
    let desired_width = u16::try_from(max_line_width)
        .unwrap_or(u16::MAX)
        .saturating_add(OVERLAY_HORIZONTAL_CHROME);
    PropertyEditorProjection {
        bounds: PropertyEditorOverlayBounds {
            outer_width: desired_width.min(available_width),
            outer_height: u16::try_from(lines.len())
                .unwrap_or(u16::MAX)
                .saturating_add(2),
        },
        lines,
    }
}

fn projection_line(
    text: &str,
    kind: PropertyEditorProjectionLineKind,
) -> PropertyEditorProjectionLine {
    PropertyEditorProjectionLine {
        text: text.to_string(),
        kind,
        caret_col: None,
    }
}

fn project_title_line(text: &str, cursor: usize, width: usize) -> PropertyEditorProjectionLine {
    let view = crate::text_box_view::build_text_box_view(text, cursor, 1, width);
    let row = &view.rows[0];
    let caret_col = row.caret_col;
    let rendered = if let Some(col) = caret_col {
        let chars: Vec<char> = row.text.chars().collect();
        let before: String = chars.iter().take(col).collect();
        let after: String = chars.iter().skip(col).collect();
        format!("{before}▏{after}")
    } else {
        row.text.clone()
    };
    PropertyEditorProjectionLine {
        text: rendered,
        kind: PropertyEditorProjectionLineKind::Title,
        caret_col,
    }
}

fn project_option_lines(
    lines: &mut Vec<PropertyEditorProjectionLine>,
    input: &PropertyEditorProjectionInput<'_>,
    view: &PropertyEditorView,
) {
    for full_index in view.iter_visible() {
        let Some((label, selected)) = input.options.get(full_index) else {
            break;
        };
        let cursor = full_index == input.selected_index && view.contains(input.selected_index);
        let text = if input.multi_select {
            let marker = if *selected { "(x)" } else { "( )" };
            format!("{marker} {label}")
        } else {
            let marker = if cursor { ">" } else { " " };
            format!("{marker} {label}")
        };
        lines.push(projection_line(
            &text,
            PropertyEditorProjectionLineKind::Option { full_index, cursor },
        ));
    }
}

fn project_footer(
    input: &PropertyEditorProjectionInput<'_>,
    windowed: bool,
) -> PropertyEditorProjectionLine {
    let (text, error) = if let Some(error) = input.error {
        (error.to_string(), true)
    } else if input.is_title {
        ("type title  Enter apply  Esc cancel".to_string(), false)
    } else {
        let base = if input.multi_select {
            "Up/Down move  Space toggle  Enter apply  Esc cancel"
        } else {
            "Up/Down move  Enter apply  Esc cancel"
        };
        let text = if windowed {
            format!("{base}  (more below)")
        } else {
            base.to_string()
        };
        (text, false)
    };
    projection_line(&text, PropertyEditorProjectionLineKind::Footer { error })
}
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_options_yields_empty_window() {
        let view = build_property_editor_view(0, 0, 10);
        assert_eq!(view.visible_count, 0);
        assert_eq!(view.window_offset, 0);
    }

    #[test]
    fn zero_viewport_yields_empty_window() {
        let view = build_property_editor_view(5, 2, 0);
        assert_eq!(view.visible_count, 0);
    }

    #[test]
    fn fewer_options_than_viewport_shows_all() {
        let view = build_property_editor_view(3, 1, 10);
        assert_eq!(view.window_offset, 0);
        assert_eq!(view.visible_count, 3);
        assert!(view.contains(0));
        assert!(view.contains(2));
    }

    #[test]
    fn cursor_at_top_starts_at_zero() {
        let view = build_property_editor_view(20, 0, 5);
        assert_eq!(view.window_offset, 0);
        assert_eq!(view.visible_count, 5);
        assert!(view.contains(0));
        assert!(!view.contains(5));
    }

    #[test]
    fn cursor_in_middle_keeps_visible() {
        let view = build_property_editor_view(20, 10, 5);
        assert_eq!(view.visible_count, 5);
        assert!(view.contains(10));
    }

    #[test]
    fn cursor_at_last_option_pins_window_to_end() {
        let view = build_property_editor_view(20, 19, 5);
        assert_eq!(view.window_offset, 15);
        assert_eq!(view.visible_count, 5);
        assert!(view.contains(19));
        assert!(view.contains(15));
        assert!(!view.contains(14));
    }

    #[test]
    fn iter_visible_and_bounds_are_consistent() {
        let view = build_property_editor_view(20, 19, 5);
        let visible: Vec<usize> = view.iter_visible().collect();
        assert_eq!(visible, vec![15, 16, 17, 18, 19]);
        assert_eq!(view.window_offset, 15);
        assert_eq!(view.window_offset + view.visible_count, 20);
    }

    #[test]
    fn cursor_clamped_when_past_end() {
        let view = build_property_editor_view(10, 99, 5);
        assert!(view.contains(9));
        assert_eq!(view.visible_count, 5);
    }
}

#[cfg(test)]
mod overlay_projection_tests {
    use super::{
        PropertyEditorProjectionInput, PropertyEditorProjectionLineKind,
        build_property_editor_projection,
    };

    fn options(count: usize) -> Vec<(String, bool)> {
        (0..count)
            .map(|index| (format!("option-{index}"), index == 0))
            .collect()
    }

    #[test]
    fn projection_routes_first_and_last_visible_options_across_heights() {
        let options = options(20);
        for terminal_rows in [12, 20, 40] {
            for selected_index in [0, 19] {
                let projection = build_property_editor_projection(PropertyEditorProjectionInput {
                    header: "Edit Labels - Issue #42",
                    options: &options,
                    selected_index,
                    multi_select: true,
                    title_text: "",
                    title_cursor: 0,
                    is_title: false,
                    error: None,
                    terminal_cols: 120,
                    terminal_rows,
                });
                assert!(projection.lines.iter().any(|line| matches!(
                    line.kind,
                    PropertyEditorProjectionLineKind::Option { full_index, .. }
                        if full_index == selected_index
                )));
                let line_count = u16::try_from(projection.lines.len())
                    .unwrap_or_else(|_| panic!("projection line count should fit u16"));
                assert_eq!(projection.bounds.outer_height, line_count + 2);
            }
        }
    }

    #[test]
    fn title_projection_aligns_caret_text_footer_and_bounds() {
        let projection = build_property_editor_projection(PropertyEditorProjectionInput {
            header: "Edit Title - PR #42",
            options: &[],
            selected_index: 0,
            multi_select: false,
            title_text: "two words",
            title_cursor: 3,
            is_title: true,
            error: None,
            terminal_cols: 120,
            terminal_rows: 20,
        });
        let title = projection
            .lines
            .iter()
            .find(|line| matches!(line.kind, PropertyEditorProjectionLineKind::Title));
        let Some(title) = title else {
            panic!("title projection should include a title line");
        };
        assert_eq!(title.text, "two▏ words");
        assert_eq!(title.caret_col, Some(3));
        assert_eq!(
            projection.lines.last().map(|line| line.text.as_str()),
            Some("type title  Enter apply  Esc cancel")
        );
        assert_eq!(projection.bounds.outer_height, 7);
    }
}

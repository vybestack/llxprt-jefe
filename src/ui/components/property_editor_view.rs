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

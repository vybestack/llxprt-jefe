//! Selection data types and pure text-extraction logic.
//!
//! These types are iocraft-free and side-effect-free so they can be unit-tested
//! in isolation and reused by both the mouse-routing layer and the renderers.

use crate::selection::PaneGeometry;

/// Identifies a selectable region of the screen.
///
/// One variant per pane the user can drag-select text in. The variants are
/// ordered roughly top-to-bottom, left-to-right to match how the panes appear,
/// but ordering carries no semantic weight — comparisons use
/// [`SelectionPoint`] ordering, not the enum.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum SelectablePane {
    /// Repository sidebar (left column, all screen modes).
    #[default]
    Sidebar,
    /// Agent list (dashboard middle column, top).
    AgentList,
    /// Terminal snapshot grid (dashboard, when not focused).
    TerminalView,
    /// Preview pane (dashboard right column).
    Preview,
    /// Issue list (issues-mode workspace, top split).
    IssueList,
    /// Issue detail scrollable document (issues-mode workspace, bottom split).
    IssueDetail,
    /// PR list (PR-mode workspace, top split).
    PrList,
    /// PR detail scrollable document (PR-mode workspace, bottom split).
    PrDetail,
    /// Help modal overlay text.
    HelpModal,
    /// Top status bar (low priority, but selectable).
    StatusBar,
    /// Bottom keybind hint bar (low priority, but selectable).
    KeybindBar,
    /// Agent-definition form (full-screen modal). Issue #178.
    AgentForm,
    /// Repository-definition form (full-screen modal). Issue #178.
    RepositoryForm,
    /// Send-to-agent chooser overlay (issues/PR mode). Issue #178.
    AgentChooser,
    /// Merge-method chooser overlay (PR mode). Issue #178.
    MergeChooser,
    /// Confirmation modal (delete/kill/preflight/dirty-copy). Issue #178.
    ConfirmModal,
}

/// A single point within a selection, expressed in *content* coordinates.
///
/// `line` is a 0-based index into the pane's content lines (i.e. already
/// adjusted for the pane's scroll offset), and `col` is a 0-based character
/// offset within that line. Using content coordinates — not screen rows — keeps
/// selection stable when the pane scrolls.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SelectionPoint {
    /// Which pane this point lives in.
    pub pane: SelectablePane,
    /// 0-based content line index.
    pub line: usize,
    /// 0-based character column within the line.
    pub col: usize,
}

impl SelectionPoint {
    /// Construct a selection point from its three components.
    #[must_use]
    pub const fn new(pane: SelectablePane, line: usize, col: usize) -> Self {
        Self { pane, line, col }
    }

    /// Lexicographic ordering key for two points *within the same pane*.
    ///
    /// Returns [`std::cmp::Ordering::Equal`] only when both points are
    /// identical. When the panes differ the result is [`std::cmp::Ordering`]'s
    /// default — callers should only compare points known to share a pane.
    #[must_use]
    fn order_key(self) -> (usize, usize) {
        (self.line, self.col)
    }
}

/// An active text selection: an anchor (mouse-down) and a focus (current/drag).
///
/// Both points always share the same [`SelectablePane`]; a selection never
/// spans panes. Use [`normalize_selection`] to get the ordered (start, end)
/// pair before extracting text.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TextSelection {
    /// Where the selection started (mouse-down position).
    pub anchor: SelectionPoint,
    /// Where the selection currently ends (drag or release position).
    pub focus: SelectionPoint,
}

impl TextSelection {
    /// Construct a collapsed selection (anchor == focus) at a single point.
    #[must_use]
    pub const fn collapsed(point: SelectionPoint) -> Self {
        Self {
            anchor: point,
            focus: point,
        }
    }

    /// The pane this selection lives in (taken from the anchor).
    #[must_use]
    pub const fn pane(self) -> SelectablePane {
        self.anchor.pane
    }

    /// Whether the selection is empty (anchor == focus).
    #[must_use]
    pub fn is_empty(self) -> bool {
        self.anchor == self.focus
    }
}

/// Order two points so the result's first element is the earlier one.
///
/// "Earlier" is defined lexicographically by `(line, col)`. The two points are
/// assumed to share a pane (selections never cross panes); if they differ in
/// pane the comparison falls back to line/col ordering regardless.
///
/// # Examples
///
/// ```
/// use jefe::selection::{SelectionPoint, SelectablePane, normalize_selection};
///
/// let early = SelectionPoint::new(SelectablePane::IssueDetail, 0, 5);
/// let late  = SelectionPoint::new(SelectablePane::IssueDetail, 2, 0);
/// let (start, end) = normalize_selection(&early, &late);
/// assert_eq!(start.line, 0);
/// assert_eq!(end.line, 2);
/// ```
#[must_use]
pub fn normalize_selection(
    anchor: &SelectionPoint,
    focus: &SelectionPoint,
) -> (SelectionPoint, SelectionPoint) {
    if anchor.order_key() <= focus.order_key() {
        (*anchor, *focus)
    } else {
        (*focus, *anchor)
    }
}

/// Extract the text covered by a selection from a slice of content lines.
///
/// The selection is normalized first; the returned string joins the covered
/// lines with `\n`. A single-line selection returns the substring between the
/// two columns; a multi-line selection returns the tail of the first line, all
/// middle lines in full, and the head of the last line.
///
/// Coordinates are clamped to the content bounds, so a selection past the end
/// of a line or past the last line yields the available text without panicking.
#[must_use]
pub fn selection_text(selection: &TextSelection, lines: &[String]) -> String {
    let (start, end) = normalize_selection(&selection.anchor, &selection.focus);
    if lines.is_empty() {
        return String::new();
    }
    if start.line == end.line {
        return single_line_text(lines, &start, &end);
    }
    multi_line_text(lines, &start, &end)
}

/// Extract text from a single content line between two columns (inclusive start, exclusive end).
fn single_line_text(lines: &[String], start: &SelectionPoint, end: &SelectionPoint) -> String {
    let line_idx = start.line.min(lines.len() - 1);
    let chars: Vec<char> = lines[line_idx].chars().collect();
    let s = start.col.min(chars.len());
    let e = end.col.min(chars.len());
    if s >= e {
        return String::new();
    }
    chars[s..e].iter().collect()
}

/// Extract text spanning multiple lines: tail of start line, full middle lines, head of end line.
fn multi_line_text(lines: &[String], start: &SelectionPoint, end: &SelectionPoint) -> String {
    let last = lines.len() - 1;
    let start_line = start.line.min(last);
    let end_line = end.line.min(last);

    let mut out = String::new();

    // Tail of the start line (from start.col to end of line).
    let start_chars: Vec<char> = lines[start_line].chars().collect();
    let s = start.col.min(start_chars.len());
    out.extend(&start_chars[s..]);
    out.push('\n');

    // Full middle lines.
    for line in lines.iter().take(end_line).skip(start_line + 1) {
        out.push_str(line);
        out.push('\n');
    }

    // Head of the end line (from 0 to end.col). When the focus was past the
    // last content line, the line was clamped down — include the full last
    // line rather than truncating at the (now-meaningless) focus column.
    let end_chars: Vec<char> = lines[end_line].chars().collect();
    let end_clamped_down = end.line > last;
    let e = if end_clamped_down {
        end_chars.len()
    } else {
        end.col.min(end_chars.len())
    };
    out.extend(&end_chars[..e]);

    out
}

/// Convert a screen-space `(col, row)` within a pane to content coordinates.
///
/// `scroll_offset` is the pane's current scroll position (lines hidden above
/// the viewport). The result is `(content_line, content_col)` — content line
/// is the screen row translated by the content origin (the first content cell,
/// after borders/title/padding) plus the scroll offset, and content col is the
/// screen column minus the content origin column. Both are clamped to be
/// non-negative via saturating subtraction.
#[must_use]
pub fn point_to_content_coords(
    screen_col: u16,
    screen_row: u16,
    scroll_offset: usize,
    geometry: &PaneGeometry,
) -> (usize, usize) {
    let content_line = usize::from(screen_row.saturating_sub(geometry.content_origin_row))
        .saturating_add(scroll_offset);
    let content_col = usize::from(screen_col.saturating_sub(geometry.content_origin_col));
    (content_line, content_col)
}

#[cfg(test)]
mod text_tests {
    // The shared, parametrized tests live in the crate-level selection::tests
    // module so they exercise the public surface exactly as callers do.
}

/// A half-open column range `[start, end)` to highlight on a single display row.
///
/// Returned by [`row_highlight_range`] when a content line falls within a
/// selection. `start` is inclusive and `end` is exclusive; both are character
/// offsets within the line. When the whole line is selected, `start == 0` and
/// `end == usize::MAX` (callers clamp to the line length).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct HighlightRange {
    /// Inclusive start column.
    pub start: usize,
    /// Exclusive end column (`usize::MAX` means "to end of line").
    pub end: usize,
}

/// Compute the highlight column range for a single content line within a
/// selection, or `None` if the line is outside the selection.
///
/// The selection is normalized internally. For a line strictly between the
/// start and end lines the entire line is highlighted (`0..usize::MAX`). For
/// the start or end line, the column range is clamped to the selection
/// boundary. A collapsed (empty) selection yields `None`.
///
/// This is a pure helper used by the renderers to paint inverse-video
/// highlights; it owns no iocraft types so it is fully unit-testable.
#[must_use]
pub fn row_highlight_range(
    selection: &TextSelection,
    content_line: usize,
) -> Option<HighlightRange> {
    if selection.is_empty() {
        return None;
    }
    let (start, end) = normalize_selection(&selection.anchor, &selection.focus);
    if content_line < start.line || content_line > end.line {
        return None;
    }
    if content_line == start.line && content_line == end.line {
        return Some(HighlightRange {
            start: start.col,
            end: end.col.max(start.col),
        });
    }
    if content_line == start.line {
        return Some(HighlightRange {
            start: start.col,
            end: usize::MAX,
        });
    }
    if content_line == end.line {
        return Some(HighlightRange {
            start: 0,
            end: end.col,
        });
    }
    // Strictly between start and end lines: whole line.
    Some(HighlightRange {
        start: 0,
        end: usize::MAX,
    })
}

//! Pane geometry: maps screen coordinates to a selectable pane using the
//! same [`crate::layout`] constants the screens render with.
//!
//! The single entry point is [`pane_at`], which mirrors the on-screen layout
//! of each [`crate::state::ScreenMode`] (dashboard, issues, PRs) and returns
//! the pane under a `(col, row)` along with its screen-space rectangle.

use crate::layout::{
    AGENT_LIST_CHROME_COLS, AGENT_LIST_CHROME_ROWS, DETAIL_PANE_CHROME_COLS,
    DETAIL_PANE_CHROME_ROWS, KEYBIND_BAR_CHROME_COLS, LEFT_COL_WIDTH, LIST_PANE_CHROME_COLS,
    LIST_PANE_CHROME_ROWS, RIGHT_COL_WIDTH, SIDEBAR_CHROME_COLS, SIDEBAR_CHROME_ROWS,
    STATUS_BAR_CHROME_COLS, TERMINAL_VIEW_CHROME_COLS, TERMINAL_VIEW_CHROME_ROWS,
    effective_render_size, issues_pane_rows,
};
use crate::selection::ScreenLayout;
use crate::selection::text::SelectablePane;

/// Screen-space rectangle of one pane, in render-grid coordinates.
///
/// `origin_col`/`origin_row` is the top-left cell (0-based) of the pane's
/// *widget box* (including borders/title/padding), while
/// `content_origin_col`/`content_origin_row` is the top-left cell of the
/// pane's *first content cell* (after borders/title/padding). Selection
/// coordinate math uses the content origin so a click on the first content
/// line maps to content line 0. `width`/`height` are the widget-box size in
/// cells. All fields are non-negative and clamped to the terminal size.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PaneGeometry {
    /// 0-based column of the pane's widget-box left edge (border).
    pub origin_col: u16,
    /// 0-based row of the pane's widget-box top edge (border).
    pub origin_row: u16,
    /// Pane widget-box width in columns.
    pub width: u16,
    /// Pane widget-box height in rows.
    pub height: u16,
    /// 0-based column of the first content cell (after left border/padding).
    pub content_origin_col: u16,
    /// 0-based row of the first content cell (after top border/title).
    pub content_origin_row: u16,
}

impl PaneGeometry {
    /// Construct a pane rectangle from its widget-box origin and size, plus the
    /// content-cell origin (the first cell inside the border/title/padding).
    #[must_use]
    pub const fn new(
        origin_col: u16,
        origin_row: u16,
        width: u16,
        height: u16,
        content_origin_col: u16,
        content_origin_row: u16,
    ) -> Self {
        Self {
            origin_col,
            origin_row,
            width,
            height,
            content_origin_col,
            content_origin_row,
        }
    }

    /// Construct a pane rectangle from the widget-box origin/size, deriving the
    /// content origin by adding the given chrome offsets.
    #[must_use]
    pub const fn with_chrome(
        origin_col: u16,
        origin_row: u16,
        width: u16,
        height: u16,
        chrome_cols: u16,
        chrome_rows: u16,
    ) -> Self {
        Self::new(
            origin_col,
            origin_row,
            width,
            height,
            origin_col.saturating_add(chrome_cols),
            origin_row.saturating_add(chrome_rows),
        )
    }

    /// Whether a screen-space `(col, row)` falls inside this widget-box rectangle.
    ///
    /// Points on the bottom/right edge (inclusive of `origin + size - 1`) count
    /// as inside.
    #[must_use]
    pub fn contains(self, col: u16, row: u16) -> bool {
        let col_end = self.origin_col.saturating_add(self.width).saturating_sub(1);
        let row_end = self
            .origin_row
            .saturating_add(self.height)
            .saturating_sub(1);
        col >= self.origin_col && col <= col_end && row >= self.origin_row && row <= row_end
    }
}

/// Map a screen-space `(col, row)` to the pane under it.
///
/// Returns `None` when the point falls outside any known pane (e.g. on a
/// border line or in the gutter). The layout is computed from `term_cols` /
/// `term_rows` and the active screen mode (read from `layout.screen_mode`),
/// using the exact [`crate::layout`] constants the screens render with, so
/// geometry can never drift from the rendered output.
///
/// `terminal_input_enabled` only matters for the dashboard: when the terminal
/// is focused, mouse events over the terminal pane are forwarded to the PTY
/// and should not start an app selection, so [`SelectablePane::TerminalView`]
/// is excluded from the result in that case.
#[must_use]
pub fn pane_at(
    col: u16,
    row: u16,
    _screen_mode: crate::state::ScreenMode,
    terminal_input_enabled: bool,
    layout: &ScreenLayout,
) -> Option<(SelectablePane, PaneGeometry)> {
    let (render_cols, render_rows) = effective_render_size(layout.term_cols, layout.term_rows);
    if col >= render_cols || row >= render_rows {
        return None;
    }

    // Outer bars span the full width.
    if row == 0 {
        return Some(status_bar(render_cols));
    }
    if row == render_rows.saturating_sub(1) {
        return Some(keybind_bar(render_cols, render_rows));
    }

    match layout.screen_mode {
        crate::state::ScreenMode::Dashboard | crate::state::ScreenMode::Split => {
            dashboard_pane_at(col, row, render_cols, render_rows, terminal_input_enabled)
        }
        crate::state::ScreenMode::DashboardIssues
        | crate::state::ScreenMode::DashboardPullRequests => {
            issues_pane_at(col, row, render_cols, render_rows, *layout)
        }
    }
}

/// Status bar geometry (row 0, full width).
fn status_bar(render_cols: u16) -> (SelectablePane, PaneGeometry) {
    (
        SelectablePane::StatusBar,
        PaneGeometry::with_chrome(
            0,
            0,
            render_cols,
            1,
            STATUS_BAR_CHROME_COLS,
            crate::layout::STATUS_BAR_CHROME_ROWS,
        ),
    )
}

/// Keybind bar geometry (last row, full width).
fn keybind_bar(render_cols: u16, render_rows: u16) -> (SelectablePane, PaneGeometry) {
    let origin_row = render_rows.saturating_sub(1);
    (
        SelectablePane::KeybindBar,
        PaneGeometry::with_chrome(
            0,
            origin_row,
            render_cols,
            1,
            KEYBIND_BAR_CHROME_COLS,
            crate::layout::KEYBIND_BAR_CHROME_ROWS,
        ),
    )
}

/// Dashboard / split layout hit-test.
fn dashboard_pane_at(
    col: u16,
    row: u16,
    render_cols: u16,
    render_rows: u16,
    terminal_input_enabled: bool,
) -> Option<(SelectablePane, PaneGeometry)> {
    let content_top = 1u16;
    let content_bottom = render_rows.saturating_sub(1);

    // Sidebar: left column, full content height.
    if col < LEFT_COL_WIDTH {
        return Some(sidebar(content_top, content_bottom));
    }

    // Preview: right column, full content height.
    let preview_col0 = render_cols.saturating_sub(RIGHT_COL_WIDTH);
    if col >= preview_col0 {
        return Some(preview(preview_col0, content_top, content_bottom));
    }

    // Middle column: agent list (top) + terminal widget (bottom).
    let (agent_rows, terminal_slot_rows) = dashboard_middle_row_heights_for_render(render_rows);
    let agent_bottom_exclusive = content_top.saturating_add(agent_rows);
    if row < agent_bottom_exclusive {
        return Some(agent_list(
            LEFT_COL_WIDTH,
            content_top,
            preview_col0,
            agent_rows,
        ));
    }

    let terminal_top = agent_bottom_exclusive;
    let terminal_bottom_exclusive = terminal_top.saturating_add(terminal_slot_rows);
    if row < terminal_bottom_exclusive {
        // Forward to PTY when terminal input is enabled; otherwise selectable
        // terminal snapshot.
        if terminal_input_enabled {
            return None;
        }
        return Some(terminal_view(
            LEFT_COL_WIDTH,
            terminal_top,
            preview_col0,
            terminal_slot_rows,
        ));
    }

    // Below the middle column (shouldn't happen — content_bottom covers it).
    None
}

/// Issues/PR-mode layout hit-test (identical geometry, different pane names).
///
/// The [`ScreenLayout`]'s screen mode determines whether the list and detail
/// panes are returned as `IssueList`/`IssueDetail` or `PrList`/`PrDetail`
/// (see [`list_pane`] and [`detail_pane`], which branch on
/// `layout.is_pr_mode()`). The geometry itself is shared between issues and
/// PR modes because both use the same `issues_pane_rows` layout math.
fn issues_pane_at(
    col: u16,
    row: u16,
    render_cols: u16,
    render_rows: u16,
    layout: ScreenLayout,
) -> Option<(SelectablePane, PaneGeometry)> {
    let content_top = 1u16;
    let content_bottom = render_rows.saturating_sub(1);

    // Sidebar: left column, full content height.
    if col < LEFT_COL_WIDTH {
        return Some(sidebar(content_top, content_bottom));
    }

    // Workspace column: vertical stack of optional bands + list + detail.
    let workspace_col0 = LEFT_COL_WIDTH;
    let workspace_width = render_cols.saturating_sub(workspace_col0);

    let cursor_row = skip_non_list_bands(row, content_top, layout)?;
    let (list_rows, detail_rows) = issues_pane_rows(
        usize::from(render_rows),
        layout.error_visible,
        layout.filter_controls_open,
    );
    let list_rows_u16 = u16::try_from(list_rows).unwrap_or(0);
    let detail_rows_u16 = u16::try_from(detail_rows).unwrap_or(0);

    let list_bottom = cursor_row.saturating_add(list_rows_u16);
    if row < list_bottom {
        return Some(list_pane(
            workspace_col0,
            cursor_row,
            workspace_width,
            list_rows_u16,
            layout,
        ));
    }

    let detail_top = list_bottom;
    if row < detail_top.saturating_add(detail_rows_u16) {
        return Some(detail_pane(
            workspace_col0,
            detail_top,
            workspace_width,
            detail_rows_u16,
            layout,
        ));
    }

    None
}

/// Advance `cursor_row` past the error banner and filter band (if present).
///
/// Returns `Some(updated_cursor_row)` when the row is not inside a skipped
/// band, or `None` when the row hits a non-selectable band (error banner) or
/// the filter-controls band (which is not selectable).
fn skip_non_list_bands(row: u16, content_top: u16, layout: ScreenLayout) -> Option<u16> {
    let mut cursor_row = content_top;
    if layout.error_visible {
        if row == cursor_row {
            return None;
        }
        cursor_row = cursor_row.saturating_add(1);
    }
    if layout.filter_controls_open {
        let band_rows = u16::try_from(crate::layout::FILTER_CONTROLS_ROWS).unwrap_or(5);
        let band_bottom = cursor_row.saturating_add(band_rows);
        if row < band_bottom {
            return None;
        }
        cursor_row = band_bottom;
    }
    Some(cursor_row)
}

/// Choose the IssueList vs PrList variant based on the screen mode in layout.
fn list_pane(
    col0: u16,
    row0: u16,
    width: u16,
    height: u16,
    layout: ScreenLayout,
) -> (SelectablePane, PaneGeometry) {
    let pane = if layout.is_pr_mode() {
        SelectablePane::PrList
    } else {
        SelectablePane::IssueList
    };
    (
        pane,
        PaneGeometry::with_chrome(
            col0,
            row0,
            width,
            height,
            LIST_PANE_CHROME_COLS,
            LIST_PANE_CHROME_ROWS,
        ),
    )
}

/// Choose the IssueDetail vs PrDetail variant based on the screen mode in layout.
fn detail_pane(
    col0: u16,
    row0: u16,
    width: u16,
    height: u16,
    layout: ScreenLayout,
) -> (SelectablePane, PaneGeometry) {
    let pane = if layout.is_pr_mode() {
        SelectablePane::PrDetail
    } else {
        SelectablePane::IssueDetail
    };
    // Detail content starts directly below the border (1 row). The fixed
    // metadata header rows (title/state/labels/url/separator) are part of the
    // selectable content — they are rendered above the scroll viewport but are
    // NOT scrolled, so `content_origin_row` points at the first header row and
    // the scroll offset is suppressed for those rows in the mouse router.
    (
        pane,
        PaneGeometry::with_chrome(
            col0,
            row0,
            width,
            height,
            DETAIL_PANE_CHROME_COLS,
            DETAIL_PANE_CHROME_ROWS,
        ),
    )
}

fn sidebar(content_top: u16, content_bottom: u16) -> (SelectablePane, PaneGeometry) {
    let height = content_bottom.saturating_sub(content_top);
    (
        SelectablePane::Sidebar,
        PaneGeometry::with_chrome(
            0,
            content_top,
            LEFT_COL_WIDTH,
            height,
            SIDEBAR_CHROME_COLS,
            SIDEBAR_CHROME_ROWS,
        ),
    )
}

fn preview(col0: u16, content_top: u16, content_bottom: u16) -> (SelectablePane, PaneGeometry) {
    let height = content_bottom.saturating_sub(content_top);
    (
        SelectablePane::Preview,
        // Preview is a bordered box like the sidebar; reuse sidebar chrome.
        PaneGeometry::with_chrome(
            col0,
            content_top,
            RIGHT_COL_WIDTH,
            height,
            SIDEBAR_CHROME_COLS,
            SIDEBAR_CHROME_ROWS,
        ),
    )
}

fn agent_list(col0: u16, row0: u16, col_end: u16, height: u16) -> (SelectablePane, PaneGeometry) {
    let width = col_end.saturating_sub(col0);
    (
        SelectablePane::AgentList,
        PaneGeometry::with_chrome(
            col0,
            row0,
            width,
            height,
            AGENT_LIST_CHROME_COLS,
            AGENT_LIST_CHROME_ROWS,
        ),
    )
}

fn terminal_view(
    col0: u16,
    row0: u16,
    col_end: u16,
    height: u16,
) -> (SelectablePane, PaneGeometry) {
    let width = col_end.saturating_sub(col0);
    (
        SelectablePane::TerminalView,
        PaneGeometry::with_chrome(
            col0,
            row0,
            width,
            height,
            TERMINAL_VIEW_CHROME_COLS,
            TERMINAL_VIEW_CHROME_ROWS,
        ),
    )
}

/// Dashboard middle-row split for a given *render* row count.
///
/// Wraps [`crate::layout::dashboard_middle_row_heights_inner`] so the selection
/// geometry uses the exact same clamping the renderer applies (single source of
/// truth). `render_rows` is the post-`effective_render_size` height.
fn dashboard_middle_row_heights_for_render(render_rows: u16) -> (u16, u16) {
    crate::layout::dashboard_middle_row_heights_inner(render_rows)
}

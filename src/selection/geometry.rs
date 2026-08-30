//! Pane geometry: maps screen coordinates to a selectable pane using the
//! same [`crate::layout`] constants the screens render with.
//!
//! The single entry point is [`pane_at`], which uses descriptor-resolved
//! geometry when available and retains arithmetic only for residual compiled
//! adapters that have not yet published one.

use crate::layout::{
    DETAIL_PANE_CHROME_COLS, DETAIL_PANE_CHROME_ROWS, KEYBIND_BAR_CHROME_COLS, LEFT_COL_WIDTH,
    LIST_PANE_CHROME_COLS, LIST_PANE_CHROME_ROWS, SIDEBAR_CHROME_COLS, SIDEBAR_CHROME_ROWS,
    STATUS_BAR_CHROME_COLS, effective_render_size, issues_pane_rows,
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
/// `term_rows` and the active screen (read from `layout.screen`),
/// using the exact [`crate::layout`] constants the screens render with, so
/// geometry can never drift from the rendered output.
///
/// `terminal_input_enabled` only matters for the dashboard: when the terminal
/// is focused, mouse events over the terminal pane are forwarded to the PTY
/// and should not start an app selection, so [`SelectablePane::TerminalView`]
/// is excluded from the result in that case.
///
/// When `layout.overlay` is active (issue #178), full-screen overlays
/// (Help/AgentForm/RepositoryForm/ConfirmModal) intercept coordinates within
/// their rendered bounds, and positioned overlays (AgentChooser/MergeChooser)
/// intercept coordinates inside their bounds before falling through to the
/// underlying pane.
#[must_use]
pub fn pane_at(
    col: u16,
    row: u16,
    resolved: Option<&crate::workbench::ResolvedLayout>,
    terminal_input_enabled: bool,
    layout: &ScreenLayout,
) -> Option<(SelectablePane, PaneGeometry)> {
    let (render_cols, render_rows) = effective_render_size(layout.term_cols, layout.term_rows);
    if col >= render_cols || row >= render_rows {
        return None;
    }

    // Full-screen overlays (modals/forms) intercept coordinates within their
    // actual rendered bounds — dimensions come from HostOverlayLayout (help:
    // 60 cols x full render height; confirmation: 50x10 clamped to the
    // terminal), the same source the renderer and selection content use.
    if layout.overlay.is_full_screen()
        && let Some((pane, geo)) =
            full_screen_overlay_pane(layout.overlay, render_cols, render_rows)
    {
        if geo.contains(col, row) {
            return Some((pane, geo));
        }
        // Point is outside the modal's rendered bounds — no pane to select
        // (the modal replaced the screen, so the base layout is not visible).
        return None;
    }

    // Positioned overlays (choosers) intercept coordinates inside their bounds.
    if let Some(chooser) = chooser_pane_if_inside(col, row, *layout) {
        return Some(chooser);
    }

    // Outer bars span the full width.
    if row == 0 {
        return Some(status_bar(render_cols));
    }
    if row == render_rows.saturating_sub(1) {
        return Some(keybind_bar(render_cols, render_rows));
    }

    // The snapshot is the geometry authority: when the frame resolved one, the
    // pane under a cell is whatever it says occupies that cell. The arithmetic
    // below is the superseded mirror, kept only for callers that have no
    // snapshot yet (the first frame, and unit tests that exercise the legacy
    // path directly).
    if let Some(resolved) = resolved {
        return crate::selection::pane_at_resolved(col, row, resolved, terminal_input_enabled);
    }

    match layout.screen.compiled() {
        Some(crate::state::ScreenId::Repositories) => {
            split_pane_at(col, row, render_cols, render_rows)
        }
        Some(
            crate::state::ScreenId::Issues
            | crate::state::ScreenId::PullRequests
            | crate::state::ScreenId::Actions
            | crate::state::ScreenId::Errors,
        ) => issues_pane_at(col, row, render_cols, render_rows, *layout),
        // Open definitions and Settings resolve every pane through their
        // descriptor and have no legacy fallback geometry. The Terminal
        // Manager reaches this None arm through its descriptor identity.
        Some(crate::state::ScreenId::Settings) | None => None,
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

/// Split layout hit-test for the full-width repository Sidebar.
fn split_pane_at(
    col: u16,
    row: u16,
    render_cols: u16,
    render_rows: u16,
) -> Option<(SelectablePane, PaneGeometry)> {
    let layout = crate::layout::split_layout_for_render_size(render_cols, render_rows);
    let geometry = PaneGeometry::with_chrome(
        layout.sidebar_origin_col,
        layout.sidebar_origin_row,
        layout.sidebar_cols,
        layout.sidebar_rows,
        SIDEBAR_CHROME_COLS,
        SIDEBAR_CHROME_ROWS,
    );
    geometry
        .contains(col, row)
        .then_some((SelectablePane::Sidebar, geometry))
}

/// Issues/PR-mode layout hit-test (identical geometry, different pane names).
///
/// The [`ScreenLayout`]'s active screen determines whether the list and detail
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

/// Choose the IssueList vs PrList variant based on the active screen in layout.
fn list_pane(
    col0: u16,
    row0: u16,
    width: u16,
    height: u16,
    layout: ScreenLayout,
) -> (SelectablePane, PaneGeometry) {
    let pane = if layout.is_pr_mode() {
        SelectablePane::PrList
    } else if layout.is_actions_mode() {
        SelectablePane::ActionsList
    } else if layout.is_errors_mode() {
        SelectablePane::ErrorList
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

/// Choose the IssueDetail vs PrDetail variant based on the active screen in layout.
fn detail_pane(
    col0: u16,
    row0: u16,
    width: u16,
    height: u16,
    layout: ScreenLayout,
) -> (SelectablePane, PaneGeometry) {
    let pane = if layout.is_pr_mode() {
        SelectablePane::PrDetail
    } else if layout.is_actions_mode() {
        SelectablePane::ActionsDetail
    } else if layout.is_errors_mode() {
        SelectablePane::ErrorDetail
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

/// Full-screen overlay geometry for each overlay type.
fn full_screen_overlay_pane(
    overlay: crate::selection::OverlayPane,
    render_cols: u16,
    render_rows: u16,
) -> Option<(SelectablePane, PaneGeometry)> {
    let pane = overlay.to_pane()?;
    let geo = match overlay {
        crate::selection::OverlayPane::HelpModal => {
            let layout = crate::overlay_controls::HostOverlayLayout::help(render_cols, render_rows);
            PaneGeometry::new(0, 0, layout.width, layout.height, 2, 2)
        }
        crate::selection::OverlayPane::ConfirmModal => {
            let layout =
                crate::overlay_controls::HostOverlayLayout::confirmation(render_cols, render_rows);
            PaneGeometry::new(0, 0, layout.width, layout.height, 2, 2)
        }
        _ => PaneGeometry::new(0, 0, render_cols, render_rows, 2, 2),
    };
    Some((pane, geo))
}

const CHOOSER_OFFSET_COL: u16 = 4;
const CHOOSER_OFFSET_ROW: u16 = 2;
const CHOOSER_WIDTH: u16 = 45;
const CHOOSER_MAX_HEIGHT: u16 = 30;

/// Resolve a chooser overlay pane if `(col, row)` falls inside the chooser's
/// bounds.
///
/// The chooser is positioned at `top: 2, left: 4` relative to the workspace
/// column (which starts after the sidebar). The workspace starts at
/// `LEFT_COL_WIDTH` (issues) or `prs_main_columns().sidebar_width` (PRs);
/// both resolve to `LEFT_COL_WIDTH` in the common 120-col case. Since
/// `prs_main_columns` is a runtime function, we use `LEFT_COL_WIDTH` as the
/// baseline and let the caller's active screen disambiguate if needed.
fn chooser_pane_if_inside(
    col: u16,
    row: u16,
    layout: ScreenLayout,
) -> Option<(SelectablePane, PaneGeometry)> {
    let pane = layout.overlay.to_pane()?;
    if layout.overlay.is_full_screen() {
        return None;
    }

    // Workspace starts after the sidebar.
    let workspace_col0 = crate::layout::LEFT_COL_WIDTH;
    let chooser_origin_col = workspace_col0.saturating_add(CHOOSER_OFFSET_COL);
    // Workspace starts at row 1 (below the status bar); chooser offset adds 2.
    let chooser_origin_row = 1u16.saturating_add(CHOOSER_OFFSET_ROW);
    let chooser_width = CHOOSER_WIDTH;
    // Use a generous height so the whole overlay is selectable; the content
    // provider clips to actual rendered lines.
    let chooser_height = CHOOSER_MAX_HEIGHT;

    let geo = PaneGeometry::new(
        chooser_origin_col,
        chooser_origin_row,
        chooser_width,
        chooser_height,
        // Content starts after border (1) + padding_left (1) = 2 cols, and
        // after the top border (1 row, no padding_top).
        chooser_origin_col.saturating_add(2),
        chooser_origin_row.saturating_add(1),
    );
    if geo.contains(col, row) {
        Some((pane, geo))
    } else {
        None
    }
}

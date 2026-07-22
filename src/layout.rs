//! Layout constants and coordinate calculation functions.
//!
//! Extracted from main.rs to isolate geometry logic and enable focused unit testing.
//!
//! # Layout invariants
//!
//! The geometry helpers below guarantee the following invariants for every
//! input terminal size (including 0×0):
//!
//! - **Viewport never collapses**: `pty_rows` is always `>= 2` and `pty_cols`
//!   is always `>= 2`. Degenerate inputs are clamped so the PTY viewport stays
//!   usable.
//! - **Pane origin is strictly positive**: `pane_col0` is always
//!   `LEFT_COL_WIDTH + 1` (i.e. `> 0`) and `pane_row0` is always `> 0`.
//! - **Agent/terminal split**: The agent pane prefers 25% of the content rows
//!   (half-up rounded) but is clamped so the terminal pane always retains at
//!   least [`TERMINAL_PANE_MIN_ROWS`] rows. Under extreme tightness (very few
//!   content rows) the terminal pane degrades but never drops below 1 row.
//! - **Single source of truth**: All fixed geometry derives from the module
//!   constants (grouped as [`AppLayoutSpec::DEFAULT`]). Change a dimension in
//!   one place and every screen + PTY layout follows automatically.

/// Left column width (repository list).
pub const LEFT_COL_WIDTH: u16 = 22;
/// Right column width (preview pane).
pub const RIGHT_COL_WIDTH: u16 = 36;
/// Height consumed by status bar + keybind bar.
pub const OUTER_BARS_HEIGHT: u16 = 2;
/// Terminal widget chrome rows: top border + header row + bottom border.
pub const TERMINAL_WIDGET_CHROME_ROWS: u16 = 3;
/// Terminal widget chrome columns: left + right border.
pub const TERMINAL_WIDGET_CHROME_COLS: u16 = 2;
/// Minimum rows reserved for the agent pane so it keeps its chrome and at least one content row.
pub const AGENT_PANE_MIN_ROWS: u16 = 3;
/// Minimum rows reserved for the terminal pane so it keeps its chrome and a usable viewport.
pub const TERMINAL_PANE_MIN_ROWS: u16 = TERMINAL_WIDGET_CHROME_ROWS + 2;

// ── Selection chrome offsets ───────────────────────────────────────────────
//
// Each bordered pane consumes a fixed number of rows/cols of "chrome" (border,
// title, padding) before its first content cell. The mouse-selection geometry
// uses these to translate screen coordinates into content coordinates so a
// click on the first content line maps to content line 0, not to the border or
// title row. They mirror exactly what each `#[component]` renders.

/// Bordered-list panes (PrList, IssueList): top border + title row.
pub const LIST_PANE_CHROME_ROWS: u16 = 2;
/// Bordered-list panes: left border column.
pub const LIST_PANE_CHROME_COLS: u16 = 1;
/// Sidebar: top border + title row + top content padding.
pub const SIDEBAR_CHROME_ROWS: u16 = 3;
/// Sidebar: left border + 1-col content padding.
pub const SIDEBAR_CHROME_COLS: u16 = 2;
/// Agent list: top border + title row + top content padding.
pub const AGENT_LIST_CHROME_ROWS: u16 = 3;
/// Agent list: left border + 1-col content padding.
pub const AGENT_LIST_CHROME_COLS: u16 = 2;
/// Terminal view: top border + title row.
pub const TERMINAL_VIEW_CHROME_ROWS: u16 = 2;
/// Terminal view: left border column.
pub const TERMINAL_VIEW_CHROME_COLS: u16 = 1;
/// Detail panes (PrDetail, IssueDetail): top border above the header rows.
///
/// The detail pane renders a border, then `DETAIL_HEADER_ROWS` metadata rows
/// (which includes the trailing separator), so the scrollable content starts
/// `DETAIL_HEADER_ROWS + 1` rows below the widget-box top.
pub const DETAIL_PANE_CHROME_ROWS: u16 = 1;
/// Detail panes: left border + 1-col left padding.
pub const DETAIL_PANE_CHROME_COLS: u16 = 2;
/// Status bar: 1-col left padding, no border.
pub const STATUS_BAR_CHROME_COLS: u16 = 1;
/// Status bar: no top chrome.
pub const STATUS_BAR_CHROME_ROWS: u16 = 0;
/// Keybind bar: 1-col left padding, no border.
pub const KEYBIND_BAR_CHROME_COLS: u16 = 1;
/// Keybind bar: no top chrome.
pub const KEYBIND_BAR_CHROME_ROWS: u16 = 0;

/// Static layout specification: the fixed geometry contract shared across the
/// dashboard and issues screens.
///
/// Grouping these into one typed value documents their cohesion and makes the
/// layout contract reviewable in a single place.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AppLayoutSpec {
    /// Width of the left repository sidebar column.
    pub left_col_width: u16,
    /// Width of the right preview column.
    pub right_col_width: u16,
    /// Rows consumed by the outer status bar + keybind bar.
    pub outer_bars_height: u16,
    /// Rows consumed by the terminal widget chrome (top border + header + bottom border).
    pub terminal_widget_chrome_rows: u16,
    /// Columns consumed by the terminal widget chrome (left + right border).
    pub terminal_widget_chrome_cols: u16,
    /// Minimum rows reserved for the agent pane.
    pub agent_pane_min_rows: u16,
    /// Minimum rows reserved for the terminal pane.
    pub terminal_pane_min_rows: u16,
}

impl AppLayoutSpec {
    /// The canonical layout spec used by the application.
    ///
    /// Every field references the corresponding module constant so there is a
    /// single source of truth for the layout contract.
    pub const DEFAULT: Self = Self {
        left_col_width: LEFT_COL_WIDTH,
        right_col_width: RIGHT_COL_WIDTH,
        outer_bars_height: OUTER_BARS_HEIGHT,
        terminal_widget_chrome_rows: TERMINAL_WIDGET_CHROME_ROWS,
        terminal_widget_chrome_cols: TERMINAL_WIDGET_CHROME_COLS,
        agent_pane_min_rows: AGENT_PANE_MIN_ROWS,
        terminal_pane_min_rows: TERMINAL_PANE_MIN_ROWS,
    };
}

/// Computed PTY viewport geometry: terminal cell dimensions plus the
/// zero-based render-grid origin (column/row) of the viewport.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PtyLayout {
    /// PTY viewport height in rows (always `>= 2`).
    pub pty_rows: u16,
    /// PTY viewport width in columns (always `>= 2`).
    pub pty_cols: u16,
    /// Zero-based render-grid column where the PTY viewport's left edge sits (always `> 0`).
    pub pane_col0: u16,
    /// Zero-based render-grid row where the PTY viewport's top edge sits (always `> 0`).
    pub pane_row0: u16,
}

/// Check if fullscreen mode is enabled.
#[must_use]
pub fn is_fullscreen_enabled() -> bool {
    std::env::var("JEFE_WINDOWED").ok().as_deref() != Some("1")
}

/// Calculate effective render dimensions for a given fullscreen flag.
#[must_use]
fn effective_render_size_inner(cols: u16, rows: u16, fullscreen: bool) -> (u16, u16) {
    if fullscreen {
        (cols, rows)
    } else {
        (cols.saturating_sub(2).max(1), rows.saturating_sub(2).max(1))
    }
}

#[must_use]
pub fn dashboard_middle_row_heights_inner(render_rows: u16) -> (u16, u16) {
    let content_rows = render_rows.saturating_sub(OUTER_BARS_HEIGHT);

    if content_rows <= AGENT_PANE_MIN_ROWS + TERMINAL_PANE_MIN_ROWS {
        let terminal_rows = content_rows.saturating_sub(AGENT_PANE_MIN_ROWS).max(1);
        let agent_rows = content_rows.saturating_sub(terminal_rows);
        return (agent_rows, terminal_rows);
    }

    // Round to nearest (half-up) to match taffy's cumulative rounding of
    // percentage-based flex children. Simple truncation (`*25/100`) under-
    // counts by 1 row whenever the product has a fractional part ≥ 0.5.
    let preferred_agent_rows = content_rows
        .saturating_mul(25)
        .saturating_add(50)
        .saturating_div(100);
    let max_agent_rows = content_rows.saturating_sub(TERMINAL_PANE_MIN_ROWS);
    let agent_rows = preferred_agent_rows
        .clamp(AGENT_PANE_MIN_ROWS, max_agent_rows)
        .min(content_rows);
    let terminal_rows = content_rows.saturating_sub(agent_rows).max(1);

    (agent_rows, terminal_rows)
}

/// Calculate effective render dimensions.
#[must_use]
pub fn effective_render_size(cols: u16, rows: u16) -> (u16, u16) {
    effective_render_size_inner(cols, rows, is_fullscreen_enabled())
}

/// Physical Split-screen repository pane within an effective render grid.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SplitLayout {
    /// Left edge of the Sidebar widget after main horizontal padding.
    pub sidebar_origin_col: u16,
    /// Top edge after status bar, main top padding, and filter band.
    pub sidebar_origin_row: u16,
    /// Sidebar widget width after main horizontal padding.
    pub sidebar_cols: u16,
    /// Sidebar widget height after outer bars, main padding, and filter band.
    pub sidebar_rows: u16,
    /// Sidebar row width after its border and horizontal content padding.
    pub sidebar_content_cols: u16,
}

/// Project Split-screen geometry from already-normalized render dimensions.
///
/// Callers at terminal boundaries must invoke [`effective_render_size`] once,
/// then pass the resulting dimensions here.
#[must_use]
pub const fn split_layout_for_render_size(render_cols: u16, render_rows: u16) -> SplitLayout {
    const MAIN_PADDING: u16 = 1;
    const FILTER_ROWS: u16 = 3;

    let sidebar_cols = render_cols.saturating_sub(MAIN_PADDING.saturating_mul(2));
    SplitLayout {
        sidebar_origin_col: MAIN_PADDING,
        sidebar_origin_row: 1 + MAIN_PADDING + FILTER_ROWS,
        sidebar_cols,
        sidebar_rows: render_rows
            .saturating_sub(OUTER_BARS_HEIGHT)
            .saturating_sub(MAIN_PADDING.saturating_mul(2))
            .saturating_sub(FILTER_ROWS),
        sidebar_content_cols: crate::list_viewport::bordered_padded_content_width(sidebar_cols),
    }
}

/// Compute adaptive middle-column row heights for the dashboard layout.
#[must_use]
pub fn dashboard_middle_row_heights(term_cols: u16, term_rows: u16) -> (u16, u16) {
    let (_, render_rows) = effective_render_size(term_cols, term_rows);
    dashboard_middle_row_heights_inner(render_rows)
}

/// Compute PTY viewport size and its origin for a given fullscreen flag.
#[must_use]
fn compute_pty_layout_inner(term_cols: u16, term_rows: u16, fullscreen: bool) -> PtyLayout {
    let (render_cols, render_rows) = effective_render_size_inner(term_cols, term_rows, fullscreen);

    let (agent_rows, terminal_slot_rows) = dashboard_middle_row_heights_inner(render_rows);
    let middle_cols = render_cols.saturating_sub(LEFT_COL_WIDTH + RIGHT_COL_WIDTH);

    let pty_rows = terminal_slot_rows
        .saturating_sub(TERMINAL_WIDGET_CHROME_ROWS)
        .max(2);
    let pty_cols = middle_cols
        .saturating_sub(TERMINAL_WIDGET_CHROME_COLS)
        .max(2);

    let pane_col0 = LEFT_COL_WIDTH.saturating_add(1);
    let pane_row0 = 1u16.saturating_add(agent_rows).saturating_add(2);

    PtyLayout {
        pty_rows,
        pty_cols,
        pane_col0,
        pane_row0,
    }
}

/// Compute PTY viewport size and its origin within the fullscreen render grid.
///
/// Layout mirrors dashboard proportions:
/// - top status bar (1 row)
/// - bottom keybind bar (1 row)
/// - middle column split: agent list prefers 25% and terminal gets the rest
/// - under tight heights, the terminal keeps enough rows for its chrome and viewport
/// - terminal widget chrome: border + header + border
#[must_use]
pub fn compute_pty_layout(term_cols: u16, term_rows: u16) -> PtyLayout {
    compute_pty_layout_inner(term_cols, term_rows, is_fullscreen_enabled())
}

/// Compute expanded PTY geometry for the shell overlay (issue #222).
///
/// When the embedded shell overlay is active, the terminal pane expands to
/// fill the area between the repository sidebar and the right edge (the
/// preview pane is hidden), using the full content height. This uses the
/// same chrome and layout constants as the normal layout — only the region
/// dimensions change.
#[must_use]
pub fn compute_shell_overlay_pty_layout(term_cols: u16, term_rows: u16) -> PtyLayout {
    let (render_cols, render_rows) =
        effective_render_size_inner(term_cols, term_rows, is_fullscreen_enabled());

    // The overlay replaces agent list + preview, so the terminal gets the full
    // content height minus the outer bars.
    let content_rows = render_rows.saturating_sub(OUTER_BARS_HEIGHT);
    // Width: from just past the left sidebar to the right edge (no preview).
    let middle_cols = render_cols.saturating_sub(LEFT_COL_WIDTH);

    let pty_rows = content_rows
        .saturating_sub(TERMINAL_WIDGET_CHROME_ROWS)
        .max(2);
    let pty_cols = middle_cols
        .saturating_sub(TERMINAL_WIDGET_CHROME_COLS)
        .max(2);

    let pane_col0 = LEFT_COL_WIDTH.saturating_add(1);
    let pane_row0 = 3u16;

    PtyLayout {
        pty_rows,
        pty_cols,
        pane_col0,
        pane_row0,
    }
}

/// Compute PTY geometry for the live Terminal Manager lower pane.
#[must_use]
pub fn compute_terminal_manager_pty_layout(term_cols: u16, term_rows: u16) -> PtyLayout {
    let (render_cols, render_rows) =
        effective_render_size_inner(term_cols, term_rows, is_fullscreen_enabled());
    let (list_rows, detail_rows) = actions_pane_rows(usize::from(render_rows), false, false);
    let terminal_slot_rows = u16::try_from(detail_rows).unwrap_or(u16::MAX);
    let workspace_cols = render_cols.saturating_sub(LEFT_COL_WIDTH);

    PtyLayout {
        pty_rows: terminal_slot_rows
            .saturating_sub(TERMINAL_WIDGET_CHROME_ROWS)
            .max(2),
        pty_cols: workspace_cols
            .saturating_sub(TERMINAL_WIDGET_CHROME_COLS)
            .max(2),
        pane_col0: LEFT_COL_WIDTH.saturating_add(1),
        pane_row0: 1u16
            .saturating_add(u16::try_from(list_rows).unwrap_or(u16::MAX))
            .saturating_add(2),
    }
}

// -----------------------------------------------------------------------------
// Issues-mode detail-pane layout
// -----------------------------------------------------------------------------
//
// The following constants and helpers describe the fixed vertical structure of
// the issue detail pane. They are shared between the rendering component
// (`ui::components::issue_detail`) and the scroll-limit logic
// (`state::types::IssuesState`) so that both agree on how much chrome the pane
// consumes. Keeping them here avoids a dependency from the state layer into the
// UI layer.

/// Number of fixed rows the detail metadata header occupies.
///
/// The header renders exactly five rows:
/// 1. title (`#<number> <title>`),
/// 2. state/author/timestamps,
/// 3. labels/assignees/milestone,
/// 4. external URL,
/// 5. a horizontal rule separator.
pub const DETAIL_HEADER_ROWS: usize = 5;

/// Fixed width of the repository sidebar in Issues mode.
pub const ISSUES_SIDEBAR_WIDTH: u16 = LEFT_COL_WIDTH;

/// Horizontal chrome consumed by the issue list pane border.
pub const ISSUE_LIST_CHROME_COLS: u16 = 2;

/// Rows consumed by the filter controls band when it is visible.
///
/// The component renders a bordered box with three fixed content rows.
pub const FILTER_CONTROLS_ROWS: usize = 5;

/// Fixed rows outside the Issues workspace split.
///
/// Accounts for the status bar and keybind bar.
pub const DETAIL_CHROME_ROWS: usize = OUTER_BARS_HEIGHT as usize;

/// Fixed local viewport height for embedded detail composer text boxes.
///
/// Kept in layout (rather than UI) because state scroll bounds must reserve
/// the same rows the component renders when it reveals the composer anchor.
///
/// @plan PLAN-20260624-PR-MODE.P14
/// @requirement REQ-PR-009
/// @requirement REQ-PR-010
/// @pseudocode component-001 lines 169-176
pub const DETAIL_COMPOSER_VIEWPORT_ROWS: usize = 5;
pub const PR_COMPOSER_VIEWPORT_ROWS: usize = DETAIL_COMPOSER_VIEWPORT_ROWS;
pub const NEW_COMMENT_COMPOSER_PREFIX: &str = "  │ ";
pub const REPLY_COMPOSER_PREFIX: &str = "    │ ";

/// Compute rows available to the read-only PR detail document after embedded
/// local editors reserve rows inside the detail pane.
///
/// @plan PLAN-20260624-PR-MODE.P14
/// @requirement REQ-PR-009
/// @pseudocode component-001 lines 169-176
#[must_use]
pub fn pr_detail_document_viewport_rows(
    detail_viewport_rows: usize,
    pr_composer_text_box_active: bool,
) -> usize {
    detail_document_viewport_rows(detail_viewport_rows, pr_composer_text_box_active)
}

/// Compute rows available to an Issues detail read-only document after an
/// embedded composer TextBox reserves rows inside the detail pane.
#[must_use]
pub fn issue_detail_document_viewport_rows(
    detail_viewport_rows: usize,
    issue_composer_text_box_active: bool,
) -> usize {
    detail_document_viewport_rows(detail_viewport_rows, issue_composer_text_box_active)
}

fn detail_document_viewport_rows(
    detail_viewport_rows: usize,
    composer_text_box_active: bool,
) -> usize {
    if detail_viewport_rows == 0 {
        return 0;
    }
    let reserved = if composer_text_box_active {
        DETAIL_COMPOSER_VIEWPORT_ROWS.min(detail_viewport_rows.saturating_sub(1))
    } else {
        0
    };
    detail_viewport_rows - reserved
}

/// Compute the rows allocated to the Issues-mode list and detail panes.
///
/// Subtracts fixed outer bars and conditional bands, then gives 30% of the
/// remaining workspace to the list and the rest to detail. The UI uses these
/// exact row counts for the pane boxes, so the detail viewport and scroll limits
/// are derived from the same allocation the renderer receives.
#[must_use]
pub fn issues_pane_rows(
    term_rows: usize,
    error_visible: bool,
    filter_controls_open: bool,
) -> (usize, usize) {
    let mut workspace_rows = term_rows.saturating_sub(DETAIL_CHROME_ROWS);
    if error_visible {
        workspace_rows = workspace_rows.saturating_sub(1);
    }
    if filter_controls_open {
        workspace_rows = workspace_rows.saturating_sub(FILTER_CONTROLS_ROWS);
    }
    let list_rows = workspace_rows * 3 / 10;
    let detail_rows = workspace_rows.saturating_sub(list_rows);
    (list_rows, detail_rows)
}

/// Compute the rows allocated to the Issues-mode detail pane.
#[must_use]
pub fn issues_detail_pane_rows(
    term_rows: usize,
    error_visible: bool,
    filter_controls_open: bool,
) -> usize {
    issues_pane_rows(term_rows, error_visible, filter_controls_open).1
}

/// Convert an allocated detail-pane height into the physical body viewport.
#[must_use]
pub const fn detail_body_viewport_rows(pane_rows: usize) -> usize {
    pane_rows.saturating_sub(DETAIL_HEADER_ROWS + 2)
}

/// Derive the single Issues-mode banner text with error precedence.
///
/// When both `error` and `draft_notice` are present, the error wins. This
/// pure projection is consumed by both the visible banner render and the
/// pane row sizing so they never disagree (issue #265).
#[must_use]
pub fn issues_banner_text<'a>(
    error: Option<&'a str>,
    draft_notice: Option<&'a str>,
) -> Option<&'a str> {
    error.or(draft_notice)
}

/// Whether the single Issues-mode error/notice banner is visible.
#[must_use]
pub fn issues_banner_visible(error: Option<&str>, draft_notice: Option<&str>) -> bool {
    issues_banner_text(error, draft_notice).is_some()
}

/// Compute the number of rows available for the detail scroll viewport given
/// the total terminal height and conditional Issues-mode bands.
#[must_use]
pub fn issues_detail_viewport_rows(
    term_rows: usize,
    error_visible: bool,
    filter_controls_open: bool,
) -> usize {
    detail_body_viewport_rows(issues_detail_pane_rows(
        term_rows,
        error_visible,
        filter_controls_open,
    ))
}

/// Compute the default detail viewport rows when no conditional bands are open.
#[must_use]
pub fn detail_viewport_rows(term_rows: usize) -> usize {
    issues_detail_viewport_rows(term_rows, false, false)
}

/// Compute the number of rows available for the PR-mode detail scroll viewport.
///
/// @plan PLAN-20260624-PR-MODE.P11
/// @requirement REQ-PR-009
/// @pseudocode component-004 lines 156-159
///
/// PR mode reuses the same conditional bands as Issues mode (an optional error
/// banner and the filter-controls band), so the geometry is identical. This
/// thin named wrapper exists so PR-mode scroll math depends on a PR-named
/// layout prop (regression guard #37/#39: viewport height is supplied as a
/// prop, never recomputed independently inside scroll math).
#[must_use]
pub fn prs_detail_viewport_rows(
    term_rows: usize,
    error_visible: bool,
    filter_controls_open: bool,
) -> usize {
    issues_detail_viewport_rows(term_rows, error_visible, filter_controls_open)
}

/// Complete Actions detail geometry shared by state transitions and rendering.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct ActionsDetailGeometry {
    /// Wrapped display rows visible below the fixed metadata header.
    pub viewport_rows: usize,
    /// Character width used by the shared document wrapping projection.
    pub content_width: usize,
}

/// Compute Actions detail geometry from terminal dimensions and visible bands.
#[must_use]
pub fn actions_detail_geometry(
    term_cols: u16,
    term_rows: u16,
    error_visible: bool,
    filter_controls_open: bool,
) -> ActionsDetailGeometry {
    ActionsDetailGeometry {
        viewport_rows: prs_detail_viewport_rows(
            usize::from(term_rows),
            error_visible,
            filter_controls_open,
        ),
        content_width: usize::from(prs_detail_content_width(term_cols)),
    }
}

/// Compute inner content width for issue-list title lines.
#[must_use]
pub fn issue_list_content_width(term_cols: u16) -> u16 {
    term_cols.saturating_sub(ISSUES_SIDEBAR_WIDTH + ISSUE_LIST_CHROME_COLS)
}

// -----------------------------------------------------------------------------
// PR-mode pane layout (REQ-PR-006, REQ-PR-009)
//
// @plan PLAN-20260624-PR-MODE.P12
// @requirement REQ-PR-006
// @requirement REQ-PR-009
// @pseudocode component-001 lines 1-12
//
// PR mode mirrors Issues mode geometry (same 30/70 band math, same sidebar
// width, same header-row count). The PR-named wrappers exist so the PR render
// path depends on PR-named layout props (regression guard #37/#39: viewport
// height and pane rows are props, never recomputed independently inside
// components).
// -----------------------------------------------------------------------------

/// Fixed width of the repository sidebar in PR mode (mirrors Issues mode).
///
/// @plan PLAN-20260624-PR-MODE.P12
/// @requirement REQ-PR-006
/// @pseudocode component-001 lines 1-12
pub const PRS_SIDEBAR_WIDTH: u16 = LEFT_COL_WIDTH;

/// Number of fixed rows the PR detail metadata header occupies.
///
/// The header renders exactly five rows (mirroring the issue detail header so
/// the geometry matches `prs_detail_viewport_rows`):
/// 1. title (`#<number> <title>`),
/// 2. state/author/timestamps,
/// 3. branch refs + labels/assignees/milestone,
/// 4. external URL,
/// 5. a horizontal rule separator.
///
/// @plan PLAN-20260624-PR-MODE.P12
/// @requirement REQ-PR-009
/// @pseudocode component-001 lines 1-12
pub const PR_DETAIL_HEADER_ROWS: usize = DETAIL_HEADER_ROWS;

/// Horizontal chrome consumed by the PR list pane border (mirrors issue list).
///
/// @plan PLAN-20260624-PR-MODE.P12
/// @requirement REQ-PR-006
/// @pseudocode component-001 lines 1-12
pub const PR_LIST_CHROME_COLS: u16 = ISSUE_LIST_CHROME_COLS;

/// Compute the rows allocated to the PR-mode list and detail panes.
///
/// @plan PLAN-20260624-PR-MODE.P12
/// @requirement REQ-PR-006
/// @requirement REQ-PR-009
/// @pseudocode component-001 lines 1-12
///
/// PR mode reuses the same conditional bands as Issues mode (an optional error
/// banner and the filter-controls band), so the geometry is identical. This
/// thin named wrapper exists so PR-mode pane sizing depends on a PR-named
/// layout prop (regression guard #37/#39).
#[must_use]
pub fn prs_pane_rows(
    term_rows: usize,
    error_visible: bool,
    filter_controls_open: bool,
) -> (usize, usize) {
    issues_pane_rows(term_rows, error_visible, filter_controls_open)
}

/// Compute the rows allocated to the Actions run list and detail panes.
///
/// Actions currently shares the PR workspace bands, but the named boundary
/// prevents input and rendering from depending on a PR-specific API.
#[must_use]
pub fn actions_pane_rows(
    term_rows: usize,
    error_visible: bool,
    filter_controls_open: bool,
) -> (usize, usize) {
    issues_pane_rows(term_rows, error_visible, filter_controls_open)
}

/// Compute the rows allocated to the PR-mode detail pane.
///
/// @plan PLAN-20260624-PR-MODE.P12
/// @requirement REQ-PR-009
/// @pseudocode component-001 lines 1-12
#[must_use]
pub fn prs_detail_pane_rows(
    term_rows: usize,
    error_visible: bool,
    filter_controls_open: bool,
) -> usize {
    prs_pane_rows(term_rows, error_visible, filter_controls_open).1
}

/// Compute inner content width for PR-list title lines.
///
/// @plan PLAN-20260624-PR-MODE.P12
/// @requirement REQ-PR-006
/// @pseudocode component-001 lines 1-12
#[must_use]
pub fn pr_list_content_width(term_cols: u16) -> u16 {
    term_cols.saturating_sub(PRS_SIDEBAR_WIDTH + PR_LIST_CHROME_COLS)
}

/// Columns of chrome the Issues detail pane subtracts from the workspace width
/// before text is rendered: left+right border (2), left padding (1),
/// scrollbar (1), and a 2-col safety margin matching the PR detail pane.
const ISSUE_DETAIL_CONTENT_CHROME_COLS: u16 = 6;

/// Compute the inner content width available for Issues-detail text lines.
#[must_use]
pub fn issues_detail_content_width(term_cols: u16) -> u16 {
    term_cols
        .saturating_sub(ISSUES_SIDEBAR_WIDTH)
        .saturating_sub(ISSUE_DETAIL_CONTENT_CHROME_COLS)
}

/// Columns of chrome the PR detail pane subtracts from the workspace width
/// before text is rendered: left+right border (2), left padding (1),
/// scrollbar (1), and a 2-col safety margin matching `ScrollableText`'s own
/// `term_cols - 28` fallback.
///
/// @plan PLAN-20260624-PR-MODE.P14
/// @requirement REQ-PR-009
/// @pseudocode component-001 lines 1-12
const PR_DETAIL_CONTENT_CHROME_COLS: u16 = 6;

/// Compute the inner content width available for PR-detail text lines.
///
/// Subtracts the sidebar, the detail-pane border, left padding, the scrollbar
/// column, and a small safety margin so wrapped lines fit exactly where
/// `ScrollableText` renders them. Returns 0 on degenerate (very narrow)
/// terminals.
///
/// @plan PLAN-20260624-PR-MODE.P14
/// @requirement REQ-PR-009
/// @pseudocode component-001 lines 1-12
#[must_use]
pub fn prs_detail_content_width(term_cols: u16) -> u16 {
    term_cols
        .saturating_sub(PRS_SIDEBAR_WIDTH)
        .saturating_sub(PR_DETAIL_CONTENT_CHROME_COLS)
}

// -----------------------------------------------------------------------------
// PR-mode pure display seams (REQ-PR-001, REQ-PR-012, REQ-PR-013)
//
// @plan PLAN-20260624-PR-MODE.P13
// @requirement REQ-PR-001
// @requirement REQ-PR-012
// @requirement REQ-PR-013
// @pseudocode component-001 lines 1-12
//
// Pure, iocraft-free projections of the PR-mode screen layout. The screen
// components delegate to these so the rendered contract (sidebar width, two-
// column split, error-banner text) is assertable without a render harness.
// -----------------------------------------------------------------------------

/// Format an error message as the banner line rendered in PR mode.
///
/// @plan PLAN-20260624-PR-MODE.P13
/// @requirement REQ-PR-013
/// @pseudocode component-001 lines 1-12
#[must_use]
pub fn pr_error_banner_text(msg: &str) -> String {
    format!("Error: {msg}")
}

/// Error-banner line as rendered in PR mode (`None` when there is no error).
///
/// @plan PLAN-20260624-PR-MODE.P13
/// @requirement REQ-PR-013
/// @pseudocode component-001 lines 1-12
#[must_use]
pub fn pr_error_banner_line(error: Option<&str>) -> Option<String> {
    error.map(pr_error_banner_text)
}

/// PR-mode main-row column geometry: fixed sidebar width + remaining workspace width.
///
/// @plan PLAN-20260624-PR-MODE.P13
/// @requirement REQ-PR-001
/// @pseudocode component-001 lines 1-12
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PrsColumns {
    /// Fixed sidebar width in columns (== [`PRS_SIDEBAR_WIDTH`]).
    ///
    /// @plan PLAN-20260624-PR-MODE.P13
    /// @requirement REQ-PR-001
    /// @pseudocode component-001 lines 1-12
    pub sidebar_width: u16,
    /// Remaining workspace width after the fixed sidebar (flex-grow column).
    ///
    /// @plan PLAN-20260624-PR-MODE.P13
    /// @requirement REQ-PR-001
    /// @pseudocode component-001 lines 1-12
    pub workspace_width: u16,
}

/// PR-mode main-row column geometry: fixed sidebar + remaining workspace.
///
/// @plan PLAN-20260624-PR-MODE.P13
/// @requirement REQ-PR-001
/// @pseudocode component-001 lines 1-12
#[must_use]
pub fn prs_main_columns(term_cols: u16) -> PrsColumns {
    PrsColumns {
        sidebar_width: PRS_SIDEBAR_WIDTH,
        workspace_width: term_cols.saturating_sub(PRS_SIDEBAR_WIDTH),
    }
}

// -----------------------------------------------------------------------------
// Detail-pane scroll-into-view helper (#151)
//
// When the detail-pane subfocus moves to an item that is scrolled out of view,
// the scroll offset must follow so the focused item stays visible — mirroring
// how list panes keep the selected row on screen. This pure helper computes
// the minimal offset adjustment from (item line range, current offset,
// viewport height). The caller MUST clamp the result to
// `[0, max_scroll_offset]` because this helper does not know the total content
// length.
// -----------------------------------------------------------------------------

/// Compute the scroll offset that minimally reveals a content-line range.
///
/// Given an item occupying content lines `[item_start, item_end]` (inclusive),
/// the current scroll `offset`, and the `viewport_rows` height, return the
/// offset that keeps the item on-screen with minimal movement:
///
/// - If the item is already fully visible, return `offset` unchanged.
/// - If the item is entirely above the viewport, scroll up so its first line
///   sits at the top (`item_start`).
/// - If the item is entirely below or straddles the bottom edge, scroll down so
///   its last line is the bottom viewport row — unless the item is taller than
///   the viewport, in which case anchor on its first line (top) instead.
/// - If the item straddles the top edge (its tail is inside the viewport but its
///   head is scrolled off the top), scroll up to `item_start` so the whole item
///   is visible from its first line.
///
/// Returns `offset` unchanged when `viewport_rows == 0` (no viewport).
#[must_use]
pub fn reveal_range_scroll_offset(
    item_start: usize,
    item_end: usize,
    offset: usize,
    viewport_rows: usize,
) -> usize {
    if viewport_rows == 0 {
        return offset;
    }
    let last_visible = offset.saturating_add(viewport_rows).saturating_sub(1);
    // Fully visible: no movement.
    if item_start >= offset && item_end <= last_visible {
        return offset;
    }
    // Entirely above the viewport: snap the first line to the top.
    if item_end < offset {
        return item_start;
    }
    // Straddling the top edge: the item's tail is inside the viewport but its
    // head is scrolled off the top. Scroll up to reveal the whole item from
    // its first line (minimal movement that makes it fully visible).
    if item_start < offset {
        return item_start;
    }
    // Entirely below or straddling the bottom: bring the last line into view as
    // the bottom row. For an item taller than the viewport this would push the
    // top off-screen, so anchor on the first line instead.
    let anchor_bottom = item_end.saturating_sub(viewport_rows.saturating_sub(1));
    anchor_bottom.min(item_start)
}

#[cfg(test)]
#[path = "layout_tests.rs"]
mod tests;

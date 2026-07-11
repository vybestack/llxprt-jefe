//! Terminal view component - embedded PTY display.
//!
//! @plan PLAN-20260216-FIRSTVERSION-V1.P09
//! @requirement REQ-FUNC-006
//! @requirement REQ-TECH-004
//!
//! # Rendering strategy
//!
//! The PTY grid is drawn by a single low-level [`TerminalGrid`] component that
//! writes directly to the iocraft [`Canvas`]. This is deliberate: an earlier
//! implementation built one flex `Box` per terminal row and an additional nested
//! `Box`/`Text` per contiguous style-run within each row. For a dense, fully
//! styled snapshot (e.g. a colorful agent TUI where nearly every cell has a
//! distinct style) that produced thousands of nested flex nodes. Taffy's layout
//! solver degraded super-linearly and a single render could never finish within
//! a frame, starving the (single-threaded) input loop and freezing all keyboard
//! input. See issue #60.
//!
//! By collapsing the grid into one taffy leaf node, layout cost is constant and
//! independent of per-cell style churn, while per-cell foreground/background/
//! weight/underline styling is preserved by drawing straight to the canvas.

use iocraft::prelude::*;

use super::terminal_theme::{
    TerminalThemeOverride, resolve_run_colors, terminal_content_background, terminal_weight,
};
use crate::runtime::{TerminalCell, TerminalSnapshot};
use crate::selection::{SelectablePane, TextSelection, row_highlight_range};
use crate::theme::{ResolvedColors, SelectionColors, ThemeColors};

/// Props for the terminal view component.
#[derive(Default, Props)]
pub struct TerminalViewProps {
    /// Terminal snapshot (styled grid from runtime/alacritty model).
    pub snapshot: Option<TerminalSnapshot>,
    /// Whether the terminal is focused (receives input).
    pub focused: bool,
    /// Theme colors for chrome around the terminal content.
    pub colors: ThemeColors,
    /// Active text selection, if any. Selected cells are painted in
    /// inverse-video over the terminal grid for live drag-selection feedback.
    pub selection: Option<TextSelection>,
    /// Whether the selected agent has a live session (Running) even though no
    /// snapshot is currently available (e.g. the viewer has not finished
    /// attaching). When true the empty-state copy distinguishes a healthy live
    /// session from a genuinely unattached terminal (issue #160).
    pub session_live: bool,
    /// Retained scrollback history lines from the runtime cache (issue #198).
    pub history_lines: Vec<String>,
    /// Terminal scrollback offset: `None` = follow-tail (live), `Some(n)` =
    /// scrolled back `n` lines. Drives the viewport projection + indicator.
    pub terminal_history_offset: Option<usize>,
    /// When true, jefe's theme fg/bg is force-applied to the agent terminal's
    /// default (transparent) cells, while explicitly-styled cells pass through
    /// unchanged (issue #179 override toggle).
    pub override_theme: bool,
}

/// Empty-state message for the terminal pane when no snapshot is available.
///
/// Pure (iocraft-free) so it is unit-testable. A Running agent with no snapshot
/// yet gets a reassuring "session live" hint; everything else reports no
/// terminal attached.
#[must_use]
pub fn terminal_empty_message(session_live: bool) -> &'static str {
    if session_live {
        "Session live - press t to focus terminal"
    } else {
        "No terminal attached"
    }
}

/// Terminal view showing the PTY output for the attached agent.
#[component]
pub fn TerminalView(props: &TerminalViewProps) -> impl Into<AnyElement<'static>> {
    let rc = ResolvedColors::from_theme(Some(&props.colors));
    let border_style = if props.focused {
        BorderStyle::Double
    } else {
        BorderStyle::Round
    };

    let focus_hint = if props.focused {
        "F12 to unfocus"
    } else {
        "F12/t to focus"
    };

    // Build the scrollback viewport projection (issue #198). When history lines
    // are present, project history+live through the offset window. When no
    // history is available, render the live snapshot as-is.
    let default_style = crate::runtime::TerminalCellStyle {
        fg: iocraft::Color::White,
        bg: rc.bg,
        bold: false,
        dim: false,
        underline: false,
    };

    let (projected_snapshot, indicator, content_start_line) =
        if let Some(live) = props.snapshot.as_ref().filter(|s| !s.is_empty()) {
            if props.history_lines.is_empty() {
                (Some(live.clone()), None, 0)
            } else {
                // Use a large viewport so the projection fills the pane; the
                // TerminalGrid clips to the actual canvas size at draw time.
                let proj = super::terminal_viewport::build_terminal_viewport(
                    live,
                    &props.history_lines,
                    props.terminal_history_offset,
                    live.rows,
                    live.cols,
                    default_style,
                );
                (Some(proj.snapshot), proj.indicator, proj.start_line)
            }
        } else {
            (None, None, 0)
        };

    element! {
        Box(
            flex_direction: FlexDirection::Column,
            width: 100pct,
            height: 100pct,
            border_style: border_style,
            border_color: rc.border,
            background_color: rc.bg,
        ) {
            // Title with focus hint
            Box(
                flex_direction: FlexDirection::Row,
                height: 1u32,
                padding_left: 1u32,
                background_color: rc.bg,
            ) {
                Text(content: "Terminal", weight: Weight::Bold, color: rc.fg)
                Text(content: format!(" ({focus_hint})"), color: rc.dim)
            }

            // Terminal content. A single low-level canvas node draws the whole
            // grid; see module docs for why this is not a flex tree.
            //
            // The follow indicator is overlaid on the last grid row by the
            // canvas draw, NOT as a separate flex Box, so it does not consume
            // a content row (issue #198 review fix #6).
            //
            // The content-area fill is transparent (Color::Reset) when theme
            // override is OFF, so the embedded agent's default cells let the
            // host terminal's real background show through instead of jefe's
            // theme bg bleeding in (issue #179). Override ON fills with jefe's
            // theme bg. Chrome (border, title bar) always carries rc.bg.
            Box(
                flex_direction: FlexDirection::Column,
                flex_grow: 1.0_f32,
                background_color: terminal_content_background(props.override_theme, rc.bg),
            ) {
                #(if let Some(snapshot) = projected_snapshot {
                    element! {
                        TerminalGrid(
                            snapshot: snapshot,
                            selection: props.selection,
                            sel_colors: SelectionColors::from_resolved(&rc),
                            content_start_line: content_start_line,
                            indicator_text: indicator.as_ref().map(|ind| ind.text.clone()),
                            dim_color: rc.dim,
                            bg_color: rc.bg,
                            theme_override: TerminalThemeOverride {
                                enabled: props.override_theme,
                                fg: rc.fg,
                                bg: rc.bg,
                            },
                        )
                    }
                    .into_any()
                } else {
                    element! {
                        Box(background_color: terminal_content_background(
                            props.override_theme,
                            rc.bg,
                        )) {
                            Text(content: terminal_empty_message(props.session_live), color: rc.dim)
                        }
                    }
                    .into_any()
                })
            }
        }
    }
}

/// Props for the low-level terminal grid renderer.
#[derive(Props)]
struct TerminalGridProps {
    /// Styled PTY grid to draw.
    snapshot: TerminalSnapshot,
    /// Active text selection, if any. When it targets the terminal pane,
    /// selected cells are overpainted in inverse-video in [`TerminalGrid::draw`].
    selection: Option<TextSelection>,
    /// Selection foreground + background colors (inverse-video).
    sel_colors: SelectionColors,
    /// Absolute content-line index of the first viewport row (issue #198
    /// review fix #5). Selection highlight math adds this to the viewport-local
    /// row index to get the absolute content row that `row_highlight_range`
    /// expects.
    content_start_line: usize,
    /// Follow indicator text to overlay on the last grid row (issue #198
    /// review fix #6). When present, the indicator is drawn on top of the last
    /// row of the canvas WITHOUT consuming an additional content row.
    indicator_text: Option<String>,
    /// Dim color for the indicator overlay.
    dim_color: iocraft::Color,
    /// Background color for the indicator overlay.
    bg_color: iocraft::Color,
    /// Jefe-theme override for the agent's default cells (issue #179).
    theme_override: TerminalThemeOverride,
}

impl Default for TerminalGridProps {
    fn default() -> Self {
        Self {
            snapshot: TerminalSnapshot::default(),
            selection: None,
            sel_colors: SelectionColors {
                fg: Color::Reset,
                bg: Color::Reset,
            },
            content_start_line: 0,
            indicator_text: None,
            dim_color: Color::Reset,
            bg_color: Color::Reset,
            theme_override: TerminalThemeOverride::default(),
        }
    }
}

/// Low-level component that paints a [`TerminalSnapshot`] directly onto the
/// canvas as a single layout node.
///
/// This keeps the taffy node count constant (one leaf) regardless of how many
/// distinct style-runs the snapshot contains, which is the fix for the render
/// lockup described in issue #60.
struct TerminalGrid {
    snapshot: TerminalSnapshot,
    selection: Option<TextSelection>,
    sel_colors: SelectionColors,
    content_start_line: usize,
    indicator_text: Option<String>,
    dim_color: iocraft::Color,
    bg_color: iocraft::Color,
    theme_override: TerminalThemeOverride,
}

impl Default for TerminalGrid {
    fn default() -> Self {
        Self {
            snapshot: TerminalSnapshot::default(),
            selection: None,
            sel_colors: SelectionColors {
                fg: Color::Reset,
                bg: Color::Reset,
            },
            content_start_line: 0,
            indicator_text: None,
            dim_color: Color::Reset,
            bg_color: Color::Reset,
            theme_override: TerminalThemeOverride::default(),
        }
    }
}

impl Component for TerminalGrid {
    type Props<'a> = TerminalGridProps;

    fn new(_props: &Self::Props<'_>) -> Self {
        Self::default()
    }

    fn update(
        &mut self,
        props: &mut Self::Props<'_>,
        _hooks: Hooks,
        updater: &mut ComponentUpdater,
    ) {
        self.snapshot = props.snapshot.clone();
        self.selection = props.selection;
        self.sel_colors = props.sel_colors;
        self.content_start_line = props.content_start_line;
        self.indicator_text.clone_from(&props.indicator_text);
        self.dim_color = props.dim_color;
        self.bg_color = props.bg_color;
        self.theme_override = props.theme_override;

        // Fill the available space; the parent Box constrains us to the pane.
        // Build the taffy style directly so node count stays at one leaf.
        let mut style = taffy::style::Style::default();
        style.size = taffy::geometry::Size {
            width: taffy::style::Dimension::Percent(1.0),
            height: taffy::style::Dimension::Percent(1.0),
        };
        updater.set_layout_style(style);
    }

    fn draw(&mut self, drawer: &mut ComponentDrawer<'_>) {
        let layout = drawer.layout();
        // taffy reports sizes as f32; clamp to a sane non-negative integer.
        let max_rows = f32_to_usize(layout.size.height);
        let max_cols = f32_to_usize(layout.size.width);
        if max_rows == 0 || max_cols == 0 {
            return;
        }

        let mut canvas = drawer.canvas();
        paint_terminal_cells(
            &mut canvas,
            &self.snapshot,
            max_rows,
            max_cols,
            self.theme_override,
        );
        let overlay = SelectionOverlay {
            selection: self.selection,
            content_start_line: self.content_start_line,
            sel_colors: self.sel_colors,
        };
        paint_selection_overlay(&mut canvas, &self.snapshot, &overlay, max_rows, max_cols);
        paint_follow_indicator_overlay(
            &mut canvas,
            self.indicator_text.as_deref(),
            max_rows,
            max_cols,
            self.dim_color,
            self.bg_color,
        );
    }
}

/// Paint the styled terminal cells onto the canvas as style-runs.
fn paint_terminal_cells(
    canvas: &mut CanvasSubviewMut<'_>,
    snapshot: &TerminalSnapshot,
    max_rows: usize,
    max_cols: usize,
    theme_override: TerminalThemeOverride,
) {
    for (row_idx, row) in snapshot
        .cells
        .iter()
        .take(snapshot.rows.min(max_rows))
        .enumerate()
    {
        let Some(y) = canvas_coord(row_idx) else {
            continue;
        };
        for run in row_to_runs(row, max_cols) {
            let Some(x) = canvas_coord(run.start_col) else {
                continue;
            };
            let (text_color, fill_color) = resolve_run_colors(&run.style, theme_override);
            // CanvasTextStyle is #[non_exhaustive]; build via Default then set fields.
            let mut style = CanvasTextStyle::default();
            style.color = Some(text_color);
            style.weight = terminal_weight(&run.style);
            style.underline = run.style.underline;

            if let Some(fill) = fill_color {
                canvas.set_background_color(x, y, run.width, 1, fill);
            }
            canvas.set_text(x, y, &run.text, style);
        }
    }
}

/// Selection overlay context for [`paint_selection_overlay`] (fix #5).
/// Groups the selection + its content-line offset + colors so the paint
/// function stays within the clippy argument budget.
struct SelectionOverlay {
    selection: Option<TextSelection>,
    content_start_line: usize,
    sel_colors: SelectionColors,
}

/// Paint inverse-video over the selected cells of the terminal grid.
///
/// Called after [`paint_terminal_cells`] so the selection highlight overlays the
/// normal content. Only acts when a selection targets the terminal pane.
/// `content_start_line` is the absolute content-line index of the first
/// viewport row, so `row_highlight_range` receives absolute row numbers that
/// match the selection coordinate space (issue #198 review fix #5).
fn paint_selection_overlay(
    canvas: &mut CanvasSubviewMut<'_>,
    snapshot: &TerminalSnapshot,
    overlay: &SelectionOverlay,
    max_rows: usize,
    max_cols: usize,
) {
    let Some(selection) = overlay.selection else {
        return;
    };
    if selection.pane() != SelectablePane::TerminalView || selection.is_empty() {
        return;
    }
    let visible_rows = snapshot.rows.min(max_rows);
    for row_idx in 0..visible_rows {
        let absolute_row = overlay.content_start_line + row_idx;
        let Some(range) = row_highlight_range(&selection, absolute_row) else {
            continue;
        };
        let Some(y) = canvas_coord(row_idx) else {
            continue;
        };
        let row_len = snapshot
            .cells
            .get(row_idx)
            .map_or(0, |row| row.len().min(max_cols));
        let start = range.start.min(row_len);
        let end = if range.end == usize::MAX {
            row_len
        } else {
            range.end.min(row_len)
        };
        if start >= end {
            continue;
        }
        let width = end - start;
        let Some(x) = canvas_coord(start) else {
            continue;
        };
        canvas.set_background_color(x, y, width, 1, overlay.sel_colors.bg);
        // Re-draw the selected cell text in the selection fg so the glyphs stay
        // legible over the inverse-video background.
        if let Some(row) = snapshot.cells.get(row_idx) {
            let chars: String = row.iter().skip(start).take(width).map(|c| c.ch).collect();
            let mut style = CanvasTextStyle::default();
            style.color = Some(overlay.sel_colors.fg);
            canvas.set_text(x, y, &chars, style);
        }
    }
}

/// Paint the follow indicator as an overlay on the last canvas row WITHOUT
/// consuming an additional content row (issue #198 review fix #6).
///
/// When `indicator_text` is present, the last row of the canvas is overpainted
/// with the indicator text in the dim color over the bg color. This keeps the
/// terminal grid at the full viewport height — no content row is clipped.
fn paint_follow_indicator_overlay(
    canvas: &mut CanvasSubviewMut<'_>,
    indicator_text: Option<&str>,
    max_rows: usize,
    max_cols: usize,
    dim_color: iocraft::Color,
    bg_color: iocraft::Color,
) {
    let Some(text) = indicator_text else {
        return;
    };
    if max_rows == 0 || max_cols == 0 {
        return;
    }
    // Overlay on the last visible row.
    let last_row = max_rows - 1;
    let Some(y) = canvas_coord(last_row) else {
        return;
    };
    // Clear the background of the last row.
    let Some(x) = canvas_coord(0) else {
        return;
    };
    canvas.set_background_color(x, y, max_cols, 1, bg_color);
    let display_text: String = text.chars().take(max_cols).collect();
    let mut style = CanvasTextStyle::default();
    style.color = Some(dim_color);
    canvas.set_text(x, y, &display_text, style);
}

fn canvas_coord(value: usize) -> Option<isize> {
    isize::try_from(value).ok()
}

/// Convert a taffy `f32` layout dimension to a clamped, non-negative `usize`.
///
/// Negative or non-finite values collapse to `0`; implausibly large values clamp
/// to a bound far beyond any supported terminal viewport.
///
/// The bounded binary search avoids float-to-integer casts so this hot-path
/// conversion stays compliant with the no-new-clippy-allows policy.
fn f32_to_usize(value: f32) -> usize {
    const MAX_VIEWPORT_CELLS: u16 = u16::MAX;

    if !value.is_finite() || value <= 0.0 {
        return 0;
    }
    if value >= f32::from(MAX_VIEWPORT_CELLS) {
        return usize::from(MAX_VIEWPORT_CELLS);
    }

    let target = value.floor();
    let mut low = 0u16;
    let mut high = MAX_VIEWPORT_CELLS;
    while low < high {
        let mid = low + ((high - low) / 2) + 1;
        if f32::from(mid) <= target {
            low = mid;
        } else {
            high = mid - 1;
        }
    }
    usize::from(low)
}

/// A contiguous run of cells sharing the same style within a single row.
#[derive(Clone)]
struct TextRun {
    /// Column where this run begins (0-based).
    start_col: usize,
    /// Display width of the run in cells.
    width: usize,
    /// Run text.
    text: String,
    /// Shared style for every cell in the run.
    style: crate::runtime::TerminalCellStyle,
}

/// Split a styled cell row into contiguous same-style runs, clamped to `max_cols`.
///
/// Trailing all-blank runs are dropped so empty line tails don't paint
/// background fills past meaningful content.
fn row_to_runs(row: &[TerminalCell], max_cols: usize) -> Vec<TextRun> {
    if row.is_empty() || max_cols == 0 {
        return Vec::new();
    }

    let mut runs: Vec<TextRun> = Vec::new();
    let mut current_style = row[0].style;
    let mut current_text = String::new();
    let mut run_start = 0usize;
    let mut col = 0usize;

    for cell in row.iter().take(max_cols) {
        if cell.style != current_style {
            if !current_text.is_empty() {
                runs.push(TextRun {
                    start_col: run_start,
                    width: col - run_start,
                    text: std::mem::take(&mut current_text),
                    style: current_style,
                });
            }
            current_style = cell.style;
            run_start = col;
        }
        current_text.push(cell.ch);
        col += 1;
    }

    if !current_text.is_empty() {
        runs.push(TextRun {
            start_col: run_start,
            width: col - run_start,
            text: current_text,
            style: current_style,
        });
    }

    while runs
        .last()
        .is_some_and(|run| run.text.chars().all(|ch| ch == ' '))
    {
        let _ = runs.pop();
    }

    runs
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::runtime::TerminalCellStyle;
    use iocraft::Color;

    fn style(fg: u8) -> TerminalCellStyle {
        TerminalCellStyle {
            fg: Color::AnsiValue(fg),
            bg: Color::Black,
            bold: false,
            dim: false,
            underline: false,
        }
    }

    fn cell(ch: char, fg: u8) -> TerminalCell {
        TerminalCell {
            ch,
            style: style(fg),
            wide_spacer: false,
        }
    }

    #[test]
    fn empty_row_yields_no_runs() {
        assert!(row_to_runs(&[], 80).is_empty());
    }

    #[test]
    fn zero_width_yields_no_runs() {
        let row = vec![cell('a', 1)];
        assert!(row_to_runs(&row, 0).is_empty());
    }

    #[test]
    fn single_style_collapses_to_one_run() {
        let row: Vec<TerminalCell> = "hello".chars().map(|c| cell(c, 1)).collect();
        let runs = row_to_runs(&row, 80);
        assert_eq!(runs.len(), 1);
        assert_eq!(runs[0].text, "hello");
        assert_eq!(runs[0].start_col, 0);
        assert_eq!(runs[0].width, 5);
    }

    #[test]
    fn style_change_splits_runs_with_correct_columns() {
        let mut row = vec![cell('a', 1), cell('b', 1)];
        row.push(cell('c', 2));
        row.push(cell('d', 2));
        let runs = row_to_runs(&row, 80);
        assert_eq!(runs.len(), 2);
        assert_eq!(runs[0].text, "ab");
        assert_eq!(runs[0].start_col, 0);
        assert_eq!(runs[0].width, 2);
        assert_eq!(runs[1].text, "cd");
        assert_eq!(runs[1].start_col, 2);
        assert_eq!(runs[1].width, 2);
    }

    #[test]
    fn trailing_blank_run_is_trimmed() {
        let mut row: Vec<TerminalCell> = "hi".chars().map(|c| cell(c, 1)).collect();
        // Different style so blanks form their own trailing run.
        for _ in 0..5 {
            row.push(cell(' ', 2));
        }
        let runs = row_to_runs(&row, 80);
        assert_eq!(runs.len(), 1);
        assert_eq!(runs[0].text, "hi");
    }

    #[test]
    fn runs_are_clamped_to_max_cols() {
        let row: Vec<TerminalCell> = "abcdefgh".chars().map(|c| cell(c, 1)).collect();
        let runs = row_to_runs(&row, 3);
        let total: usize = runs.iter().map(|r| r.width).sum();
        assert_eq!(total, 3);
        assert_eq!(runs[0].text, "abc");
    }

    // --- terminal_empty_message (issue #160) ---

    #[test]
    fn empty_message_live_session_when_session_live() {
        assert_eq!(
            terminal_empty_message(true),
            "Session live - press t to focus terminal"
        );
    }

    #[test]
    fn empty_message_no_terminal_when_not_live() {
        assert_eq!(terminal_empty_message(false), "No terminal attached");
    }
}

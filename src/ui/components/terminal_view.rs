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

use crate::runtime::{TerminalCell, TerminalSnapshot};
use crate::theme::{ResolvedColors, ThemeColors};

/// Props for the terminal view component.
#[derive(Default, Props)]
pub struct TerminalViewProps {
    /// Terminal snapshot (styled grid from runtime/alacritty model).
    pub snapshot: Option<TerminalSnapshot>,
    /// Whether the terminal is focused (receives input).
    pub focused: bool,
    /// Theme colors for chrome around the terminal content.
    pub colors: ThemeColors,
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
            Box(
                flex_direction: FlexDirection::Column,
                flex_grow: 1.0,
                background_color: rc.bg,
            ) {
                #(if let Some(snapshot) = props.snapshot.clone() {
                    element! {
                        TerminalGrid(snapshot: snapshot)
                    }
                    .into_any()
                } else {
                    element! {
                        Box {
                            Text(content: "No terminal attached", color: rc.dim)
                        }
                    }
                    .into_any()
                })
            }
        }
    }
}

/// Props for the low-level terminal grid renderer.
#[derive(Default, Props)]
struct TerminalGridProps {
    /// Styled PTY grid to draw.
    snapshot: TerminalSnapshot,
}

/// Low-level component that paints a [`TerminalSnapshot`] directly onto the
/// canvas as a single layout node.
///
/// This keeps the taffy node count constant (one leaf) regardless of how many
/// distinct style-runs the snapshot contains, which is the fix for the render
/// lockup described in issue #60.
#[derive(Default)]
struct TerminalGrid {
    snapshot: TerminalSnapshot,
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

        for (row_idx, row) in self
            .snapshot
            .cells
            .iter()
            .take(self.snapshot.rows.min(max_rows))
            .enumerate()
        {
            for run in row_to_runs(row, max_cols) {
                // CanvasTextStyle is #[non_exhaustive]; build via Default then set fields.
                let mut style = CanvasTextStyle::default();
                style.color = Some(run.style.fg);
                style.weight = if run.style.bold {
                    Weight::Bold
                } else {
                    Weight::Normal
                };
                style.underline = run.style.underline;

                // Background is painted as a filled region under the run so that
                // per-cell background colors are preserved.
                #[allow(clippy::cast_possible_wrap)]
                canvas.set_background_color(
                    run.start_col as isize,
                    row_idx as isize,
                    run.width,
                    1,
                    run.style.bg,
                );
                #[allow(clippy::cast_possible_wrap)]
                canvas.set_text(run.start_col as isize, row_idx as isize, &run.text, style);
            }
        }
    }
}

/// Convert a taffy `f32` layout dimension to a clamped, non-negative `usize`.
///
/// Negative or non-finite values collapse to `0`; large values saturate.
#[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
fn f32_to_usize(value: f32) -> usize {
    if !value.is_finite() || value <= 0.0 {
        0
    } else {
        value.floor() as usize
    }
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
            underline: false,
        }
    }

    fn cell(ch: char, fg: u8) -> TerminalCell {
        TerminalCell {
            ch,
            style: style(fg),
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
}

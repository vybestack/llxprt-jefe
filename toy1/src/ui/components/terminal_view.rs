//! Terminal view component — renders a PTY session's screen content.

use iocraft::prelude::*;

use crate::pty::{TerminalCellStyle, TerminalSnapshot};
use crate::theme::{ResolvedColors, ThemeColors};

/// Props for the terminal view.
#[derive(Default, Props)]
pub struct TerminalViewProps {
    /// Plain lines fallback from the terminal emulator model.
    pub lines: Vec<String>,
    /// Full styled terminal snapshot.
    pub snapshot: Option<TerminalSnapshot>,
    /// Whether the terminal has input focus (F12 to toggle).
    pub focused: bool,
    /// Theme colors.
    pub colors: Option<ThemeColors>,
}

/// Renders PTY screen content inside a bordered box.
#[component]
pub fn TerminalView(props: &TerminalViewProps) -> impl Into<AnyElement<'static>> {
    let rc = ResolvedColors::from_theme(props.colors.as_ref());
    let border_style = if props.focused {
        BorderStyle::Double
    } else {
        BorderStyle::Round
    };

    let header = if props.focused {
        " Terminal (F12 to detach) ".to_owned()
    } else {
        " Terminal (F12 to attach) ".to_owned()
    };

    let fallback_snapshot = {
        let display_lines: Vec<String> = if props.lines.is_empty() {
            vec!["  (no output)".to_owned()]
        } else {
            props.lines.clone()
        };
        let cols = display_lines
            .iter()
            .map(|line| line.chars().count())
            .max()
            .unwrap_or(1)
            .max(1);
        let rows = display_lines.len().max(1);
        let mut cells = vec![
            vec![
                crate::pty::TerminalCell {
                    ch: ' ',
                    style: TerminalCellStyle {
                        fg: rc.fg,
                        bg: rc.bg,
                        bold: false,
                        underline: false,
                    }
                };
                cols
            ];
            rows
        ];
        for (row, line) in display_lines.iter().enumerate() {
            for (col, ch) in line.chars().enumerate() {
                if col < cols {
                    cells[row][col].ch = ch;
                }
            }
        }
        TerminalSnapshot { rows, cols, cells }
    };

    let snapshot = props.snapshot.clone().unwrap_or(fallback_snapshot);

    element! {
        Box(
            border_style: border_style,
            border_color: rc.border,
            background_color: rc.bg,
            width: 100pct,
            height: 100pct,
            flex_direction: FlexDirection::Column,
        ) {
            Box(height: 1u32) {
                Text(content: header, color: rc.fg, weight: Weight::Bold)
            }
            #(snapshot
                .cells
                .into_iter()
                .map(|row| {
                    element! {
                        Box(height: 1u32, width: 100pct, background_color: rc.bg) {
                            #(row_to_runs(&row)
                                .into_iter()
                                .map(|run| {
                                    let weight = if run.style.bold {
                                        Weight::Bold
                                    } else {
                                        Weight::Normal
                                    };
                                    let decoration = if run.style.underline {
                                        TextDecoration::Underline
                                    } else {
                                        TextDecoration::None
                                    };

                                    element! {
                                        Box(background_color: run.style.bg) {
                                            Text(
                                                content: run.text,
                                                color: run.style.fg,
                                                weight: weight,
                                                decoration: decoration,
                                                wrap: TextWrap::NoWrap,
                                            )
                                        }
                                    }
                                }))
                        }
                    }
                }))
        }
    }
}

#[derive(Clone)]
struct TextRun {
    text: String,
    style: TerminalCellStyle,
}

fn row_to_runs(row: &[crate::pty::TerminalCell]) -> Vec<TextRun> {
    if row.is_empty() {
        return vec![];
    }

    let mut runs: Vec<TextRun> = Vec::new();
    let mut current_style = row[0].style;
    let mut current_text = String::new();

    for cell in row {
        if cell.style != current_style {
            if !current_text.is_empty() {
                runs.push(TextRun {
                    text: std::mem::take(&mut current_text),
                    style: current_style,
                });
            }
            current_style = cell.style;
        }
        current_text.push(cell.ch);
    }

    if !current_text.is_empty() {
        runs.push(TextRun {
            text: current_text,
            style: current_style,
        });
    }

    while runs
        .last()
        .map(|run| run.text.chars().all(|ch| ch == ' '))
        .unwrap_or(false)
    {
        let _ = runs.pop();
    }

    runs
}

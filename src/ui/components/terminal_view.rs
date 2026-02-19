//! Terminal view component - embedded PTY display.
//!
//! @plan PLAN-20260216-FIRSTVERSION-V1.P09
//! @requirement REQ-FUNC-006
//! @requirement REQ-TECH-004

use iocraft::prelude::*;

use crate::runtime::{TerminalCell, TerminalCellStyle, TerminalSnapshot};
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
        "F12/t to unfocus"
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

            // Terminal content
            Box(
                flex_direction: FlexDirection::Column,
                flex_grow: 1.0,
                background_color: rc.bg,
            ) {
                #(if let Some(snapshot) = &props.snapshot {
                    element! {
                        Box(flex_direction: FlexDirection::Column) {
                            #(snapshot
                                .cells
                                .iter()
                                .take(snapshot.rows)
                                .map(|row| {
                                    element! {
                                        Box(height: 1u32, width: 100pct, background_color: rc.bg) {
                                            #(row_to_runs(row)
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
                } else {
                    element! {
                        Box {
                            Text(content: "No terminal attached", color: rc.dim)
                        }
                    }
                })
            }
        }
    }
}

#[derive(Clone)]
struct TextRun {
    text: String,
    style: TerminalCellStyle,
}

fn row_to_runs(row: &[TerminalCell]) -> Vec<TextRun> {
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

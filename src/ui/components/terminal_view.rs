//! Terminal view component - embedded PTY display.
//!
//! @plan PLAN-20260216-FIRSTVERSION-V1.P09
//! @requirement REQ-FUNC-006
//! @requirement REQ-TECH-004

use iocraft::prelude::*;

use crate::runtime::TerminalSnapshot;
use crate::theme::{ResolvedColors, ThemeColors};

/// Props for the terminal view component.
#[derive(Default, Props)]
pub struct TerminalViewProps {
    /// Terminal snapshot (grid of characters).
    pub snapshot: Option<TerminalSnapshot>,
    /// Whether the terminal is focused (receives input).
    pub focused: bool,
    /// Theme colors.
    pub colors: ThemeColors,
}

/// Terminal view showing the PTY output for the attached agent.
#[component]
pub fn TerminalView(props: &TerminalViewProps) -> impl Into<AnyElement<'static>> {
    let rc = ResolvedColors::from_theme(Some(&props.colors));
    let border_color = if props.focused { rc.border_focused } else { rc.border };

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
            border_style: BorderStyle::Round,
            border_color: border_color,
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
                    // Render terminal lines from snapshot
                    element! {
                        Box(flex_direction: FlexDirection::Column) {
                            #(snapshot.lines.iter().take(snapshot.rows as usize).map(|line| {
                                let text: String = line.iter().collect();
                                element! {
                                    Text(content: text, color: rc.fg)
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

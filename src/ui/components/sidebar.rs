//! Sidebar component - repository list.
//!
//! @plan PLAN-20260216-FIRSTVERSION-V1.P09
//! @requirement REQ-FUNC-002

use iocraft::prelude::*;

use crate::domain::Repository;
use crate::selection::{SelectablePane, TextSelection, row_highlight_range};
use crate::theme::{ResolvedColors, ThemeColors};

/// Props for the sidebar component.
#[derive(Default, Props)]
pub struct SidebarProps {
    /// List of repositories.
    pub repositories: Vec<Repository>,
    /// Visible agent counts per repository (parallel to `repositories`).
    pub agent_counts: Vec<usize>,
    /// Currently selected repository index.
    pub selected: usize,
    /// Whether this pane is focused.
    pub focused: bool,
    /// Visible index of a grabbed repository (dashboard reorder indicator).
    pub grabbed: Option<usize>,
    /// Theme colors.
    pub colors: ThemeColors,
    /// Active text selection, if any (and if it targets this pane). Selected
    /// rows are painted in inverse video for live drag-selection feedback.
    pub selection: Option<TextSelection>,
}

/// Sidebar showing the list of repositories.
#[component]
pub fn Sidebar(props: &SidebarProps) -> impl Into<AnyElement<'static>> {
    let rc = ResolvedColors::from_theme(Some(&props.colors));
    let border_style = if props.focused {
        BorderStyle::Double
    } else {
        BorderStyle::Round
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
            // Title
            Box(height: 1u32, padding_left: 1u32) {
                Text(content: "Repositories", weight: Weight::Bold, color: rc.fg)
            }

            // Repository list
            Box(
                flex_direction: FlexDirection::Column,
                flex_grow: 1.0,
                padding: 1u32,
                background_color: rc.bg,
            ) {
                #(props.repositories.iter().enumerate().map(|(i, repo)| {
                    let selected = i == props.selected;
                    let grabbed = props.grabbed.is_some_and(|idx| idx == i);
                    let prefix = if grabbed {
                        "\u{2195} "
                    } else if selected {
                        "> "
                    } else {
                        "  "
                    };
                    let agent_count = props.agent_counts.get(i).copied()
                        .unwrap_or(repo.agent_ids.len());
                    let label = format!("{}{} ({})", prefix, repo.name, agent_count);
                    let highlighted = props.selection.as_ref()
                        .filter(|s| s.pane() == SelectablePane::Sidebar)
                        .and_then(|s| row_highlight_range(s, i))
                        .is_some();
                    let row_bg = if highlighted || selected { rc.sel_bg } else { Color::Reset };
                    let fg = if highlighted || selected { rc.sel_fg } else { rc.fg };
                    let weight = if selected { Weight::Bold } else { Weight::Normal };
                    element! {
                        Box(height: 1u32, background_color: row_bg) {
                            Text(content: label, color: fg, weight: weight)
                        }
                    }
                    .into_any()
                }))
            }
        }
    }
}

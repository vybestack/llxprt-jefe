//! Sidebar component - repository list.
//!
//! @plan PLAN-20260216-FIRSTVERSION-V1.P09
//! @requirement REQ-FUNC-002

use iocraft::prelude::*;

use crate::domain::Repository;
use crate::theme::{ResolvedColors, ThemeColors};

/// Props for the sidebar component.
#[derive(Default, Props)]
pub struct SidebarProps {
    /// List of repositories.
    pub repositories: Vec<Repository>,
    /// Currently selected repository index.
    pub selected: usize,
    /// Whether this pane is focused.
    pub focused: bool,
    /// Theme colors.
    pub colors: ThemeColors,
}

/// Sidebar showing the list of repositories.
#[component]
pub fn Sidebar(props: &SidebarProps) -> impl Into<AnyElement<'static>> {
    let rc = ResolvedColors::from_theme(Some(&props.colors));
    let border_color = if props.focused {
        rc.border_focused
    } else {
        rc.border
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
                    let prefix = if selected { "> " } else { "  " };
                    let agent_count = repo.agent_ids.len();
                    element! {
                        Text(
                            content: format!("{}{} ({})", prefix, repo.name, agent_count),
                            color: if selected { rc.bright } else { rc.dim },
                        )
                    }
                }))
            }
        }
    }
}

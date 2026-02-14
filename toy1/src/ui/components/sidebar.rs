//! Repository sidebar component.

use iocraft::prelude::*;

use crate::data::Repository;
use crate::theme::{ResolvedColors, ThemeColors};

/// Props for the repository sidebar.
#[derive(Default, Props)]
pub struct SidebarProps {
    /// The list of repositories to display.
    pub repositories: Vec<Repository>,
    /// Index of the currently selected repository.
    pub selected: usize,
    /// Whether this pane is focused.
    pub focused: bool,
    /// Theme colors.
    pub colors: Option<ThemeColors>,
}

/// Left-side repository list sidebar.
#[component]
pub fn Sidebar(props: &SidebarProps) -> impl Into<AnyElement<'static>> {
    let rc = ResolvedColors::from_theme(props.colors.as_ref());
    let border_style = if props.focused { BorderStyle::Double } else { BorderStyle::Round };

    let selected = props.selected;
    let repositories: Vec<(String, bool)> = props
        .repositories
        .iter()
        .enumerate()
        .map(|(i, repo)| {
            let is_selected = i == selected;
            let indicator = if is_selected { " \u{25b8} " } else { "   " };
            let line = format!("{}{} ({})", indicator, repo.name, repo.agents.len());
            (line, is_selected)
        })
        .collect();

    element! {
        Box(
            border_style: border_style,
            border_color: rc.border,
            background_color: rc.bg,
            width: 100pct,
            height: 100pct,
            flex_direction: FlexDirection::Column,
            padding_left: 1i32,
            padding_right: 1i32,
        ) {
            Box(height: 1u32) {
                Text(content: " Repositories".to_owned(), color: rc.fg, weight: Weight::Bold)
            }
            #(repositories.into_iter().map(|(line, is_sel): (String, bool)| {
                if is_sel {
                    element! {
                        Box(height: 1u32, background_color: rc.sel_bg) {
                            Text(content: line, color: rc.sel_fg, weight: Weight::Bold)
                        }
                    }
                } else {
                    element! {
                        Box(height: 1u32) {
                            Text(content: line, color: rc.fg)
                        }
                    }
                }
            }))
        }
    }
}

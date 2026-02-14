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
    /// Show an "All" entry at position 0 (for split mode filtering).
    pub show_all: bool,
}

/// Left-side repository list sidebar.
#[component]
pub fn Sidebar(props: &SidebarProps) -> impl Into<AnyElement<'static>> {
    let rc = ResolvedColors::from_theme(props.colors.as_ref());
    let border_style = if props.focused { BorderStyle::Double } else { BorderStyle::Round };

    let selected = props.selected;
    let show_all = props.show_all;

    // Build row list: optionally "All" at index 0, then repos.
    let mut rows: Vec<(String, bool)> = Vec::new();
    if show_all {
        let is_sel = selected == 0;
        let indicator = if is_sel { " \u{25b8} " } else { "   " };
        let total: usize = props.repositories.iter().map(|r| r.agents.len()).sum();
        rows.push((format!("{}All ({})", indicator, total), is_sel));
    }
    for (i, repo) in props.repositories.iter().enumerate() {
        let cursor_idx = if show_all { i + 1 } else { i };
        let is_sel = cursor_idx == selected;
        let indicator = if is_sel { " \u{25b8} " } else { "   " };
        rows.push((format!("{}{} ({})", indicator, repo.name, repo.agents.len()), is_sel));
    }

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
            #(rows.into_iter().map(|(line, is_sel): (String, bool)| {
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

//! Repository-sidebar projection onto the shared selectable-list component.

use iocraft::prelude::*;

use crate::domain::Repository;
use crate::list_viewport::{ListGeometry, ListViewport, PaneRows, RowsPerItem};
use crate::selection::{SelectablePane, TextSelection};
use crate::theme::ThemeColors;
use crate::ui::components::selectable_list::{
    ListBorder, SelectableListProps, SelectableRow, SelectableSpan, SelectionStyle, SpanColor,
    selectable_list_element,
};

/// Props for the sidebar component.
#[derive(Default, Props)]
pub struct SidebarProps {
    pub repositories: Vec<Repository>,
    pub agent_counts: Vec<usize>,
    pub selected: usize,
    pub focused: bool,
    pub grabbed: Option<usize>,
    /// Full sidebar height, including border, title, and content padding.
    pub pane_rows: u16,
    /// Row width after border and horizontal content padding.
    pub content_width: u16,
    pub colors: ThemeColors,
    pub selection: Option<TextSelection>,
}

/// Project repository rows with the same trailing-edge follow policy as every
/// other selectable list.
#[must_use]
pub fn sidebar_list_props(props: &SidebarProps) -> SelectableListProps {
    let geometry = ListGeometry::bordered_padded(RowsPerItem::new(1));
    let viewport = ListViewport::uniform(
        props.repositories.len(),
        Some(props.selected),
        geometry.content_rows(PaneRows::new(usize::from(props.pane_rows))),
        RowsPerItem::new(1),
    );
    let first_visible = viewport.first_visible_item();
    let rows = props.repositories[viewport.visible_range()]
        .iter()
        .enumerate()
        .map(|(window_index, repository)| {
            let absolute_index = first_visible + window_index;
            let selected = absolute_index == props.selected;
            let grabbed = props.grabbed == Some(absolute_index);
            let prefix = if grabbed {
                "\u{2195} "
            } else if selected {
                "> "
            } else {
                "  "
            };
            let agent_count = props
                .agent_counts
                .get(absolute_index)
                .copied()
                .unwrap_or(repository.agent_ids.len());
            SelectableRow {
                source_index: absolute_index,
                spans: vec![SelectableSpan {
                    text: format!("{prefix}{} ({agent_count})", repository.name),
                    color: SpanColor::Themed,
                }],
                meta_line: Some(String::new()),
                is_selected: selected,
            }
        })
        .collect();

    SelectableListProps {
        title: "Repositories".to_owned(),
        rows,
        focused: props.focused,
        empty_message: None,
        colors: props.colors.clone(),
        selection: props.selection,
        pane: SelectablePane::Sidebar,
        border: ListBorder::DoubleOnFocus,
        content_padding: true,
        selection_style: SelectionStyle::BoldSelected,
        content_width: usize::from(props.content_width),
    }
}

/// Sidebar showing the windowed repository list.
#[component]
pub fn Sidebar(props: &SidebarProps) -> impl Into<AnyElement<'static>> {
    selectable_list_element(sidebar_list_props(props))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::RepositoryId;
    use std::path::PathBuf;

    fn repository(index: usize) -> Repository {
        Repository::new(
            RepositoryId(format!("repo-{index}")),
            format!("Repository {index}"),
            format!("repo-{index}"),
            PathBuf::from("/tmp"),
        )
    }

    #[test]
    fn twenty_five_row_sidebar_keeps_twenty_fifth_repository_visible() {
        let props = SidebarProps {
            repositories: (0..25).map(repository).collect(),
            selected: 24,
            pane_rows: 25,
            content_width: 20,
            ..SidebarProps::default()
        };
        let projected = sidebar_list_props(&props);

        assert_eq!(projected.rows.len(), 20);
        assert_eq!(projected.rows.first().map(|row| row.source_index), Some(5));
        assert_eq!(projected.rows.last().map(|row| row.source_index), Some(24));
        assert!(projected.rows.last().is_some_and(|row| row.is_selected));
        assert!(projected.rows.first().is_some_and(|row| {
            row.spans
                .iter()
                .any(|span| span.text.contains("Repository 5"))
        }));
        let lines = crate::ui::components::selectable_list::projected_content_lines(&projected);
        assert_eq!(lines.first().map(|line| line.source_index), Some(5));
        assert_eq!(lines.last().map(|line| line.source_index), Some(24));
        assert!(
            lines
                .last()
                .is_some_and(|line| line.text.contains("Repository 24"))
        );
    }

    #[test]
    fn split_sized_sidebar_windows_to_selected_unicode_repository() {
        let props = SidebarProps {
            repositories: (0..30)
                .map(|index| {
                    let mut repo = repository(index);
                    repo.name = format!("１２長いリポジトリ名-{index}");
                    repo
                })
                .collect(),
            selected: 29,
            grabbed: Some(29),
            pane_rows: 18,
            content_width: 12,
            ..SidebarProps::default()
        };
        let projected = sidebar_list_props(&props);

        assert_eq!(projected.rows.len(), 13);
        assert_eq!(projected.content_width, 12);
        assert!(projected.rows.last().is_some_and(|row| {
            row.is_selected
                && row
                    .spans
                    .iter()
                    .any(|span| span.text.contains("１２長いリポジトリ名-29"))
        }));
    }
}

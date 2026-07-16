//! Issue list pane projection for the generic [`SelectableList`] component.
//!
//! The issue list pane used to have its own iocraft `IssueList` component; the
//! rendering is now owned by [`crate::ui::components::SelectableList`]. This
//! module keeps the pure projection ([`issue_list_visible_rows`]) plus the
//! [`issue_list_props`] wrapper that maps the projected rows into
//! [`crate::ui::components::SelectableRow`]s.
//!
//! @plan PLAN-20260329-ISSUES-MODE.P12
//! @plan PLAN-20260329-ISSUES-MODE.P14
//! @requirement REQ-ISS-006

use unicode_width::UnicodeWidthStr;

use crate::domain::{Issue, IssueState};
use crate::list_viewport::{ListGeometry, ListViewport, PaneRows, RowsPerItem};
use crate::selection::{SelectablePane, TextSelection};
use crate::theme::ThemeColors;
use crate::ui::components::selectable_list::{
    ListBorder, SelectableListProps, SelectableRow, SelectableSpan, SelectionStyle, SpanColor,
};

/// Issue list density variant.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum IssueListLayout {
    /// Show title and metadata for each issue.
    #[default]
    Full,
    /// Show only the title row for each issue.
    Compact,
}

impl IssueListLayout {
    fn is_compact(self) -> bool {
        matches!(self, Self::Compact)
    }
}

/// Projected issue-list row exactly as the component renders it.
/// @plan PLAN-20260630-ISSUES-REGRESSION.P01
/// @requirement REQ-ISS-006
/// @pseudocode component-001 lines 1-12
pub struct IssueListRowView {
    /// Absolute issue index represented by this visible row.
    pub source_index: usize,
    /// Title line: prefix plus issue number and truncated title.
    pub title_line: String,
    /// Metadata line: state, author, updated timestamp, counts, assignees, labels.
    pub meta_line: String,
    /// Whether this row is the selected row.
    pub is_selected: bool,
}

/// Build the title line for an issue-list row.
/// @plan PLAN-20260630-ISSUES-REGRESSION.P01
/// @requirement REQ-ISS-006
/// @pseudocode component-001 lines 1-12
fn build_title_line(issue: &Issue, prefix: &str, available_width: Option<u16>) -> String {
    let number_prefix = format!("{prefix}#{} ", issue.number);
    let title = match available_width {
        Some(width) => {
            let used = UnicodeWidthStr::width(number_prefix.as_str());
            let budget = (width as usize).saturating_sub(used);
            crate::ui::util::truncate_with_ellipsis(&issue.title, budget)
        }
        None => issue.title.clone(),
    };
    format!("{number_prefix}{title}")
}

/// Build the metadata line for an issue-list row.
/// @plan PLAN-20260630-ISSUES-REGRESSION.P01
/// @requirement REQ-ISS-006
/// @pseudocode component-001 lines 1-12
fn build_meta_line(issue: &Issue) -> String {
    let state_tag = match issue.state {
        IssueState::Open => "OPEN",
        IssueState::Closed => "CLSD",
    };
    let mut meta_parts = vec![
        state_tag.to_string(),
        format!("@{}", issue.author_login),
        format!("updated:{}", issue.updated_at),
    ];
    if issue.comment_count > 0 {
        meta_parts.push(format!("{} comments", issue.comment_count));
    }
    if !issue.assignee_summary.is_empty() {
        meta_parts.push(format!("assigned:{}", issue.assignee_summary));
    }
    if !issue.labels_summary.is_empty() {
        meta_parts.push(format!("[{}]", issue.labels_summary));
    }
    format!("     {}", meta_parts.join("  "))
}

/// Pure projection of the visible issue rows. The component renders this same
/// projection, so selection-follow behavior is unit-testable outside iocraft.
/// @plan PLAN-20260630-ISSUES-REGRESSION.P01
/// @requirement REQ-ISS-006
/// @pseudocode component-001 lines 1-12
pub fn issue_list_visible_rows(
    issues: &[Issue],
    selected_index: Option<usize>,
    list_pane_rows: u16,
    layout: IssueListLayout,
    available_width: Option<u16>,
) -> Vec<IssueListRowView> {
    let rows_per_item = RowsPerItem::new(if layout.is_compact() { 1 } else { 2 });
    let geometry = ListGeometry::bordered(rows_per_item);
    let viewport = ListViewport::uniform(
        issues.len(),
        selected_index,
        geometry.content_rows(PaneRows::new(usize::from(list_pane_rows))),
        rows_per_item,
    );
    let first_visible = viewport.first_visible_item();
    issues[viewport.visible_range()]
        .iter()
        .enumerate()
        .map(|(window_i, issue)| {
            let is_selected = selected_index == Some(first_visible + window_i);
            let prefix = if is_selected { "> " } else { "  " };
            IssueListRowView {
                source_index: first_visible + window_i,
                title_line: build_title_line(issue, prefix, available_width),
                meta_line: if layout.is_compact() {
                    String::new()
                } else {
                    build_meta_line(issue)
                },
                is_selected,
            }
        })
        .collect()
}

/// Map already-windowed [`IssueListRowView`]s into [`SelectableRow`]s for the
/// generic [`crate::ui::components::SelectableList`]. The title line becomes a
/// single themed span; the meta line is carried through (empty string in
/// compact mode signals a single-line row). Takes the views by value so the
/// owned strings move into the spans without per-row clones.
fn to_selectable_rows(views: Vec<IssueListRowView>) -> Vec<SelectableRow> {
    views
        .into_iter()
        .map(|v| SelectableRow {
            source_index: v.source_index,
            spans: vec![SelectableSpan {
                text: v.title_line,
                color: SpanColor::Themed,
            }],
            meta_line: Some(v.meta_line),
            is_selected: v.is_selected,
        })
        .collect()
}

/// Windowing/geometry inputs for [`issue_list_props`].
///
/// Bundles the parameters that [`issue_list_visible_rows`] needs to compute the
/// visible window. Keeping them in a struct keeps [`issue_list_props`] under the
/// argument-count limit.
///
/// @plan PLAN-20260630-ISSUES-REGRESSION.P01
/// @requirement REQ-ISS-006
#[derive(Clone, Copy, Debug)]
pub struct IssueListWindow {
    /// Currently selected issue index.
    pub selected_index: Option<usize>,
    /// Issue-list pane height in rows, including border and title chrome.
    pub list_pane_rows: u16,
    /// List density variant.
    pub layout: IssueListLayout,
    /// Available content width (in terminal columns) for title truncation.
    pub available_width: Option<u16>,
}

/// Build [`SelectableListProps`] for the issue list pane.
///
/// Calls the unchanged [`issue_list_visible_rows`] projection and maps each
/// [`IssueListRowView`] into a [`SelectableRow`]. The empty/loading message is
/// computed by the caller via [`issue_list_status_message`] and passed in as
/// `empty_message`.
///
/// @plan PLAN-20260630-ISSUES-REGRESSION.P01
/// @requirement REQ-ISS-006
#[must_use]
pub fn issue_list_props(
    issues: &[Issue],
    window: IssueListWindow,
    focused: bool,
    empty_message: Option<&str>,
    colors: ThemeColors,
    selection: Option<TextSelection>,
) -> SelectableListProps {
    let rows = issue_list_visible_rows(
        issues,
        window.selected_index,
        window.list_pane_rows,
        window.layout,
        window.available_width,
    );
    SelectableListProps {
        title: "Issues".to_string(),
        rows: to_selectable_rows(rows),
        focused,
        empty_message: empty_message.map(String::from),
        colors,
        selection,
        pane: SelectablePane::IssueList,
        border: ListBorder::DoubleOnFocus,
        content_padding: false,
        selection_style: SelectionStyle::BoldSelected,
        content_width: window
            .available_width
            .map_or_else(|| usize::from(u16::MAX), usize::from),
    }
}

/// Loading/empty status line for the issue list. Returns `None` when rows show.
/// @plan PLAN-20260630-ISSUES-REGRESSION.P01
/// @requirement REQ-ISS-006
/// @pseudocode component-001 lines 1-12
#[must_use]
pub fn issue_list_status_message(
    loading: bool,
    is_empty: bool,
    has_filters: bool,
) -> Option<&'static str> {
    if loading {
        Some("Loading issues...")
    } else if is_empty {
        Some(if has_filters {
            "No issues match filters"
        } else {
            "No issues found"
        })
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::{IssueListLayout, issue_list_visible_rows};
    use crate::domain::{Issue, IssueState};
    use unicode_width::UnicodeWidthStr;

    fn issue(number: u64) -> Issue {
        Issue {
            number,
            node_id: String::new(),
            title: format!("Issue {number}"),
            state: IssueState::Open,
            author_login: "octocat".to_string(),
            updated_at: "2026-06-30".to_string(),
            assignee_summary: String::new(),
            labels_summary: String::new(),
            assignees: Vec::new(),
            labels: Vec::new(),
            issue_type: String::new(),
            milestone: String::new(),
            module: String::new(),
            comment_count: 0,
            body: String::new(),
            state_reason: None,
        }
    }

    #[test]
    fn full_width_prefix_display_width_is_counted_in_title_budget() {
        let number_prefix = "  #１２ ";
        let title = crate::ui::util::truncate_with_ellipsis(
            "abcdef",
            8usize.saturating_sub(UnicodeWidthStr::width(number_prefix)),
        );
        let line = format!("{number_prefix}{title}");
        assert_eq!(UnicodeWidthStr::width(line.as_str()), 8);
    }

    #[test]
    fn visible_rows_follow_selection_below_viewport() {
        let issues: Vec<Issue> = (1..=12).map(issue).collect();
        let rows = issue_list_visible_rows(&issues, Some(8), 8, IssueListLayout::Compact, Some(40));

        assert_eq!(rows.len(), 5);
        assert!(rows[0].title_line.contains("#5 "));
        assert!(rows[4].title_line.contains("> #9 "));
        assert!(rows[4].is_selected);
        assert!(rows.iter().all(|row| !row.title_line.contains("#1 ")));
    }

    #[test]
    fn visible_rows_follow_selection_to_last_page_without_overscroll() {
        let issues: Vec<Issue> = (1..=12).map(issue).collect();
        let rows =
            issue_list_visible_rows(&issues, Some(11), 8, IssueListLayout::Compact, Some(40));

        assert!(rows[0].title_line.contains("#8 "));
        assert!(rows[4].title_line.contains("> #12 "));
        assert!(rows[4].is_selected);
    }

    #[test]
    fn visible_rows_subtract_container_chrome_from_pane_height() {
        let issues: Vec<Issue> = (1..=12).map(issue).collect();
        let rows = issue_list_visible_rows(&issues, Some(8), 5, IssueListLayout::Compact, Some(40));

        assert_eq!(rows.len(), 2);
        assert!(rows[0].title_line.contains("#8 "));
        assert!(rows[1].title_line.contains("> #9 "));
        assert!(rows[1].is_selected);
    }

    #[test]
    fn full_layout_counts_two_terminal_rows_per_issue() {
        let issues: Vec<Issue> = (1..=12).map(issue).collect();
        let rows = issue_list_visible_rows(&issues, Some(8), 9, IssueListLayout::Full, Some(40));

        assert_eq!(rows.len(), 3);
        assert!(rows[0].title_line.contains("#7 "));
        assert!(rows[2].title_line.contains("> #9 "));
        assert!(rows[2].is_selected);
    }
}

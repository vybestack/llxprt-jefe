//! Issue list pane component.
//! @plan PLAN-20260329-ISSUES-MODE.P12
//! @plan PLAN-20260329-ISSUES-MODE.P14
//! @requirement REQ-ISS-006

use iocraft::prelude::*;
use unicode_width::{UnicodeWidthChar, UnicodeWidthStr};

use crate::domain::{Issue, IssueState};
use crate::theme::{ResolvedColors, ThemeColors};

/// Ellipsis character appended when a title is truncated.
const ELLIPSIS: char = '…';

/// Rows consumed by the bordered list container and title before item content.
const LIST_CHROME_ROWS: u16 = 3;
/// Truncate `text` to fit within `max_width` terminal columns, appending an
/// ellipsis when truncation occurs.
///
/// Uses character boundaries and Unicode display width so multi-byte characters
/// are never split and wide characters are accounted for.
fn truncate_title(text: &str, max_width: usize) -> String {
    if max_width == 0 {
        return String::new();
    }
    if UnicodeWidthStr::width(text) <= max_width {
        return text.to_string();
    }

    let ellipsis_width = ELLIPSIS.width().unwrap_or(1);
    if max_width <= ellipsis_width {
        return ELLIPSIS.to_string();
    }

    let content_width = max_width - ellipsis_width;
    let mut used = 0usize;
    let mut result = String::new();
    for ch in text.chars() {
        let width = ch.width().unwrap_or(0);
        if used + width > content_width {
            break;
        }
        used += width;
        result.push(ch);
    }
    result.push(ELLIPSIS);
    result
}

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

/// Props for the issue list pane.
#[derive(Default, Props)]
pub struct IssueListProps {
    /// Issues to display.
    pub issues: Vec<Issue>,
    /// Currently selected index.
    pub selected_index: Option<usize>,
    /// Issue-list pane height in rows, including border and title chrome.
    pub list_pane_rows: u16,
    /// Whether this pane is focused.
    pub focused: bool,
    /// Whether issues are loading.
    pub loading: bool,
    /// Whether filters are active (affects empty-state message).
    pub has_filters: bool,
    /// List density variant.
    pub layout: IssueListLayout,
    /// Theme colors.
    pub colors: ThemeColors,
    /// Available content width (in terminal columns) for title truncation.
    ///
    /// When provided, long issue titles are truncated with an ellipsis to fit.
    pub available_width: Option<u16>,
}

/// Projected issue-list row exactly as the component renders it.
/// @plan PLAN-20260630-ISSUES-REGRESSION.P01
/// @requirement REQ-ISS-006
/// @pseudocode component-001 lines 1-12
pub struct IssueListRowView {
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
            truncate_title(&issue.title, budget)
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
    let rows_per_item = if layout.is_compact() { 1 } else { 2 };
    let viewport =
        (list_pane_rows.saturating_sub(LIST_CHROME_ROWS) / rows_per_item).max(1) as usize;
    let first_visible = crate::layout::list_first_visible_index(
        selected_index.unwrap_or(0),
        issues.len(),
        viewport,
    );
    crate::layout::list_visible_window(issues, selected_index.unwrap_or(0), viewport)
        .iter()
        .enumerate()
        .map(|(window_i, issue)| {
            let is_selected = selected_index == Some(first_visible + window_i);
            let prefix = if is_selected { "> " } else { "  " };
            IssueListRowView {
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

/// Loading/empty status line for the issue list. Returns `None` when rows show.
/// @plan PLAN-20260630-ISSUES-REGRESSION.P01
/// @requirement REQ-ISS-006
/// @pseudocode component-001 lines 1-12
fn issue_list_status_message(
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

/// Issue list pane — renders issues with selection highlight, loading, and empty states.
/// @plan PLAN-20260329-ISSUES-MODE.P14
/// @requirement REQ-ISS-006
#[component]
pub fn IssueList(props: &IssueListProps) -> impl Into<AnyElement<'static>> {
    let rc = ResolvedColors::from_theme(Some(&props.colors));
    let border_style = if props.focused {
        BorderStyle::Double
    } else {
        BorderStyle::Round
    };
    let status_msg =
        issue_list_status_message(props.loading, props.issues.is_empty(), props.has_filters);

    element! {
        Box(
            flex_direction: FlexDirection::Column,
            width: 100pct,
            height: 100pct,
            border_style: border_style,
            border_color: rc.border,
            background_color: rc.bg,
        ) {
            Box(height: 1u32, padding_left: 1u32) {
                Text(content: "Issues", weight: Weight::Bold, color: rc.fg)
            }
            Box(
                flex_direction: FlexDirection::Column,
                flex_grow: 1.0,
                background_color: rc.bg,
            ) {
                #(if let Some(msg) = status_msg {
                    vec![element! {
                        Box(padding_left: 1u32, height: 1u32) {
                            Text(content: msg, color: rc.dim)
                        }
                    }]
                } else {
                    let projected = issue_list_visible_rows(
                        &props.issues,
                        props.selected_index,
                        props.list_pane_rows,
                        props.layout,
                        props.available_width,
                    );
                    projected.iter().map(|view| {
                        let title_line = view.title_line.as_str();
                        let meta_line = view.meta_line.as_str();
                        if view.is_selected {
                            if props.layout.is_compact() {
                                element! {
                                    Box(height: 1u32, background_color: rc.sel_bg) {
                                        Text(
                                            content: title_line,
                                            color: rc.sel_fg,
                                            weight: Weight::Bold,
                                        )
                                    }
                                }
                            } else {
                                element! {
                                    Box(flex_direction: FlexDirection::Column) {
                                        Box(height: 1u32, background_color: rc.sel_bg) {
                                            Text(
                                                content: title_line,
                                                color: rc.sel_fg,
                                                weight: Weight::Bold,
                                            )
                                        }
                                        Box(height: 1u32, background_color: rc.sel_bg) {
                                            Text(content: meta_line, color: rc.sel_fg)
                                        }
                                    }
                                }
                            }
                        } else if props.layout.is_compact() {
                            element! {
                                Box(height: 1u32) {
                                    Text(content: title_line, color: rc.fg)
                                }
                            }
                        } else {
                            element! {
                                Box(flex_direction: FlexDirection::Column) {
                                    Box(height: 1u32) {
                                        Text(content: title_line, color: rc.fg)
                                    }
                                    Box(height: 1u32) {
                                        Text(content: meta_line, color: rc.dim)
                                    }
                                }
                            }
                        }
                    }).collect()
                })
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{IssueListLayout, issue_list_visible_rows, truncate_title};
    use crate::domain::{Issue, IssueState};
    use unicode_width::UnicodeWidthStr;

    fn issue(number: u64) -> Issue {
        Issue {
            number,
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
        }
    }

    #[test]
    fn short_title_is_returned_unchanged() {
        assert_eq!(truncate_title("hello", 10), "hello");
    }

    #[test]
    fn long_title_is_truncated_with_ellipsis() {
        let result = truncate_title("a very long title that exceeds the budget", 10);
        assert!(result.ends_with('\u{2026}'));
        assert_eq!(UnicodeWidthStr::width(result.as_str()), 10);
    }

    #[test]
    fn exact_fit_title_is_not_truncated() {
        assert_eq!(truncate_title("exact", 5), "exact");
    }

    #[test]
    fn unicode_title_truncates_on_character_boundary() {
        let title = "\u{1F600}\u{1F601}\u{1F602}\u{1F603}\u{1F604}\u{1F605}\u{1F606}\u{1F607}\u{1F608}\u{1F609}";
        let result = truncate_title(title, 5);
        assert!(UnicodeWidthStr::width(result.as_str()) <= 5);
        assert!(result.ends_with('\u{2026}'));
        assert!(result.chars().next().is_some());
    }

    #[test]
    fn one_column_budget_returns_ellipsis() {
        assert_eq!(truncate_title("abcdef", 1), "…");
    }

    #[test]
    fn full_width_prefix_display_width_is_counted_in_title_budget() {
        let number_prefix = "  #１２ ";
        let title = truncate_title(
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

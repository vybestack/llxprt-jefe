//! Issue list component.
//!
//! @plan PLAN-20260329-ISSUES-MODE.P12
//! @plan PLAN-20260329-ISSUES-MODE.P14
//! @requirement REQ-ISS-006

use iocraft::prelude::*;

use crate::domain::{Issue, IssueState};
use crate::theme::ThemeColors;

use super::{ListPanel, ListPanelRow};

/// Props for the issue list pane.
#[derive(Default, Props)]
#[allow(clippy::struct_excessive_bools)]
pub struct IssueListProps {
    /// Issues to display.
    pub issues: Vec<Issue>,
    /// Currently selected index.
    pub selected_index: Option<usize>,
    /// Whether this pane is focused.
    pub focused: bool,
    /// Whether issues are loading.
    pub loading: bool,
    /// Whether filters are active (affects empty-state message).
    pub has_filters: bool,
    /// Whether this is the compact (split) variant for detail view.
    pub compact: bool,
    /// Top row offset for the visible issue window.
    pub scroll_offset: usize,
    /// Theme colors.
    pub colors: ThemeColors,
}

fn issue_row(issue: &Issue, selected: bool) -> ListPanelRow {
    let prefix = if selected { "> " } else { "  " };
    let state_tag = match issue.state {
        IssueState::Open => "OPEN",
        IssueState::Closed => "CLSD",
    };

    let primary = format!("{}#{} {}", prefix, issue.number, issue.title);

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

    ListPanelRow {
        primary,
        secondary: Some(format!("     {}", meta_parts.join("  "))),
    }
}

/// Issue list pane — renders issues with selection highlight, loading, and empty states.
/// @plan PLAN-20260329-ISSUES-MODE.P14
/// @requirement REQ-ISS-006
#[component]
pub fn IssueList(props: &IssueListProps) -> impl Into<AnyElement<'static>> {
    let empty_message = if props.has_filters {
        "No issues match filters"
    } else {
        "No issues found"
    };
    let rows: Vec<ListPanelRow> = props
        .issues
        .iter()
        .enumerate()
        .map(|(i, issue)| issue_row(issue, props.selected_index == Some(i)))
        .collect();

    element! {
        ListPanel(
            title: "Issues".to_string(),
            rows: rows,
            selected_index: props.selected_index,
            focused: props.focused,
            loading: props.loading,
            loading_message: "Loading issues...".to_string(),
            empty_message: empty_message.to_string(),
            compact: props.compact,
            scroll_offset: props.scroll_offset,
            colors: props.colors.clone(),
        )
    }
}

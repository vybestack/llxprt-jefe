//! Issue #265 notice-only banner list projection tests.

use super::{SelectablePane, pane_content_lines};
use crate::domain::{Issue, IssueState};
use crate::state::{AppState, IssuesState};

fn make_issues() -> Vec<Issue> {
    (0..30)
        .map(|number| Issue {
            number,
            node_id: format!("I_kwDOtest{number}"),
            title: format!("Issue {number}"),
            state: IssueState::Open,
            author_login: "octocat".to_string(),
            updated_at: "2026-07-13T12:00:00Z".to_string(),
            assignee_summary: String::new(),
            labels_summary: String::new(),
            assignees: Vec::new(),
            labels: Vec::new(),
            issue_type: String::new(),
            milestone: String::new(),
            module: String::new(),
            comment_count: 0,
            body: format!("Body for issue {number}"),
        })
        .collect()
}

fn issue_list_line_count(error: Option<&str>, notice: Option<&str>) -> usize {
    let mut state = AppState {
        issues_state: IssuesState {
            error: error.map(str::to_string),
            draft_notice: notice.map(str::to_string),
            ..IssuesState::default()
        },
        ..AppState::default()
    };
    state.issues_state.list.items_mut().extend(make_issues());
    state.issues_state.list.set_selected_index(Some(0));
    pane_content_lines(SelectablePane::IssueList, &state, None, &[], 120, 19)
        .lines
        .len()
}

#[test]
fn issue_list_lines_notice_only_banner_shrinks_window_like_error() {
    let count_none = issue_list_line_count(None, None);
    let count_notice = issue_list_line_count(None, Some("No agents available"));
    let count_error = issue_list_line_count(Some("load failed"), None);
    let count_both = issue_list_line_count(Some("load failed"), Some("No agents available"));

    assert_eq!(count_notice, count_error);
    assert_eq!(count_both, count_error);
    assert!(count_none > count_notice);
}

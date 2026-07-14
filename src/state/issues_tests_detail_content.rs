//! Issues detail content line-count tests.
//!
//! Extracted from `issues_tests_detail` to keep that module under the
//! source-file-size limit. The helpers (`dashboard_issues_state`,
//! `p15_detail`, `p15_comment`) are shared from the parent module.

use super::issues_tests_detail::{dashboard_issues_state, p15_comment, p15_detail};

#[test]
fn test_detail_content_line_count_includes_empty_comments_separator() {
    let mut state = dashboard_issues_state();
    state.issues_state.issue_detail = Some(p15_detail(1));

    assert_eq!(state.issues_state.detail_content_line_count(), 8);
}

#[test]
fn test_detail_content_line_count_includes_loading_comments_separator() {
    let mut state = dashboard_issues_state();
    state.issues_state.issue_detail = Some(p15_detail(1));
    state.issues_state.loading.comments = true;

    assert_eq!(state.issues_state.detail_content_line_count(), 8);
}

#[test]
fn test_detail_content_line_count_includes_non_empty_comments_separator() {
    let mut detail = p15_detail(1);
    detail.comments.replace_items(vec![p15_comment(
        101,
        "alice",
        "2024-01-03T00:00:00Z",
        "hello",
    )]);
    let mut state = dashboard_issues_state();
    state.issues_state.issue_detail = Some(detail);

    assert_eq!(state.issues_state.detail_content_line_count(), 10);
}

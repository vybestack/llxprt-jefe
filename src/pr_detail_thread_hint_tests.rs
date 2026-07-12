//! Collapse-hint rendering tests for comment-less review threads
//! (issue #155 review remediation). Child module of
//! `pr_detail_content_tests` so it shares the fixtures; split out to keep
//! the parent file under the source-size limit.

use super::*;

/// A collapsed thread with ZERO comments must not render the misleading
/// "0 comments (select to expand)" hint — expanding would reveal nothing.
/// The header (location/tags) still renders so the thread stays visible.
#[test]
fn empty_collapsed_thread_hides_expand_hint() {
    let mut detail = detail_with_threads();
    // Make the resolved (collapsible) thread comment-less.
    let Some(resolved) = detail.reviews[0]
        .review_threads
        .iter_mut()
        .find(|t| t.is_resolved)
    else {
        panic!("fixture has a resolved thread");
    };
    resolved.comments.clear();
    let resolved_path = resolved.path.clone().unwrap_or_default();
    assert!(!resolved_path.is_empty(), "fixture thread has a path");
    let content = build_pr_detail_content(
        &detail,
        PrDetailSubfocus::Body,
        &InlineState::None,
        false,
        false,
    );
    assert!(
        !content.text.contains("0 comments"),
        "no 0-comment expand hint: {}",
        content.text
    );
    assert!(
        content.text.contains(&resolved_path),
        "empty thread header still renders: {}",
        content.text
    );
}

use super::*;
use crate::domain::{
    IssueComment, PrCheck, PrCheckStatus, PrReview, PrReviewState, PrState, PullRequestDetail,
};
use crate::state::{ComposerTarget, InlineState};

fn require_range(
    detail: &PullRequestDetail,
    subfocus: PrDetailSubfocus,
    inline: &InlineState,
    detail_loading: bool,
    comments_loading: bool,
) -> (usize, usize) {
    let Some(range) =
        pr_subfocus_line_range(detail, subfocus, inline, detail_loading, comments_loading)
    else {
        panic!("expected subfocus range");
    };
    range
}
/// @plan PLAN-20260624-PR-MODE.P12
/// @requirement REQ-PR-009
/// @pseudocode component-001 lines 1-12
fn sample_detail() -> PullRequestDetail {
    PullRequestDetail {
        repo_owner_name: "owner/repo".to_string(),
        number: 84,
        title: "Add PR mode".to_string(),
        state: PrState::Open,
        is_draft: false,
        author_login: "pat".to_string(),
        created_at: "2026-06-20".to_string(),
        updated_at: "2026-06-24".to_string(),
        head_ref: "issue20".to_string(),
        base_ref: "main".to_string(),
        labels: vec!["feat".to_string()],
        assignees: vec![],
        milestone: None,
        body: "Implements the PR mode UI surface.".to_string(),
        external_url: "https://github.com/owner/repo/pull/84".to_string(),
        review_decision: Some(PrReviewState::ReviewRequired),
        checks_status: PrCheckStatus::Success,
        reviews: vec![PrReview {
            review_id: Some("PRR_kw001".to_string()),
            author_login: "ada".to_string(),
            state: PrReviewState::ChangesRequested,
            submitted_at: "2026-06-23".to_string(),
            body: Some("please split handler".to_string()),
            review_threads: vec![],
        }],
        checks: vec![PrCheck {
            name: "ci/fmt".to_string(),
            status: PrCheckStatus::Success,
            conclusion: "passed".to_string(),
            url: None,
        }],
        comments: vec![IssueComment {
            comment_id: 1,
            author_login: "pat".to_string(),
            created_at: "2026-06-22".to_string(),
            edited_at: None,
            body: "ready for review".to_string(),
        }],
        has_more_comments: false,
        comments_cursor: None,
        mergeable: None,
        merge_state_status: None,
    }
}

/// @plan PLAN-20260624-PR-MODE.P12
/// @requirement REQ-PR-009
/// @pseudocode component-001 lines 1-12
#[test]
fn build_pr_detail_content_includes_all_section_labels() {
    let detail = sample_detail();
    let content = build_pr_detail_content(
        &detail,
        PrDetailSubfocus::Body,
        &InlineState::None,
        false,
        false,
    );
    assert!(content.text.contains("Description"), "missing Description");
    assert!(content.text.contains("Reviews"), "missing Reviews");
    assert!(content.text.contains("Checks"), "missing Checks");
    assert!(content.text.contains("Comments"), "missing Comments");
    assert!(content.text.contains("New comment"), "missing New comment");
    assert!(content.cursor.is_none());
}

/// @plan PLAN-20260624-PR-MODE.P12
/// @requirement REQ-PR-009
/// @pseudocode component-001 lines 1-12
#[test]
fn build_pr_detail_content_renders_loading_state() {
    let detail = sample_detail();
    let content = build_pr_detail_content(
        &detail,
        PrDetailSubfocus::Body,
        &InlineState::None,
        false,
        true,
    );
    assert!(
        content.text.contains("Loading comments..."),
        "missing loading indicator"
    );
}

/// A loading PR detail surfaces a body-level loading indicator so the pane
/// is never silently empty while the full detail is being fetched.
///
/// @plan PLAN-20260624-PR-MODE.P12
/// @requirement REQ-PR-009
/// @pseudocode component-001 lines 1-12
#[test]
fn build_pr_detail_content_renders_detail_loading_indicator() {
    let mut detail = sample_detail();
    detail.body = String::new();
    let content = build_pr_detail_content(
        &detail,
        PrDetailSubfocus::Body,
        &InlineState::None,
        true,
        false,
    );
    assert!(
        content.text.contains("Loading pull request..."),
        "missing detail loading indicator"
    );
}

/// `pr_detail_content_line_count` must remain in lockstep with the rendered
/// content when the detail-loading indicator is shown. Mirrors Issues mode:
/// the reducer never wraps, so the count is the unwrapped line count the
/// renderer also derives.
///
/// @plan PLAN-20260624-PR-MODE.P12
/// @requirement REQ-PR-009
/// @pseudocode component-001 lines 169-176
#[test]
fn pr_detail_content_line_count_matches_render_when_detail_loading() {
    let mut detail = sample_detail();
    detail.body = String::new();
    let rendered = build_pr_detail_content(
        &detail,
        PrDetailSubfocus::Body,
        &InlineState::None,
        true,
        false,
    );
    let count = pr_detail_content_line_count(
        &detail,
        PrDetailSubfocus::Body,
        &InlineState::None,
        true,
        false,
    );
    assert_eq!(
        count,
        rendered.text.lines().count(),
        "line count must match rendered content while detail loading"
    );
}

// ── Bug A: cursor propagation ──────────────────────────────────────────

/// Opening a NewComment composer must NOT flatten the composer text/cursor
/// into the read-only document — the composer is rendered by the dedicated
/// TextBox component, so `build_pr_detail_content` returns `cursor: None`
/// and emits only a stable anchor/help line.
///
/// @plan PLAN-20260624-PR-MODE.P14
/// @requirement REQ-PR-009
/// @requirement REQ-PR-010
/// @pseudocode component-001 lines 169-176
#[test]
fn new_comment_composer_not_flattened_into_document() {
    let detail = sample_detail();
    let inline = InlineState::Composer {
        target: ComposerTarget::NewComment,
        text: "abc".to_string(),
        cursor: 3,
    };
    let content =
        build_pr_detail_content(&detail, PrDetailSubfocus::NewComment, &inline, false, false);
    assert!(
        content.cursor.is_none(),
        "NewComment composer must NOT flatten a cursor into the read-only document"
    );
    // The anchor/help line must still be present so the section is visible.
    assert!(
        content.text.contains("Ctrl+Enter submit | Esc cancel"),
        "NewComment composer anchor/help line must be present"
    );
    // The composer text must NOT be flattened into the document.
    assert!(
        !content.text.contains("abc"),
        "NewComment composer text must NOT be flattened into the read-only document"
    );
}

/// A Reply composer must emit only a stable anchor/help section in the
/// read-only document; the editable text/cursor is rendered by `TextBox`.
///
/// @plan PLAN-20260624-PR-MODE.P14
/// @requirement REQ-PR-009
/// @pseudocode component-001 lines 1-12
#[test]
fn reply_composer_not_flattened_into_document() {
    let detail = sample_detail();
    let reply_text = "@pat hi".to_string();
    let inline = InlineState::Composer {
        target: ComposerTarget::Reply {
            comment_index: 0,
            author: "@pat ".to_string(),
        },
        text: reply_text.clone(),
        cursor: reply_text.len(),
    };
    let content =
        build_pr_detail_content(&detail, PrDetailSubfocus::Comment(0), &inline, false, false);
    assert!(
        content.cursor.is_none(),
        "Reply composer cursor belongs to TextBox"
    );
    assert!(content.text.contains(PR_REPLY_ANCHOR));
    assert!(content.text.contains("    Ctrl+Enter save | Esc cancel"));
    assert!(
        !content.text.contains("@pat hi"),
        "Reply composer text must NOT be flattened into the read-only document"
    );
}

/// A NewComment composer with a multibyte string must NOT flatten the text
/// or cursor into the read-only document (no panic, cursor stays `None`).
/// The multibyte caret projection is exercised by the `text_box_view` module
/// tests.
///
/// @plan PLAN-20260624-PR-MODE.P14
/// @requirement REQ-PR-009
/// @requirement REQ-PR-010
/// @pseudocode component-001 lines 169-176
#[test]
fn multibyte_new_comment_composer_not_flattened_into_document() {
    let detail = sample_detail();
    let text = "héllo".to_string();
    let byte_cursor = 1;
    let inline = InlineState::Composer {
        target: ComposerTarget::NewComment,
        text: text.clone(),
        cursor: byte_cursor,
    };
    let content =
        build_pr_detail_content(&detail, PrDetailSubfocus::NewComment, &inline, false, false);
    assert!(
        content.cursor.is_none(),
        "NewComment composer must NOT flatten a cursor (multibyte or otherwise)"
    );
    // And the text must not appear in the document.
    assert!(
        !content.text.contains("héllo"),
        "NewComment composer text must NOT be flattened into the document"
    );
}

// ── FIX 1: empty composer input row ────────────────────────────────────

/// Opening a NewComment composer with empty text must NOT flatten a blank
/// input row or cursor into the read-only document — the composer is rendered
/// by the TextBox. The document emits only the anchor + help line.
///
/// @plan PLAN-20260624-PR-MODE.P14
/// @requirement REQ-PR-009
/// @requirement REQ-PR-010
/// @pseudocode component-001 lines 169-176
#[test]
fn empty_new_comment_composer_emits_only_anchor_no_flattened_row() {
    let detail = sample_detail();
    let inline = InlineState::Composer {
        target: ComposerTarget::NewComment,
        text: String::new(),
        cursor: 0,
    };
    let content =
        build_pr_detail_content(&detail, PrDetailSubfocus::NewComment, &inline, false, false);
    assert!(
        content.cursor.is_none(),
        "empty NewComment composer must NOT flatten a cursor into the document"
    );
    assert!(
        content.text.contains("Ctrl+Enter submit | Esc cancel"),
        "NewComment composer anchor/help line must be present"
    );
    // The composer gutter prefix must NOT appear in the document (no flattened row).
    assert!(
        !content.text.lines().any(|l| l == "  │ " || l == "  │"),
        "document must NOT contain a flattened composer prefix row"
    );
}

/// Opening an empty Reply composer must emit only the stable reply anchor/help
/// section in the read-only document.
///
/// @plan PLAN-20260624-PR-MODE.P14
/// @requirement REQ-PR-009
/// @pseudocode component-001 lines 1-12
#[test]
fn empty_reply_composer_emits_only_anchor_no_flattened_row() {
    let detail = sample_detail();
    let inline = InlineState::Composer {
        target: ComposerTarget::Reply {
            comment_index: 0,
            author: String::new(),
        },
        text: String::new(),
        cursor: 0,
    };
    let content =
        build_pr_detail_content(&detail, PrDetailSubfocus::Comment(0), &inline, false, false);
    assert!(
        content.cursor.is_none(),
        "Reply composer cursor belongs to TextBox"
    );
    assert!(content.text.contains(PR_REPLY_ANCHOR));
    assert!(content.text.contains("    Ctrl+Enter save | Esc cancel"));
    assert!(
        !content.text.lines().any(|l| l == "    │ " || l == "    │"),
        "document must NOT contain a flattened reply composer prefix row"
    );
}

// =============================================================================
// Review-thread rendering tests (issue #119)
// =============================================================================

use crate::domain::PrReviewThread;

fn detail_with_threads() -> PullRequestDetail {
    let mut detail = sample_detail();
    detail.reviews[0].review_threads = vec![
        PrReviewThread {
            thread_id: "PRRT_kw1".to_string(),
            is_resolved: false,
            is_outdated: false,
            review_id: Some("PRR_kw001".to_string()),
            path: Some("src/parser.rs".to_string()),
            line: Some(42),
            comments: vec![
                IssueComment {
                    comment_id: 10,
                    author_login: "bob".to_string(),
                    created_at: "2026-06-23T11:00:00Z".to_string(),
                    edited_at: None,
                    body: "This unwrap can panic.".to_string(),
                },
                IssueComment {
                    comment_id: 11,
                    author_login: "ada".to_string(),
                    created_at: "2026-06-23T12:00:00Z".to_string(),
                    edited_at: None,
                    body: "Good catch.".to_string(),
                },
            ],
        },
        PrReviewThread {
            thread_id: "PRRT_kw2".to_string(),
            is_resolved: true,
            is_outdated: false,
            review_id: Some("PRR_kw001".to_string()),
            path: Some("src/main.rs".to_string()),
            line: Some(5),
            comments: vec![IssueComment {
                comment_id: 20,
                author_login: "carol".to_string(),
                created_at: "2026-06-23T10:00:00Z".to_string(),
                edited_at: None,
                body: "Looks good.".to_string(),
            }],
        },
    ];
    detail
}

#[test]
fn reviews_section_renders_nested_thread_comments() {
    let detail = detail_with_threads();
    let content = build_pr_detail_content(
        &detail,
        PrDetailSubfocus::Body,
        &InlineState::None,
        false,
        false,
    );
    assert!(
        content.text.contains("src/parser.rs:42"),
        "thread path:line"
    );
    assert!(content.text.contains("[UNRESOLVED]"), "unresolved tag");
    assert!(content.text.contains("bob"), "thread comment author");
    assert!(
        content.text.contains("This unwrap can panic."),
        "thread body"
    );
    assert!(content.text.contains("ada"), "second comment author");
}

#[test]
fn reviews_section_renders_resolution_state() {
    let detail = detail_with_threads();
    let content = build_pr_detail_content(
        &detail,
        PrDetailSubfocus::Body,
        &InlineState::None,
        false,
        false,
    );
    assert!(
        content.text.contains("[RESOLVED]"),
        "resolved tag for second thread"
    );
    assert!(
        content.text.contains("[UNRESOLVED]"),
        "unresolved tag for first thread"
    );
}

#[test]
fn focused_review_thread_shows_reply_and_resolve_hints() {
    let detail = detail_with_threads();
    let content = build_pr_detail_content(
        &detail,
        PrDetailSubfocus::ReviewThread(0),
        &InlineState::None,
        false,
        false,
    );
    assert!(
        content.text.contains("[ r reply ]"),
        "focused thread must show reply hint"
    );
    assert!(
        content.text.contains("[ R resolve ]"),
        "focused thread must show resolve hint"
    );
}

#[test]
fn unfocused_review_thread_hides_reply_resolve_hints() {
    let detail = detail_with_threads();
    let content = build_pr_detail_content(
        &detail,
        PrDetailSubfocus::Body,
        &InlineState::None,
        false,
        false,
    );
    assert!(
        !content.text.contains("[ r reply ]"),
        "unfocused threads must NOT show hints"
    );
}

#[test]
fn review_thread_focused_shows_focus_marker() {
    let detail = detail_with_threads();
    let content = build_pr_detail_content(
        &detail,
        PrDetailSubfocus::ReviewThread(0),
        &InlineState::None,
        false,
        false,
    );
    assert!(
        content.text.contains(">     src/parser.rs:42"),
        "focused thread must have > marker"
    );
}

// ── Collapse/expand-on-focus for resolved/outdated threads (#155 f-up) ───

/// Resolved threads collapse to their header line when NOT focused: the
/// conversation body is hidden and a "(select to expand)" hint shows instead.
#[test]
fn resolved_thread_collapses_body_when_not_focused() {
    let detail = detail_with_threads();
    let content = build_pr_detail_content(
        &detail,
        PrDetailSubfocus::Body,
        &InlineState::None,
        false,
        false,
    );
    assert!(
        !content.text.contains("Looks good."),
        "resolved thread body must be hidden while unfocused"
    );
    assert!(
        content.text.contains("(select to expand)"),
        "collapsed thread must hint how to expand"
    );
    assert!(
        content.text.contains("1 comment"),
        "collapsed thread must show its comment count"
    );
}

/// Focusing a resolved thread expands its full conversation WITHOUT
/// mutating the resolve state (read access must not require unresolve).
#[test]
fn resolved_thread_expands_on_focus() {
    let detail = detail_with_threads();
    // Thread flat index 1 is the resolved src/main.rs thread.
    let content = build_pr_detail_content(
        &detail,
        PrDetailSubfocus::ReviewThread(1),
        &InlineState::None,
        false,
        false,
    );
    assert!(
        content.text.contains("Looks good."),
        "focused resolved thread must render its conversation"
    );
    assert!(
        content.text.contains("carol"),
        "focused resolved thread must render comment authors"
    );
    assert!(
        content.text.contains("[RESOLVED]"),
        "expanding must NOT flip the resolve tag"
    );
}

/// Unresolved current threads always render expanded, focused or not.
#[test]
fn unresolved_thread_stays_expanded_when_unfocused() {
    let detail = detail_with_threads();
    let content = build_pr_detail_content(
        &detail,
        PrDetailSubfocus::Body,
        &InlineState::None,
        false,
        false,
    );
    assert!(
        content.text.contains("This unwrap can panic."),
        "unresolved thread body must always render"
    );
}

/// Outdated (but unresolved) threads collapse like resolved ones and carry
/// an [OUTDATED] tag on the header line.
#[test]
fn outdated_thread_collapses_and_shows_outdated_tag() {
    let mut detail = detail_with_threads();
    detail.reviews[0].review_threads[0].is_outdated = true;
    let content = build_pr_detail_content(
        &detail,
        PrDetailSubfocus::Body,
        &InlineState::None,
        false,
        false,
    );
    assert!(
        content.text.contains("[OUTDATED]"),
        "outdated thread must show the [OUTDATED] tag"
    );
    assert!(
        !content.text.contains("This unwrap can panic."),
        "outdated thread body must collapse while unfocused"
    );

    // Focus expands it, tag stays.
    let focused = build_pr_detail_content(
        &detail,
        PrDetailSubfocus::ReviewThread(0),
        &InlineState::None,
        false,
        false,
    );
    assert!(
        focused.text.contains("This unwrap can panic."),
        "focused outdated thread must expand"
    );
    assert!(focused.text.contains("[OUTDATED]"));
}

/// The focused resolve hint flips to "unresolve" for resolved threads so the
/// toggle's effect is honest.
#[test]
fn focused_resolved_thread_shows_unresolve_hint() {
    let detail = detail_with_threads();
    let content = build_pr_detail_content(
        &detail,
        PrDetailSubfocus::ReviewThread(1),
        &InlineState::None,
        false,
        false,
    );
    assert!(
        content.text.contains("[ R unresolve ]"),
        "resolved thread hint must say unresolve"
    );
    assert!(
        !content.text.contains("[ R resolve ]"),
        "resolved thread must not show the resolve label"
    );
}

#[test]
fn pr_detail_line_count_with_threads_exceeds_base() {
    let detail_no_threads = sample_detail();
    let base_count = pr_detail_content_line_count(
        &detail_no_threads,
        PrDetailSubfocus::Body,
        &InlineState::None,
        false,
        false,
    );

    let detail_with_threads = detail_with_threads();
    let thread_count = pr_detail_content_line_count(
        &detail_with_threads,
        PrDetailSubfocus::ReviewThread(0),
        &InlineState::None,
        false,
        false,
    );
    assert!(
        thread_count > base_count,
        "threads must add rendered lines: {thread_count} vs base {base_count}"
    );
}

// ── pr_subfocus_line_range (#151) ────────────────────────────────────────
//
// The scroll-into-view feature needs the content-line range of the focused
// subfocus item. These tests verify the pure projection returns correct
// ranges for each subfocus variant using the rendered content as the source
// of truth.

#[test]
fn pr_subfocus_line_range_body_covers_description_section() {
    let detail = sample_detail();
    let range = require_range(
        &detail,
        PrDetailSubfocus::Body,
        &InlineState::None,
        false,
        false,
    );
    let content = build_pr_detail_content(
        &detail,
        PrDetailSubfocus::Body,
        &InlineState::None,
        false,
        false,
    );
    let lines: Vec<&str> = content.text.lines().collect();
    // The range must start at the "Description" label and end just before the
    // first separator.
    assert_eq!(lines[range.0], "Description");
    assert!(
        !lines[range.1].is_empty() && !lines[range.1].starts_with('─'),
        "Body range end must be a content line, not a separator"
    );
    // The line after the range should be the separator.
    if range.1 + 1 < lines.len() {
        assert!(
            lines[range.1 + 1].starts_with('─'),
            "Line after Body range must be a separator"
        );
    }
}

#[test]
fn pr_subfocus_line_range_review_locates_focused_review() {
    let detail = sample_detail();
    let range = require_range(
        &detail,
        PrDetailSubfocus::Review(0),
        &InlineState::None,
        false,
        false,
    );
    let content = build_pr_detail_content(
        &detail,
        PrDetailSubfocus::Review(0),
        &InlineState::None,
        false,
        false,
    );
    let lines: Vec<&str> = content.text.lines().collect();
    // The focused review line starts with "> " and contains the author "ada".
    let line = lines[range.0];
    assert!(line.starts_with("> "), "focused review must have > marker");
    assert!(line.contains("ada"), "focused review must contain author");
    assert!(line.contains("CHANGES_REQUESTED"));
    // Single-line item.
    assert_eq!(range.0, range.1);
}

#[test]
fn pr_subfocus_line_range_review_thread_spans_thread_block() {
    let detail = detail_with_threads();
    let range = require_range(
        &detail,
        PrDetailSubfocus::ReviewThread(0),
        &InlineState::None,
        false,
        false,
    );
    let content = build_pr_detail_content(
        &detail,
        PrDetailSubfocus::ReviewThread(0),
        &InlineState::None,
        false,
        false,
    );
    let lines: Vec<&str> = content.text.lines().collect();
    let header = lines[range.0];
    assert!(
        header.contains("src/parser.rs:42"),
        "thread header must contain location"
    );
    // The range must include the thread comments and the reply/resolve hint
    // (since thread 0 is focused), ending at the last content line before the
    // trailing blank.
    let block: Vec<&str> = lines[range.0..=range.1].to_vec();
    assert!(
        block.iter().any(|l| l.contains("This unwrap can panic.")),
        "thread block must include comment body"
    );
    assert!(
        block.iter().any(|l| l.contains("[ r reply ]")),
        "focused thread block must include reply hint"
    );
}

#[test]
fn pr_subfocus_line_range_check_locates_focused_check() {
    let detail = sample_detail();
    let range = require_range(
        &detail,
        PrDetailSubfocus::Check(0),
        &InlineState::None,
        false,
        false,
    );
    let content = build_pr_detail_content(
        &detail,
        PrDetailSubfocus::Check(0),
        &InlineState::None,
        false,
        false,
    );
    let lines: Vec<&str> = content.text.lines().collect();
    let line = lines[range.0];
    assert!(line.starts_with("> "), "focused check must have > marker");
    assert!(line.contains("ci/fmt"));
    assert!(line.contains("success"));
    assert_eq!(range.0, range.1, "check is a single-line item");
}

#[test]
fn pr_subfocus_line_range_comment_spans_comment_block() {
    let detail = sample_detail();
    let range = require_range(
        &detail,
        PrDetailSubfocus::Comment(0),
        &InlineState::None,
        false,
        false,
    );
    let content = build_pr_detail_content(
        &detail,
        PrDetailSubfocus::Comment(0),
        &InlineState::None,
        false,
        false,
    );
    let lines: Vec<&str> = content.text.lines().collect();
    let header = lines[range.0];
    assert!(
        header.starts_with("> "),
        "focused comment must have > marker"
    );
    assert!(header.contains("pat"), "comment header must contain author");
    let block: Vec<&str> = lines[range.0..=range.1].to_vec();
    assert!(
        block.iter().any(|l| l.contains("ready for review")),
        "comment block must include body"
    );
}

#[test]
fn pr_subfocus_line_range_new_comment_locates_label_and_hint() {
    let detail = sample_detail();
    let range = require_range(
        &detail,
        PrDetailSubfocus::NewComment,
        &InlineState::None,
        false,
        false,
    );
    let content = build_pr_detail_content(
        &detail,
        PrDetailSubfocus::NewComment,
        &InlineState::None,
        false,
        false,
    );
    let lines: Vec<&str> = content.text.lines().collect();
    assert_eq!(lines[range.0], "> New comment");
    // The hint line is the second line of the section.
    assert!(
        lines[range.1].contains("Press c to add a comment")
            || lines[range.1].contains("Ctrl+Enter submit"),
        "NewComment range must include the hint line"
    );
}

#[test]
fn pr_subfocus_line_range_returns_none_for_out_of_bounds_index() {
    let detail = sample_detail();
    let range = pr_subfocus_line_range(
        &detail,
        PrDetailSubfocus::Comment(99),
        &InlineState::None,
        false,
        false,
    );
    assert!(
        range.is_none(),
        "out-of-bounds comment index must return None"
    );
}

/// Consecutive bodyless reviews (e.g. bot reviews whose content lives in
/// threads) still get a blank separator line between their headers so they
/// never render visually glued together.
#[test]
fn bodyless_reviews_are_separated_by_blank_lines() {
    let mut detail = sample_detail();
    detail.reviews = vec![
        PrReview {
            review_id: Some("PRR_kw001".to_string()),
            author_login: "bot".to_string(),
            state: PrReviewState::Commented,
            submitted_at: "2026-06-23".to_string(),
            body: None,
            review_threads: vec![],
        },
        PrReview {
            review_id: Some("PRR_kw002".to_string()),
            author_login: "bot".to_string(),
            state: PrReviewState::Commented,
            submitted_at: "2026-06-24".to_string(),
            body: None,
            review_threads: vec![],
        },
    ];
    let content = build_pr_detail_content(
        &detail,
        PrDetailSubfocus::Body,
        &InlineState::None,
        false,
        false,
    );
    let lines: Vec<&str> = content.text.lines().collect();
    let Some(first) = lines.iter().position(|l| l.contains("2026-06-23")) else {
        panic!("first review header present");
    };
    assert_eq!(
        lines.get(first + 1).copied(),
        Some(""),
        "blank separator after a bodyless review header"
    );
    assert!(
        lines
            .get(first + 2)
            .is_some_and(|l| l.contains("2026-06-24")),
        "second review header follows the separator"
    );
}

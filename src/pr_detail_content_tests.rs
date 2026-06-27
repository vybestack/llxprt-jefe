use super::*;
use crate::domain::{
    IssueComment, PrCheck, PrCheckStatus, PrReview, PrReviewState, PrState, PullRequestDetail,
};
use crate::state::{ComposerTarget, InlineState};
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
            author_login: "ada".to_string(),
            state: PrReviewState::ChangesRequested,
            submitted_at: "2026-06-23".to_string(),
            body: Some("please split handler".to_string()),
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

/// @plan PLAN-20260624-PR-MODE.P12
/// @requirement REQ-PR-009
/// @pseudocode component-001 lines 1-12
#[test]
fn build_new_pr_comment_content_renders_composer_prompt() {
    let inline = InlineState::None;
    let content = build_new_pr_comment_content(&inline);
    assert!(content.text.contains("New comment"));
    assert!(content.text.contains("Ctrl+Enter submit | Esc cancel"));
}

// ── Bug A: cursor propagation ──────────────────────────────────────────

/// Opening a NewComment composer must surface a cursor pointing at the
/// composer line within the joined content (NOT `None`).
///
/// @plan PLAN-20260624-PR-MODE.P14
/// @requirement REQ-PR-009
/// @pseudocode component-001 lines 1-12
#[test]
fn new_comment_composer_surfaces_cursor_at_composer_line() {
    let detail = sample_detail();
    let inline = InlineState::Composer {
        target: ComposerTarget::NewComment,
        text: "abc".to_string(),
        cursor: 3,
    };
    let content =
        build_pr_detail_content(&detail, PrDetailSubfocus::NewComment, &inline, false, false);
    let cursor = content
        .cursor
        .unwrap_or_else(|| panic!("NewComment composer must surface a cursor"));
    let lines: Vec<&str> = content.text.lines().collect();
    let (line_idx, col) = cursor;
    assert!(
        line_idx < lines.len(),
        "cursor line {line_idx} out of range ({} lines)",
        lines.len()
    );
    assert!(
        lines[line_idx].contains("abc"),
        "cursor line must be the composer line, got: {:?}",
        lines[line_idx]
    );
    assert_eq!(
        col, 7,
        "cursor col must be end-of-text within composer line"
    );
}

/// A Reply composer must surface a cursor pointing at the reply composer
/// line within the joined content.
///
/// @plan PLAN-20260624-PR-MODE.P14
/// @requirement REQ-PR-009
/// @pseudocode component-001 lines 1-12
#[test]
fn reply_composer_surfaces_cursor_at_reply_line() {
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
    let cursor = content
        .cursor
        .unwrap_or_else(|| panic!("Reply composer must surface a cursor"));
    let lines: Vec<&str> = content.text.lines().collect();
    let (line_idx, _col) = cursor;
    assert!(
        line_idx < lines.len(),
        "cursor line {line_idx} out of range"
    );
    assert!(
        lines[line_idx].contains("@pat hi"),
        "cursor line must be the reply composer line, got: {:?}",
        lines[line_idx]
    );
}

/// A composer with a multibyte string and a byte_cursor landing mid-
/// codepoint must NOT panic and must yield a correct char-column cursor.
///
/// @plan PLAN-20260624-PR-MODE.P14
/// @requirement REQ-PR-009
/// @pseudocode component-001 lines 1-12
#[test]
fn multibyte_composer_cursor_does_not_panic_and_yields_char_col() {
    let detail = sample_detail();
    let text = "héllo".to_string();
    let mid_codepoint_cursor = 7;
    let inline = InlineState::Composer {
        target: ComposerTarget::NewComment,
        text: text.clone(),
        cursor: mid_codepoint_cursor,
    };
    let content =
        build_pr_detail_content(&detail, PrDetailSubfocus::NewComment, &inline, false, false);
    let (_line, col) = content
        .cursor
        .unwrap_or_else(|| panic!("multibyte composer must still surface a cursor"));
    assert_eq!(col, 9, "cursor col must reflect char boundary after prefix");
}

// ── FIX 1: empty composer input row ────────────────────────────────────

/// Opening a NewComment composer with empty text must push a blank input
/// row and record the cursor on THAT row — NOT the following help/controls
/// line.
///
/// @plan PLAN-20260624-PR-MODE.P14
/// @requirement REQ-PR-009
/// @pseudocode component-001 lines 1-12
#[test]
fn empty_new_comment_composer_pushes_blank_input_row_with_cursor() {
    let detail = sample_detail();
    let inline = InlineState::Composer {
        target: ComposerTarget::NewComment,
        text: String::new(),
        cursor: 0,
    };
    let content =
        build_pr_detail_content(&detail, PrDetailSubfocus::NewComment, &inline, false, false);
    let lines: Vec<&str> = content.text.lines().collect();
    let cursor = content
        .cursor
        .unwrap_or_else(|| panic!("empty NewComment composer must surface a cursor"));
    let (line_idx, _col) = cursor;
    assert!(
        line_idx < lines.len(),
        "cursor line {line_idx} out of range ({} lines)",
        lines.len()
    );
    let cursor_row = lines[line_idx];
    assert!(
        !cursor_row.contains("Ctrl+Enter"),
        "cursor must NOT be on the controls/help line, got: {cursor_row:?}"
    );
    assert!(
        cursor_row == "  │ " || cursor_row.is_empty(),
        "cursor row must be the blank composer prefix, got: {cursor_row:?}"
    );
}

/// Opening a Reply composer with empty text must push a blank input row
/// and record the cursor on THAT row.
///
/// @plan PLAN-20260624-PR-MODE.P14
/// @requirement REQ-PR-009
/// @pseudocode component-001 lines 1-12
#[test]
fn empty_reply_composer_pushes_blank_input_row_with_cursor() {
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
    let lines: Vec<&str> = content.text.lines().collect();
    let cursor = content
        .cursor
        .unwrap_or_else(|| panic!("empty Reply composer must surface a cursor"));
    let (line_idx, _col) = cursor;
    assert!(
        line_idx < lines.len(),
        "cursor line {line_idx} out of range"
    );
    let cursor_row = lines[line_idx];
    assert!(
        !cursor_row.contains("Ctrl+Enter"),
        "cursor must NOT be on the controls/help line, got: {cursor_row:?}"
    );
}

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
        None,
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
        None,
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
        None,
    );
    assert!(
        content.text.contains("Loading pull request..."),
        "missing detail loading indicator"
    );
}

/// `pr_detail_content_line_count` must remain in lockstep with the rendered
/// content when the detail-loading indicator is shown.
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
        None,
    );
    let count = pr_detail_content_line_count(
        &detail,
        PrDetailSubfocus::Body,
        &InlineState::None,
        true,
        false,
        None,
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
    let content = build_pr_detail_content(
        &detail,
        PrDetailSubfocus::NewComment,
        &inline,
        false,
        false,
        None,
    );
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
    let content = build_pr_detail_content(
        &detail,
        PrDetailSubfocus::Comment(0),
        &inline,
        false,
        false,
        None,
    );
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
    let content = build_pr_detail_content(
        &detail,
        PrDetailSubfocus::NewComment,
        &inline,
        false,
        false,
        None,
    );
    let (_line, col) = content
        .cursor
        .unwrap_or_else(|| panic!("multibyte composer must still surface a cursor"));
    assert_eq!(col, 9, "cursor col must reflect char boundary after prefix");
}

// ── Bug B: wrapping ────────────────────────────────────────────────────

/// `wrap_lines` splits a line longer than the width into ceil(len/w)
/// wrapped lines and maps the cursor accordingly.
///
/// @plan PLAN-20260624-PR-MODE.P14
/// @requirement REQ-PR-009
/// @pseudocode component-001 lines 1-12
#[test]
fn wrap_lines_splits_long_line_and_maps_cursor() {
    let lines = vec!["abcdefghij".to_string()];
    let cursor = Some((0usize, 7usize));
    let (wrapped, mapped) = wrap_lines(&lines, cursor, 4);
    assert_eq!(wrapped.len(), 3, "10 chars at width 4 -> 3 lines");
    assert_eq!(wrapped[0], "abcd");
    assert_eq!(wrapped[1], "efgh");
    assert_eq!(wrapped[2], "ij");
    assert_eq!(mapped, Some((1, 3)));
}

/// `wrap_lines` leaves lines shorter than the width unchanged.
///
/// @plan PLAN-20260624-PR-MODE.P14
/// @requirement REQ-PR-009
/// @pseudocode component-001 lines 1-12
#[test]
fn wrap_lines_leaves_short_lines_unchanged() {
    let lines = vec!["ab".to_string(), "cde".to_string()];
    let cursor = Some((1usize, 2usize));
    let (wrapped, mapped) = wrap_lines(&lines, cursor, 10);
    assert_eq!(wrapped, lines);
    assert_eq!(mapped, cursor);
}

/// Parity: `pr_detail_content_line_count` with a wrap width must equal
/// `build_pr_detail_content(..).text.lines().count()`.
///
/// @plan PLAN-20260624-PR-MODE.P14
/// @requirement REQ-PR-009
/// @pseudocode component-001 lines 1-12
#[test]
fn wrapping_parity_count_matches_rendered_and_no_line_exceeds_width() {
    let mut detail = sample_detail();
    detail.body = "this is a very long body line that exceeds twenty chars".to_string();
    detail.comments = vec![IssueComment {
        comment_id: 2,
        author_login: "longauthor".to_string(),
        created_at: "2026-06-22".to_string(),
        edited_at: None,
        body: "a similarly long comment body line that also overflows".to_string(),
    }];
    let width = 20usize;
    let rendered = build_pr_detail_content(
        &detail,
        PrDetailSubfocus::Body,
        &InlineState::None,
        false,
        false,
        Some(width),
    );
    let count = pr_detail_content_line_count(
        &detail,
        PrDetailSubfocus::Body,
        &InlineState::None,
        false,
        false,
        Some(width),
    );
    assert_eq!(
        count,
        rendered.text.lines().count(),
        "wrapped line count must match rendered count"
    );
    for (i, line) in rendered.text.lines().enumerate() {
        assert!(
            line.chars().count() <= width,
            "line {i} exceeds width {width}: {} ({} chars)",
            line,
            line.chars().count()
        );
    }
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
    let content = build_pr_detail_content(
        &detail,
        PrDetailSubfocus::NewComment,
        &inline,
        false,
        false,
        None,
    );
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
    let content = build_pr_detail_content(
        &detail,
        PrDetailSubfocus::Comment(0),
        &inline,
        false,
        false,
        None,
    );
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

/// Parity: with an empty composer open AND a wrap width, the line count
/// must equal the rendered text's line count.
///
/// @plan PLAN-20260624-PR-MODE.P14
/// @requirement REQ-PR-009
/// @pseudocode component-001 lines 1-12
#[test]
fn empty_composer_line_count_matches_rendered_with_wrap() {
    let detail = sample_detail();
    let inline = InlineState::Composer {
        target: ComposerTarget::NewComment,
        text: String::new(),
        cursor: 0,
    };
    let rendered = build_pr_detail_content(
        &detail,
        PrDetailSubfocus::NewComment,
        &inline,
        false,
        false,
        Some(20),
    );
    let count = pr_detail_content_line_count(
        &detail,
        PrDetailSubfocus::NewComment,
        &inline,
        false,
        false,
        Some(20),
    );
    assert_eq!(
        count,
        rendered.text.lines().count(),
        "line count must match rendered with empty composer + wrap"
    );
}

// ── FIX 2: cursor remap off-by-one at exact wrap boundary ──────────────

/// A line whose length is an exact multiple of `width` must map an
/// end-of-line cursor to the END of the final segment, NOT one row past.
///
/// @plan PLAN-20260624-PR-MODE.P14
/// @requirement REQ-PR-009
/// @pseudocode component-001 lines 1-12
#[test]
fn wrap_cursor_at_exact_boundary_maps_to_final_segment_end() {
    let lines = vec!["abcd".to_string()];
    let cursor = Some((0usize, 4usize));
    let (_wrapped, mapped) = wrap_lines(&lines, cursor, 4);
    assert_eq!(
        mapped,
        Some((0, 4)),
        "cursor at end of len==width line must map to (row 0, col 4), not one past"
    );
}

/// A line of length 2*width with cursor at end must map to (row 1, col width).
///
/// @plan PLAN-20260624-PR-MODE.P14
/// @requirement REQ-PR-009
/// @pseudocode component-001 lines 1-12
#[test]
fn wrap_cursor_at_double_width_boundary_maps_to_second_row_end() {
    let lines = vec!["abcdefgh".to_string()];
    let cursor = Some((0usize, 8usize));
    let (_wrapped, mapped) = wrap_lines(&lines, cursor, 4);
    assert_eq!(
        mapped,
        Some((1, 4)),
        "cursor at end of len==2*width line must map to (row 1, col 4)"
    );
}

/// A composer prefix+text whose total length exactly equals the wrap_width
/// must place the caret at the end of the first wrapped row.
///
/// @plan PLAN-20260624-PR-MODE.P14
/// @requirement REQ-PR-009
/// @pseudocode component-001 lines 1-12
#[test]
fn composer_exact_wrap_width_caret_on_first_row() {
    let lines = vec!["  │ ab".to_string()];
    let cursor = Some((0usize, 6usize));
    let (wrapped, mapped) = wrap_lines(&lines, cursor, 6);
    assert_eq!(
        wrapped.len(),
        1,
        "a line of exactly width 6 must produce exactly 1 wrapped row"
    );
    assert_eq!(
        mapped,
        Some((0, 6)),
        "caret at end of exact-width line must be on row 0"
    );
}

// ── FIX 3: wrap by display width, not scalar char count ────────────────

/// A line of N full-width (width-2) characters with wrap_width=4 must
/// wrap every 2 characters (not every 4).
///
/// @plan PLAN-20260624-PR-MODE.P14
/// @requirement REQ-PR-009
/// @pseudocode component-001 lines 1-12
#[test]
fn wrap_full_width_chars_by_display_width() {
    let line = "你好世界".to_string();
    let lines = vec![line];
    let (wrapped, _mapped) = wrap_lines(&lines, None, 4);
    assert_eq!(
        wrapped.len(),
        2,
        "4 full-width chars (display 8) at width 4 -> 2 lines"
    );
    assert_eq!(wrapped[0], "你好");
    assert_eq!(wrapped[1], "世界");
}

/// A cursor after K full-width characters maps to the expected display
/// column.
///
/// @plan PLAN-20260624-PR-MODE.P14
/// @requirement REQ-PR-009
/// @pseudocode component-001 lines 1-12
#[test]
fn cursor_after_full_width_chars_maps_to_display_col() {
    let line = "你好世界".to_string();
    let lines = vec![line];
    let (_wrapped, mapped) = wrap_lines(&lines, Some((0usize, 3usize)), 4);
    assert_eq!(
        mapped,
        Some((1, 2)),
        "cursor after 3 full-width chars (display 6) at width 4 -> (row 1, col 2)"
    );
}

/// An ASCII line must still wrap exactly as before (regression lock).
///
/// @plan PLAN-20260624-PR-MODE.P14
/// @requirement REQ-PR-009
/// @pseudocode component-001 lines 1-12
#[test]
fn wrap_ascii_lines_unchanged_after_display_width_fix() {
    let lines = vec!["abcdefghij".to_string()];
    let cursor = Some((0usize, 7usize));
    let (wrapped, mapped) = wrap_lines(&lines, cursor, 4);
    assert_eq!(wrapped.len(), 3, "10 chars at width 4 -> 3 lines");
    assert_eq!(wrapped[0], "abcd");
    assert_eq!(wrapped[1], "efgh");
    assert_eq!(wrapped[2], "ij");
    assert_eq!(mapped, Some((1, 3)));
}

/// No wrapped line may exceed the wrap_width in DISPLAY width, even with
/// mixed ASCII + CJK content.
///
/// @plan PLAN-20260624-PR-MODE.P14
/// @requirement REQ-PR-009
/// @pseudocode component-001 lines 1-12
#[test]
fn wrapped_lines_never_exceed_display_width() {
    use unicode_width::UnicodeWidthStr;
    let lines = vec!["ab你好cd世界ef".to_string()];
    let (wrapped, _mapped) = wrap_lines(&lines, None, 5);
    for (i, seg) in wrapped.iter().enumerate() {
        let dw = UnicodeWidthStr::width(seg.as_str());
        assert!(dw <= 5, "segment {i} display width {dw} exceeds 5: {seg:?}");
    }
}

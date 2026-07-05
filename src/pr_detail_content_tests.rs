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

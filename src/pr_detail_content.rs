//! Shared PR-detail content construction.
//!
//! Mirrors `issue_detail_content.rs` for the PR unified scrollable detail view
//! (metadata → body → reviews → checks → comments → new-comment composer).
//! The detail component (`ui::components::pr_detail`) uses this for rendering
//! and the state scroll math uses it for scroll bounds, so rendered line counts
//! cannot drift from scroll limits.
//!
//! @plan PLAN-20260624-PR-MODE.P12
//! @requirement REQ-PR-009
//! @pseudocode component-001 lines 1-12

use crate::domain::{IssueComment, PrCheckStatus, PrReviewState, PrState, PullRequestDetail};
use crate::issue_detail_content::DetailContent;
use crate::state::{ComposerTarget, InlineState, PrDetailSubfocus};

/// Count the rendered scrollable lines for a PR detail.
///
/// Mirrors `issue_detail_content::detail_content_line_count` so the PR scroll
/// bounds derive from the REAL rendered length and cannot drift from what
/// `build_pr_detail_content` emits.
///
/// @plan PLAN-20260624-PR-MODE.P14
/// @requirement REQ-PR-009
/// @pseudocode component-001 lines 169-176
#[must_use]
pub fn pr_detail_content_line_count(
    detail: &PullRequestDetail,
    subfocus: PrDetailSubfocus,
    inline_state: &InlineState,
    comments_loading: bool,
) -> usize {
    build_pr_detail_content(detail, subfocus, inline_state, comments_loading)
        .text
        .lines()
        .count()
}

/// Build the scrollable content string for the unified PR detail view.
///
/// @plan PLAN-20260624-PR-MODE.P12
/// @requirement REQ-PR-009
/// @pseudocode component-001 lines 1-12
///
/// Section order (metadata is already rendered in the fixed header, so the
/// scroll region STARTS at the body): Description (body), Reviews, Checks,
/// Comments, New comment. `cursor` is `None` for this stub (inline composer
/// wiring lands in P14); the function returns a faithful but minimal textual
/// rendering with no deferred-macro sentinels (clippy denies both).
#[must_use]
pub fn build_pr_detail_content(
    detail: &PullRequestDetail,
    subfocus: PrDetailSubfocus,
    inline_state: &InlineState,
    comments_loading: bool,
) -> DetailContent {
    let mut lines: Vec<String> = Vec::new();

    build_body_section(detail, &mut lines);
    lines.push(separator());
    build_reviews_section(detail, subfocus, &mut lines);
    lines.push(separator());
    build_checks_section(detail, subfocus, &mut lines);
    lines.push(separator());
    build_comments_section(detail, subfocus, inline_state, comments_loading, &mut lines);
    lines.push(separator());
    build_new_comment_section(subfocus, inline_state, &mut lines);

    DetailContent {
        text: lines.join("\n"),
        cursor: None,
    }
}

/// Build a full-screen content block for creating a new PR comment.
///
/// @plan PLAN-20260624-PR-MODE.P12
/// @requirement REQ-PR-009
/// @pseudocode component-001 lines 1-12
///
/// Mirrors `build_new_issue_content` but for a PR comment composer. Safe
/// default text; cursor positioned at the end of the composer text when there
/// is composer text (otherwise `None`).
#[must_use]
pub fn build_new_pr_comment_content(inline_state: &InlineState) -> DetailContent {
    let mut lines: Vec<String> = Vec::new();
    lines.push("New comment".to_string());
    lines.push("Title: first line | Body: remaining lines".to_string());
    lines.push(String::new());

    let cursor = if let InlineState::Composer {
        target: ComposerTarget::NewComment,
        text,
        cursor: byte_cursor,
    } = inline_state
    {
        for line in text.lines() {
            lines.push(format!("  │ {line}"));
        }
        if text.ends_with('\n') {
            lines.push("  │ ".to_string());
        }
        composer_end_cursor(text, *byte_cursor)
    } else {
        None
    };

    lines.push("Ctrl+Enter submit | Esc cancel".to_string());

    DetailContent {
        text: lines.join("\n"),
        cursor,
    }
}

/// @plan PLAN-20260624-PR-MODE.P12
/// @requirement REQ-PR-009
/// @pseudocode component-001 lines 1-12
fn build_body_section(detail: &PullRequestDetail, lines: &mut Vec<String>) {
    lines.push("Description".to_string());
    if detail.body.is_empty() {
        lines.push("  (no description)".to_string());
    } else {
        for body_line in detail.body.lines() {
            lines.push(format!("  {body_line}"));
        }
    }
}

/// @plan PLAN-20260624-PR-MODE.P12
/// @requirement REQ-PR-009
/// @pseudocode component-001 lines 1-12
fn build_reviews_section(
    detail: &PullRequestDetail,
    subfocus: PrDetailSubfocus,
    lines: &mut Vec<String>,
) {
    let decision = match detail.review_decision {
        Some(state) => review_decision_label(state),
        None => "NONE",
    };
    lines.push(format!("Reviews  (decision: {decision})"));
    if detail.reviews.is_empty() {
        lines.push("  No reviews yet.".to_string());
    } else {
        for (idx, review) in detail.reviews.iter().enumerate() {
            let prefix = if subfocus == PrDetailSubfocus::Review(idx) {
                "> "
            } else {
                "- "
            };
            let state_label = review_state_label(review.state);
            let body_excerpt = review
                .body
                .as_deref()
                .filter(|b| !b.is_empty())
                .map_or_else(String::new, |b| format!("  \"{b}\""));
            lines.push(format!(
                "{prefix}{:<8} {:<18} {}{}",
                review.author_login, state_label, review.submitted_at, body_excerpt
            ));
        }
    }
}

/// @plan PLAN-20260624-PR-MODE.P12
/// @requirement REQ-PR-009
/// @pseudocode component-001 lines 1-12
fn build_checks_section(
    detail: &PullRequestDetail,
    subfocus: PrDetailSubfocus,
    lines: &mut Vec<String>,
) {
    let rollup = checks_rollup_label(detail.checks_status);
    lines.push(format!("Checks  (rollup: {rollup})"));
    if detail.checks.is_empty() {
        lines.push("  No checks reported.".to_string());
    } else {
        for (idx, check) in detail.checks.iter().enumerate() {
            let prefix = if subfocus == PrDetailSubfocus::Check(idx) {
                "> "
            } else {
                "- "
            };
            let status_label = check_status_label(check.status);
            lines.push(format!(
                "{prefix}{:<14} {:<10} {}",
                check.name, status_label, check.conclusion
            ));
        }
    }
}

/// @plan PLAN-20260624-PR-MODE.P12
/// @requirement REQ-PR-009
/// @pseudocode component-001 lines 1-12
fn build_comments_section(
    detail: &PullRequestDetail,
    subfocus: PrDetailSubfocus,
    inline_state: &InlineState,
    comments_loading: bool,
    lines: &mut Vec<String>,
) {
    lines.push("Comments".to_string());
    if comments_loading {
        lines.push("  Loading comments...".to_string());
    } else if detail.comments.is_empty() {
        lines.push("  No comments yet.".to_string());
    } else {
        for (idx, comment) in detail.comments.iter().enumerate() {
            build_single_comment(idx, comment, subfocus, inline_state, lines);
        }
    }
    if detail.has_more_comments {
        lines.push("  (more comments available)".to_string());
    }
}

/// @plan PLAN-20260624-PR-MODE.P12
/// @requirement REQ-PR-009
/// @pseudocode component-001 lines 1-12
fn build_single_comment(
    idx: usize,
    comment: &IssueComment,
    subfocus: PrDetailSubfocus,
    inline_state: &InlineState,
    lines: &mut Vec<String>,
) {
    let prefix = if subfocus == PrDetailSubfocus::Comment(idx) {
        "> "
    } else {
        "- "
    };
    lines.push(format!(
        "{}{}  {}",
        prefix, comment.author_login, comment.created_at
    ));
    for body_line in comment.body.lines() {
        lines.push(format!("    {body_line}"));
    }

    if let InlineState::Composer {
        target: ComposerTarget::Reply { comment_index, .. },
        text,
        cursor: byte_cursor,
    } = inline_state
        && *comment_index == idx
    {
        lines.push("    [Reply]".to_string());
        for reply_line in text.lines() {
            lines.push(format!("    │ {reply_line}"));
        }
        if text.ends_with('\n') {
            lines.push("    │ ".to_string());
        }
        let _ = composer_end_cursor(text, *byte_cursor);
        lines.push("    Ctrl+Enter save | Esc cancel".to_string());
    }

    lines.push(String::new());
}

/// @plan PLAN-20260624-PR-MODE.P12
/// @requirement REQ-PR-009
/// @pseudocode component-001 lines 1-12
fn build_new_comment_section(
    subfocus: PrDetailSubfocus,
    inline_state: &InlineState,
    lines: &mut Vec<String>,
) {
    let label = if subfocus == PrDetailSubfocus::NewComment {
        "> New comment"
    } else {
        "  New comment"
    };
    lines.push(label.to_string());

    if let InlineState::Composer {
        target: ComposerTarget::NewComment,
        text,
        cursor: byte_cursor,
    } = inline_state
    {
        for composer_line in text.lines() {
            lines.push(format!("  │ {composer_line}"));
        }
        if text.ends_with('\n') {
            lines.push("  │ ".to_string());
        }
        let _ = composer_end_cursor(text, *byte_cursor);
        lines.push("  Ctrl+Enter submit | Esc cancel".to_string());
    } else {
        lines.push("  Press c to add a comment".to_string());
    }
}

/// Position the cursor at the end of the composer text (line, column).
///
/// For the stub the cursor is a best-effort end-of-text position; full inline
/// composer wiring lands in P14.
///
/// @plan PLAN-20260624-PR-MODE.P12
/// @requirement REQ-PR-009
/// @pseudocode component-001 lines 1-12
fn composer_end_cursor(text: &str, byte_cursor: usize) -> Option<(usize, usize)> {
    if text.is_empty() {
        return None;
    }
    let clamped = byte_cursor.min(text.len());
    let before = &text[..clamped];
    let line_idx = before.matches('\n').count();
    let last_nl = before.rfind('\n').map_or(0, |p| p + 1);
    let col = before[last_nl..].chars().count();
    Some((line_idx, col))
}

/// @plan PLAN-20260624-PR-MODE.P12
/// @requirement REQ-PR-009
/// @pseudocode component-001 lines 1-12
fn separator() -> String {
    "─────────────────────────────────────────".to_string()
}

/// @plan PLAN-20260624-PR-MODE.P12
/// @requirement REQ-PR-009
/// @pseudocode component-001 lines 1-12
fn review_decision_label(state: PrReviewState) -> &'static str {
    match state {
        PrReviewState::Approved => "APPROVED",
        PrReviewState::ChangesRequested => "CHANGES_REQUESTED",
        PrReviewState::Commented => "COMMENTED",
        PrReviewState::Pending => "PENDING",
        PrReviewState::Dismissed => "DISMISSED",
        PrReviewState::ReviewRequired => "REVIEW_REQUIRED",
        PrReviewState::None => "NONE",
    }
}

fn review_state_label(state: PrReviewState) -> &'static str {
    review_decision_label(state)
}

/// @plan PLAN-20260624-PR-MODE.P12
/// @requirement REQ-PR-009
/// @pseudocode component-001 lines 1-12
fn check_status_label(status: PrCheckStatus) -> &'static str {
    match status {
        PrCheckStatus::Pending => "pending",
        PrCheckStatus::Success => "success",
        PrCheckStatus::Failure => "failure",
        PrCheckStatus::Neutral => "neutral",
        PrCheckStatus::None => "none",
    }
}

/// @plan PLAN-20260624-PR-MODE.P12
/// @requirement REQ-PR-009
/// @pseudocode component-001 lines 1-12
fn checks_rollup_label(status: PrCheckStatus) -> &'static str {
    match status {
        PrCheckStatus::Pending => "PENDING",
        PrCheckStatus::Success => "SUCCESS",
        PrCheckStatus::Failure => "FAILURE",
        PrCheckStatus::Neutral => "NEUTRAL",
        PrCheckStatus::None => "NONE",
    }
}

/// Render a PR state tag for header/summary display.
///
/// @plan PLAN-20260624-PR-MODE.P12
/// @requirement REQ-PR-009
/// @pseudocode component-001 lines 1-12
#[must_use]
pub fn pr_state_tag(state: PrState) -> &'static str {
    match state {
        PrState::Open => "OPEN",
        PrState::Closed => "CLOSED",
        PrState::Merged => "MERGED",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::{
        IssueComment, PrCheck, PrCheckStatus, PrReview, PrReviewState, PrState, PullRequestDetail,
    };
    use crate::state::InlineState;

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
        let content =
            build_pr_detail_content(&detail, PrDetailSubfocus::Body, &InlineState::None, false);
        assert!(content.text.contains("Description"), "missing Description");
        assert!(content.text.contains("Reviews"), "missing Reviews");
        assert!(content.text.contains("Checks"), "missing Checks");
        assert!(content.text.contains("Comments"), "missing Comments");
        assert!(content.text.contains("New comment"), "missing New comment");
        // Cursor is None for the stub.
        assert!(content.cursor.is_none());
    }

    /// @plan PLAN-20260624-PR-MODE.P12
    /// @requirement REQ-PR-009
    /// @pseudocode component-001 lines 1-12
    #[test]
    fn build_pr_detail_content_renders_loading_state() {
        let detail = sample_detail();
        let content =
            build_pr_detail_content(&detail, PrDetailSubfocus::Body, &InlineState::None, true);
        assert!(
            content.text.contains("Loading comments..."),
            "missing loading indicator"
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
}

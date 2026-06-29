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

/// Stable anchor rendered where a reply TextBox is logically attached.
///
/// @plan PLAN-20260624-PR-MODE.P14
/// @requirement REQ-PR-009
/// @pseudocode component-001 lines 169-176
pub(crate) const PR_REPLY_ANCHOR: &str = "    [Reply]";

/// Count the rendered scrollable lines for a PR detail.
///
/// Mirrors `issue_detail_content::detail_content_line_count` so the PR scroll
/// bounds derive from the REAL rendered length and cannot drift from what
/// `build_pr_detail_content` emits. Like Issues mode, the reducer NEVER wraps:
/// the renderer (ScrollableText) truncates long lines, so line counts and
/// cursor coordinates can never drift between the two layers.
///
/// @plan PLAN-20260624-PR-MODE.P14
/// @requirement REQ-PR-009
/// @pseudocode component-001 lines 169-176
#[must_use]
pub fn pr_detail_content_line_count(
    detail: &PullRequestDetail,
    subfocus: PrDetailSubfocus,
    inline_state: &InlineState,
    detail_loading: bool,
    comments_loading: bool,
) -> usize {
    build_pr_detail_content(
        detail,
        subfocus,
        inline_state,
        detail_loading,
        comments_loading,
    )
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
/// Comments, New comment. PR composer text/cursor is rendered by the embedded
/// TextBox, so this read-only document returns no cursor for composer paths and
/// emits only stable anchors/hints. Mirrors `issue_detail_content` section
/// ordering while avoiding editable text inside the read-only document.
#[must_use]
pub fn build_pr_detail_content(
    detail: &PullRequestDetail,
    subfocus: PrDetailSubfocus,
    inline_state: &InlineState,
    detail_loading: bool,
    comments_loading: bool,
) -> DetailContent {
    let mut builder = ContentBuilder::new();
    build_body_section(detail, detail_loading, &mut builder);
    builder.lines.push(separator());
    build_reviews_section(detail, subfocus, &mut builder);
    builder.lines.push(separator());
    build_checks_section(detail, subfocus, &mut builder);
    builder.lines.push(separator());
    build_comments_section(
        detail,
        subfocus,
        inline_state,
        comments_loading,
        &mut builder,
    );
    builder.lines.push(separator());
    build_new_comment_section(subfocus, inline_state, &mut builder);
    builder.finish()
}

/// Accumulator for joined read-only PR detail content lines.
///
/// PR composer text/cursors are rendered by the embedded TextBox component, so
/// this builder deliberately never records an editable cursor.
///
/// @plan PLAN-20260624-PR-MODE.P14
/// @requirement REQ-PR-009
/// @pseudocode component-001 lines 1-12
struct ContentBuilder {
    lines: Vec<String>,
}

impl ContentBuilder {
    /// @plan PLAN-20260624-PR-MODE.P14
    /// @requirement REQ-PR-009
    /// @pseudocode component-001 lines 1-12
    fn new() -> Self {
        Self { lines: Vec::new() }
    }

    /// Join the accumulated read-only lines into the final content.
    ///
    /// PR composer cursors belong to the embedded TextBox, so this always
    /// returns `cursor: None` for the read-only document.
    ///
    /// @plan PLAN-20260624-PR-MODE.P14
    /// @requirement REQ-PR-009
    /// @pseudocode component-001 lines 1-12
    fn finish(self) -> DetailContent {
        let nl = String::from(char::from(0x0Au8));
        DetailContent {
            text: self.lines.join(&nl),
            cursor: None,
        }
    }
}

/// @plan PLAN-20260624-PR-MODE.P12
/// @requirement REQ-PR-009
/// @pseudocode component-001 lines 1-12
fn build_body_section(
    detail: &PullRequestDetail,
    detail_loading: bool,
    builder: &mut ContentBuilder,
) {
    builder.lines.push("Description".to_string());
    if detail_loading {
        builder.lines.push("  Loading pull request...".to_string());
    } else if detail.body.is_empty() {
        builder.lines.push("  (no description)".to_string());
    } else {
        for body_line in detail.body.lines() {
            builder.lines.push(format!("  {body_line}"));
        }
    }
}

/// @plan PLAN-20260624-PR-MODE.P12
/// @requirement REQ-PR-009
/// @pseudocode component-001 lines 1-12
fn build_reviews_section(
    detail: &PullRequestDetail,
    subfocus: PrDetailSubfocus,
    builder: &mut ContentBuilder,
) {
    let decision = match detail.review_decision {
        Some(state) => review_decision_label(state),
        None => "NONE",
    };
    builder
        .lines
        .push(format!("Reviews  (decision: {decision})"));
    if detail.reviews.is_empty() {
        builder.lines.push("  No reviews yet.".to_string());
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
            builder.lines.push(format!(
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
    builder: &mut ContentBuilder,
) {
    let rollup = checks_rollup_label(detail.checks_status);
    builder.lines.push(format!("Checks  (rollup: {rollup})"));
    if detail.checks.is_empty() {
        builder.lines.push("  No checks reported.".to_string());
    } else {
        for (idx, check) in detail.checks.iter().enumerate() {
            let prefix = if subfocus == PrDetailSubfocus::Check(idx) {
                "> "
            } else {
                "- "
            };
            let status_label = check_status_label(check.status);
            builder.lines.push(format!(
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
    builder: &mut ContentBuilder,
) {
    builder.lines.push("Comments".to_string());
    if comments_loading {
        builder.lines.push("  Loading comments...".to_string());
    } else if detail.comments.is_empty() {
        builder.lines.push("  No comments yet.".to_string());
    } else {
        for (idx, comment) in detail.comments.iter().enumerate() {
            build_single_comment(idx, comment, subfocus, inline_state, builder);
        }
    }
    if detail.has_more_comments {
        builder
            .lines
            .push("  (more comments available)".to_string());
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
    builder: &mut ContentBuilder,
) {
    let prefix = if subfocus == PrDetailSubfocus::Comment(idx) {
        "> "
    } else {
        "- "
    };
    builder.lines.push(format!(
        "{}{}  {}",
        prefix, comment.author_login, comment.created_at
    ));
    for body_line in comment.body.lines() {
        builder.lines.push(format!("    {body_line}"));
    }

    if let InlineState::Composer {
        target: ComposerTarget::Reply { comment_index, .. },
        ..
    } = inline_state
        && *comment_index == idx
    {
        builder.lines.push(PR_REPLY_ANCHOR.to_string());
        builder
            .lines
            .push("    Ctrl+Enter save | Esc cancel".to_string());
    }

    builder.lines.push(String::new());
}

/// Build the new-comment section header/anchor.
///
/// @plan PLAN-20260624-PR-MODE.P14
/// @requirement REQ-PR-009
/// @requirement REQ-PR-010
/// @pseudocode component-001 lines 169-176
///
/// When a `NewComment` composer is active this emits ONLY the section label
/// and help hint (no flattened composer text/cursor) in the read-only document.
/// The composer text is rendered by the dedicated `TextBox` component, which
/// owns its own local viewport/caret invariant.
fn build_new_comment_section(
    subfocus: PrDetailSubfocus,
    inline_state: &InlineState,
    builder: &mut ContentBuilder,
) {
    let label = if subfocus == PrDetailSubfocus::NewComment {
        "> New comment"
    } else {
        "  New comment"
    };
    builder.lines.push(label.to_string());

    match inline_state {
        InlineState::Composer {
            target: ComposerTarget::NewComment,
            ..
        } => {
            // Stable anchor: the composer text/cursor is rendered by the
            // embedded TextBox component, NOT flattened here. Emit a hint so
            // the line count stays stable while typing.
            builder
                .lines
                .push("  Ctrl+Enter submit | Esc cancel".to_string());
        }
        _ => {
            builder.lines.push("  Press c to add a comment".to_string());
        }
    }
}

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
#[path = "pr_detail_content_tests.rs"]
mod tests;

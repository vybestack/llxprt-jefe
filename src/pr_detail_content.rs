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

/// Compute the inclusive content-line range `[start, end]` of the focused
/// subfocus item, so the reducer can scroll it into view (#151).
///
/// The range is derived from the rendered content by locating the focus
/// marker (`"> "`) or section label of the focused item. This is a pure
/// projection over the rendered text — no AppState, no side effects — and
/// uses the same `build_pr_detail_content` output the renderer paints, so it
/// cannot drift.
///
/// Returns `None` when the subfocus item cannot be located (e.g. empty
/// detail or an index beyond the available items) — the caller should leave
/// the offset unchanged in that case.
///
/// @plan PLAN-20260624-PR-MODE.P14
/// @requirement REQ-PR-009
#[must_use]
pub fn pr_subfocus_line_range(
    detail: &PullRequestDetail,
    subfocus: PrDetailSubfocus,
    inline_state: &InlineState,
    detail_loading: bool,
    comments_loading: bool,
) -> Option<(usize, usize)> {
    let content = build_pr_detail_content(
        detail,
        subfocus,
        inline_state,
        detail_loading,
        comments_loading,
    );
    let lines: Vec<&str> = content.text.lines().collect();
    pr_subfocus_range_from_lines(&lines, &subfocus)
}

/// Resolve the content-line range for `subfocus` by scanning the rendered
/// lines. The PR detail renders each focusable item with a `"> "` prefix
/// (reviews, checks, comments) or a `"> New comment"` label (NewComment) or
/// the `"Description"` label (Body). Review threads use a `">     ` prefix
/// (four spaces after the marker) to distinguish them from reviews.
fn pr_subfocus_range_from_lines(
    lines: &[&str],
    subfocus: &PrDetailSubfocus,
) -> Option<(usize, usize)> {
    match subfocus {
        PrDetailSubfocus::Body => pr_body_range(lines),
        PrDetailSubfocus::Review(target_idx) => {
            pr_section_single_line(lines, "Reviews", *target_idx, pr_line_is_review_header)
        }
        PrDetailSubfocus::ReviewThread(target_flat_idx) => pr_indexed_block(
            lines,
            *target_flat_idx,
            pr_line_is_review_thread_header,
            pr_thread_end_line,
        ),
        PrDetailSubfocus::Check(target_idx) => {
            pr_section_single_line(lines, "Checks", *target_idx, pr_line_is_check)
        }
        PrDetailSubfocus::Comment(target_idx) => pr_comment_range(lines, *target_idx),
        PrDetailSubfocus::NewComment => {
            let start = lines.iter().position(|l| *l == "> New comment")?;
            let end = (start + 1).min(lines.len().saturating_sub(1));
            Some((start, end))
        }
    }
}

/// Body section: the "Description" label through the line before the next
/// separator.
fn pr_body_range(lines: &[&str]) -> Option<(usize, usize)> {
    let start = lines.iter().position(|l| *l == "Description")?;
    Some((start, pr_find_section_end(lines, start)))
}

/// Find the content-line range of a named section (e.g. "Reviews", "Checks").
/// Returns `(start, end)` where `start` is the section-label line index and
/// `end` is the last line before the next separator (or the last content line
/// if no separator follows). The label match is a prefix match because the
/// rendered section header includes a parenthetical suffix (e.g.
/// "Reviews  (decision: APPROVED)").
fn pr_section_range(lines: &[&str], label: &str) -> Option<(usize, usize)> {
    let start = lines.iter().position(|l| l.starts_with(label))?;
    Some((start, pr_find_section_end(lines, start)))
}

/// Find the last line of a section (the line before the next separator). If
/// no separator follows, the last content line is used. Falls back to `start`
/// itself if the section is a single label line.
fn pr_find_section_end(lines: &[&str], start: usize) -> usize {
    if let Some((i, _)) = lines
        .iter()
        .enumerate()
        .skip(start + 1)
        .find(|(_, l)| l.starts_with('─'))
    {
        return i.saturating_sub(1);
    }
    lines.len().saturating_sub(1).max(start)
}

/// Scan for the `target_idx`-th line matching `predicate` within a named
/// section, returning its single-line range. Scoping to the section prevents
/// index drift from similar-looking lines in other sections (e.g. a review
/// body excerpt containing "pending" being miscounted as a check).
fn pr_section_single_line(
    lines: &[&str],
    section_label: &str,
    target_idx: usize,
    predicate: fn(&str) -> bool,
) -> Option<(usize, usize)> {
    let (start, end) = pr_section_range(lines, section_label)?;
    let mut count = 0usize;
    for (i, line) in lines.iter().enumerate().take(end + 1).skip(start + 1) {
        if predicate(line) {
            if count == target_idx {
                return Some((i, i));
            }
            count += 1;
        }
    }
    None
}

/// Scan for the `target_idx`-th header matching `predicate`, returning the
/// block range from the header through `end_fn`. Used for review threads.
fn pr_indexed_block(
    lines: &[&str],
    target_idx: usize,
    predicate: fn(&str) -> bool,
    end_fn: fn(&[&str], usize) -> usize,
) -> Option<(usize, usize)> {
    let mut count = 0usize;
    for (i, line) in lines.iter().enumerate() {
        if predicate(line) {
            if count == target_idx {
                let end = end_fn(lines, i);
                return Some((i, end));
            }
            count += 1;
        }
    }
    None
}

/// Comment range: comment lines live in the Comments section (after the
/// "Comments" label).
fn pr_comment_range(lines: &[&str], target_idx: usize) -> Option<(usize, usize)> {
    let comments_start = lines.iter().position(|l| *l == "Comments")?;
    let mut comment_count = 0usize;
    for (i, line) in lines.iter().enumerate().skip(comments_start + 1) {
        if pr_line_is_comment(line) {
            if comment_count == target_idx {
                let end = pr_comment_end_line(lines, i);
                return Some((i, end));
            }
            comment_count += 1;
        }
    }
    None
}

/// True for a review header line: starts with `"> "` or `"- "`, is NOT a
/// thread header (no 4-space indent), and contains a review state token.
fn pr_line_is_review_header(line: &str) -> bool {
    let Some(rest) = line.strip_prefix("> ").or_else(|| line.strip_prefix("- ")) else {
        return false;
    };
    if rest.starts_with("    ") {
        return false;
    }
    [
        "APPROVED",
        "CHANGES_REQUESTED",
        "COMMENTED",
        "PENDING",
        "DISMISSED",
        "REVIEW_REQUIRED",
    ]
    .iter()
    .any(|state| rest.contains(state))
}

/// True for a review-thread header line: marker (`">     ` or `      `)
/// followed by a path/location and a `[RESOLVED]`/`[UNRESOLVED]` tag.
fn pr_line_is_review_thread_header(line: &str) -> bool {
    let Some(rest) = line
        .strip_prefix(">     ")
        .or_else(|| line.strip_prefix("      "))
    else {
        return false;
    };
    rest.contains("[RESOLVED]") || rest.contains("[UNRESOLVED]")
}

/// True for a check line: prefix `"> "` or `"- "` + a check status label.
fn pr_line_is_check(line: &str) -> bool {
    let Some(rest) = line.strip_prefix("> ").or_else(|| line.strip_prefix("- ")) else {
        return false;
    };
    ["pending", "success", "failure", "neutral"]
        .iter()
        .any(|s| rest.contains(s))
}

/// True for a comment line in the Comments section: prefix marker plus
/// author login (no leading spaces) and date. The author/date separator is
/// two spaces, so we verify the line is NOT an indented body line and uses
/// the separator after stripping the marker prefix.
///
/// `rest` is the text after the marker prefix. The two-space separator
/// between the author login and the date distinguishes comment header lines
/// from indented thread body lines.
fn pr_line_is_comment(line: &str) -> bool {
    let Some(rest) = line.strip_prefix("> ").or_else(|| line.strip_prefix("- ")) else {
        return false;
    };
    if rest.starts_with("    ") {
        return false;
    }
    // Comment headers are `author  date`; body sub-lines are indented and
    // excluded by the 4-space check above. The two-space separator is the
    // distinguishing marker between the author and the date.
    rest.contains("  ")
}

/// Find the last content line of a review thread (the blank line terminates
/// the thread block).
fn pr_thread_end_line(lines: &[&str], start: usize) -> usize {
    let mut end = start;
    for (i, line) in lines.iter().enumerate().skip(start + 1) {
        if line.is_empty() {
            return i.saturating_sub(1).max(start);
        }
        if pr_line_is_review_thread_header(line) || line.starts_with('─') {
            return i.saturating_sub(1).max(start);
        }
        end = i;
    }
    end
}

/// Find the last content line of a comment block (the blank line that
/// terminates each comment).
fn pr_comment_end_line(lines: &[&str], start: usize) -> usize {
    for (i, line) in lines.iter().enumerate().skip(start + 1) {
        if line.is_empty() {
            return i.saturating_sub(1).max(start);
        }
    }
    start
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
        let mut flat_thread_idx = 0usize;
        for (idx, review) in detail.reviews.iter().enumerate() {
            build_single_review(idx, review, subfocus, builder);
            for thread in &review.review_threads {
                build_review_thread(flat_thread_idx, thread, subfocus, builder);
                flat_thread_idx += 1;
            }
        }
    }
}

/// Render a single review header line (author, state, submitted_at, body).
///
/// @plan PLAN-20260624-PR-MODE.P12
/// @requirement REQ-PR-009
fn build_single_review(
    idx: usize,
    review: &crate::domain::PrReview,
    subfocus: PrDetailSubfocus,
    builder: &mut ContentBuilder,
) {
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

/// Render a review thread with its comments (indented under the review).
///
/// @plan PLAN-20260624-PR-MODE.P12
/// @requirement REQ-PR-009
fn build_review_thread(
    flat_idx: usize,
    thread: &crate::domain::PrReviewThread,
    subfocus: PrDetailSubfocus,
    builder: &mut ContentBuilder,
) {
    let focused = subfocus == PrDetailSubfocus::ReviewThread(flat_idx);
    let marker = if focused { "> " } else { "  " };
    let resolve_tag = if thread.is_resolved {
        "[RESOLVED]"
    } else {
        "[UNRESOLVED]"
    };
    let location = match (&thread.path, thread.line) {
        (Some(path), Some(line)) => format!("{path}:{line}"),
        (Some(path), None) => path.clone(),
        (None, _) => "(no file)".to_string(),
    };
    builder
        .lines
        .push(format!("{marker}    {location}  {resolve_tag}"));
    for comment in &thread.comments {
        builder.lines.push(format!(
            "      {}  {}",
            comment.author_login, comment.created_at
        ));
        for body_line in comment.body.lines() {
            builder.lines.push(format!("        {body_line}"));
        }
    }
    if focused {
        builder
            .lines
            .push("      [ r reply ]  [ R resolve ]".to_string());
    }
    builder.lines.push(String::new());
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

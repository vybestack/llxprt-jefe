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
use unicode_width::UnicodeWidthChar;

/// Count the rendered scrollable lines for a PR detail.
///
/// Mirrors `issue_detail_content::detail_content_line_count` so the PR scroll
/// bounds derive from the REAL rendered length and cannot drift from what
/// `build_pr_detail_content` emits. `wrap_width` must match the width the
/// renderer passes so wrapped line counts agree (parity invariant).
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
    wrap_width: Option<usize>,
) -> usize {
    build_pr_detail_content(
        detail,
        subfocus,
        inline_state,
        detail_loading,
        comments_loading,
        wrap_width,
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
/// Comments, New comment. When a Composer is active the returned `cursor`
/// points at the composer line within the joined (and optionally wrapped)
/// content so the renderer can draw a caret. `wrap_width`, when `Some(w)` with
/// `w > 0`, reflows every line to at most `w` display chars and remaps the
/// cursor to the wrapped coordinates.
#[must_use]
pub fn build_pr_detail_content(
    detail: &PullRequestDetail,
    subfocus: PrDetailSubfocus,
    inline_state: &InlineState,
    detail_loading: bool,
    comments_loading: bool,
    wrap_width: Option<usize>,
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
    builder.finish(wrap_width)
}

/// Build a full-screen content block for creating a new PR comment.
///
/// @plan PLAN-20260624-PR-MODE.P12
/// @requirement REQ-PR-009
/// @pseudocode component-001 lines 1-12
///
/// Mirrors `build_new_issue_content` but for a PR comment composer. The cursor
/// points at the composer line when composer text is present.
#[must_use]
pub fn build_new_pr_comment_content(inline_state: &InlineState) -> DetailContent {
    let mut builder = ContentBuilder::new();
    builder.lines.push("New comment".to_string());
    builder
        .lines
        .push("Title: first line | Body: remaining lines".to_string());
    builder.lines.push(String::new());

    if let InlineState::Composer {
        target: ComposerTarget::NewComment,
        text,
        cursor: byte_cursor,
    } = inline_state
    {
        builder.push_composer_lines(text.as_str(), *byte_cursor, "  │ ");
    }

    builder
        .lines
        .push("Ctrl+Enter submit | Esc cancel".to_string());
    builder.finish(None)
}

/// Accumulator for joined content lines plus the optional inline cursor.
///
/// Mirrors `issue_detail_content::ContentBuilder`: plain section builders push
/// lines without touching the cursor; the composer sub-builders call
/// `push_composer_lines` which records the cursor position relative to the
/// WHOLE joined content (absolute line index + char column).
///
/// @plan PLAN-20260624-PR-MODE.P14
/// @requirement REQ-PR-009
/// @pseudocode component-001 lines 1-12
struct ContentBuilder {
    lines: Vec<String>,
    cursor_pos: Option<(usize, usize)>,
}

impl ContentBuilder {
    /// @plan PLAN-20260624-PR-MODE.P14
    /// @requirement REQ-PR-009
    /// @pseudocode component-001 lines 1-12
    fn new() -> Self {
        Self {
            lines: Vec::new(),
            cursor_pos: None,
        }
    }

    /// Push the `format!("{prefix}{line}")` lines for a composer/editor block
    /// and record the cursor at `(content_start + line_idx, prefix_chars + col)`.
    ///
    /// Preserves the existing prefix conventions (NewComment `"  │ "`, Reply
    /// `"    │ "`) and the trailing-empty-line behaviour when `text` ends with
    /// a newline. The byte cursor is clamped to a UTF-8 char boundary before
    /// slicing to prevent a panic on multibyte input.
    ///
    /// @plan PLAN-20260624-PR-MODE.P14
    /// @requirement REQ-PR-009
    /// @pseudocode component-001 lines 1-12
    fn push_composer_lines(&mut self, text: &str, byte_cursor: usize, prefix: &str) {
        let prefix_chars = prefix.chars().count();
        let content_start = self.lines.len();
        if text.is_empty() {
            // An empty composer still needs exactly one blank input row so the
            // caret lands on it (not the following help/controls line) and the
            // line count includes the expected input row. Mirror the
            // no-trailing-space form used for the ends_with('\n') case.
            self.lines.push(prefix.to_string());
        } else {
            for line in text.lines() {
                self.lines.push(format!("{prefix}{line}"));
            }
            if text.ends_with('\n') {
                self.lines.push(prefix.to_string());
            }
        }
        self.cursor_pos = Some(byte_cursor_to_line_col(
            text,
            byte_cursor,
            content_start,
            prefix_chars,
        ));
    }

    /// Join the accumulated lines and apply optional wrapping, remapping the
    /// cursor to the wrapped coordinates.
    ///
    /// @plan PLAN-20260624-PR-MODE.P14
    /// @requirement REQ-PR-009
    /// @pseudocode component-001 lines 1-12
    fn finish(self, wrap_width: Option<usize>) -> DetailContent {
        match wrap_width {
            Some(w) if w > 0 => {
                let (wrapped, mapped) = wrap_lines(&self.lines, self.cursor_pos, w);
                DetailContent {
                    text: wrapped.join("\n"),
                    cursor: mapped,
                }
            }
            _ => DetailContent {
                text: self.lines.join("\n"),
                cursor: self.cursor_pos,
            },
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
        text,
        cursor: byte_cursor,
    } = inline_state
        && *comment_index == idx
    {
        builder.lines.push("    [Reply]".to_string());
        builder.push_composer_lines(text.as_str(), *byte_cursor, "    │ ");
        builder
            .lines
            .push("    Ctrl+Enter save | Esc cancel".to_string());
    }

    builder.lines.push(String::new());
}

/// @plan PLAN-20260624-PR-MODE.P12
/// @requirement REQ-PR-009
/// @pseudocode component-001 lines 1-12
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

    if let InlineState::Composer {
        target: ComposerTarget::NewComment,
        text,
        cursor: byte_cursor,
    } = inline_state
    {
        builder.push_composer_lines(text.as_str(), *byte_cursor, "  │ ");
        builder
            .lines
            .push("  Ctrl+Enter submit | Esc cancel".to_string());
    } else {
        builder.lines.push("  Press c to add a comment".to_string());
    }
}

/// Map a byte cursor within `text` to an absolute `(line, char_col)` position.
///
/// Mirrors `issue_detail_content::byte_cursor_to_line_col`: clamp the byte
/// cursor to `text.len()`, walk it DOWN to the nearest UTF-8 char boundary
/// (so multibyte input cannot panic the slice), count newlines before it for
/// the line index, and char-count the remainder for the column.
///
/// @plan PLAN-20260624-PR-MODE.P14
/// @requirement REQ-PR-009
/// @pseudocode component-001 lines 1-12
fn byte_cursor_to_line_col(
    text: &str,
    byte_cursor: usize,
    content_line_start: usize,
    prefix_len: usize,
) -> (usize, usize) {
    let clamped = byte_cursor.min(text.len());
    let boundary = floor_char_boundary(text, clamped);
    let before = &text[..boundary];
    let line_idx = before.matches('\n').count();
    let last_nl = before.rfind('\n').map_or(0, |p| p + 1);
    let col = before[last_nl..].chars().count();
    (content_line_start + line_idx, prefix_len + col)
}

/// Walk `idx` down to the nearest UTF-8 char boundary at or before `idx`.
///
/// @plan PLAN-20260624-PR-MODE.P14
/// @requirement REQ-PR-009
/// @pseudocode component-001 lines 1-12
fn floor_char_boundary(text: &str, idx: usize) -> usize {
    let mut i = idx.min(text.len());
    while i > 0 && !text.is_char_boundary(i) {
        i -= 1;
    }
    i
}

/// Greedily wrap each line to at most `width` display columns and remap the
/// cursor `(line, col)` from the unwrapped char coordinates to the wrapped
/// display-column coordinates.
///
/// Wrapping uses DISPLAY width (via `unicode_width`) so CJK/full-width/emoji
/// lines do not overflow the pane. The cursor column is converted from a
/// char index to a display column consistent with this segmentation, and an
/// end-of-line cursor maps to the END of the final segment (not one past).
///
/// Wrapped continuation rows carry a HANGING INDENT equal to the logical
/// line's leading prefix (leading spaces, plus a composer gutter `"│ "` if
/// present right after the spaces). Continuation rows render the indent as
/// PLAIN SPACES of equal display width (the bar is not repeated) so wrapped
/// text aligns under the first row's content. The cursor column already
/// includes the prefix; the remap accounts for the indent on every row.
///
/// @plan PLAN-20260624-PR-MODE.P14
/// @requirement REQ-PR-009
/// @pseudocode component-001 lines 1-12
fn wrap_lines(
    lines: &[String],
    cursor: Option<(usize, usize)>,
    width: usize,
) -> (Vec<String>, Option<(usize, usize)>) {
    if width == 0 {
        return (lines.to_vec(), cursor);
    }
    let mut out: Vec<String> = Vec::new();
    // Start index (in `out`) of each original line's wrapped block.
    let mut wrapped_starts: Vec<usize> = Vec::with_capacity(lines.len());
    // Per-line wrap plan (prefix, content segments, content_width) for cursor
    // remapping.
    let mut plans: Vec<LineWrapPlan> = Vec::with_capacity(lines.len());
    for line in lines {
        wrapped_starts.push(out.len());
        let plan = wrap_single_line(line, width);
        for row in &plan.rows {
            out.push(row.clone());
        }
        plans.push(plan);
    }
    let mapped = cursor.map(|(line_idx, col)| {
        let block_start = *wrapped_starts.get(line_idx).unwrap_or(&0);
        let plan = plans.get(line_idx);
        let chars: Vec<char> = lines
            .get(line_idx)
            .map_or_else(Vec::new, |l| l.chars().collect());
        remap_cursor(plan, block_start, &chars, col)
    });
    (out, mapped)
}

/// Per-logical-line wrap plan: the rendered rows plus the metadata needed to
/// remap a cursor `(line, char_col)` onto the wrapped `(row, col)`.
///
/// `prefix_dw` is the display width of the hanging indent applied to EVERY
/// rendered row (row 0 carries the original prefix string; later rows carry
/// plain spaces of the same width). `content_width` is the wrap width used for
/// the content (after the prefix); it equals `width - prefix_dw`, or just
/// `width` when the prefix is too wide for a hanging indent (degenerate pane).
///
/// @plan PLAN-20260624-PR-MODE.P14
/// @requirement REQ-PR-009
/// @pseudocode component-001 lines 1-12
struct LineWrapPlan {
    rows: Vec<String>,
    prefix_dw: usize,
    content_width: usize,
}

/// Wrap a single logical line with a hanging indent. Extracts the leading
/// prefix (spaces + optional `"│ "` gutter), wraps the content after it, and
/// prepends the prefix to row 0 / plain spaces to continuation rows. Falls
/// back to wrapping the whole line at `width` (no indent) when the prefix is
/// too wide for `width`.
///
/// @plan PLAN-20260624-PR-MODE.P14
/// @requirement REQ-PR-009
/// @pseudocode component-001 lines 1-12
fn wrap_single_line(line: &str, width: usize) -> LineWrapPlan {
    let (prefix, prefix_char_count) = line_prefix(line);
    let prefix_dw: usize = prefix.chars().map(display_width_of).sum();
    let all_chars: Vec<char> = line.chars().collect();
    let content_chars: &[char] = &all_chars[prefix_char_count..];

    // Guard: if the prefix leaves no room for content, fall back to wrapping
    // the whole line at `width` with no hanging indent (degenerate narrow pane).
    if width <= prefix_dw {
        let segments = display_wrap_segments(&all_chars, width);
        let rows: Vec<String> = segments
            .iter()
            .map(|&(s, e)| all_chars[s..e].iter().collect())
            .collect();
        return LineWrapPlan {
            rows,
            prefix_dw: 0,
            content_width: width,
        };
    }
    let content_width = width - prefix_dw;
    let segments = if content_chars.is_empty() {
        vec![(0, 0)]
    } else {
        display_wrap_segments(content_chars, content_width)
    };
    let cont_indent: String = " ".repeat(prefix_dw);
    let rows: Vec<String> = segments
        .iter()
        .enumerate()
        .map(|(i, &(s, e))| {
            let body: String = content_chars[s..e].iter().collect();
            if i == 0 {
                format!("{prefix}{body}")
            } else {
                format!("{cont_indent}{body}")
            }
        })
        .collect();
    LineWrapPlan {
        rows,
        prefix_dw,
        content_width,
    }
}

/// Extract the leading prefix of a logical line for hanging-indent purposes:
/// the run of leading spaces, plus a composer gutter (`"│ "`) if it appears
/// immediately after the leading spaces. Returns `(prefix_str, prefix_char_count)`.
///
/// The prefix is what continuation rows indent to; the bar is NOT repeated on
/// continuation rows (plain spaces replace it in `wrap_single_line`). Counts
/// in CHARACTERS (not bytes) so the multibyte `│` is handled correctly.
///
/// @plan PLAN-20260624-PR-MODE.P14
/// @requirement REQ-PR-009
/// @pseudocode component-001 lines 1-12
fn line_prefix(line: &str) -> (String, usize) {
    let chars: Vec<char> = line.chars().collect();
    let leading_spaces = chars.iter().take_while(|&&c| c == ' ').count();
    // Detect a composer gutter "│ " right after the leading spaces.
    let prefix_char_count = if chars.get(leading_spaces) == Some(&'│')
        && chars.get(leading_spaces + 1) == Some(&' ')
    {
        leading_spaces + 2
    } else {
        leading_spaces
    };
    let prefix: String = chars[..prefix_char_count].iter().collect();
    (prefix, prefix_char_count)
}

/// Remap a cursor `(char_col within the whole logical line)` to the wrapped
/// `(row_in_block, col)` using the per-line wrap plan. The cursor column
/// already includes the prefix; the remap accounts for the hanging indent on
/// every row.
///
/// @plan PLAN-20260624-PR-MODE.P14
/// @requirement REQ-PR-009
/// @pseudocode component-001 lines 1-12
fn remap_cursor(
    plan: Option<&LineWrapPlan>,
    block_start: usize,
    chars: &[char],
    col: usize,
) -> (usize, usize) {
    let Some(plan) = plan else {
        return (block_start, col);
    };
    let display_col = char_index_to_display_col(chars, col);
    // A cursor at or before the end of the hanging prefix belongs on the first
    // wrapped row at its literal display column. Without this guard the
    // `saturating_sub(prefix_dw)` below would clamp every in-prefix column to 0
    // and then add `prefix_dw` back, snapping the caret to the prefix end.
    if display_col <= plan.prefix_dw {
        return (block_start, display_col);
    }
    // When there is no hanging indent (degenerate fallback), remap against the
    // whole line at `content_width` (== width) with prefix_dw 0.
    let content_display_col = display_col.saturating_sub(plan.prefix_dw);
    let row = if content_display_col == 0 {
        0
    } else {
        (content_display_col - 1) / plan.content_width
    };
    let num_rows = plan.rows.len();
    let row = row.min(num_rows.saturating_sub(1));
    let col_in_row = content_display_col - row * plan.content_width;
    let abs_col = plan.prefix_dw + col_in_row;
    (block_start + row, abs_col)
}

/// Segment `chars` into `(start_idx, end_idx)` ranges, each occupying at most
/// `width` display columns. Never splits a single wide char across rows; if a
/// char's own width exceeds the remaining space it starts a new segment.
///
/// @plan PLAN-20260624-PR-MODE.P14
/// @requirement REQ-PR-009
/// @pseudocode component-001 lines 1-12
fn display_wrap_segments(chars: &[char], width: usize) -> Vec<(usize, usize)> {
    let mut segments: Vec<(usize, usize)> = Vec::new();
    let mut start = 0usize;
    let mut acc = 0usize;
    for (i, &ch) in chars.iter().enumerate() {
        let cw = display_width_of(ch);
        if acc + cw > width && acc > 0 {
            segments.push((start, i));
            start = i;
            acc = 0;
        }
        acc += cw;
    }
    segments.push((start, chars.len()));
    segments
}

/// Convert a char index within a logical line to a DISPLAY column position by
/// accumulating the display width of each char before the index.
///
/// @plan PLAN-20260624-PR-MODE.P14
/// @requirement REQ-PR-009
/// @pseudocode component-001 lines 1-12
fn char_index_to_display_col(chars: &[char], char_idx: usize) -> usize {
    let idx = char_idx.min(chars.len());
    chars[..idx].iter().map(|&ch| display_width_of(ch)).sum()
}

/// Display width of a single char, treating control/zero-width as 0.
///
/// @plan PLAN-20260624-PR-MODE.P14
/// @requirement REQ-PR-009
/// @pseudocode component-001 lines 1-12
fn display_width_of(ch: char) -> usize {
    UnicodeWidthChar::width(ch).unwrap_or(0)
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

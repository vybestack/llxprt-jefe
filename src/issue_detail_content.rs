//! Shared issue-detail content construction.
//!
//! The Issues state layer uses this module for scroll bounds and the UI layer
//! uses it for rendering, so rendered line counts cannot drift from scroll
//! limits.

use crate::domain::{IssueComment, IssueDetail};
use crate::state::{ComposerTarget, DetailSubfocus, EditorTarget, InlineState};

/// Stable anchor rendered where a reply TextBox is logically attached.
pub(crate) const ISSUE_REPLY_ANCHOR: &str = "    [Reply]";

/// Scrollable issue-detail content and optional inline cursor position.
pub struct DetailContent {
    pub text: String,
    /// Cursor position as (line_index, char_column) within the content lines.
    pub cursor: Option<(usize, usize)>,
}

/// Count the rendered scrollable lines for an issue detail.
#[must_use]
pub fn detail_content_line_count(
    detail: &IssueDetail,
    inline_state: &InlineState,
    comments_loading: bool,
) -> usize {
    build_detail_lines(detail, DetailSubfocus::Body, inline_state, comments_loading).len()
}

/// Compute the inclusive content-line range `[start, end]` of the focused
/// issue detail subfocus item, so the reducer can scroll it into view (#151).
///
/// This is a pure projection over the rendered content — no AppState, no side
/// effects — and uses the same `build_detail_content` output the renderer
/// paints, so it cannot drift.
///
/// Returns `None` when the subfocus item cannot be located (e.g. empty
/// detail or an index beyond the available comments) — the caller should
/// leave the offset unchanged in that case.
#[must_use]
pub fn issue_subfocus_line_range(
    detail: &IssueDetail,
    subfocus: DetailSubfocus,
    inline_state: &InlineState,
    comments_loading: bool,
) -> Option<(usize, usize)> {
    let content = build_detail_content(detail, subfocus, inline_state, comments_loading);
    let lines: Vec<&str> = content.text.lines().collect();
    issue_subfocus_range_from_lines(&lines, &subfocus)
}

/// Resolve the content-line range for `subfocus` by scanning the rendered
/// lines. Issue detail renders Body with a `"> Body"` label, comments with a
/// `"> "` prefix, and NewComment with a `"> New Comment"` label.
fn issue_subfocus_range_from_lines(
    lines: &[&str],
    subfocus: &DetailSubfocus,
) -> Option<(usize, usize)> {
    match subfocus {
        DetailSubfocus::Body => {
            let start = lines
                .iter()
                .position(|l| *l == "> Body" || *l == "  Body")?;
            // Body spans from its label to the line before the next separator.
            // Fall back to the last content line if no separator follows.
            let end = find_section_end(lines, start);
            Some((start, end))
        }
        DetailSubfocus::Comment(target_idx) => {
            // Comments live in the Comments section (after the "Comments"
            // label). Each comment starts with "> " or "  " + author + date.
            let comments_start = lines.iter().position(|l| *l == "Comments")?;
            let mut comment_count = 0usize;
            for (i, line) in lines.iter().enumerate().skip(comments_start + 1) {
                if issue_line_is_comment(line) {
                    if comment_count == *target_idx {
                        let end = issue_comment_end_line(lines, i);
                        return Some((i, end));
                    }
                    comment_count += 1;
                }
            }
            None
        }
        DetailSubfocus::NewComment => {
            let start = lines
                .iter()
                .position(|l| *l == "> New Comment" || *l == "  New Comment")?;
            // NewComment is the last section; it spans from its label to the
            // end of content (no separator follows). This adapts to the
            // variable-length composer editor when a NewComment composer is
            // active (the editor pushes several rows into the document).
            let end = lines.len().saturating_sub(1).max(start);
            Some((start, end))
        }
    }
}

/// True for a comment header line in the Comments section: prefix `"> "` or
/// `  "` + `@author` + date. Editor gutter lines (`"│"`) and reply anchors
/// (`"[Reply]"`) are excluded. Body sub-lines (6-space indent rendered as
/// `"  "  body text"` after stripping the 2-space prefix → 4-space indent) are
/// excluded because they start with spaces, not `@`.
fn issue_line_is_comment(line: &str) -> bool {
    let Some(rest) = line.strip_prefix("> ").or_else(|| line.strip_prefix("  ")) else {
        return false;
    };
    // Editor gutter lines, reply anchors, and editing markers start with
    // special chars; comment headers start with `@author`.
    rest.starts_with('@')
}

/// Find the last line of a section (the line before the next separator). If
/// no separator follows, the last content line is used. Falls back to
/// `start` itself if the section is a single label line.
fn find_section_end(lines: &[&str], start: usize) -> usize {
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

/// Find the last content line of a comment block (the blank line that
/// terminates each comment, or the next separator).
fn issue_comment_end_line(lines: &[&str], start: usize) -> usize {
    for (i, line) in lines.iter().enumerate().skip(start + 1) {
        if line.is_empty() || line.starts_with('─') {
            return i.saturating_sub(1).max(start);
        }
    }
    start
}

/// Build the scrollable content string for the body + comments + new-comment area.
#[must_use]
pub fn build_detail_content(
    detail: &IssueDetail,
    subfocus: DetailSubfocus,
    inline_state: &InlineState,
    comments_loading: bool,
) -> DetailContent {
    let mut builder = ContentBuilder::new();
    build_detail_lines_into(
        detail,
        subfocus,
        inline_state,
        comments_loading,
        &mut builder,
    );
    builder.finish()
}

/// Build a full-screen content block for creating a new issue.
#[must_use]
pub fn build_new_issue_content(inline_state: &InlineState) -> DetailContent {
    let mut builder = ContentBuilder::new();

    builder.lines.push("New Issue".to_string());
    builder
        .lines
        .push("Title: first line | Body: remaining lines".to_string());
    builder.lines.push(String::new());

    if let InlineState::Composer {
        target: ComposerTarget::NewIssue,
        text,
        cursor,
    } = inline_state
    {
        builder.push_editor_lines(text.as_str(), *cursor, true, "  │ ", "  │ ");
    }

    builder
        .lines
        .push(String::from("Ctrl+Enter submit | Esc cancel"));
    builder.finish()
}

fn build_detail_lines(
    detail: &IssueDetail,
    subfocus: DetailSubfocus,
    inline_state: &InlineState,
    comments_loading: bool,
) -> Vec<String> {
    let mut builder = ContentBuilder::new();
    build_detail_lines_into(
        detail,
        subfocus,
        inline_state,
        comments_loading,
        &mut builder,
    );
    builder.lines
}

fn build_detail_lines_into(
    detail: &IssueDetail,
    subfocus: DetailSubfocus,
    inline_state: &InlineState,
    comments_loading: bool,
    builder: &mut ContentBuilder,
) {
    build_body_section(detail, subfocus, inline_state, builder);
    builder
        .lines
        .push("─────────────────────────────────────────".to_string());
    build_comments_section(detail, subfocus, inline_state, comments_loading, builder);
    builder
        .lines
        .push("─────────────────────────────────────────".to_string());
    build_new_comment_section(subfocus, inline_state, builder);
}

struct ContentBuilder {
    lines: Vec<String>,
    cursor_pos: Option<(usize, usize)>,
}

impl ContentBuilder {
    fn new() -> Self {
        Self {
            lines: Vec::new(),
            cursor_pos: None,
        }
    }

    fn push_editor_lines(
        &mut self,
        text: &str,
        cursor: usize,
        editing: bool,
        prefix_edit: &str,
        prefix_view: &str,
    ) {
        let prefix = if editing { prefix_edit } else { prefix_view };
        let prefix_len = prefix.chars().count();
        let content_start = self.lines.len();
        if text.is_empty() {
            self.lines.push(format!("{prefix}(empty)"));
        } else {
            for line in text.lines() {
                self.lines.push(format!("{prefix}{line}"));
            }
            if text.ends_with(char::from(0x0Au8)) {
                self.lines.push(prefix.to_string());
            }
        }
        if editing {
            self.cursor_pos = Some(byte_cursor_to_line_col(
                text,
                cursor,
                content_start,
                prefix_len,
            ));
        }
    }

    fn finish(self) -> DetailContent {
        let nl = String::from(char::from(0x0Au8));
        DetailContent {
            text: self.lines.join(&nl),
            cursor: self.cursor_pos,
        }
    }
}

fn build_body_section(
    detail: &IssueDetail,
    subfocus: DetailSubfocus,
    inline_state: &InlineState,
    builder: &mut ContentBuilder,
) {
    let body_label = if subfocus == DetailSubfocus::Body {
        "> Body"
    } else {
        "  Body"
    };
    builder.lines.push(body_label.to_string());

    let (body_text, body_cursor, body_editing) = match inline_state {
        InlineState::Editor {
            target: EditorTarget::IssueBody,
            text,
            cursor,
        } => (text.as_str(), *cursor, true),
        _ => (detail.body.as_str(), 0, false),
    };

    if body_editing {
        builder.lines.push("[editing]".to_string());
    }
    if body_editing {
        // While editing, render the raw composer text with a gutter so the
        // caret projection matches the live TextBox.
        builder.push_editor_lines(body_text, body_cursor, true, "  │ ", "    ");
    } else {
        // View mode: render the markdown body through comrak instead of
        // dumping it raw (issue #155 — shared with the PR detail bug).
        let mut rendered = false;
        for line in crate::markdown_render::render_markdown_lines(body_text) {
            builder.lines.push(format!("    {line}"));
            rendered = true;
        }
        if !rendered {
            builder.lines.push("    (no description)".to_string());
        }
    }
    if body_editing {
        builder
            .lines
            .push("  Ctrl+Enter save | Esc cancel".to_string());
    }
}

fn build_comments_section(
    detail: &IssueDetail,
    subfocus: DetailSubfocus,
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
}

fn build_single_comment(
    idx: usize,
    comment: &IssueComment,
    subfocus: DetailSubfocus,
    inline_state: &InlineState,
    builder: &mut ContentBuilder,
) {
    let prefix = if subfocus == DetailSubfocus::Comment(idx) {
        "> "
    } else {
        "  "
    };
    builder.lines.push(format!(
        "{}@{}  {}",
        prefix, comment.author_login, comment.created_at
    ));

    let (comment_text, comment_cursor, comment_editing) = match inline_state {
        InlineState::Editor {
            target: EditorTarget::Comment { comment_index },
            text,
            cursor,
        } if *comment_index == idx => (text.as_str(), *cursor, true),
        _ => (comment.body.as_str(), 0, false),
    };

    if comment_editing {
        builder.lines.push("  [editing]".to_string());
    }
    if comment_editing {
        builder.push_editor_lines(comment_text, comment_cursor, true, "    │ ", "      ");
    } else {
        // View mode: render the comment body as markdown (issue #155).
        for line in crate::markdown_render::render_markdown_lines(comment_text) {
            builder.lines.push(format!("      {line}"));
        }
    }
    if comment_editing {
        builder
            .lines
            .push("    Ctrl+Enter save | Esc cancel".to_string());
    }

    if let InlineState::Composer {
        target: ComposerTarget::Reply { comment_index, .. },
        ..
    } = inline_state
        && *comment_index == idx
    {
        builder.lines.push(ISSUE_REPLY_ANCHOR.to_string());
        builder
            .lines
            .push("    Ctrl+Enter save | Esc cancel".to_string());
    }

    builder.lines.push(String::new());
}

fn build_new_comment_section(
    subfocus: DetailSubfocus,
    inline_state: &InlineState,
    builder: &mut ContentBuilder,
) {
    let nc_label = if subfocus == DetailSubfocus::NewComment {
        "> New Comment"
    } else {
        "  New Comment"
    };
    builder.lines.push(nc_label.to_string());

    if let InlineState::Composer {
        target: ComposerTarget::NewComment,
        ..
    } = inline_state
    {
        builder
            .lines
            .push("  Ctrl+Enter submit | Esc cancel".to_string());
    } else {
        builder.lines.push("  Press c to add a comment".to_string());
    }
}

fn byte_cursor_to_line_col(
    text: &str,
    byte_cursor: usize,
    content_line_start: usize,
    prefix_len: usize,
) -> (usize, usize) {
    let clamped = byte_cursor.min(text.len());
    let before = &text[..clamped];
    let line_idx = before.matches(char::from(0x0Au8)).count();
    let last_nl = before.rfind(char::from(0x0Au8)).map_or(0, |p| p + 1);
    let col = before[last_nl..].chars().count();
    (content_line_start + line_idx, prefix_len + col)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::{IssueComment, IssueDetail, IssueState};

    fn detail_with_body(body: &str) -> IssueDetail {
        IssueDetail {
            repo_owner_name: "owner/repo".to_string(),
            number: 1,
            title: "Title".to_string(),
            state: IssueState::Open,
            author_login: "octocat".to_string(),
            created_at: String::new(),
            updated_at: String::new(),
            labels: Vec::new(),
            assignees: Vec::new(),
            milestone: None,
            body: body.to_string(),
            external_url: String::new(),
            comments: Vec::new(),
            has_more_comments: false,
            comments_cursor: None,
        }
    }

    fn comment(body: &str) -> IssueComment {
        IssueComment {
            comment_id: 1,
            author_login: "octocat".to_string(),
            created_at: String::new(),
            edited_at: None,
            body: body.to_string(),
        }
    }

    /// The scroll-limit line count MUST equal the number of lines the renderer
    /// derives from the joined content string. They diverge only if the final
    /// builder line is empty (because `str::lines()` drops a trailing newline),
    /// so these cases guard against that drift.
    fn assert_count_matches_rendered(
        detail: &IssueDetail,
        inline_state: &InlineState,
        comments_loading: bool,
    ) {
        let count = detail_content_line_count(detail, inline_state, comments_loading);
        let rendered =
            build_detail_content(detail, DetailSubfocus::Body, inline_state, comments_loading);
        let rendered_lines = if rendered.text.is_empty() {
            0
        } else {
            rendered.text.lines().count()
        };
        assert_eq!(
            count, rendered_lines,
            "line count drifted from rendered content"
        );
    }

    #[test]
    fn count_matches_rendered_for_empty_body_no_comments() {
        let detail = detail_with_body("");
        assert_count_matches_rendered(&detail, &InlineState::None, false);
    }

    #[test]
    fn count_matches_rendered_for_trailing_newline_body() {
        let detail = detail_with_body("first line\nsecond line\n");
        assert_count_matches_rendered(&detail, &InlineState::None, false);
    }

    #[test]
    fn count_matches_rendered_with_comments_and_trailing_newline() {
        let mut detail = detail_with_body("body\n");
        detail.comments = vec![comment("comment body\n"), comment("another")];
        assert_count_matches_rendered(&detail, &InlineState::None, false);
    }

    #[test]
    fn count_matches_rendered_while_comments_loading() {
        let detail = detail_with_body("body");
        assert_count_matches_rendered(&detail, &InlineState::None, true);
    }

    #[test]
    fn count_matches_rendered_with_new_comment_composer() {
        let detail = detail_with_body("body");
        let inline = InlineState::Composer {
            target: ComposerTarget::NewComment,
            text: "draft comment\n".to_string(),
            cursor: 0,
        };
        assert_count_matches_rendered(&detail, &inline, false);
    }

    #[test]
    fn count_matches_rendered_while_editing_body() {
        let detail = detail_with_body("body");
        let inline = InlineState::Editor {
            target: EditorTarget::IssueBody,
            text: "edited body\n".to_string(),
            cursor: 0,
        };
        assert_count_matches_rendered(&detail, &inline, false);
    }

    fn detail_with_comments(n: usize) -> IssueDetail {
        let mut detail = detail_with_body("body line one\nbody line two");
        detail.comments = (0..n)
            .map(|i| comment(format!("comment {i} body").as_str()))
            .collect();
        detail
    }

    fn require_range(
        detail: &IssueDetail,
        subfocus: DetailSubfocus,
        inline: &InlineState,
        loading: bool,
    ) -> (usize, usize) {
        let Some(range) = issue_subfocus_line_range(detail, subfocus, inline, loading) else {
            panic!("expected subfocus range for {subfocus:?}");
        };
        range
    }

    // ── issue_subfocus_line_range (#151) ─────────────────────────────────

    #[test]
    fn issue_subfocus_line_range_body_covers_body_section() {
        let detail = detail_with_body("body line");
        let range = require_range(&detail, DetailSubfocus::Body, &InlineState::None, false);
        let content =
            build_detail_content(&detail, DetailSubfocus::Body, &InlineState::None, false);
        let lines: Vec<&str> = content.text.lines().collect();
        // The range starts at the Body label.
        assert!(lines[range.0] == "> Body" || lines[range.0] == "  Body");
        // The line after the range must be the separator.
        if range.1 + 1 < lines.len() {
            assert!(
                lines[range.1 + 1].starts_with('─'),
                "line after Body range must be a separator"
            );
        }
    }

    #[test]
    fn issue_subfocus_line_range_comment_locates_focused_comment() {
        let detail = detail_with_comments(2);
        let range = require_range(
            &detail,
            DetailSubfocus::Comment(0),
            &InlineState::None,
            false,
        );
        let content = build_detail_content(
            &detail,
            DetailSubfocus::Comment(0),
            &InlineState::None,
            false,
        );
        let lines: Vec<&str> = content.text.lines().collect();
        let header = lines[range.0];
        assert!(
            header.starts_with("> "),
            "focused comment must have > marker"
        );
        // The range must span the full comment block (header + body lines),
        // ending at the blank line that terminates the comment.
        let block: Vec<&str> = lines[range.0..=range.1].to_vec();
        assert!(
            block
                .iter()
                .any(|l| l.contains("comment 0 body") || l.contains("comment 0")),
            "comment block must include the comment body within its range: {block:?}"
        );
    }

    #[test]
    fn issue_subfocus_line_range_new_comment_locates_label_and_hint() {
        let detail = detail_with_body("body");
        let range = require_range(
            &detail,
            DetailSubfocus::NewComment,
            &InlineState::None,
            false,
        );
        let content = build_detail_content(
            &detail,
            DetailSubfocus::NewComment,
            &InlineState::None,
            false,
        );
        let lines: Vec<&str> = content.text.lines().collect();
        assert!(lines[range.0] == "> New Comment" || lines[range.0] == "  New Comment");
        assert!(
            lines[range.1].contains("Press c to add a comment")
                || lines[range.1].contains("Ctrl+Enter submit"),
            "NewComment range must include the hint line"
        );
    }

    #[test]
    fn issue_subfocus_line_range_returns_none_for_out_of_bounds_comment() {
        let detail = detail_with_comments(1);
        let range = issue_subfocus_line_range(
            &detail,
            DetailSubfocus::Comment(99),
            &InlineState::None,
            false,
        );
        assert!(range.is_none(), "out-of-bounds comment must return None");
    }

    #[test]
    fn issue_subfocus_line_range_comment_1_locates_second_comment() {
        let detail = detail_with_comments(2);
        let range = require_range(
            &detail,
            DetailSubfocus::Comment(1),
            &InlineState::None,
            false,
        );
        let content = build_detail_content(
            &detail,
            DetailSubfocus::Comment(1),
            &InlineState::None,
            false,
        );
        let lines: Vec<&str> = content.text.lines().collect();
        let block: Vec<&str> = lines[range.0..=range.1].to_vec();
        assert!(
            block.iter().any(|l| l.contains("comment 1")),
            "Comment(1) must locate the second comment, got block: {block:?}"
        );
        // The header line must start with the comment marker, not a body line.
        let header = lines[range.0];
        assert!(
            header
                .strip_prefix("> ")
                .is_some_and(|r| r.starts_with('@'))
                || header
                    .strip_prefix("  ")
                    .is_some_and(|r| r.starts_with('@')),
            "Comment(1) header must be a comment header starting with @, got: {header:?}"
        );
    }

    #[test]
    fn issue_subfocus_line_range_comment_not_confused_by_body_lines() {
        // Two comments with multi-line bodies. Body lines are indented and must
        // NOT be miscounted as comment headers.
        let mut detail = detail_with_body("body");
        detail.comments = vec![
            comment(
                "comment 0 line 1
comment 0 line 2",
            ),
            comment(
                "comment 1 line 1
comment 1 line 2",
            ),
        ];
        let range = require_range(
            &detail,
            DetailSubfocus::Comment(1),
            &InlineState::None,
            false,
        );
        let content = build_detail_content(
            &detail,
            DetailSubfocus::Comment(1),
            &InlineState::None,
            false,
        );
        let lines: Vec<&str> = content.text.lines().collect();
        let header = lines[range.0];
        assert!(
            header
                .strip_prefix("> ")
                .is_some_and(|r| r.starts_with('@'))
                || header
                    .strip_prefix("  ")
                    .is_some_and(|r| r.starts_with('@')),
            "Comment(1) must point at a header line, not a body line: {header:?}"
        );
        let block: Vec<&str> = lines[range.0..=range.1].to_vec();
        assert!(
            block.iter().any(|l| l.contains("comment 1")),
            "Comment(1) block must contain the second comment body, got: {block:?}"
        );
    }
}

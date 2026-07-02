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
    builder.push_editor_lines(body_text, body_cursor, body_editing, "  │ ", "    ");
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
    builder.push_editor_lines(
        comment_text,
        comment_cursor,
        comment_editing,
        "    │ ",
        "      ",
    );
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
}

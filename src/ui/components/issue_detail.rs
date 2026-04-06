//! Unified issue detail + comments view.
//! @plan PLAN-20260329-ISSUES-MODE.P12
//! @plan PLAN-20260329-ISSUES-MODE.P14
//! @requirement REQ-ISS-009

use iocraft::prelude::*;

use crate::domain::{IssueComment, IssueDetail, IssueState};
use crate::state::{ComposerTarget, DetailSubfocus, InlineState};
use crate::theme::{ResolvedColors, ThemeColors};

use super::scrollable_text::ScrollableText;

/// Fixed number of rows the metadata header occupies (title, state, labels, url, separator).
const HEADER_ROWS: usize = 5;

/// Overhead rows outside the detail pane:
///   status bar (1) + keybind bar (1) + detail border (2) = 4
/// The issue list occupies ~30% of the remaining height.
const CHROME_ROWS: usize = 4;

/// Convert a byte-offset cursor in raw editor text to (line_index, char_column)
/// relative to the content lines built by `build_detail_content`.
///
/// `content_line_start` is the index in the output `lines` vec where this editor's
/// text lines begin. `prefix_len` is the char-length of the prefix prepended to each line.
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

/// Result of building detail content: the display string and optional cursor position.
struct DetailContent {
    text: String,
    /// Cursor position as (line_index, char_column) within the content lines.
    cursor: Option<(usize, usize)>,
}

/// Accumulated state for building detail content lines.
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

    /// Push editor/viewer lines and compute cursor position if editing.
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
}

/// Build body section lines.
fn build_body_section(
    detail: &IssueDetail,
    subfocus: DetailSubfocus,
    inline_state: &InlineState,
    builder: &mut ContentBuilder,
) {
    let body_focused = subfocus == DetailSubfocus::Body;
    let body_label = if body_focused { "> Body" } else { "  Body" };
    builder.lines.push(body_label.to_string());

    let (body_text, body_cursor, body_editing) = match inline_state {
        InlineState::Editor {
            target: crate::state::EditorTarget::IssueBody,
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

/// Build comments section lines.
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

/// Build lines for a single comment (including inline editor/reply).
fn build_single_comment(
    idx: usize,
    comment: &IssueComment,
    subfocus: DetailSubfocus,
    inline_state: &InlineState,
    builder: &mut ContentBuilder,
) {
    let comment_focused = subfocus == DetailSubfocus::Comment(idx);
    let prefix = if comment_focused { "> " } else { "  " };
    builder.lines.push(format!(
        "{}@{}  {}",
        prefix, comment.author_login, comment.created_at
    ));

    let (cmt_text, cmt_cursor, cmt_editing) = match inline_state {
        InlineState::Editor {
            target: crate::state::EditorTarget::Comment { comment_index },
            text,
            cursor,
        } if *comment_index == idx => (text.as_str(), *cursor, true),
        _ => (comment.body.as_str(), 0, false),
    };

    if cmt_editing {
        builder.lines.push("  [editing]".to_string());
    }
    builder.push_editor_lines(cmt_text, cmt_cursor, cmt_editing, "    │ ", "      ");
    if cmt_editing {
        builder
            .lines
            .push("    Ctrl+Enter save | Esc cancel".to_string());
    }

    if let InlineState::Composer {
        target: crate::state::ComposerTarget::Reply { comment_index, .. },
        text,
        cursor,
    } = inline_state
        && *comment_index == idx
    {
        builder.lines.push("    [Reply]".to_string());
        builder.push_editor_lines(text.as_str(), *cursor, true, "    │ ", "    │ ");
        builder
            .lines
            .push("    Ctrl+Enter save | Esc cancel".to_string());
    }

    builder.lines.push(String::new());
}

/// Build new-comment section lines.
fn build_new_comment_section(
    subfocus: DetailSubfocus,
    inline_state: &InlineState,
    builder: &mut ContentBuilder,
) {
    let nc_focused = subfocus == DetailSubfocus::NewComment;
    let nc_label = if nc_focused {
        "> New Comment"
    } else {
        "  New Comment"
    };
    builder.lines.push(nc_label.to_string());

    if let InlineState::Composer {
        target: crate::state::ComposerTarget::NewComment,
        text,
        cursor,
    } = inline_state
    {
        builder.push_editor_lines(text.as_str(), *cursor, true, "  │ ", "  │ ");
        builder
            .lines
            .push("  Ctrl+Enter submit | Esc cancel".to_string());
    } else {
        builder.lines.push("  Press c to add a comment".to_string());
    }
}

/// Build the scrollable content string for the body + comments + new-comment area.
fn build_detail_content(
    detail: &IssueDetail,
    subfocus: DetailSubfocus,
    inline_state: &InlineState,
    comments_loading: bool,
) -> DetailContent {
    let nl = String::from(char::from(0x0Au8));
    let mut builder = ContentBuilder::new();

    build_body_section(detail, subfocus, inline_state, &mut builder);
    builder
        .lines
        .push("─────────────────────────────────────────".to_string());
    build_comments_section(
        detail,
        subfocus,
        inline_state,
        comments_loading,
        &mut builder,
    );
    builder
        .lines
        .push("─────────────────────────────────────────".to_string());
    build_new_comment_section(subfocus, inline_state, &mut builder);

    DetailContent {
        text: builder.lines.join(&nl),
        cursor: builder.cursor_pos,
    }
}

/// Build a full-screen content block for creating a new issue.
fn build_new_issue_content(inline_state: &InlineState) -> DetailContent {
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

    DetailContent {
        text: builder.lines.join("\n"),
        cursor: builder.cursor_pos,
    }
}

/// Props for the issue detail view.
#[derive(Default, Props)]
pub struct IssueDetailViewProps {
    /// Full issue detail (metadata, body, comments).
    pub issue_detail: Option<IssueDetail>,
    /// Which sub-element is focused within the detail view.
    pub detail_subfocus: DetailSubfocus,
    /// Active inline editor/composer state.
    pub inline_state: InlineState,
    /// Whether comments are loading.
    pub comments_loading: bool,
    /// Whether this pane is focused.
    pub focused: bool,
    /// Scroll offset for the content viewport.
    pub scroll_offset: usize,
    /// Theme colors.
    pub colors: ThemeColors,
}

/// Issue detail view — fixed structure that NEVER changes layout.
///
/// ALWAYS renders: border box → 5 header rows → fixed-row scrollable viewport.
/// When no issue is selected, header rows are blank and viewport shows a message.
/// This ensures layout is identical regardless of whether an issue is loaded.
///
/// @plan PLAN-20260329-ISSUES-MODE.P14
/// @requirement REQ-ISS-009
#[component]
pub fn IssueDetailView(props: &IssueDetailViewProps) -> impl Into<AnyElement<'static>> {
    let rc = ResolvedColors::from_theme(Some(&props.colors));
    let border_style = if props.focused {
        BorderStyle::Double
    } else {
        BorderStyle::Round
    };

    // Compute viewport rows from terminal height.
    // The iocraft layout gives the detail pane ~70% of workspace via flex_grow.
    // We compute the same 70% here to know how many fixed-height row children to emit.
    let term_rows = crossterm::terminal::size().map_or(40, |(_, h)| h as usize);
    let workspace_rows = term_rows.saturating_sub(CHROME_ROWS);
    let list_rows = workspace_rows * 3 / 10; // 30% for issue list
    let detail_pane_rows = workspace_rows.saturating_sub(list_rows);
    // Subtract header rows and border (2 rows for top+bottom border)
    let scroll_rows = detail_pane_rows.saturating_sub(HEADER_ROWS + 2).max(5);

    // Build header and content.
    let showing_new_issue_composer = matches!(
        &props.inline_state,
        InlineState::Composer {
            target: ComposerTarget::NewIssue,
            ..
        }
    );

    let (h_title, h_state, h_labels, h_url, detail_content, state_color) =
        if showing_new_issue_composer {
            (
                "New Issue".to_string(),
                "Draft".to_string(),
                String::new(),
                String::new(),
                build_new_issue_content(&props.inline_state),
                rc.bright,
            )
        } else if let Some(detail) = props.issue_detail.as_ref() {
            let state_tag = match detail.state {
                IssueState::Open => "OPEN",
                IssueState::Closed => "CLOSED",
            };
            let sc = match detail.state {
                IssueState::Open => rc.bright,
                IssueState::Closed => rc.dim,
            };
            let labels_str = if detail.labels.is_empty() {
                "-".to_string()
            } else {
                detail.labels.join(", ")
            };
            let assignees_str = if detail.assignees.is_empty() {
                "-".to_string()
            } else {
                detail.assignees.join(", ")
            };
            let milestone_str = detail.milestone.as_deref().unwrap_or("-").to_string();

            (
                format!("#{} {}", detail.number, detail.title),
                format!(
                    "{}  by @{}  opened: {}  updated: {}",
                    state_tag, detail.author_login, detail.created_at, detail.updated_at
                ),
                format!(
                    "labels: {labels_str}  assignees: {assignees_str}  milestone: {milestone_str}"
                ),
                detail.external_url.clone(),
                build_detail_content(
                    detail,
                    props.detail_subfocus,
                    &props.inline_state,
                    props.comments_loading,
                ),
                sc,
            )
        } else {
            (
                String::new(),
                String::new(),
                String::new(),
                String::new(),
                DetailContent {
                    text: "No issue selected".to_string(),
                    cursor: None,
                },
                rc.dim,
            )
        };

    element! {
        Box(
            flex_direction: FlexDirection::Column,
            width: 100pct,
            height: 100pct,
            border_style: border_style,
            border_color: rc.border,
            background_color: rc.bg,
        ) {
            // ── Metadata header — always exactly HEADER_ROWS rows ─────────
            Box(flex_direction: FlexDirection::Column, padding_left: 1u32, padding_right: 1u32) {
                Box(height: 1u32) {
                    Text(content: h_title, color: rc.fg)
                }
                Box(height: 1u32) {
                    Text(content: h_state, color: state_color)
                }
                Box(height: 1u32) {
                    Text(content: h_labels, color: rc.dim)
                }
                Box(height: 1u32) {
                    Text(content: h_url, color: rc.dim)
                }
                Box(height: 1u32) {
                    Text(
                        content: "─────────────────────────────────────────",
                        color: rc.dim,
                    )
                }
            }

            // ── Scrollable viewport — always exactly scroll_rows rows ─────
            Box(width: 100pct, padding_left: 1u32) {
                ScrollableText(
                    content: detail_content.text,
                    scroll_offset: props.scroll_offset,
                    viewport_rows: scroll_rows,
                    cursor_line: detail_content.cursor.map(|(l, _)| l),
                    cursor_col: detail_content.cursor.map(|(_, c)| c),
                    color: rc.fg,
                    cursor_color: rc.bg,
                    cursor_bg: rc.bright,
                    track_color: rc.dim,
                    thumb_color: rc.bright,
                )
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{DetailContent, build_new_issue_content};
    use crate::state::{ComposerTarget, InlineState};

    #[test]
    fn build_new_issue_content_renders_prompt_and_cursor() {
        let inline = InlineState::Composer {
            target: ComposerTarget::NewIssue,
            text: "Issue title\nIssue body".to_string(),
            cursor: "Issue title\nIssue body".len(),
        };

        let DetailContent { text, cursor } = build_new_issue_content(&inline);

        assert!(text.contains("New Issue"));
        assert!(text.contains("Title: first line | Body: remaining lines"));
        assert!(text.contains("Ctrl+Enter submit | Esc cancel"));
        assert!(cursor.is_some());
    }
}

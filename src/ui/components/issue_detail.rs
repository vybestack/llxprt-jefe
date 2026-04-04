//! Unified issue detail + comments view.
//! @plan PLAN-20260329-ISSUES-MODE.P12
//! @plan PLAN-20260329-ISSUES-MODE.P14
//! @requirement REQ-ISS-009

use iocraft::prelude::*;

use crate::domain::{IssueDetail, IssueState};
use crate::state::{DetailSubfocus, InlineState};
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

/// Build the scrollable content string for the body + comments + new-comment area.
#[allow(clippy::too_many_lines)]
fn build_detail_content(
    detail: &IssueDetail,
    subfocus: DetailSubfocus,
    inline_state: &InlineState,
    comments_loading: bool,
) -> DetailContent {
    let nl = String::from(char::from(0x0Au8));
    let mut lines: Vec<String> = Vec::new();
    let mut cursor_pos: Option<(usize, usize)> = None;

    // Helper: push lines from editor text and compute cursor if editing
    macro_rules! push_editor_lines {
        ($text:expr, $cursor:expr, $editing:expr, $prefix_edit:expr, $prefix_view:expr, $lines:expr, $cursor_pos:expr) => {
            let prefix = if $editing { $prefix_edit } else { $prefix_view };
            let prefix_len = prefix.chars().count();
            let content_start = $lines.len();
            let source_text = $text;
            if source_text.is_empty() {
                $lines.push(format!("{prefix}(empty)"));
            } else {
                for line in source_text.lines() {
                    $lines.push(format!("{prefix}{line}"));
                }
                // Handle trailing newline: `lines()` doesn't yield an empty final element
                if source_text.ends_with(char::from(0x0Au8)) {
                    $lines.push(format!("{prefix}"));
                }
            }
            if $editing {
                $cursor_pos = Some(byte_cursor_to_line_col(
                    source_text,
                    $cursor,
                    content_start,
                    prefix_len,
                ));
            }
        };
    }

    // ── Body section ────────────────────────────────────────────────
    let body_focused = subfocus == DetailSubfocus::Body;
    let body_label = if body_focused { "> Body" } else { "  Body" };
    lines.push(body_label.to_string());

    let (body_text, body_cursor, body_editing) = match inline_state {
        InlineState::Editor {
            target: crate::state::EditorTarget::IssueBody,
            text,
            cursor,
        } => (text.as_str(), *cursor, true),
        _ => (detail.body.as_str(), 0, false),
    };

    if body_editing {
        lines.push("[editing]".to_string());
    }
    push_editor_lines!(
        body_text,
        body_cursor,
        body_editing,
        "  │ ",
        "    ",
        lines,
        cursor_pos
    );
    if body_editing {
        lines.push("  Ctrl+Enter save | Esc cancel".to_string());
    }

    lines.push("─────────────────────────────────────────".to_string());

    // ── Comments section ────────────────────────────────────────────
    lines.push("Comments".to_string());
    if comments_loading {
        lines.push("  Loading comments...".to_string());
    } else if detail.comments.is_empty() {
        lines.push("  No comments yet.".to_string());
    } else {
        for (idx, comment) in detail.comments.iter().enumerate() {
            let comment_focused = subfocus == DetailSubfocus::Comment(idx);
            let prefix = if comment_focused { "> " } else { "  " };
            lines.push(format!(
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
                lines.push("  [editing]".to_string());
            }
            push_editor_lines!(
                cmt_text,
                cmt_cursor,
                cmt_editing,
                "    │ ",
                "      ",
                lines,
                cursor_pos
            );
            if cmt_editing {
                lines.push("    Ctrl+Enter save | Esc cancel".to_string());
            }

            if let InlineState::Composer {
                target: crate::state::ComposerTarget::Reply { comment_index, .. },
                text,
                cursor,
            } = inline_state
                && *comment_index == idx
            {
                lines.push("    [Reply]".to_string());
                let reply_cursor = *cursor;
                push_editor_lines!(
                    text.as_str(),
                    reply_cursor,
                    true,
                    "    │ ",
                    "    │ ",
                    lines,
                    cursor_pos
                );
                lines.push("    Ctrl+Enter save | Esc cancel".to_string());
            }

            lines.push(String::new());
        }
    }

    lines.push("─────────────────────────────────────────".to_string());

    // ── New Comment section ─────────────────────────────────────────
    let nc_focused = subfocus == DetailSubfocus::NewComment;
    let nc_label = if nc_focused {
        "> New Comment"
    } else {
        "  New Comment"
    };
    lines.push(nc_label.to_string());

    if let InlineState::Composer {
        target: crate::state::ComposerTarget::NewComment,
        text,
        cursor,
    } = inline_state
    {
        let nc_cursor = *cursor;
        push_editor_lines!(
            text.as_str(),
            nc_cursor,
            true,
            "  │ ",
            "  │ ",
            lines,
            cursor_pos
        );
        lines.push("  Ctrl+Enter submit | Esc cancel".to_string());
    } else {
        lines.push("  Press c to add a comment".to_string());
    }

    DetailContent {
        text: lines.join(&nl),
        cursor: cursor_pos,
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

    // Build header and content — same structure whether issue is loaded or not
    let (h_title, h_state, h_labels, h_url, detail_content, state_color) = if let Some(detail) =
        props.issue_detail.as_ref()
    {
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
            format!("labels: {labels_str}  assignees: {assignees_str}  milestone: {milestone_str}"),
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

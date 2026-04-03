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

/// Overhead rows outside the detail pane: status bar (1) + keybind bar (1) + border (2).
const CHROME_ROWS: usize = 4;

/// Insert a caret character at the given byte-offset cursor position in the text.
fn render_text_with_caret(value: &str, cursor: usize) -> String {
    let byte_idx = cursor.min(value.len());
    let byte_idx = if byte_idx == 0 || byte_idx >= value.len() {
        byte_idx
    } else {
        value[..byte_idx]
            .char_indices()
            .last()
            .map_or(0, |(i, c)| i + c.len_utf8())
    };
    format!("{}▏{}", &value[..byte_idx], &value[byte_idx..])
}

/// Build the scrollable content string for the body + comments + new-comment area.
#[allow(clippy::too_many_lines)]
fn build_detail_content(
    detail: &IssueDetail,
    subfocus: DetailSubfocus,
    inline_state: &InlineState,
    comments_loading: bool,
) -> String {
    let nl = String::from(char::from(0x0Au8));
    let mut lines: Vec<String> = Vec::new();

    // ── Body section ────────────────────────────────────────────────
    let body_focused = subfocus == DetailSubfocus::Body;
    let body_label = if body_focused { "> Body" } else { "  Body" };
    lines.push(body_label.to_string());

    let (body_text, body_editing) = match inline_state {
        InlineState::Editor {
            target: crate::state::EditorTarget::IssueBody,
            text,
            cursor,
        } => (render_text_with_caret(text, *cursor), true),
        _ => (detail.body.clone(), false),
    };

    if body_editing {
        lines.push("[editing]".to_string());
    }
    for line in body_text.lines() {
        let prefix = if body_editing { "  │ " } else { "    " };
        lines.push(format!("{prefix}{line}"));
    }
    if body_text.is_empty() {
        let prefix = if body_editing { "  │ " } else { "    " };
        lines.push(format!("{prefix}(empty)"));
    }
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

            let (cmt_text, cmt_editing) = match inline_state {
                InlineState::Editor {
                    target: crate::state::EditorTarget::Comment { comment_index },
                    text,
                    cursor,
                } if *comment_index == idx => (render_text_with_caret(text, *cursor), true),
                _ => (comment.body.clone(), false),
            };

            if cmt_editing {
                lines.push("  [editing]".to_string());
            }
            for line in cmt_text.lines() {
                let prefix = if cmt_editing { "    │ " } else { "      " };
                lines.push(format!("{prefix}{line}"));
            }
            if cmt_text.is_empty() {
                let prefix = if cmt_editing { "    │ " } else { "      " };
                lines.push(format!("{prefix}(empty)"));
            }
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
                let reply_text = render_text_with_caret(text, *cursor);
                for line in reply_text.lines() {
                    lines.push(format!("    │ {line}"));
                }
                if reply_text.is_empty() {
                    lines.push("    │ ".to_string());
                }
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
        let composer_text = render_text_with_caret(text, *cursor);
        for line in composer_text.lines() {
            lines.push(format!("  │ {line}"));
        }
        if composer_text.is_empty() {
            lines.push("  │ ".to_string());
        }
        lines.push("  Ctrl+Enter submit | Esc cancel".to_string());
    } else {
        lines.push("  Press c to add a comment".to_string());
    }

    lines.join(&nl)
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

    // Compute viewport rows dynamically from terminal height.
    // The detail pane gets ~70% of the available height (after chrome).
    let term_rows = crossterm::terminal::size().map_or(40, |(_, h)| h as usize);
    let pane_rows = (term_rows.saturating_sub(CHROME_ROWS)) * 7 / 10;
    let scroll_rows = pane_rows.saturating_sub(HEADER_ROWS).max(5);

    // Build header and content — same structure whether issue is loaded or not
    let (h_title, h_state, h_labels, h_url, content, state_color) = if let Some(detail) =
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
            "No issue selected".to_string(),
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
                    content: content,
                    scroll_offset: props.scroll_offset,
                    viewport_rows: scroll_rows,
                    color: rc.fg,
                    track_color: rc.dim,
                    thumb_color: rc.bright,
                )
            }
        }
    }
}

//! Unified issue detail + comments view.
//! @plan PLAN-20260329-ISSUES-MODE.P12
//! @plan PLAN-20260329-ISSUES-MODE.P14
//! @requirement REQ-ISS-009

use iocraft::prelude::*;

use crate::domain::{IssueDetail, IssueState};
use crate::issue_detail_content::{DetailContent, build_detail_content, build_new_issue_content};
use crate::layout::DETAIL_HEADER_ROWS as HEADER_ROWS;
use crate::selection::TextSelection;
use crate::state::{ComposerTarget, DetailSubfocus, InlineState};
use crate::theme::{ResolvedColors, ThemeColors};

use super::scrollable_text::ScrollableText;
use super::text_box::TextBox;

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
    /// Actual available height (in terminal rows) for the detail pane.
    ///
    /// When provided, this is used instead of computing the height from
    /// `crossterm::terminal::size()`, ensuring the viewport matches the real
    /// flex allocation in the parent layout. Falls back to the terminal-size
    /// calculation when `None`.
    pub available_height: Option<u16>,
    /// Actual available width (in terminal columns) for detail/composer text.
    pub available_width: Option<u16>,
    /// Active text selection, if any (and if it targets this pane). Passed
    /// through to the `ScrollableText` so selected cells render inverse-video.
    pub selection: Option<TextSelection>,
}

fn active_issue_composer(inline_state: &InlineState) -> Option<(String, usize, &'static str)> {
    match inline_state {
        InlineState::Composer {
            target: ComposerTarget::NewComment,
            text,
            cursor,
        } => Some((
            text.clone(),
            *cursor,
            crate::layout::NEW_COMMENT_COMPOSER_PREFIX,
        )),
        InlineState::Composer {
            target: ComposerTarget::Reply { .. } | ComposerTarget::ReplyToReviewThread { .. },
            text,
            cursor,
        } => Some((text.clone(), *cursor, crate::layout::REPLY_COMPOSER_PREFIX)),
        InlineState::Composer {
            target: ComposerTarget::NewIssue,
            ..
        }
        | InlineState::Editor { .. }
        | InlineState::None => None,
    }
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

    // Compute viewport rows. Prefer the actual available height passed from the
    // parent layout; fall back to deriving it from the terminal size when the
    // parent did not supply one.
    let detail_viewport_rows = if let Some(height) = props.available_height {
        // The available height is the real flex allocation for the detail pane,
        // including its borders and header. Subtract header + border (2) to get
        // the scrollable viewport. Do NOT force the minimum-viewport floor here:
        // on a very short terminal that floor would exceed the parent allocation
        // and overflow the layout. The floor is only a fallback heuristic used
        // when the parent does not report an actual height.
        (height as usize).saturating_sub(HEADER_ROWS + 2)
    } else {
        let term_rows = crossterm::terminal::size().map_or(40, |(_, h)| h as usize);
        crate::layout::detail_viewport_rows(term_rows)
    };
    let composer = active_issue_composer(&props.inline_state);
    let composer_active = composer.is_some();
    let reserved_document_rows =
        crate::layout::issue_detail_document_viewport_rows(detail_viewport_rows, composer_active);
    let detail_content_width = usize::from(props.available_width.unwrap_or_else(|| {
        crate::layout::issues_detail_content_width(
            crossterm::terminal::size().map_or(120, |(w, _)| w),
        )
    }));

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

    let document_line_count = detail_content.text.lines().count().max(1);
    let scroll_rows = if composer_active {
        reserved_document_rows.min(document_line_count)
    } else {
        reserved_document_rows
    };
    let composer_rows = if composer_active {
        crate::layout::DETAIL_COMPOSER_VIEWPORT_ROWS
            .min(detail_viewport_rows.saturating_sub(scroll_rows))
    } else {
        0
    };

    let composer_element: Option<AnyElement<'static>> =
        composer.map(|(text, byte_cursor, prefix)| {
            element! {
                Box(width: 100pct, padding_left: 1u32) {
                    TextBox(
                        text: text,
                        byte_cursor: byte_cursor,
                        viewport_rows: composer_rows,
                        content_width: detail_content_width,
                        prefix: prefix.to_string(),
                        color: rc.fg,
                        caret_color: rc.bg,
                        caret_bg: rc.bright,
                    )
                }
            }
            .into()
        });

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
                    max_line_width: detail_content_width,
                    cursor_line: detail_content.cursor.map(|(l, _)| l),
                    cursor_col: detail_content.cursor.map(|(_, c)| c),
                    color: rc.fg,
                    cursor_color: rc.bg,
                    cursor_bg: rc.bright,
                    track_color: rc.dim,
                    thumb_color: rc.bright,
                    selection: props.selection,
                    selection_bg: Some(rc.sel_bg),
                    selection_fg: Some(rc.sel_fg),
                )
            }

            #(composer_element)
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
        // Cursor should be positioned at the end of the text
        assert!(cursor.is_some());
        if let Some((line, col)) = cursor {
            // Cursor on second line (after the newline), at end of body
            assert!(line > 0, "cursor should be on a text line");
            assert!(col > 0, "cursor column should be non-zero at end of text");
        }
    }
}

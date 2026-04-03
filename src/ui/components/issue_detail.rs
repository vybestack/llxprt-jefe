//! Unified issue detail + comments view.
//! @plan PLAN-20260329-ISSUES-MODE.P12
//! @plan PLAN-20260329-ISSUES-MODE.P14
//! @requirement REQ-ISS-009

use iocraft::prelude::*;

use crate::domain::{IssueDetail, IssueState};
use crate::state::{DetailSubfocus, InlineState};
use crate::theme::{ResolvedColors, ThemeColors};

/// Insert a caret character at the given cursor position in the text.
/// Uses the theme's bright color caret (▏) consistent with form field editing.
fn render_text_with_caret(value: &str, cursor: usize) -> String {
    let char_len = value.chars().count();
    let clamped = cursor.min(char_len);

    let byte_idx = if clamped == 0 {
        0
    } else {
        value
            .char_indices()
            .nth(clamped)
            .map_or_else(|| value.len(), |(idx, _)| idx)
    };

    format!("{}▏{}", &value[..byte_idx], &value[byte_idx..])
}

/// Maximum visible lines for body/comment text before truncation.
const MAX_BODY_LINES: usize = 20;
/// Maximum visible lines for a single comment body.
const MAX_COMMENT_LINES: usize = 12;

/// Split text into lines, truncating to `max_lines` and appending an indicator if needed.
fn truncate_lines(text: &str, max_lines: usize) -> Vec<String> {
    let lines: Vec<&str> = text.lines().collect();
    if lines.len() <= max_lines {
        lines.iter().map(|s| (*s).to_string()).collect()
    } else {
        let mut result: Vec<String> = lines[..max_lines]
            .iter()
            .map(|s| (*s).to_string())
            .collect();
        let remaining = lines.len() - max_lines;
        result.push(format!("... ({remaining} more lines)"));
        result
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
    /// Theme colors.
    pub colors: ThemeColors,
}

/// Issue detail view — unified scrollable: metadata → body → comments → new comment field.
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

    let Some(detail) = props.issue_detail.as_ref() else {
        return element! {
            Box(
                flex_direction: FlexDirection::Column,
                width: 100pct,
                height: 100pct,
                border_style: border_style,
                border_color: rc.border,
                background_color: rc.bg,
            ) {
                Box(padding_left: 1u32, height: 1u32) {
                    Text(content: "No issue selected", color: rc.dim)
                }
            }
        };
    };

    let state_tag = match detail.state {
        IssueState::Open => "OPEN",
        IssueState::Closed => "CLOSED",
    };
    let state_color = match detail.state {
        IssueState::Open => rc.bright,
        IssueState::Closed => rc.dim,
    };

    // Build labels/assignees/milestone display strings
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

    // Determine body display: active editor replaces body text
    let body_focused = props.detail_subfocus == DetailSubfocus::Body;
    let (body_display, body_editing) = match &props.inline_state {
        InlineState::Editor {
            target: crate::state::EditorTarget::IssueBody,
            text,
            cursor,
        } => (render_text_with_caret(text, *cursor), true),
        _ => (detail.body.clone(), false),
    };

    // Build inline composer text if a new-comment composer is active
    let (composer_text, composer_active) = match &props.inline_state {
        InlineState::Composer {
            target: crate::state::ComposerTarget::NewComment,
            text,
            cursor,
        } => (render_text_with_caret(text, *cursor), true),
        _ => (String::new(), false),
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
            // ── Metadata header ─────────────────────────────────────────────
            Box(flex_direction: FlexDirection::Column, padding_left: 1u32, padding_right: 1u32) {
                // Title line
                Box(height: 1u32) {
                    Text(
                        content: format!("#{} {}", detail.number, detail.title),
                        weight: Weight::Bold,
                        color: rc.fg,
                    )
                }
                // State + author + dates
                Box(height: 1u32) {
                    Text(content: state_tag, color: state_color, weight: Weight::Bold)
                    Text(
                        content: format!(
                            "  by @{}  opened: {}  updated: {}",
                            detail.author_login, detail.created_at, detail.updated_at
                        ),
                        color: rc.dim,
                    )
                }
                // Labels / assignees / milestone
                Box(height: 1u32) {
                    Text(content: "labels: ", color: rc.dim)
                    Text(content: labels_str, color: rc.fg)
                    Text(content: "  assignees: ", color: rc.dim)
                    Text(content: assignees_str, color: rc.fg)
                    Text(content: "  milestone: ", color: rc.dim)
                    Text(content: milestone_str, color: rc.fg)
                }
                // URL
                Box(height: 1u32) {
                    Text(content: detail.external_url.clone(), color: rc.dim)
                }
                // Separator
                Box(height: 1u32) {
                    Text(
                        content: "─────────────────────────────────────────",
                        color: rc.dim,
                    )
                }
            }

            // ── Body ─────────────────────────────────────────────────────────
            Box(flex_direction: FlexDirection::Column, padding_left: 1u32, padding_right: 1u32) {
                Box(height: 1u32) {
                    Text(
                        content: if body_focused { "> Body" } else { "  Body" },
                        color: if body_focused { rc.bright } else { rc.fg },
                        weight: Weight::Bold,
                    )
                }
                #({
                    let lines = truncate_lines(&body_display, MAX_BODY_LINES);
                    let mut elems = Vec::new();
                    if body_editing {
                        for line in &lines {
                            elems.push(element! {
                                Box(
                                    height: 1u32,
                                    border_color: rc.bright,
                                    padding_left: 1u32,
                                    padding_right: 1u32,
                                ) {
                                    Text(content: line.clone(), color: rc.fg)
                                }
                            });
                        }
                        elems.push(element! {
                            Box(height: 1u32) {
                                Text(content: "  Ctrl+Enter save | Esc cancel".to_string(), color: rc.dim)
                            }
                        });
                    } else {
                        for line in &lines {
                            elems.push(element! {
                                Box(height: 1u32, padding_left: 2u32, padding_right: 1u32) {
                                    Text(content: line.clone(), color: rc.fg)
                                }
                            });
                        }
                    }
                    elems
                })
                Box(height: 1u32) {
                    Text(
                        content: "─────────────────────────────────────────",
                        color: rc.dim,
                    )
                }
            }

            // ── Comments ──────────────────────────────────────────────────────
            Box(flex_direction: FlexDirection::Column, padding_left: 1u32, padding_right: 1u32) {
                Box(height: 1u32) {
                    Text(content: "Comments", weight: Weight::Bold, color: rc.fg)
                }
                #(if props.comments_loading {
                    vec![element! {
                        Box(height: 1u32, padding_left: 1u32) {
                            Text(content: "Loading comments...", color: rc.dim)
                        }
                    }]
                } else if detail.comments.is_empty() {
                    vec![element! {
                        Box(height: 1u32, padding_left: 1u32) {
                            Text(content: "No comments yet.", color: rc.dim)
                        }
                    }]
                } else {
                    detail.comments.iter().enumerate().map(|(idx, comment)| {
                        let comment_focused = props.detail_subfocus == DetailSubfocus::Comment(idx);

                        // Check for reply composer targeting this comment
                        let (reply_text, reply_active) = match &props.inline_state {
                            InlineState::Composer {
                                target: crate::state::ComposerTarget::Reply { comment_index, .. },
                                text,
                                cursor,
                            } if *comment_index == idx => (render_text_with_caret(text, *cursor), true),
                            _ => (String::new(), false),
                        };

                        // Check for editor targeting this comment
                        let (edit_text, edit_active) = match &props.inline_state {
                            InlineState::Editor {
                                target: crate::state::EditorTarget::Comment { comment_index },
                                text,
                                cursor,
                            } if *comment_index == idx => (render_text_with_caret(text, *cursor), true),
                            _ => (String::new(), false),
                        };

                        let prefix = if comment_focused { "> " } else { "  " };
                        let author_line = format!(
                            "{}@{}  {}",
                            prefix, comment.author_login, comment.created_at
                        );

                        element! {
                            Box(flex_direction: FlexDirection::Column, padding_bottom: 1u32) {
                                Box(height: 1u32) {
                                    Text(
                                        content: author_line,
                                        color: if comment_focused { rc.bright } else { rc.dim },
                                        weight: if comment_focused { Weight::Bold } else { Weight::Normal },
                                    )
                                }
                                #({
                                    let mut cmt_elems = Vec::new();
                                    if edit_active {
                                        let edit_lines = truncate_lines(&edit_text, MAX_COMMENT_LINES);
                                        for line in &edit_lines {
                                            cmt_elems.push(element! {
                                                Box(
                                                    height: 1u32,
                                                    border_color: rc.bright,
                                                    padding_left: 1u32,
                                                    padding_right: 1u32,
                                                ) {
                                                    Text(content: line.clone(), color: rc.fg)
                                                }
                                            });
                                        }
                                        cmt_elems.push(element! {
                                            Box(height: 1u32) {
                                                Text(content: "  Ctrl+Enter save | Esc cancel".to_string(), color: rc.dim)
                                            }
                                        });
                                    } else {
                                        let cmt_lines = truncate_lines(&comment.body, MAX_COMMENT_LINES);
                                        for line in &cmt_lines {
                                            cmt_elems.push(element! {
                                                Box(height: 1u32, padding_left: 4u32, padding_right: 1u32) {
                                                    Text(content: line.clone(), color: rc.fg)
                                                }
                                            });
                                        }
                                    }
                                    cmt_elems
                                })
                                #(if reply_active {
                                    vec![
                                        element! {
                                            Box(height: 1u32, padding_left: 4u32) {
                                                Text(content: "[Reply]", color: rc.bright)
                                            }
                                        },
                                        element! {
                                            Box(
                                                border_style: BorderStyle::Round,
                                                border_color: rc.bright,
                                                padding_left: 1u32,
                                                padding_right: 1u32,
                                                margin_left: 4u32,
                                                width: 100pct,
                                            ) {
                                                Text(content: reply_text.clone(), color: rc.fg)
                                            }
                                        },
                                        element! {
                                            Box(height: 1u32, padding_left: 4u32) {
                                                Text(content: "Ctrl+Enter save | Esc cancel", color: rc.dim)
                                            }
                                        },
                                    ]
                                } else {
                                    vec![]
                                })
                            }
                        }
                    }).collect()
                })
            }

            // ── New Comment field ─────────────────────────────────────────────
            Box(flex_direction: FlexDirection::Column, padding_left: 1u32, padding_right: 1u32) {
                Box(height: 1u32) {
                    Text(
                        content: if props.detail_subfocus == DetailSubfocus::NewComment {
                            "> New Comment"
                        } else {
                            "  New Comment"
                        },
                        color: if props.detail_subfocus == DetailSubfocus::NewComment {
                            rc.bright
                        } else {
                            rc.fg
                        },
                        weight: Weight::Bold,
                    )
                }
                #(if composer_active {
                    vec![
                        element! {
                            Box(
                                border_style: BorderStyle::Round,
                                border_color: rc.bright,
                                padding_left: 1u32,
                                padding_right: 1u32,
                                width: 100pct,
                            ) {
                                Text(content: composer_text, color: rc.fg)
                            }
                        },
                        element! {
                            Box(height: 1u32) {
                                Text(content: "  Ctrl+Enter submit | Esc cancel", color: rc.dim)
                            }
                        },
                    ]
                } else {
                    vec![element! {
                        Box(height: 1u32, padding_left: 2u32) {
                            Text(
                                content: "Press c to add a comment",
                                color: rc.dim,
                            )
                        }
                    }]
                })
            }
        }
    }
}

//! Issue list pane component.
//! @plan PLAN-20260329-ISSUES-MODE.P12
//! @plan PLAN-20260329-ISSUES-MODE.P14
//! @requirement REQ-ISS-006

use iocraft::prelude::*;

use crate::domain::{Issue, IssueState};
use crate::theme::{ResolvedColors, ThemeColors};

/// Props for the issue list pane.
#[derive(Default, Props)]
#[allow(clippy::struct_excessive_bools)]
pub struct IssueListProps {
    /// Issues to display.
    pub issues: Vec<Issue>,
    /// Currently selected index.
    pub selected_index: Option<usize>,
    /// Whether this pane is focused.
    pub focused: bool,
    /// Whether issues are loading.
    pub loading: bool,
    /// Whether filters are active (affects empty-state message).
    pub has_filters: bool,
    /// Whether this is the compact (split) variant for detail view.
    pub compact: bool,
    /// Theme colors.
    pub colors: ThemeColors,
}

/// Issue list pane — renders issues with selection highlight, loading, and empty states.
/// @plan PLAN-20260329-ISSUES-MODE.P14
/// @requirement REQ-ISS-006
#[component]
pub fn IssueList(props: &IssueListProps) -> impl Into<AnyElement<'static>> {
    let rc = ResolvedColors::from_theme(Some(&props.colors));
    let border_style = if props.focused {
        BorderStyle::Double
    } else {
        BorderStyle::Round
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
            // Title row
            Box(height: 1u32, padding_left: 1u32) {
                Text(content: "Issues", weight: Weight::Bold, color: rc.fg)
            }

            // Content
            Box(
                flex_direction: FlexDirection::Column,
                flex_grow: 1.0,
                background_color: rc.bg,
            ) {
                #(if props.loading {
                    vec![element! {
                        Box(padding_left: 1u32, height: 1u32) {
                            Text(content: "Loading issues...", color: rc.dim)
                        }
                    }]
                } else if props.issues.is_empty() {
                    let msg = if props.has_filters {
                        "No issues match filters"
                    } else {
                        "No issues found"
                    };
                    vec![element! {
                        Box(padding_left: 1u32, height: 1u32) {
                            Text(content: msg, color: rc.dim)
                        }
                    }]
                } else {
                    props.issues.iter().enumerate().map(|(i, issue)| {
                        let selected = props.selected_index == Some(i);
                        let prefix = if selected { "> " } else { "  " };
                        let state_tag = match issue.state {
                            IssueState::Open => "OPEN",
                            IssueState::Closed => "CLSD",
                        };

                        // Primary line: prefix + number + title
                        let title_line = format!("{}#{} {}", prefix, issue.number, issue.title);

                        // Secondary line: state, author, updated, comment count
                        let mut meta_parts = vec![
                            state_tag.to_string(),
                            format!("@{}", issue.author_login),
                            format!("updated:{}", issue.updated_at),
                        ];
                        if issue.comment_count > 0 {
                            meta_parts.push(format!("{} comments", issue.comment_count));
                        }
                        if !issue.assignee_summary.is_empty() {
                            meta_parts.push(format!("assigned:{}", issue.assignee_summary));
                        }
                        if !issue.labels_summary.is_empty() {
                            meta_parts.push(format!("[{}]", issue.labels_summary));
                        }
                        let meta_line = format!("     {}", meta_parts.join("  "));

                        if selected {
                            if props.compact {
                                element! {
                                    Box(height: 1u32, background_color: rc.sel_bg) {
                                        Text(
                                            content: title_line,
                                            color: rc.sel_fg,
                                            weight: Weight::Bold,
                                        )
                                    }
                                }
                            } else {
                                element! {
                                    Box(flex_direction: FlexDirection::Column) {
                                        Box(height: 1u32, background_color: rc.sel_bg) {
                                            Text(
                                                content: title_line,
                                                color: rc.sel_fg,
                                                weight: Weight::Bold,
                                            )
                                        }
                                        Box(height: 1u32, background_color: rc.sel_bg) {
                                            Text(content: meta_line, color: rc.sel_fg)
                                        }
                                    }
                                }
                            }
                        } else if props.compact {
                            element! {
                                Box(height: 1u32) {
                                    Text(content: title_line, color: rc.fg)
                                }
                            }
                        } else {
                            element! {
                                Box(flex_direction: FlexDirection::Column) {
                                    Box(height: 1u32) {
                                        Text(content: title_line, color: rc.fg)
                                    }
                                    Box(height: 1u32) {
                                        Text(content: meta_line, color: rc.dim)
                                    }
                                }
                            }
                        }
                    }).collect()
                })
            }
        }
    }
}

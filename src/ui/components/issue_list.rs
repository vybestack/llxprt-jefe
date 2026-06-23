//! Issue list pane component.
//! @plan PLAN-20260329-ISSUES-MODE.P12
//! @plan PLAN-20260329-ISSUES-MODE.P14
//! @requirement REQ-ISS-006

use iocraft::prelude::*;
use unicode_width::{UnicodeWidthChar, UnicodeWidthStr};

use crate::domain::{Issue, IssueState};
use crate::theme::{ResolvedColors, ThemeColors};

/// Ellipsis character appended when a title is truncated.
const ELLIPSIS: char = '…';

/// Truncate `text` to fit within `max_width` terminal columns, appending an
/// ellipsis when truncation occurs.
///
/// Uses character boundaries and Unicode display width so multi-byte characters
/// are never split and wide characters are accounted for.
fn truncate_title(text: &str, max_width: usize) -> String {
    if max_width == 0 {
        return String::new();
    }
    if UnicodeWidthStr::width(text) <= max_width {
        return text.to_string();
    }

    let ellipsis_width = ELLIPSIS.width().unwrap_or(1);
    if max_width <= ellipsis_width {
        return ELLIPSIS.to_string();
    }

    let content_width = max_width - ellipsis_width;
    let mut used = 0usize;
    let mut result = String::new();
    for ch in text.chars() {
        let width = ch.width().unwrap_or(0);
        if used + width > content_width {
            break;
        }
        used += width;
        result.push(ch);
    }
    result.push(ELLIPSIS);
    result
}

/// Issue list density variant.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum IssueListLayout {
    /// Show title and metadata for each issue.
    #[default]
    Full,
    /// Show only the title row for each issue.
    Compact,
}

impl IssueListLayout {
    fn is_compact(self) -> bool {
        matches!(self, Self::Compact)
    }
}

/// Props for the issue list pane.
#[derive(Default, Props)]
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
    /// List density variant.
    pub layout: IssueListLayout,
    /// Theme colors.
    pub colors: ThemeColors,
    /// Available content width (in terminal columns) for title truncation.
    ///
    /// When provided, long issue titles are truncated with an ellipsis to fit.
    pub available_width: Option<u16>,
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

                        // Truncate the title to fit the available width.
                        // Layout: "<prefix>#<number> <title>"
                        let number_prefix = format!("{}#{} ", prefix, issue.number);
                        let title = match props.available_width {
                            Some(width) => {
                                // Account for the prefix+number columns already consumed.
                                let used = UnicodeWidthStr::width(number_prefix.as_str());
                                let budget = (width as usize).saturating_sub(used);
                                truncate_title(&issue.title, budget)
                            }
                            None => issue.title.clone(),
                        };

                        // Primary line: prefix + number + title
                        let title_line = format!("{number_prefix}{title}");

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
                            if props.layout.is_compact() {
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
                        } else if props.layout.is_compact() {
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

#[cfg(test)]
mod tests {
    use super::truncate_title;
    use unicode_width::UnicodeWidthStr;

    #[test]
    fn short_title_is_returned_unchanged() {
        assert_eq!(truncate_title("hello", 10), "hello");
    }

    #[test]
    fn long_title_is_truncated_with_ellipsis() {
        let result = truncate_title("a very long title that exceeds the budget", 10);
        assert!(result.ends_with('\u{2026}'));
        assert_eq!(UnicodeWidthStr::width(result.as_str()), 10);
    }

    #[test]
    fn exact_fit_title_is_not_truncated() {
        assert_eq!(truncate_title("exact", 5), "exact");
    }

    #[test]
    fn unicode_title_truncates_on_character_boundary() {
        // Each emoji is one char but multiple bytes; truncation must never
        // split a multi-byte code point.
        let title = "\u{1F600}\u{1F601}\u{1F602}\u{1F603}\u{1F604}\u{1F605}\u{1F606}\u{1F607}\u{1F608}\u{1F609}";
        let result = truncate_title(title, 5);
        assert!(UnicodeWidthStr::width(result.as_str()) <= 5);
        assert!(result.ends_with('\u{2026}'));
        // Ensure no panic on multi-byte slicing.
        assert!(result.chars().next().is_some());
    }

    #[test]
    fn one_column_budget_returns_ellipsis() {
        assert_eq!(truncate_title("abcdef", 1), "…");
    }

    #[test]
    fn full_width_prefix_display_width_is_counted_in_title_budget() {
        let number_prefix = "  #１２ ";
        let title = truncate_title(
            "abcdef",
            8usize.saturating_sub(UnicodeWidthStr::width(number_prefix)),
        );
        let line = format!("{number_prefix}{title}");
        assert_eq!(UnicodeWidthStr::width(line.as_str()), 8);
    }
}

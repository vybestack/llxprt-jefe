//! PR list pane component.
//! @plan PLAN-20260624-PR-MODE.P12
//! @requirement REQ-PR-006
//! @requirement REQ-PR-014

use iocraft::prelude::*;
use unicode_width::{UnicodeWidthChar, UnicodeWidthStr};

use crate::domain::{PrCheckStatus, PrReviewState, PrState, PullRequest};
use crate::theme::{ResolvedColors, ThemeColors};

/// Ellipsis character appended when a title is truncated.
const ELLIPSIS: char = '…';

/// Truncate `text` to fit within `max_width` terminal columns, appending an
/// ellipsis when truncation occurs.
///
/// Uses character boundaries and Unicode display width so multi-byte characters
/// are never split and wide characters are accounted for.
///
/// @plan PLAN-20260624-PR-MODE.P12
/// @requirement REQ-PR-006
/// @pseudocode component-001 lines 1-12
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

/// PR list density variant.
///
/// @plan PLAN-20260624-PR-MODE.P12
/// @requirement REQ-PR-006
/// @pseudocode component-001 lines 1-12
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum PrListLayout {
    /// Show title and metadata for each PR.
    #[default]
    Full,
    /// Show only the title row for each PR.
    Compact,
}

impl PrListLayout {
    /// @plan PLAN-20260624-PR-MODE.P12
    /// @requirement REQ-PR-006
    /// @pseudocode component-001 lines 1-12
    fn is_compact(self) -> bool {
        matches!(self, Self::Compact)
    }
}

/// Props for the PR list pane.
///
/// @plan PLAN-20260624-PR-MODE.P12
/// @requirement REQ-PR-006
/// @requirement REQ-PR-014
/// @pseudocode component-001 lines 1-12
#[derive(Default, Props)]
pub struct PrListProps {
    /// Pull requests to display.
    pub pull_requests: Vec<PullRequest>,
    /// Currently selected index.
    pub selected_index: Option<usize>,
    /// First-visible row offset (selection-follow).
    pub list_scroll_offset: usize,
    /// PR-list pane height in rows.
    pub list_pane_rows: u16,
    /// Whether this pane is focused.
    pub focused: bool,
    /// Whether pull requests are loading.
    pub loading: bool,
    /// Whether filters are active (affects empty-state message).
    pub has_filters: bool,
    /// List density variant.
    pub layout: PrListLayout,
    /// Theme colors.
    pub colors: ThemeColors,
    /// Available content width (in terminal columns) for title truncation.
    pub available_width: Option<u16>,
}

/// PR list pane — renders pull requests with selection highlight, loading, and empty states.
///
/// @plan PLAN-20260624-PR-MODE.P12
/// @requirement REQ-PR-006
/// @requirement REQ-PR-014
/// @pseudocode component-001 lines 1-12
#[component]
pub fn PrList(props: &PrListProps) -> impl Into<AnyElement<'static>> {
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
                Text(content: "Pull Requests", weight: Weight::Bold, color: rc.fg)
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
                            Text(content: "Loading pull requests...", color: rc.dim)
                        }
                    }]
                } else if props.pull_requests.is_empty() {
                    let msg = if props.has_filters {
                        "No pull requests match filters"
                    } else {
                        "No pull requests found"
                    };
                    vec![element! {
                        Box(padding_left: 1u32, height: 1u32) {
                            Text(content: msg, color: rc.dim)
                        }
                    }]
                } else {
                    let viewport = props.list_pane_rows as usize;
                    let window = crate::layout::list_visible_window(
                        &props.pull_requests,
                        props.selected_index.unwrap_or(0),
                        viewport,
                    );
                    let first_visible = crate::layout::list_first_visible_index(
                        props.selected_index.unwrap_or(0),
                        props.pull_requests.len(),
                        viewport,
                    );
                    let rows = props;
                    window.iter().enumerate().map(|(window_i, pr)| {
                        let is_selected = rows.selected_index == Some(first_visible + window_i);
                        let prefix = if is_selected { "> " } else { "  " };
                        let state_tag = pr_state_tag(pr.state);

                        let number_prefix = format!("{}#{} ", prefix, pr.number);
                        let title = match rows.available_width {
                            Some(width) => {
                                let used = UnicodeWidthStr::width(number_prefix.as_str());
                                let budget = (width as usize).saturating_sub(used);
                                truncate_title(&pr.title, budget)
                            }
                            None => pr.title.clone(),
                        };
                        let title_line = format!("{number_prefix}{title}");

                        let mut meta_parts = vec![
                            state_tag.to_string(),
                            review_glyph(pr.review_decision).to_string(),
                            checks_glyph(pr.checks_status).to_string(),
                            format!("@{}", pr.author_login),
                        ];
                        if pr.is_draft {
                            meta_parts.push("draft".to_string());
                        }
                        if pr.comment_count > 0 {
                            meta_parts.push(format!("{}c", pr.comment_count));
                        }
                        if !pr.assignee_summary.is_empty() {
                            meta_parts.push(format!("assigned:{}", pr.assignee_summary));
                        }
                        if !pr.labels_summary.is_empty() {
                            meta_parts.push(format!("[{}]", pr.labels_summary));
                        }
                        let meta_line = format!("     {}", meta_parts.join("  "));

                        if is_selected {
                            if rows.layout.is_compact() {
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
                        } else if rows.layout.is_compact() {
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

///
/// @plan PLAN-20260624-PR-MODE.P12
/// @requirement REQ-PR-006
/// @pseudocode component-001 lines 1-12
fn pr_state_tag(state: PrState) -> &'static str {
    match state {
        PrState::Open => "OPEN",
        PrState::Closed => "CLSD",
        PrState::Merged => "MERGED",
    }
}

/// Review-decision glyph for the list meta line.
///
/// @plan PLAN-20260624-PR-MODE.P12
/// @requirement REQ-PR-006
/// @pseudocode component-001 lines 1-12
fn review_glyph(decision: Option<PrReviewState>) -> &'static str {
    match decision {
        Some(PrReviewState::Approved) => "\u{2714}review",
        Some(
            PrReviewState::ChangesRequested
            | PrReviewState::ReviewRequired
            | PrReviewState::Pending
            | PrReviewState::Commented,
        ) => "~review",
        Some(PrReviewState::Dismissed | PrReviewState::None) | None => "-review",
    }
}

/// CI/checks rollup glyph for the list meta line.
///
/// @plan PLAN-20260624-PR-MODE.P12
/// @requirement REQ-PR-006
/// @pseudocode component-001 lines 1-12
fn checks_glyph(status: PrCheckStatus) -> &'static str {
    match status {
        PrCheckStatus::Success => "✓checks",
        PrCheckStatus::Failure => "✗checks",
        PrCheckStatus::Pending => "•checks",
        PrCheckStatus::Neutral => "·checks",
        PrCheckStatus::None => "-checks",
    }
}

#[cfg(test)]
mod tests {
    use super::truncate_title;
    use unicode_width::UnicodeWidthStr;

    /// @plan PLAN-20260624-PR-MODE.P12
    /// @requirement REQ-PR-006
    /// @pseudocode component-001 lines 1-12
    #[test]
    fn short_title_is_returned_unchanged() {
        assert_eq!(truncate_title("hello", 10), "hello");
    }

    /// @plan PLAN-20260624-PR-MODE.P12
    /// @requirement REQ-PR-006
    /// @pseudocode component-001 lines 1-12
    #[test]
    fn long_title_is_truncated_with_ellipsis() {
        let result = truncate_title("a very long title that exceeds the budget", 10);
        assert!(result.ends_with('\u{2026}'));
        assert_eq!(UnicodeWidthStr::width(result.as_str()), 10);
    }

    /// @plan PLAN-20260624-PR-MODE.P12
    /// @requirement REQ-PR-006
    /// @pseudocode component-001 lines 1-12
    #[test]
    fn exact_fit_title_is_not_truncated() {
        assert_eq!(truncate_title("exact", 5), "exact");
    }

    /// @plan PLAN-20260624-PR-MODE.P12
    /// @requirement REQ-PR-006
    /// @pseudocode component-001 lines 1-12
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

    /// @plan PLAN-20260624-PR-MODE.P12
    /// @requirement REQ-PR-006
    /// @pseudocode component-001 lines 1-12
    #[test]
    fn one_column_budget_returns_ellipsis() {
        assert_eq!(truncate_title("abcdef", 1), "…");
    }

    /// @plan PLAN-20260624-PR-MODE.P12
    /// @requirement REQ-PR-006
    /// @pseudocode component-001 lines 1-12
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

    use super::{checks_glyph, review_glyph};
    use crate::domain::{PrCheckStatus, PrReviewState};

    /// @plan PLAN-20260624-PR-MODE.P12
    /// @requirement REQ-PR-006
    /// @pseudocode component-001 lines 1-12
    #[test]
    fn review_glyph_maps_each_decision() {
        assert_eq!(review_glyph(Some(PrReviewState::Approved)), "✔review");
        assert_eq!(
            review_glyph(Some(PrReviewState::ChangesRequested)),
            "~review"
        );
        assert_eq!(review_glyph(Some(PrReviewState::ReviewRequired)), "~review");
        assert_eq!(review_glyph(Some(PrReviewState::Dismissed)), "-review");
        assert_eq!(review_glyph(None), "-review");
    }

    /// @plan PLAN-20260624-PR-MODE.P12
    /// @requirement REQ-PR-006
    /// @pseudocode component-001 lines 1-12
    #[test]
    fn checks_glyph_maps_each_status() {
        assert_eq!(checks_glyph(PrCheckStatus::Success), "✓checks");
        assert_eq!(checks_glyph(PrCheckStatus::Failure), "✗checks");
        assert_eq!(checks_glyph(PrCheckStatus::Pending), "•checks");
        assert_eq!(checks_glyph(PrCheckStatus::Neutral), "·checks");
        assert_eq!(checks_glyph(PrCheckStatus::None), "-checks");
    }
}

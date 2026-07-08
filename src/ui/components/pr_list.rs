//! PR list pane component.
//! @plan PLAN-20260624-PR-MODE.P12
//! @requirement REQ-PR-006
//! @requirement REQ-PR-014

use iocraft::prelude::*;
use unicode_width::UnicodeWidthStr;

use crate::domain::{PrCheckStatus, PrReviewState, PrState, PullRequest};
use crate::selection::{HighlightRange, TextSelection, row_highlight_range};
use crate::theme::{ResolvedColors, RowColors, SelectionColors, ThemeColors};

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
    /// Active text selection, if any (and if it targets this pane). Selected
    /// cells are painted in inverse video for live drag-selection feedback.
    pub selection: Option<TextSelection>,
}

/// Projected PR list row exactly as the component renders it.
///
/// @plan PLAN-20260624-PR-MODE.P13
/// @requirement REQ-PR-006
/// @pseudocode component-001 lines 1-12
pub struct PrListRowView {
    /// Title line (prefix + "#number " + truncated title). The PR number is
    /// embedded here as "#N "; tests assert identity via this rendered string.
    pub title_line: String,
    /// Meta line (state tag + review/checks glyphs + author + draft + ...).
    pub meta_line: String,
    /// Whether this row is the selected row.
    pub is_selected: bool,
}

/// Build the title line for a PR list row: `prefix` + `#number ` + truncated title.
///
/// The title budget is the available content width minus the width consumed by
/// the number prefix; when `available_width` is `None` the title is shown whole.
///
/// @plan PLAN-20260624-PR-MODE.P13
/// @requirement REQ-PR-006
/// @pseudocode component-001 lines 1-12
fn build_title_line(pr: &PullRequest, prefix: &str, available_width: Option<u16>) -> String {
    let number_prefix = format!("{prefix}#{} ", pr.number);
    let title = match available_width {
        Some(width) => {
            let used = UnicodeWidthStr::width(number_prefix.as_str());
            let budget = (width as usize).saturating_sub(used);
            crate::ui::util::truncate_with_ellipsis(&pr.title, budget)
        }
        None => pr.title.clone(),
    };
    format!("{number_prefix}{title}")
}

/// Build the meta line for a PR list row (state tag + review/checks glyphs +
/// author + draft + comment count + assignee + labels).
///
/// @plan PLAN-20260624-PR-MODE.P13
/// @requirement REQ-PR-006
/// @pseudocode component-001 lines 1-12
fn build_meta_line(pr: &PullRequest) -> String {
    let state_tag = pr_state_tag(pr.state);
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
    format!("     {}", meta_parts.join("  "))
}

/// Pure projection of the visible PR rows exactly as the component renders
/// them. Consumes the `crate::layout` selection-follow window helpers so the
/// rows returned here are the SAME rows the `#[component]` renders (#54/#55).
///
/// @plan PLAN-20260624-PR-MODE.P13
/// @requirement REQ-PR-006
/// @pseudocode component-001 lines 1-12
pub fn pr_list_visible_rows(
    pull_requests: &[PullRequest],
    selected_index: Option<usize>,
    list_pane_rows: u16,
    available_width: Option<u16>,
) -> Vec<PrListRowView> {
    let viewport = list_pane_rows as usize;
    let window =
        crate::layout::list_visible_window(pull_requests, selected_index.unwrap_or(0), viewport);
    let first_visible = crate::layout::list_first_visible_index(
        selected_index.unwrap_or(0),
        pull_requests.len(),
        viewport,
    );
    window
        .iter()
        .enumerate()
        .map(|(window_i, pr)| {
            let is_selected = selected_index == Some(first_visible + window_i);
            let prefix = if is_selected { "> " } else { "  " };
            PrListRowView {
                title_line: build_title_line(pr, prefix, available_width),
                meta_line: build_meta_line(pr),
                is_selected,
            }
        })
        .collect()
}

/// Loading/empty status line for the PR list. Returns `None` when rows are
/// shown (i.e. when the list is non-empty and not loading) — REQ-PR-014.
///
/// @plan PLAN-20260624-PR-MODE.P13
/// @requirement REQ-PR-014
/// @pseudocode component-001 lines 1-12
pub fn pr_list_status_message(
    loading: bool,
    is_empty: bool,
    has_filters: bool,
) -> Option<&'static str> {
    if loading {
        Some("Loading pull requests...")
    } else if is_empty {
        if has_filters {
            Some("No pull requests match filters")
        } else {
            Some("No pull requests found")
        }
    } else {
        None
    }
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

    let status_msg = pr_list_status_message(
        props.loading,
        props.pull_requests.is_empty(),
        props.has_filters,
    );

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
                #(if let Some(msg) = status_msg {
                    vec![element! {
                        Box(padding_left: 1u32, height: 1u32) {
                            Text(content: msg, color: rc.dim)
                        }
                    }
                    .into_any()]
                } else {
                    let rows = props;
                    let projected = pr_list_visible_rows(
                        &rows.pull_requests,
                        rows.selected_index,
                        rows.list_pane_rows,
                        rows.available_width,
                    );
                    projected.iter().enumerate().map(|(window_i, view)| {
                        let highlight = rows.selection.as_ref().and_then(|s| {
                            if s.pane() == crate::selection::SelectablePane::PrList {
                                row_highlight_range(s, window_i)
                            } else {
                                None
                            }
                        });
                        render_pr_row(
                            view,
                            rows.layout.is_compact(),
                            highlight,
                            RowColors::from_resolved(&rc),
                            SelectionColors::from_resolved(&rc),
                            rc.dim,
                        )
                    }).collect()
                })
            }
        }
    }
}

/// Render a single PR list row, applying the selection-row highlight when the
/// row falls inside an active drag selection.
fn render_pr_row(
    view: &PrListRowView,
    compact: bool,
    highlight: Option<HighlightRange>,
    row_colors: RowColors,
    highlight_colors: SelectionColors,
    dim: Color,
) -> AnyElement<'static> {
    let title_line = view.title_line.as_str();
    let meta_line = view.meta_line.as_str();
    let is_selected = view.is_selected;

    // When a drag selection covers this row, paint the whole row in the
    // selection colors. Keyboard selection uses bold for text emphasis but
    // not inverse-video, which is reserved for active drag selection.
    let highlighted = highlight.is_some();
    let row_bg = if highlighted {
        highlight_colors.bg
    } else {
        row_colors.bg
    };
    let title_fg = if highlighted {
        highlight_colors.fg
    } else {
        row_colors.fg
    };
    let weight = if is_selected {
        Weight::Bold
    } else {
        Weight::Normal
    };

    if compact {
        return element! {
            Box(height: 1u32, background_color: row_bg) {
                Text(content: title_line, color: title_fg, weight: weight)
            }
        }
        .into_any();
    }

    element! {
        Box(flex_direction: FlexDirection::Column) {
            Box(height: 1u32, background_color: row_bg) {
                Text(content: title_line, color: title_fg, weight: weight)
            }
            Box(height: 1u32, background_color: row_bg) {
                Text(content: meta_line, color: if highlighted { highlight_colors.fg } else { dim })
            }
        }
    }
    .into_any()
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
    use unicode_width::UnicodeWidthStr;

    /// @plan PLAN-20260624-PR-MODE.P12
    /// @requirement REQ-PR-006
    /// @pseudocode component-001 lines 1-12
    #[test]
    fn full_width_prefix_display_width_is_counted_in_title_budget() {
        let number_prefix = "  #１２ ";
        let title = crate::ui::util::truncate_with_ellipsis(
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

    /// The rendered list row (as projected by `pr_list_visible_rows`, which the
    /// `#[component]` consumes) truncates a very long title to the pane width:
    /// the projected `title_line` ends with the ellipsis and its display width
    /// never exceeds the available pane width (regression #37h).
    ///
    /// @plan PLAN-20260624-PR-MODE.P13
    /// @requirement REQ-PR-006
    /// @pseudocode component-001 lines 1-12
    #[test]
    fn test_pr_list_truncates_long_title_with_ellipsis_by_pane_width() {
        use super::pr_list_visible_rows;
        use crate::domain::{PrCheckStatus, PrState, PullRequest};

        let pr = PullRequest {
            number: 7,
            title: "This is an extremely long pull request title that far exceeds any reasonable pane width".to_string(),
            state: PrState::Open,
            author_login: "octocat".to_string(),
            updated_at: "2026-01-01T00:00:00Z".to_string(),
            head_ref: "feature".to_string(),
            base_ref: "main".to_string(),
            is_draft: false,
            review_decision: None,
            checks_status: PrCheckStatus::None,
            assignee_summary: String::new(),
            labels_summary: String::new(),
            comment_count: 0,
        };
        let available_width: u16 = 20;
        let rows =
            pr_list_visible_rows(std::slice::from_ref(&pr), Some(0), 8, Some(available_width));
        let Some(row) = rows.first() else {
            panic!("expected one projected row for a single PR")
        };
        assert!(
            row.title_line.contains("#7"),
            "projected row must carry the PR number, got: {}",
            row.title_line
        );
        assert!(
            row.title_line.ends_with('\u{2026}'),
            "projected title_line must end with ellipsis when truncated, got: {}",
            row.title_line
        );
        assert!(
            UnicodeWidthStr::width(row.title_line.as_str()) <= available_width as usize,
            "projected title_line width must never exceed the pane width ({available_width}), got: {} for {}",
            UnicodeWidthStr::width(row.title_line.as_str()),
            row.title_line
        );
    }

    /// `review_glyph` covers all remaining decision states so a PR list row
    /// surfaces each review-decision distinctly (Pending/Commented map to the
    /// "needs-attention" glyph; None maps to the neutral glyph).
    ///
    /// @plan PLAN-20260624-PR-MODE.P13
    /// @requirement REQ-PR-006
    /// @pseudocode component-001 lines 1-12
    #[test]
    fn test_pr_list_review_glyph_pending_commented_none_distinct() {
        assert_eq!(
            review_glyph(Some(PrReviewState::Pending)),
            "~review",
            "Pending review must map to the needs-attention glyph"
        );
        assert_eq!(
            review_glyph(Some(PrReviewState::Commented)),
            "~review",
            "Commented review must map to the needs-attention glyph"
        );
        assert_eq!(
            review_glyph(Some(PrReviewState::None)),
            "-review",
            "None review must map to the neutral glyph"
        );
    }
}

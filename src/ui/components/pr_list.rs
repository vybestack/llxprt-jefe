//! PR list pane projection for the generic [`SelectableList`] component.
//!
//! The PR list pane used to have its own iocraft `PrList` component; the
//! rendering is now owned by [`crate::ui::components::SelectableList`]. This
//! module keeps the pure projection ([`pr_list_visible_rows`]) plus the
//! [`pr_list_props`] wrapper that maps the projected rows into
//! [`crate::ui::components::SelectableRow`]s.
//!
//! @plan PLAN-20260624-PR-MODE.P12
//! @requirement REQ-PR-006
//! @requirement REQ-PR-014

use unicode_width::UnicodeWidthStr;

use crate::domain::{PrCheckStatus, PrReviewState, PrState, PullRequest};
use crate::selection::{SelectablePane, TextSelection};
use crate::theme::ThemeColors;
use crate::ui::components::selectable_list::{
    ListBorder, SelectableListProps, SelectableRow, SelectableSpan, SelectionStyle, SpanColor,
};

/// Rows consumed by the bordered list container and title before item content.
/// Must match the issue list constant (3: top border + title + bottom border).
const LIST_CHROME_ROWS: u16 = 3;

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
    let viewport = (list_pane_rows.saturating_sub(LIST_CHROME_ROWS)).max(1) as usize;
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

/// Map already-windowed [`PrListRowView`]s into [`SelectableRow`]s for the
/// generic [`crate::ui::components::SelectableList`]. In compact mode the meta
/// line is replaced with an empty string so the list renders a single-line row
/// (matching the pre-refactor `render_pr_row` compact path). Takes the views by
/// value so the owned strings move into the spans without per-row clones.
fn to_selectable_rows(views: Vec<PrListRowView>, compact: bool) -> Vec<SelectableRow> {
    views
        .into_iter()
        .map(|v| SelectableRow {
            spans: vec![SelectableSpan {
                text: v.title_line,
                color: SpanColor::Themed,
            }],
            meta_line: Some(if compact { String::new() } else { v.meta_line }),
            is_selected: v.is_selected,
        })
        .collect()
}

/// Windowing/geometry inputs for [`pr_list_props`].
///
/// Bundles the parameters that [`pr_list_visible_rows`] needs to compute the
/// visible window, plus the layout variant (which decides compact vs full row
/// rendering). Keeping them in a struct keeps [`pr_list_props`] under the
/// argument-count limit.
///
/// @plan PLAN-20260624-PR-MODE.P13
/// @requirement REQ-PR-006
#[derive(Clone, Copy, Debug)]
pub struct PrListWindow {
    /// Currently selected PR index.
    pub selected_index: Option<usize>,
    /// PR-list pane height in rows.
    pub list_pane_rows: u16,
    /// Available content width (in terminal columns) for title truncation.
    pub available_width: Option<u16>,
    /// List density variant (compact rows omit the meta line).
    pub layout: PrListLayout,
}

/// Build [`SelectableListProps`] for the PR list pane.
///
/// Calls the unchanged [`pr_list_visible_rows`] projection and maps each
/// [`PrListRowView`] into a [`SelectableRow`]. The empty/loading message is
/// computed by the caller via [`pr_list_status_message`] and passed in as
/// `empty_message`.
///
/// @plan PLAN-20260624-PR-MODE.P13
/// @requirement REQ-PR-006
#[must_use]
pub fn pr_list_props(
    pull_requests: &[PullRequest],
    window: PrListWindow,
    focused: bool,
    empty_message: Option<&str>,
    colors: ThemeColors,
    selection: Option<TextSelection>,
) -> SelectableListProps {
    let rows = pr_list_visible_rows(
        pull_requests,
        window.selected_index,
        window.list_pane_rows,
        window.available_width,
    );
    SelectableListProps {
        title: "Pull Requests".to_string(),
        rows: to_selectable_rows(rows, window.layout.is_compact()),
        focused,
        empty_message: empty_message.map(String::from),
        colors,
        selection,
        pane: SelectablePane::PrList,
        border: ListBorder::DoubleOnFocus,
        content_padding: false,
        selection_style: SelectionStyle::BoldSelected,
    }
}

/// Loading/empty status line for the PR list. Returns `None` when rows are
/// shown (i.e. when the list is non-empty and not loading) — REQ-PR-014.
///
/// @plan PLAN-20260624-PR-MODE.P13
/// @requirement REQ-PR-014
/// @pseudocode component-001 lines 1-12
#[must_use]
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

/// Map a PR state to its short uppercase tag used in the list meta line.
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

    /// The PR list viewport must subtract chrome rows (border + title + border
    /// = 3 rows) from `list_pane_rows` before computing the item-level
    /// viewport. Without this subtraction the selection goes off-screen by ~3
    /// rows before the scroll window catches up.
    ///
    /// This test creates 30 PRs with a 10-row pane. With the chrome subtraction
    /// (10 - 3 = 7 visible items), selecting index 9 should scroll so that
    /// the window starts at index 3 (items 3-9 visible). Without the fix,
    /// viewport = 10 and no scrolling happens at all (items 0-9 all "fit"),
    /// leaving the selected item off the visible area.
    #[test]
    fn test_pr_list_viewport_subtracts_chrome_rows() {
        use super::pr_list_visible_rows;
        use crate::domain::{PrCheckStatus, PrState, PullRequest};

        let prs: Vec<PullRequest> = (0..30)
            .map(|i| PullRequest {
                number: i + 1,
                title: format!("PR {i}"),
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
            })
            .collect();

        let pane_rows: u16 = 10;
        let rows = pr_list_visible_rows(&prs, Some(9), pane_rows, None);
        let visible_count = rows.len();
        // 10 pane rows - 3 chrome (border+title+border) = 7 visible items.
        assert_eq!(
            visible_count, 7,
            "with 10-row pane and 3 chrome rows, exactly 7 items must be visible"
        );
        // The last visible item must be the selected index 9.
        let Some(last) = rows.last() else {
            panic!("should have rows for 30 PRs in a 10-row pane");
        };
        assert!(
            last.is_selected,
            "last visible row must be the selected row (index 9)"
        );
        assert!(
            last.title_line.contains("#10"),
            "last visible row must be PR #10 (index 9)"
        );
    }
}

//! PR detail pane projection.
//!
//! The pure header projection ([`PrDetailHeaderView`] /
//! [`pr_detail_header_view`]) is shared with the selection content provider.
//! The full-pane projection ([`pr_detail_props`]) computes all layout math +
//! semantic colors and builds a [`DetailPaneProps`] that the generic
//! [`DetailPane`] renders via [`detail_pane_element`]. This module stays
//! iocraft-free (pure-views pattern): it emits semantic [`DetailHeaderColor`]
//! roles, never concrete `Color`.

use crate::domain::{PrState, PullRequestDetail};
use crate::layout::PR_DETAIL_HEADER_ROWS as HEADER_ROWS;
use crate::pr_detail_content::{
    build_pr_detail_content, checks_status_glyph, mergeable_glyph, pr_state_tag,
    review_status_glyph,
};
use crate::selection::{SelectablePane, TextSelection};
use crate::state::{InlineState, PrDetailSubfocus};
use crate::theme::ThemeColors;

use super::detail_pane::{
    DetailComposerProps, DetailHeaderColor, DetailHeaderRow, DetailPaneProps,
    composer_from_inline_state,
};

/// Pure fallback term-rows constant used when no viewport height is supplied
/// by the screen (the component never reads the terminal itself). Mirrors the
/// `40`-row default the screen uses for its own terminal-size fallback.
const DEFAULT_PR_DETAIL_TERM_ROWS: usize = 40;

/// Projected PR detail header exactly as the component renders it (the four
/// fixed metadata rows). Tests assert the SAME header the renderer renders.
///
/// @plan PLAN-20260624-PR-MODE.P13
/// @requirement REQ-PR-009
/// @requirement REQ-PR-012
/// @pseudocode component-001 lines 1-12
pub struct PrDetailHeaderView {
    /// "#number title[DRAFT]" row.
    pub title: String,
    /// State tag + author/created/updated row.
    pub state: String,
    /// "{head} -> {base}  labels: ...  assignees: ...  milestone: ..." row.
    pub branches: String,
    /// Display-only external URL row.
    pub url: String,
}

/// Pure projection of the PR detail's four fixed header rows exactly as the
/// component renders them.
///
/// Issue #155 redesign: the previous header was a leaky key=value run-on (raw
/// ISO timestamps, `-` placeholders, a single-arrow branch line). The state
/// row now uses human dates (`Jul 6, 2026`) instead of raw ISO, the branch
/// line uses a double arrow (`-->`) and `—` for empty fields. The four-row
/// structure is preserved so the fixed `PR_DETAIL_HEADER_ROWS` layout
/// invariant (and the scroll viewport math it drives) is unchanged.
///
/// Issue #314: the state row now also carries mergeable + checks-rollup +
/// review-decision status glyphs, so the mergeability, workflow-error, and
/// approval-needed signals are visible in the fixed header (mirroring the
/// PR list meta line glyphs).
///
/// @plan PLAN-20260624-PR-MODE.P13
/// @requirement REQ-PR-009
/// @requirement REQ-PR-012
/// @pseudocode component-001 lines 1-12
#[must_use]
pub fn pr_detail_header_view(detail: &PullRequestDetail) -> PrDetailHeaderView {
    let state_tag = pr_state_tag(detail.state);
    let draft_marker = if detail.is_draft { " [DRAFT]" } else { "" };
    let created = crate::ui::util::format_iso_date(&detail.created_at);
    let updated = crate::ui::util::format_iso_date(&detail.updated_at);
    let labels_str = crate::ui::util::field_list(&detail.labels);
    let assignees_str = crate::ui::util::field_list(&detail.assignees);
    let milestone_str = crate::ui::util::field_opt(detail.milestone.as_deref());
    let merge_glyph = mergeable_glyph(detail.mergeable);
    let checks_glyph = checks_status_glyph(detail.checks_status);
    let review_glyph = review_status_glyph(detail.review_decision);
    PrDetailHeaderView {
        title: format!("#{} {}{}", detail.number, detail.title, draft_marker),
        state: format!(
            "{state_tag}  {merge_glyph}  {checks_glyph}  {review_glyph}  by @{}  created: {created}  updated: {updated}",
            detail.author_login
        ),
        branches: format!(
            "{} --> {}  labels: {labels_str}  assignees: {assignees_str}  milestone: {milestone_str}",
            detail.head_ref, detail.base_ref
        ),
        url: detail.external_url.clone(),
    }
}

/// Inputs the PRs screen passes to [`pr_detail_props`], bundled to stay under
/// the clippy::too_many-arguments threshold (max 6).
///
/// @plan PLAN-20260624-PR-MODE.P12
/// @requirement REQ-PR-009
/// @pseudocode component-001 lines 1-12
pub struct PrDetailProjectionInputs<'a> {
    /// Full PR detail (metadata, body, reviews, checks, comments).
    pub detail: Option<&'a PullRequestDetail>,
    /// Which sub-element is focused within the detail view.
    pub subfocus: PrDetailSubfocus,
    /// Scroll offset for the content viewport.
    pub scroll_offset: usize,
    /// Detail pane viewport height in rows, supplied by the screen.
    ///
    /// @plan PLAN-20260624-PR-MODE.P12
    /// @requirement REQ-PR-009
    pub viewport_rows: Option<u16>,
    /// Whether the full PR detail is loading (after instant list preview).
    pub detail_loading: bool,
    /// Whether comments are loading.
    pub comments_loading: bool,
    /// Whether this pane is focused.
    pub focused: bool,
    /// Active inline editor/composer state.
    pub inline_state: &'a InlineState,
    /// Content width (terminal cols) for truncating PR-detail text.
    ///
    /// @plan PLAN-20260624-PR-MODE.P14
    /// @requirement REQ-PR-009
    pub detail_content_width: usize,
    /// Theme colors.
    pub colors: ThemeColors,
    /// Active text selection, if any (and if it targets this pane).
    pub selection: Option<TextSelection>,
}

/// Semantic state-row color for a PR: `Bright` when Open, `Dim` when Closed or
/// Merged. Matches the pre-refactor component (`rc.bright` / `rc.dim`).
fn pr_state_color(state: PrState) -> DetailHeaderColor {
    match state {
        PrState::Open => DetailHeaderColor::Bright,
        PrState::Closed | PrState::Merged => DetailHeaderColor::Dim,
    }
}

/// Compute the detail viewport rows (scrollable area) from the supplied pane
/// height. Mirrors the pre-refactor component's height-derived viewport math.
fn detail_viewport_rows(available_height: Option<u16>) -> usize {
    if let Some(height) = available_height {
        crate::layout::detail_body_viewport_rows(usize::from(height))
    } else {
        crate::layout::prs_detail_viewport_rows(DEFAULT_PR_DETAIL_TERM_ROWS, false, false)
    }
}

/// Build the five fixed header rows (title, state, branches, url, separator)
/// with their semantic colors and selection-line indices.
fn build_header_rows(
    h_title: String,
    h_state: String,
    h_branches: String,
    h_url: String,
    state_color: DetailHeaderColor,
) -> Vec<DetailHeaderRow> {
    vec![
        DetailHeaderRow {
            content: h_title,
            color: DetailHeaderColor::Fg,
            line: 0,
        },
        DetailHeaderRow {
            content: h_state,
            color: state_color,
            line: 1,
        },
        DetailHeaderRow {
            content: h_branches,
            color: DetailHeaderColor::Dim,
            line: 2,
        },
        DetailHeaderRow {
            content: h_url,
            color: DetailHeaderColor::Dim,
            line: 3,
        },
        DetailHeaderRow {
            content: super::SEPARATOR_LINE.to_string(),
            color: DetailHeaderColor::Dim,
            line: 4,
        },
    ]
}

/// Resolve the content + header for a loaded PR detail.
fn loaded_pr_content(
    detail: &PullRequestDetail,
    subfocus: PrDetailSubfocus,
    inline_state: &InlineState,
    detail_loading: bool,
    comments_loading: bool,
) -> (
    Vec<DetailHeaderRow>,
    crate::issue_detail_content::DetailContent,
) {
    let header = pr_detail_header_view(detail);
    let rows = build_header_rows(
        header.title,
        header.state,
        header.branches,
        header.url,
        pr_state_color(detail.state),
    );
    (
        rows,
        build_pr_detail_content(
            detail,
            subfocus,
            inline_state,
            detail_loading,
            comments_loading,
        ),
    )
}

/// Resolve the content + header for the "no PR selected" placeholder branch.
fn empty_pr_content() -> (
    Vec<DetailHeaderRow>,
    crate::issue_detail_content::DetailContent,
) {
    let rows = build_header_rows(
        String::new(),
        String::new(),
        String::new(),
        String::new(),
        DetailHeaderColor::Dim,
    );
    (
        rows,
        crate::issue_detail_content::DetailContent {
            text: "No pull request selected".to_string(),
            cursor: None,
        },
    )
}

/// Pure projection of the PR detail pane into a [`DetailPaneProps`].
///
/// Encapsulates ALL the logic the pre-refactor `PrDetailView` component body
/// owned: the loaded-detail / empty branching, the viewport/composer row math,
/// the semantic state color, and the composer extraction. The result is
/// rendered byte-identically by the generic [`DetailPane`] via
/// [`pr_detail_element`].
///
/// This function is iocraft-free: it emits semantic [`DetailHeaderColor`] roles
/// (never concrete `Color`), keeping the pure-views invariant.
///
/// @plan PLAN-20260624-PR-MODE.P12
/// @plan PLAN-20260624-PR-MODE.P14
/// @requirement REQ-PR-009
/// @requirement REQ-PR-010
/// @pseudocode component-001 lines 169-176
#[must_use]
pub fn pr_detail_props(inputs: PrDetailProjectionInputs<'_>) -> DetailPaneProps {
    let detail_viewport_rows = detail_viewport_rows(inputs.viewport_rows);
    let composer = composer_from_inline_state(inputs.inline_state);
    let text_box_active = composer.is_some();

    let scroll_rows =
        crate::layout::pr_detail_document_viewport_rows(detail_viewport_rows, text_box_active);
    let composer_rows = detail_viewport_rows.saturating_sub(scroll_rows);

    let (header_rows, detail_content) = if let Some(detail) = inputs.detail {
        loaded_pr_content(
            detail,
            inputs.subfocus,
            inputs.inline_state,
            inputs.detail_loading,
            inputs.comments_loading,
        )
    } else {
        empty_pr_content()
    };

    let composer_props = composer.map(|(text, byte_cursor, prefix)| DetailComposerProps {
        text,
        byte_cursor,
        content_width: inputs.detail_content_width,
        prefix,
    });

    DetailPaneProps {
        header_rows,
        content: detail_content.text,
        content_cursor: detail_content.cursor,
        scroll_offset: inputs.scroll_offset,
        viewport_rows: scroll_rows,
        content_line_offset: HEADER_ROWS,
        max_line_width: inputs.detail_content_width,
        focused: inputs.focused,
        pane: SelectablePane::PrDetail,
        colors: inputs.colors,
        selection: inputs.selection,
        composer: composer_props,
        composer_rows,
    }
}

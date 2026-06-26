//! Unified PR detail + reviews + checks + comments view.
//! @plan PLAN-20260624-PR-MODE.P12
//! @requirement REQ-PR-009

use iocraft::prelude::*;

use crate::domain::PullRequestDetail;
use crate::layout::PR_DETAIL_HEADER_ROWS as HEADER_ROWS;
use crate::pr_detail_content::{build_pr_detail_content, pr_state_tag};
use crate::state::{InlineState, PrDetailSubfocus};
use crate::theme::{ResolvedColors, ThemeColors};

use super::scrollable_text::ScrollableText;

/// Pure fallback term-rows constant used when no viewport height is supplied
/// by the screen (the component never reads the terminal itself). Mirrors the
/// `40`-row default the screen uses for its own terminal-size fallback.
const DEFAULT_PR_DETAIL_TERM_ROWS: usize = 40;

/// Projected PR detail header exactly as the component renders it (the four
/// fixed metadata rows). The `#[component]` delegates to this so tests assert
/// the SAME header the component renders (REQ-PR-009 / REQ-PR-012).
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
/// @plan PLAN-20260624-PR-MODE.P13
/// @requirement REQ-PR-009
/// @requirement REQ-PR-012
/// @pseudocode component-001 lines 1-12
pub fn pr_detail_header_view(detail: &PullRequestDetail) -> PrDetailHeaderView {
    let state_tag = pr_state_tag(detail.state);
    let draft_marker = if detail.is_draft { " [DRAFT]" } else { "" };
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
    PrDetailHeaderView {
        title: format!("#{} {}{}", detail.number, detail.title, draft_marker),
        state: format!(
            "{}  author: {}  created: {}  updated: {}",
            state_tag, detail.author_login, detail.created_at, detail.updated_at
        ),
        branches: format!(
            "{} -> {}  labels: {}  assignees: {}  milestone: {}",
            detail.head_ref, detail.base_ref, labels_str, assignees_str, milestone_str
        ),
        url: detail.external_url.clone(),
    }
}

/// Props for the PR detail view.
///
/// @plan PLAN-20260624-PR-MODE.P12
/// @requirement REQ-PR-009
/// @pseudocode component-001 lines 1-12
#[derive(Default, Props)]
pub struct PrDetailViewProps {
    /// Full PR detail (metadata, body, reviews, checks, comments).
    pub detail: Option<PullRequestDetail>,
    /// Which sub-element is focused within the detail view.
    pub subfocus: PrDetailSubfocus,
    /// Scroll offset for the content viewport.
    pub scroll_offset: usize,
    /// Detail pane viewport height in rows, supplied by the screen.
    ///
    /// @plan PLAN-20260624-PR-MODE.P12
    /// @requirement REQ-PR-009
    /// @pseudocode component-001 lines 1-12
    pub viewport_rows: Option<u16>,
    /// Whether comments are loading.
    pub comments_loading: bool,
    /// Whether this pane is focused.
    pub focused: bool,
    /// Active inline editor/composer state.
    pub inline_state: InlineState,
    /// Theme colors.
    pub colors: ThemeColors,
}

/// PR detail view — fixed structure that NEVER changes layout.
///
/// ALWAYS renders: border box → `HEADER_ROWS` header rows → fixed-row scrollable viewport.
/// When no PR is selected, header rows are blank and viewport shows a message.
/// This ensures layout is identical regardless of whether a PR is loaded.
///
/// @plan PLAN-20260624-PR-MODE.P12
/// @requirement REQ-PR-009
/// @pseudocode component-001 lines 1-12
#[component]
pub fn PrDetailView(props: &PrDetailViewProps) -> impl Into<AnyElement<'static>> {
    let rc = ResolvedColors::from_theme(Some(&props.colors));
    let border_style = if props.focused {
        BorderStyle::Double
    } else {
        BorderStyle::Round
    };

    // Compute viewport rows. Prefer the actual viewport height passed from
    // the parent layout; otherwise fall back to a pure constant default that
    // does NOT read the terminal (the component must never query the terminal).
    let scroll_rows = if let Some(height) = props.viewport_rows {
        (height as usize).saturating_sub(HEADER_ROWS + 2)
    } else {
        crate::layout::prs_detail_viewport_rows(DEFAULT_PR_DETAIL_TERM_ROWS, false, false)
    };

    let (h_title, h_state, h_branches, h_url, detail_content, state_color) =
        if let Some(detail) = props.detail.as_ref() {
            let sc = match detail.state {
                crate::domain::PrState::Open => rc.bright,
                crate::domain::PrState::Closed | crate::domain::PrState::Merged => rc.dim,
            };
            let header = pr_detail_header_view(detail);
            (
                header.title,
                header.state,
                header.branches,
                header.url,
                build_pr_detail_content(
                    detail,
                    props.subfocus,
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
                crate::issue_detail_content::DetailContent {
                    text: "No pull request selected".to_string(),
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
                    Text(content: h_branches, color: rc.dim)
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

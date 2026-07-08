//! Unified PR detail + reviews + checks + comments view.
//! @plan PLAN-20260624-PR-MODE.P12
//! @requirement REQ-PR-009

use iocraft::prelude::*;

use crate::domain::PullRequestDetail;
use crate::layout::PR_DETAIL_HEADER_ROWS as HEADER_ROWS;
use crate::pr_detail_content::{build_pr_detail_content, pr_state_tag};
use crate::selection::{SelectablePane, TextSelection};
use crate::state::{ComposerTarget, InlineState, PrDetailSubfocus};
use crate::theme::{ResolvedColors, ThemeColors};

use super::issue_detail::header_row;
use super::scrollable_text::ScrollableText;
use super::text_box::TextBox;

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
    /// Whether the full PR detail is loading (after instant list preview).
    pub detail_loading: bool,
    /// Whether comments are loading.
    pub comments_loading: bool,
    /// Whether this pane is focused.
    pub focused: bool,
    /// Active inline editor/composer state.
    pub inline_state: InlineState,
    /// Content width (terminal cols) for truncating PR-detail text via the
    /// ScrollableText `max_line_width`, supplied by the screen from the same
    /// `crossterm::terminal::size()` read the screen already performs. The
    /// reducer NEVER wraps — truncation is safe (it clips columns only, never
    /// changing line counts or cursor line), mirroring Issues mode.
    ///
    /// @plan PLAN-20260624-PR-MODE.P14
    /// @requirement REQ-PR-009
    /// @pseudocode component-001 lines 1-12
    pub detail_content_width: usize,
    /// Theme colors.
    pub colors: ThemeColors,
    /// Active text selection, if any (and if it targets this pane). Passed
    /// through to the `ScrollableText` so selected cells render inverse-video.
    pub selection: Option<TextSelection>,
}

/// Extract the active PR composer `(text, byte_cursor, prefix)`.
///
/// @plan PLAN-20260624-PR-MODE.P14
/// @requirement REQ-PR-009
/// @requirement REQ-PR-010
/// @pseudocode component-001 lines 169-176
fn active_pr_composer(inline_state: &InlineState) -> Option<(String, usize, &'static str)> {
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

/// PR detail view — fixed structure that NEVER changes layout.
///
/// ALWAYS renders: border box → `HEADER_ROWS` header rows → fixed-row scrollable viewport.
/// When no PR is selected, header rows are blank and viewport shows a message.
/// When a NewComment composer is active, an embedded `TextBox` is rendered
/// below the scroll viewport (the read-only document no longer flattens the
/// composer text/cursor, so the document scroll offset stays stable while
/// typing and the TextBox owns its own local caret viewport).
///
/// @plan PLAN-20260624-PR-MODE.P12
/// @plan PLAN-20260624-PR-MODE.P14
/// @requirement REQ-PR-009
/// @requirement REQ-PR-010
/// @pseudocode component-001 lines 169-176
#[component]
pub fn PrDetailView(props: &PrDetailViewProps) -> impl Into<AnyElement<'static>> {
    let rc = ResolvedColors::from_theme(Some(&props.colors));
    let border_style = if props.focused {
        BorderStyle::Double
    } else {
        BorderStyle::Round
    };

    // Detect an active PR composer so we can reserve space for the embedded
    // TextBox and reduce the scroll viewport accordingly.
    let composer = active_pr_composer(&props.inline_state);
    let text_box_active = composer.is_some();

    // Compute viewport rows. Production screens pass the actual pane height;
    // the fallback is a pure test/default path and intentionally avoids reading
    // terminal size inside this component. Use the shared helper so the state
    // scroll bounds and ScrollableText row count stay in the same coordinate
    // system, including tiny panes where one read-only row is preserved.
    let detail_viewport_rows = if let Some(height) = props.viewport_rows {
        (height as usize).saturating_sub(HEADER_ROWS + 2)
    } else {
        crate::layout::prs_detail_viewport_rows(DEFAULT_PR_DETAIL_TERM_ROWS, false, false)
    };
    let scroll_rows =
        crate::layout::pr_detail_document_viewport_rows(detail_viewport_rows, text_box_active);
    let composer_rows = detail_viewport_rows.saturating_sub(scroll_rows);

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
                    props.detail_loading,
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

    // Compute the TextBox props for the active PR composer (if any). The
    // composer content width mirrors the detail content width so gutter-aligned
    // text lines up with the read-only document.
    let composer_element: Option<AnyElement<'static>> =
        composer.map(|(text, byte_cursor, prefix)| {
            element! {
                Box(width: 100pct, padding_left: 1u32) {
                    TextBox(
                        text: text,
                        byte_cursor: byte_cursor,
                        viewport_rows: composer_rows,
                        content_width: props.detail_content_width,
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
                #(header_row(h_title, rc.fg, 0, props.selection.as_ref(), SelectablePane::PrDetail, &rc))
                #(header_row(h_state, state_color, 1, props.selection.as_ref(), SelectablePane::PrDetail, &rc))
                #(header_row(h_branches, rc.dim, 2, props.selection.as_ref(), SelectablePane::PrDetail, &rc))
                #(header_row(h_url, rc.dim, 3, props.selection.as_ref(), SelectablePane::PrDetail, &rc))
                #(header_row(
                    "─────────────────────────────────────────".to_string(),
                    rc.dim,
                    4,
                    props.selection.as_ref(),
                    SelectablePane::PrDetail,
                    &rc,
                ))
            }

            // ── Scrollable viewport — always exactly scroll_rows rows ─────
            Box(width: 100pct, padding_left: 1u32) {
                ScrollableText(
                    content: detail_content.text,
                    scroll_offset: props.scroll_offset,
                    viewport_rows: scroll_rows,
                    max_line_width: props.detail_content_width,
                    cursor_line: detail_content.cursor.map(|(l, _)| l),
                    cursor_col: detail_content.cursor.map(|(_, c)| c),
                    color: rc.fg,
                    cursor_color: rc.bg,
                    cursor_bg: rc.bright,
                    track_color: rc.dim,
                    thumb_color: rc.bright,
                    selection: props
                        .selection
                        .filter(|s| s.pane() == crate::selection::SelectablePane::PrDetail),
                    selection_bg: Some(rc.sel_bg),
                    selection_fg: Some(rc.sel_fg),
                    bg: Some(rc.bg),
                    content_line_offset: HEADER_ROWS,
                )
            }

            // ── Embedded NewComment TextBox composer (when active) ────────
            #(composer_element)
        }
    }
}

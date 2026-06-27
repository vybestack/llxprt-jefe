//! Render-path tests for PrDetailView (#20): exercises the REAL iocraft
//! render to a Canvas and inspects the ANSI bytes + plain-text output. Catches
//! defects the pure-content tests miss (caret SGR, double-truncation,
//! continuation indent at the component layer).
//!
//! @plan PLAN-20260624-PR-MODE.P14
//! @requirement REQ-PR-009
//! @requirement REQ-PR-010

use crate::domain::{IssueComment, PrCheckStatus, PrState, PullRequestDetail};
use crate::state::{ComposerTarget, InlineState, PrDetailSubfocus};
use crate::theme::ThemeColors;
use crate::ui::components::PrDetailView;

use iocraft::prelude::*;

/// Build a PR detail with a long comment body so truncation is exercised.
///
/// @plan PLAN-20260624-PR-MODE.P14
/// @requirement REQ-PR-009
/// @pseudocode component-001 lines 1-12
fn detail_with_long_comment() -> PullRequestDetail {
    PullRequestDetail {
        repo_owner_name: "owner/repo".to_string(),
        number: 20,
        title: "Fix PR mode rendering".to_string(),
        state: PrState::Open,
        is_draft: false,
        author_login: "octocat".to_string(),
        created_at: "2024-01-01T00:00:00Z".to_string(),
        updated_at: "2024-01-02T00:00:00Z".to_string(),
        head_ref: "feature".to_string(),
        base_ref: "main".to_string(),
        labels: vec![],
        assignees: vec![],
        milestone: None,
        body: "Short body".to_string(),
        external_url: "https://github.com/owner/repo/pull/20".to_string(),
        review_decision: None,
        checks_status: PrCheckStatus::None,
        reviews: vec![],
        checks: vec![],
        comments: vec![IssueComment {
            comment_id: 1,
            author_login: "alice".to_string(),
            created_at: "2024-01-03T00:00:00Z".to_string(),
            edited_at: None,
            body: "this is a very long comment body that should wrap across multiple rendered rows when the detail content width is narrow".to_string(),
        }],
        has_more_comments: false,
        comments_cursor: None,
    }
}

/// Bundle of render params to keep the render helpers under the argument
/// limit (clippy::too_many_arguments).
///
/// @plan PLAN-20260624-PR-MODE.P14
/// @requirement REQ-PR-009
/// @pseudocode component-001 lines 1-12
struct RenderParams<'a> {
    detail: &'a PullRequestDetail,
    subfocus: PrDetailSubfocus,
    inline_state: &'a InlineState,
    scroll_offset: usize,
    detail_content_width: usize,
    pane_height: u16,
    cols: u16,
}

/// Render PrDetailView inside a fixed-size Box and return the Canvas (for
/// ANSI byte inspection).
///
/// @plan PLAN-20260624-PR-MODE.P14
/// @requirement REQ-PR-009
/// @pseudocode component-001 lines 1-12
fn render_detail_canvas(p: RenderParams) -> iocraft::Canvas {
    let rows: u16 = p.pane_height + 10;
    let mut elem = element! {
        Box(width: u32::from(p.cols), height: u32::from(rows)) {
            PrDetailView(
                detail: Some(p.detail.clone()),
                subfocus: p.subfocus,
                inline_state: p.inline_state.clone(),
                detail_loading: false,
                comments_loading: false,
                focused: true,
                scroll_offset: p.scroll_offset,
                detail_content_width: p.detail_content_width,
                colors: ThemeColors::default(),
                viewport_rows: Some(p.pane_height),
            )
        }
    };
    elem.render(Some(usize::from(p.cols)))
}

/// Render PrDetailView inside a fixed-size Box and return the plain-text
/// canvas output. This exercises the REAL iocraft layout + ScrollableText
/// truncation path, not just the content builder.
///
/// @plan PLAN-20260624-PR-MODE.P14
/// @requirement REQ-PR-009
/// @pseudocode component-001 lines 1-12
fn render_detail(p: RenderParams) -> String {
    render_detail_canvas(p).to_string()
}

/// No rendered content line may exceed `term_cols` in char count — the
/// ScrollableText truncation width must match the wrap width so nothing
/// overflows the pane (regression: spurious "…" / "off screen").
///
/// @plan PLAN-20260624-PR-MODE.P14
/// @requirement REQ-PR-009
/// @pseudocode component-001 lines 1-12
#[test]
fn rendered_lines_never_exceed_term_cols() {
    let detail = detail_with_long_comment();
    let cols: u16 = 60;
    // content width smaller than cols to exercise truncation guard.
    let content_width = usize::from(crate::layout::prs_detail_content_width(cols));
    let rendered = render_detail(RenderParams {
        detail: &detail,
        subfocus: PrDetailSubfocus::Body,
        inline_state: &InlineState::None,
        scroll_offset: 0,
        detail_content_width: content_width,
        pane_height: 30,
        cols,
    });
    for (i, line) in rendered.lines().enumerate() {
        assert!(
            line.chars().count() <= usize::from(cols),
            "rendered line {i} exceeds term_cols {cols}: {} ({} chars)",
            line,
            line.chars().count()
        );
    }
}

/// A long comment body must be TRUNCATED (clipped to the content width) rather
/// than overflow the pane — mirroring Issues mode, where the reducer never wraps
/// and the renderer (ScrollableText) truncates long lines via `max_line_width`.
/// This confirms the component does NOT overflow when a line exceeds the width.
///
/// @plan PLAN-20260624-PR-MODE.P14
/// @requirement REQ-PR-009
/// @pseudocode component-001 lines 1-12
#[test]
fn long_comment_truncated_to_content_width() {
    let detail = detail_with_long_comment();
    let cols: u16 = 50;
    let content_width = usize::from(crate::layout::prs_detail_content_width(cols));
    let rendered = render_detail(RenderParams {
        detail: &detail,
        subfocus: PrDetailSubfocus::Body,
        inline_state: &InlineState::None,
        scroll_offset: 0,
        detail_content_width: content_width,
        pane_height: 40,
        cols,
    });
    // No rendered line may exceed the terminal column width (the ScrollableText
    // truncation clips long lines so nothing overflows the pane).
    for (i, line) in rendered.lines().enumerate() {
        assert!(
            line.chars().count() <= usize::from(cols),
            "rendered line {i} exceeds term_cols {cols}: {} ({} chars) — \
             long lines must be truncated, not overflow",
            line,
            line.chars().count()
        );
    }
    // The long comment body is a single (unwrapped) line; the start of it must
    // still be visible (truncated, not dropped entirely).
    assert!(
        rendered.contains("this is a very"),
        "the start of a long comment body must still be visible (truncated, not dropped)"
    );
}

/// Count the background-color SGR sequences (`48;2;` truecolor or `48;5;`
/// indexed) in a component's ANSI output. Used to prove the caret's
/// reverse-video cell adds background fills beyond the baseline chrome.
///
/// @plan PLAN-20260624-PR-MODE.P14
/// @requirement REQ-PR-009
/// @pseudocode component-001 lines 1-12
fn background_sgr_count(p: RenderParams) -> usize {
    let canvas = render_detail_canvas(p);
    let mut buf = Vec::new();
    canvas
        .write_ansi(&mut buf)
        .unwrap_or_else(|e| panic!("write_ansi failed: {e}"));
    let ansi = String::from_utf8_lossy(&buf);
    ansi.matches("\u{1b}[48;2;").count() + ansi.matches("\u{1b}[48;5;").count()
}

/// The reverse-video caret cell must render as ADDITIONAL background SGR
/// sequences beyond the baseline (same component with no composer/caret).
/// Comparing against a caret-free baseline avoids a vacuous assertion that
/// would also pass on the box's own background fills.
///
/// @plan PLAN-20260624-PR-MODE.P14
/// @requirement REQ-PR-009
/// @pseudocode component-001 lines 1-12
#[test]
fn caret_renders_as_reverse_video_sgr() {
    let detail = detail_with_long_comment();
    let cols: u16 = 60;
    let content_width = usize::from(crate::layout::prs_detail_content_width(cols));

    // Baseline: no composer, so no caret cell is drawn.
    let baseline = background_sgr_count(RenderParams {
        detail: &detail,
        subfocus: PrDetailSubfocus::NewComment,
        inline_state: &InlineState::None,
        scroll_offset: 0,
        detail_content_width: content_width,
        pane_height: 40,
        cols,
    });

    // With an open composer the caret reverse-video cell must add background
    // SGR sequences on top of the baseline chrome.
    let inline = InlineState::Composer {
        target: ComposerTarget::NewComment,
        text: "hello".to_string(),
        cursor: 5,
    };
    let with_caret = background_sgr_count(RenderParams {
        detail: &detail,
        subfocus: PrDetailSubfocus::NewComment,
        inline_state: &inline,
        scroll_offset: 0,
        detail_content_width: content_width,
        pane_height: 40,
        cols,
    });

    assert!(
        with_caret > baseline,
        "caret must add background SGR sequences ({with_caret}) beyond the \
         caret-free baseline ({baseline})"
    );
}

/// A long composer line must be TRUNCATED (clipped to the content width) rather
/// than overflow the pane — mirroring Issues mode, where the reducer never wraps
/// and the renderer truncates long lines via `max_line_width`. The composer
/// content stays on a SINGLE row (no wrapping, no continuation indent).
///
/// Uses a detail with NO comments/reviews/checks so the composer is near the
/// top of the content (visible without scrolling).
///
/// @plan PLAN-20260624-PR-MODE.P14
/// @requirement REQ-PR-009
/// @pseudocode component-001 lines 1-12
#[test]
fn rendered_long_composer_line_truncated_to_content_width() {
    let detail = PullRequestDetail {
        repo_owner_name: "owner/repo".to_string(),
        number: 20,
        title: "T".to_string(),
        state: PrState::Open,
        is_draft: false,
        author_login: "o".to_string(),
        created_at: "d".to_string(),
        updated_at: "d".to_string(),
        head_ref: "f".to_string(),
        base_ref: "m".to_string(),
        labels: vec![],
        assignees: vec![],
        milestone: None,
        body: String::new(),
        external_url: "u".to_string(),
        review_decision: None,
        checks_status: PrCheckStatus::None,
        reviews: vec![],
        checks: vec![],
        comments: vec![],
        has_more_comments: false,
        comments_cursor: None,
    };
    let long_text =
        "abcdefghijklmnopqrstuvwxyz0123456789abcdefghijklmnopqrstuvwxyz0123456789".to_string();
    let inline = InlineState::Composer {
        target: ComposerTarget::NewComment,
        text: long_text,
        cursor: 30,
    };
    let cols: u16 = 80;
    let content_width = usize::from(crate::layout::prs_detail_content_width(cols));
    let rendered = render_detail(RenderParams {
        detail: &detail,
        subfocus: PrDetailSubfocus::NewComment,
        inline_state: &inline,
        scroll_offset: 0,
        detail_content_width: content_width,
        pane_height: 40,
        cols,
    });
    // No rendered line may exceed the terminal column width: a long composer
    // line must be truncated (clipped), not wrapped or overflowed.
    for (i, line) in rendered.lines().enumerate() {
        assert!(
            line.chars().count() <= usize::from(cols),
            "rendered line {i} exceeds term_cols {cols}: {} ({} chars) — \
             a long composer line must be truncated, not overflow",
            line,
            line.chars().count()
        );
    }
    // The composer line starts with the gutter "│ " then the (truncated) text;
    // the start of the typed content must still be visible.
    assert!(
        rendered.contains("│ abcdef"),
        "the start of a long composer line must be visible (truncated, not dropped): {rendered}"
    );
}

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
use crate::ui::components::{PrDetailProjectionInputs, detail_pane_element, pr_detail_props};

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
        comments: crate::domain::PaginatedList::from_loaded(
            crate::domain::CommentDetailIdentity {
                scope_repo_id: crate::domain::RepositoryId::default(),
                number: 20,
            },
            vec![IssueComment {
            comment_id: 1,
            author_login: "alice".to_string(),
            created_at: "2024-01-03T00:00:00Z".to_string(),
            edited_at: None,
            body: "this is a very long comment body that should wrap across multiple rendered rows when the detail content width is narrow".to_string(),
        }],
            crate::domain::PageToken::from_cursor(None, false),
        ),
        mergeable: None,
        merge_state_status: None,
    }
}

/// A minimal PR detail (single-char fields, no comments) so the composer sits
/// near the top of the content and is visible without scrolling. Used by the
/// composer-wrap render test.
fn bare_pr_detail() -> PullRequestDetail {
    PullRequestDetail {
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
        comments: crate::domain::PaginatedList::from_loaded(
            crate::domain::CommentDetailIdentity {
                scope_repo_id: crate::domain::RepositoryId::default(),
                number: 20,
            },
            vec![],
            crate::domain::PageToken::from_cursor(None, false),
        ),
        mergeable: None,
        merge_state_status: None,
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
            #(vec![detail_pane_element(pr_detail_props(
                PrDetailProjectionInputs {
                    detail: Some(p.detail),
                    subfocus: p.subfocus,
                    inline_state: p.inline_state,
                    detail_loading: false,
                    comments_loading: false,
                    focused: true,
                    scroll_offset: p.scroll_offset,
                    detail_content_width: p.detail_content_width,
                    colors: ThemeColors::default(),
                    viewport_rows: Some(p.pane_height),
                    selection: None,
                },
            ))])
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

/// A long comment body must WORD-WRAP across several rendered rows so its
/// full text is visible (not truncated with an ellipsis). This is the render
/// regression guard for the read-only comment display follow-up to #212.
///
/// @plan PLAN-20260624-PR-MODE.P14
/// @requirement REQ-PR-009
#[test]
fn long_comment_wraps_across_rendered_rows() {
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
    // No rendered row may exceed the terminal column width.
    for (i, line) in rendered.lines().enumerate() {
        assert!(
            line.chars().count() <= usize::from(cols),
            "rendered line {i} exceeds term_cols {cols}: {} ({} chars) — \
             wrapped rows must never overflow the pane",
            line,
            line.chars().count()
        );
    }
    // Wrapping makes BOTH the start and the tail of the long comment visible
    // (truncation would drop the tail).
    assert!(
        rendered.contains("this is a very"),
        "the start of a long comment body must be visible: {rendered}"
    );
    assert!(
        rendered.contains("narrow"),
        "the TAIL of a long comment body must be visible (wrap, not truncate): {rendered}"
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

/// A long composer line must WORD-WRAP across several rendered rows (rather
/// than overflow the pane) via the embedded `TextBox`. The composer content
/// folds at word boundaries so neither the start nor the tail is dropped.
///
/// Uses a detail with NO comments/reviews/checks so the composer is near the
/// top of the content (visible without scrolling).
///
/// @plan PLAN-20260624-PR-MODE.P14
/// @requirement REQ-PR-009
/// @requirement REQ-TEXTBOX-WRAP
#[test]
fn rendered_long_composer_line_wraps_to_content_width() {
    let detail = bare_pr_detail();
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
    // line must word-wrap (fold onto following rows), not overflow.
    for (i, line) in rendered.lines().enumerate() {
        assert!(
            line.chars().count() <= usize::from(cols),
            "rendered line {i} exceeds term_cols {cols}: {} ({} chars) — \
             a long composer line must wrap, not overflow",
            line,
            line.chars().count()
        );
    }
    // The composer line starts with the gutter "│ " then the wrapped text;
    // BOTH the start and the tail must be visible (truncation would drop the
    // tail), proving the long composer line wraps rather than truncates.
    assert!(
        rendered.contains("│ abcdef"),
        "the start of a long composer line must be visible (wrap, not truncate): {rendered}"
    );
    assert!(
        rendered.contains("56789"),
        "the TAIL of a long composer line must be visible (wrap, not truncate): {rendered}"
    );
}

/// An active NewComment composer with multi-line text must render the
/// composer's current text via the embedded TextBox (the text must be visible
/// in the rendered output), even with an intentionally stale detail scroll
/// offset. This proves the TextBox owns its own viewport independent of the
/// document scroll.
///
/// @plan PLAN-20260624-PR-MODE.P14
/// @requirement REQ-PR-009
/// @requirement REQ-PR-010
/// @pseudocode component-001 lines 169-176
#[test]
fn active_new_comment_composer_renders_text_box_text_with_stale_offset() {
    let detail = detail_with_long_comment();
    let inline = InlineState::Composer {
        target: ComposerTarget::NewComment,
        text: "first line\nsecond line\nthird line".to_string(),
        cursor: "first line\nsecond line\nthird line".len(),
    };
    let cols: u16 = 80;
    let content_width = usize::from(crate::layout::prs_detail_content_width(cols));
    // Intentionally stale scroll offset (top) — the TextBox must still show
    // the current (last) line because it owns its own viewport.
    let rendered = render_detail(RenderParams {
        detail: &detail,
        subfocus: PrDetailSubfocus::NewComment,
        inline_state: &inline,
        scroll_offset: 0,
        detail_content_width: content_width,
        pane_height: 40,
        cols,
    });
    // The current (last) composer line must be visible in the TextBox output.
    assert!(
        rendered.contains("third line"),
        "the TextBox must render the current (last) composer line 'third line' even with a stale document offset: {rendered}"
    );
}

/// An active NewComment composer must render the caret as additional
/// background SGR sequences (reverse-video cell) via the embedded TextBox,
/// even when the document scroll offset is stale (0 / top). This proves the
/// caret visibility is independent of the document scroll.
///
/// @plan PLAN-20260624-PR-MODE.P14
/// @requirement REQ-PR-009
/// @pseudocode component-001 lines 169-176
#[test]
fn active_new_comment_composer_renders_caret_with_stale_offset() {
    let detail = detail_with_long_comment();
    let cols: u16 = 80;
    let content_width = usize::from(crate::layout::prs_detail_content_width(cols));

    // Baseline: no composer, so no TextBox caret cell.
    let baseline = background_sgr_count(RenderParams {
        detail: &detail,
        subfocus: PrDetailSubfocus::NewComment,
        inline_state: &InlineState::None,
        scroll_offset: 0,
        detail_content_width: content_width,
        pane_height: 40,
        cols,
    });

    // Active composer with a stale (top) document scroll offset.
    let inline = InlineState::Composer {
        target: ComposerTarget::NewComment,
        text: "hello\nworld".to_string(),
        cursor: "hello\nworld".len(),
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
        "TextBox caret must add background SGR sequences ({with_caret}) beyond the \
         caret-free baseline ({baseline}) even with a stale document offset"
    );
}

/// `build_pr_detail_content` must return `cursor: None` for an active
/// NewComment composer — the composer text/cursor is rendered by the TextBox,
/// not flattened into the read-only document. (Render-path assertion of the
/// pure-content contract.)
///
/// @plan PLAN-20260624-PR-MODE.P14
/// @requirement REQ-PR-009
/// @pseudocode component-001 lines 169-176
#[test]
fn build_pr_detail_content_cursor_none_for_new_comment_composer() {
    let detail = detail_with_long_comment();
    let inline = InlineState::Composer {
        target: ComposerTarget::NewComment,
        text: "draft".to_string(),
        cursor: 5,
    };
    let content = crate::pr_detail_content::build_pr_detail_content(
        &detail,
        PrDetailSubfocus::NewComment,
        &inline,
        false,
        false,
    );
    assert!(
        content.cursor.is_none(),
        "build_pr_detail_content must return cursor None for an active NewComment composer"
    );
    // The composer text must NOT appear in the document.
    assert!(
        !content.text.contains("draft"),
        "NewComment composer text must NOT be flattened into the read-only document"
    );
}

/// An active Reply composer must render its draft text through the same
/// embedded TextBox as NewComment, even though the read-only document only
/// emits a stable reply anchor/help line.
///
/// @plan PLAN-20260624-PR-MODE.P14
/// @requirement REQ-PR-009
/// @requirement REQ-PR-010
/// @pseudocode component-001 lines 169-176
#[test]
fn active_reply_composer_renders_text_box_text() {
    let detail = detail_with_long_comment();
    let cols: u16 = 80;
    let content_width = usize::from(crate::layout::prs_detail_content_width(cols));
    let inline = InlineState::Composer {
        target: ComposerTarget::Reply {
            comment_index: 0,
            author: "@alice ".to_string(),
        },
        text: "@alice first\nreply current".to_string(),
        cursor: "@alice first\nreply current".len(),
    };
    let rendered = render_detail(RenderParams {
        detail: &detail,
        subfocus: PrDetailSubfocus::Comment(0),
        inline_state: &inline,
        scroll_offset: 0,
        detail_content_width: content_width,
        pane_height: 40,
        cols,
    });
    assert!(
        rendered.contains("reply current"),
        "Reply TextBox must render the current reply line: {rendered}"
    );
    assert!(
        !crate::pr_detail_content::build_pr_detail_content(
            &detail,
            PrDetailSubfocus::Comment(0),
            &inline,
            false,
            false,
        )
        .text
        .contains("reply current"),
        "Reply text must not be flattened into the read-only document"
    );
}

/// Reply composer caret rendering must add a reverse-video caret cell through
/// TextBox just like NewComment.
///
/// @plan PLAN-20260624-PR-MODE.P14
/// @requirement REQ-PR-009
/// @pseudocode component-001 lines 169-176
#[test]
fn active_reply_composer_renders_caret() {
    let detail = detail_with_long_comment();
    let cols: u16 = 80;
    let content_width = usize::from(crate::layout::prs_detail_content_width(cols));
    let baseline = background_sgr_count(RenderParams {
        detail: &detail,
        subfocus: PrDetailSubfocus::Comment(0),
        inline_state: &InlineState::None,
        scroll_offset: 0,
        detail_content_width: content_width,
        pane_height: 40,
        cols,
    });
    let inline = InlineState::Composer {
        target: ComposerTarget::Reply {
            comment_index: 0,
            author: "@alice ".to_string(),
        },
        text: "@alice reply".to_string(),
        cursor: "@alice reply".len(),
    };
    let with_caret = background_sgr_count(RenderParams {
        detail: &detail,
        subfocus: PrDetailSubfocus::Comment(0),
        inline_state: &inline,
        scroll_offset: 0,
        detail_content_width: content_width,
        pane_height: 40,
        cols,
    });
    assert!(
        with_caret > baseline,
        "Reply TextBox caret must add background SGR sequences ({with_caret}) beyond baseline ({baseline})"
    );
}

/// Tiny panes preserve one read-only document row and give the remaining rows
/// to the embedded TextBox; the TextBox must use that derived height, not the
/// fixed normal composer height, or render/state geometry diverges.
///
/// @plan PLAN-20260624-PR-MODE.P14
/// @requirement REQ-PR-009
/// @pseudocode component-001 lines 169-176
#[test]
fn tiny_pane_composer_uses_reserved_rows_not_fixed_height() {
    let detail = detail_with_long_comment();
    let inline = InlineState::Composer {
        target: ComposerTarget::NewComment,
        text: "hidden-alpha\nvisible-beta\nvisible-gamma\nvisible-delta\nvisible-epsilon"
            .to_string(),
        cursor: "hidden-alpha\nvisible-beta\nvisible-gamma\nvisible-delta\nvisible-epsilon".len(),
    };
    let rendered = render_detail(RenderParams {
        detail: &detail,
        subfocus: PrDetailSubfocus::NewComment,
        inline_state: &inline,
        scroll_offset: 0,
        detail_content_width: 70,
        pane_height: 12,
        cols: 100,
    });
    assert!(
        !rendered.contains("hidden-alpha"),
        "with one read-only row preserved, the four-row TextBox window should hide the first draft line"
    );
    assert!(
        rendered.contains("visible-epsilon"),
        "TextBox should still show the current caret line in the tiny-pane window"
    );
}

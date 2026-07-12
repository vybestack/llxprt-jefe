//! Render-path tests for IssueDetailView composer ownership.
//!
//! @requirement REQ-ISS-009

use crate::domain::{IssueComment, IssueDetail, IssueState};
use crate::state::{ComposerTarget, DetailSubfocus, InlineState};
use crate::theme::ThemeColors;
use crate::ui::components::{IssueDetailProjectionInputs, detail_pane_element, issue_detail_props};

use iocraft::prelude::*;

fn issue_detail_with_comment() -> IssueDetail {
    IssueDetail {
        repo_owner_name: "owner/repo".to_string(),
        number: 94,
        title: "Migrate issue composers".to_string(),
        state: IssueState::Open,
        author_login: "octocat".to_string(),
        created_at: "2026-07-01T00:00:00Z".to_string(),
        updated_at: "2026-07-01T01:00:00Z".to_string(),
        labels: vec![],
        assignees: vec![],
        milestone: None,
        body: "Body line".to_string(),
        external_url: "https://github.com/owner/repo/issues/94".to_string(),
        comments: vec![IssueComment {
            comment_id: 1,
            author_login: "alice".to_string(),
            created_at: "2026-07-01T02:00:00Z".to_string(),
            edited_at: None,
            body: "A comment to reply to".to_string(),
        }],
        has_more_comments: false,
        comments_cursor: None,
    }
}

struct RenderParams<'a> {
    detail: &'a IssueDetail,
    subfocus: DetailSubfocus,
    inline_state: &'a InlineState,
    scroll_offset: usize,
    pane_height: u16,
    cols: u16,
}

fn render_detail_canvas(p: RenderParams) -> iocraft::Canvas {
    let rows: u16 = p.pane_height + 10;
    let mut elem = element! {
        Box(width: u32::from(p.cols), height: u32::from(rows)) {
            #(vec![detail_pane_element(issue_detail_props(
                IssueDetailProjectionInputs {
                    issue_detail: Some(p.detail),
                    detail_subfocus: p.subfocus,
                    inline_state: p.inline_state,
                    comments_loading: false,
                    focused: true,
                    scroll_offset: p.scroll_offset,
                    colors: ThemeColors::default(),
                    available_height: Some(p.pane_height),
                    available_width: Some(p.cols),
                    selection: None,
                },
            ))])
        }
    };
    elem.render(Some(usize::from(p.cols)))
}

fn render_detail(p: RenderParams) -> String {
    render_detail_canvas(p).to_string()
}

fn background_sgr_count(p: RenderParams) -> usize {
    let canvas = render_detail_canvas(p);
    let mut buf = Vec::new();
    canvas
        .write_ansi(&mut buf)
        .unwrap_or_else(|e| panic!("write_ansi failed: {e}"));
    let ansi = String::from_utf8_lossy(&buf);
    ansi.matches("\u{1b}[48;2;").count() + ansi.matches("\u{1b}[48;5;").count()
}

/// Active Issues NewComment draft text must be rendered by the embedded TextBox,
/// not flattened into the read-only detail document.
#[test]
fn build_issue_detail_content_cursor_none_for_new_comment_composer() {
    let detail = issue_detail_with_comment();
    let inline = InlineState::Composer {
        target: ComposerTarget::NewComment,
        text: "draft issue comment".to_string(),
        cursor: "draft issue comment".len(),
    };

    let content = crate::issue_detail_content::build_detail_content(
        &detail,
        DetailSubfocus::NewComment,
        &inline,
        false,
    );

    assert!(content.cursor.is_none());
    assert!(!content.text.contains("draft issue comment"));
}

/// Active Issues NewComment composer text and caret must remain visible even
/// when the parent document scroll offset is stale.
#[test]
fn active_issue_new_comment_renders_text_box_text_and_caret_with_stale_offset() {
    let detail = issue_detail_with_comment();
    let inline = InlineState::Composer {
        target: ComposerTarget::NewComment,
        text: "first line\nsecond line\ncurrent line".to_string(),
        cursor: "first line\nsecond line\ncurrent line".len(),
    };

    let rendered = render_detail(RenderParams {
        detail: &detail,
        subfocus: DetailSubfocus::NewComment,
        inline_state: &inline,
        scroll_offset: 999,
        pane_height: 28,
        cols: 80,
    });
    assert!(
        rendered.contains("current line"),
        "rendered output: {rendered}"
    );

    let baseline = background_sgr_count(RenderParams {
        detail: &detail,
        subfocus: DetailSubfocus::NewComment,
        inline_state: &InlineState::None,
        scroll_offset: 0,
        pane_height: 28,
        cols: 80,
    });
    let with_caret = background_sgr_count(RenderParams {
        detail: &detail,
        subfocus: DetailSubfocus::NewComment,
        inline_state: &inline,
        scroll_offset: 0,
        pane_height: 28,
        cols: 80,
    });
    assert!(with_caret > baseline);
}

/// For short issue details, the embedded composer should appear immediately after
/// the read-only document content instead of being pushed to the pane bottom.
#[test]
fn active_issue_new_comment_on_short_detail_starts_after_comments() {
    let detail = issue_detail_with_comment();
    let inline = InlineState::Composer {
        target: ComposerTarget::NewComment,
        text: "short draft".to_string(),
        cursor: "short draft".len(),
    };

    let rendered = render_detail(RenderParams {
        detail: &detail,
        subfocus: DetailSubfocus::NewComment,
        inline_state: &inline,
        scroll_offset: 0,
        pane_height: 36,
        cols: 80,
    });
    let lines: Vec<&str> = rendered.lines().collect();
    let comment_line = lines
        .iter()
        .position(|line| line.contains("A comment to reply to"))
        .unwrap_or_else(|| panic!("missing final comment line in: {rendered}"));
    let help_line = lines
        .iter()
        .position(|line| line.contains("Ctrl+Enter submit | Esc cancel"))
        .unwrap_or_else(|| panic!("missing composer help line in: {rendered}"));
    let draft_line = lines
        .iter()
        .position(|line| line.contains("short draft"))
        .unwrap_or_else(|| panic!("missing TextBox draft line in: {rendered}"));

    assert!(
        draft_line <= comment_line + 5,
        "composer should stay near the final comment instead of the pane bottom: {rendered}"
    );
    assert_eq!(
        draft_line,
        help_line + 1,
        "composer should follow the new-comment help line without blank gap: {rendered}"
    );
}

/// Long single-line Issues composer drafts must keep the caret-owned tail visible
/// through TextBox even when the parent detail scroll offset is stale.
#[test]
fn active_issue_new_comment_long_line_keeps_tail_visible_with_stale_offset() {
    let detail = issue_detail_with_comment();
    let text = "abcdefghijklmnopqrstuvwxyz0123456789-tail".to_string();
    let inline = InlineState::Composer {
        target: ComposerTarget::NewComment,
        cursor: text.len(),
        text,
    };

    let rendered = render_detail(RenderParams {
        detail: &detail,
        subfocus: DetailSubfocus::NewComment,
        inline_state: &inline,
        scroll_offset: 999,
        pane_height: 28,
        cols: 32,
    });

    assert!(rendered.contains("6789-tai"), "rendered output: {rendered}");
}

#[test]
fn build_issue_detail_content_cursor_none_for_reply_composer() {
    let detail = issue_detail_with_comment();
    let inline = InlineState::Composer {
        target: ComposerTarget::Reply {
            comment_index: 0,
            author: "@alice ".to_string(),
        },
        text: "@alice draft reply".to_string(),
        cursor: "@alice draft reply".len(),
    };

    let content = crate::issue_detail_content::build_detail_content(
        &detail,
        DetailSubfocus::Comment(0),
        &inline,
        false,
    );

    assert!(content.cursor.is_none());
    assert!(!content.text.contains("draft reply"));
}

/// Active Issues Reply composer text and caret must be rendered by TextBox.
#[test]
fn active_issue_reply_renders_text_box_text_and_caret() {
    let detail = issue_detail_with_comment();
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
        subfocus: DetailSubfocus::Comment(0),
        inline_state: &inline,
        scroll_offset: 999,
        pane_height: 28,
        cols: 80,
    });
    assert!(
        rendered.contains("reply current"),
        "rendered output: {rendered}"
    );

    let baseline = background_sgr_count(RenderParams {
        detail: &detail,
        subfocus: DetailSubfocus::Comment(0),
        inline_state: &InlineState::None,
        scroll_offset: 0,
        pane_height: 28,
        cols: 80,
    });
    let with_caret = background_sgr_count(RenderParams {
        detail: &detail,
        subfocus: DetailSubfocus::Comment(0),
        inline_state: &inline,
        scroll_offset: 0,
        pane_height: 28,
        cols: 80,
    });
    assert!(with_caret > baseline);
}

/// Regression for issue #212: the new-issue composer must WRAP long text via
/// the embedded TextBox (no ellipsis truncation, no off-screen overflow).
#[test]
fn new_issue_composer_wraps_long_text_via_text_box() {
    let detail = issue_detail_with_comment();
    let long_text = "alpha beta gamma delta epsilon zeta eta theta iota kappa lambda".to_string();
    let inline = InlineState::Composer {
        target: ComposerTarget::NewIssue,
        text: long_text.clone(),
        cursor: long_text.len(),
    };
    let rendered = render_detail(RenderParams {
        detail: &detail,
        subfocus: DetailSubfocus::Body,
        inline_state: &inline,
        scroll_offset: 0,
        pane_height: 24,
        cols: 40,
    });
    assert!(
        !rendered.contains('\u{2026}'),
        "new-issue editor must wrap, not truncate with ellipsis: {rendered}"
    );
    assert!(
        rendered.contains("alpha"),
        "the start of the new-issue editor text must be visible: {rendered}"
    );
    for (i, line) in rendered.lines().enumerate() {
        assert!(
            line.chars().count() <= 60,
            "rendered line {i} overflows: {} ({} chars)",
            line,
            line.chars().count()
        );
    }
}

/// Regression for issue #212: the new-issue editor caret renders as a
/// reverse-video cell via the embedded TextBox.
#[test]
fn new_issue_composer_renders_caret_via_text_box() {
    let detail = issue_detail_with_comment();
    let baseline = background_sgr_count(RenderParams {
        detail: &detail,
        subfocus: DetailSubfocus::Body,
        inline_state: &InlineState::None,
        scroll_offset: 0,
        pane_height: 24,
        cols: 80,
    });
    let inline = InlineState::Composer {
        target: ComposerTarget::NewIssue,
        text: "hello".to_string(),
        cursor: 5,
    };
    let with_caret = background_sgr_count(RenderParams {
        detail: &detail,
        subfocus: DetailSubfocus::Body,
        inline_state: &inline,
        scroll_offset: 0,
        pane_height: 24,
        cols: 80,
    });
    assert!(
        with_caret > baseline,
        "new-issue TextBox caret must add background SGR ({with_caret}) beyond baseline ({baseline})"
    );
}

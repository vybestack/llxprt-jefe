//! Behavioral layout contracts for parent-sized text boxes (issue #408).

use jefe::state::{ComposerTarget, DetailSubfocus, InlineState};
use jefe::text_box_view::build_text_box_view;
use jefe::theme::ThemeColors;
use jefe::ui::components::{IssueDetailProjectionInputs, issue_detail_props};

fn new_issue_props(pane_height: u16) -> jefe::ui::components::DetailPaneProps {
    new_issue_props_at_width(pane_height, 80)
}

fn new_issue_props_at_width(
    pane_height: u16,
    available_width: u16,
) -> jefe::ui::components::DetailPaneProps {
    let inline = InlineState::Composer {
        target: ComposerTarget::NewIssue,
        text: String::new(),
        cursor: 0,
    };
    issue_detail_props(IssueDetailProjectionInputs {
        issue_detail: None,
        detail_subfocus: DetailSubfocus::Body,
        inline_state: &inline,
        new_issue_form: None,
        comments_loading: false,
        focused: true,
        scroll_offset: 0,
        colors: ThemeColors::default(),
        available_height: Some(pane_height),
        available_width: Some(available_width),
        selection: None,
    })
}

#[test]
fn new_issue_composer_uses_all_rows_after_static_guidance() {
    let pane_height = 28;
    let props = new_issue_props(pane_height);
    let body_rows = jefe::layout::detail_body_viewport_rows(usize::from(pane_height));
    let guidance_rows = jefe::issue_detail_content::build_new_issue_content(&InlineState::None)
        .text
        .lines()
        .count()
        .max(1);

    assert_eq!(
        props.viewport_rows, guidance_rows,
        "all guidance rows stay visible"
    );
    assert_eq!(
        props.composer_rows,
        body_rows.saturating_sub(guidance_rows),
        "composer fills the remaining body rows"
    );
    assert_eq!(props.viewport_rows + props.composer_rows, body_rows);
}

#[test]
fn constrained_new_issue_composer_keeps_one_editable_row() {
    let props = new_issue_props(8);
    assert_eq!(props.viewport_rows, 0);
    assert_eq!(props.composer_rows, 1);
}

#[test]
fn narrow_new_issue_reserves_wrapped_guidance_rows() {
    let pane_height = 28;
    let props = new_issue_props_at_width(pane_height, 20);
    let body_rows = jefe::layout::detail_body_viewport_rows(usize::from(pane_height));
    let logical_guidance_rows =
        jefe::issue_detail_content::build_new_issue_content(&InlineState::None)
            .text
            .lines()
            .count()
            .max(1);

    assert!(
        props.viewport_rows > logical_guidance_rows,
        "wrapped guidance should occupy more display rows than logical lines"
    );
    assert_eq!(props.viewport_rows + props.composer_rows, body_rows);
    assert!(props.composer_rows > 0, "composer must remain editable");
}

#[test]
fn scroll_direction_queries_follow_wrapped_display_rows() {
    let text = "one\ntwo\nthree\nfour\nfive\nsix\nseven\neight";

    let at_top = build_text_box_view(text, 0, 3, 20);
    assert!(!at_top.can_scroll_up());
    assert!(at_top.can_scroll_down());

    let middle_cursor = text
        .find("four")
        .unwrap_or_else(|| panic!("fixture line must exist"));
    let in_middle = build_text_box_view(text, middle_cursor, 3, 20);
    assert!(in_middle.can_scroll_up());
    assert!(in_middle.can_scroll_down());

    let at_bottom = build_text_box_view(text, text.len(), 3, 20);
    assert!(at_bottom.can_scroll_up());
    assert!(!at_bottom.can_scroll_down());
    assert_eq!(at_bottom.total_display_rows, 8);

    let all_visible = build_text_box_view("one\ntwo", 0, 3, 20);
    assert!(!all_visible.can_scroll_up());
    assert!(!all_visible.can_scroll_down());

    let wrapped = build_text_box_view("one two three four", 0, 2, 5);
    assert!(wrapped.total_display_rows > wrapped.rows.len());
    assert!(!wrapped.can_scroll_up());
    assert!(wrapped.can_scroll_down());

    let no_viewport = build_text_box_view(text, 0, 0, 20);
    assert!(!no_viewport.can_scroll_up());
    assert!(!no_viewport.can_scroll_down());
}

#[test]
fn scroll_direction_queries_handle_multibyte_wrapped_text() {
    let text = "éééééé";

    let at_top = build_text_box_view(text, 0, 1, 3);
    assert_eq!(at_top.total_display_rows, 2);
    assert!(!at_top.can_scroll_up());
    assert!(at_top.can_scroll_down());

    let at_bottom = build_text_box_view(text, text.len(), 1, 3);
    assert_eq!(at_bottom.total_display_rows, 2);
    assert!(at_bottom.can_scroll_up());
    assert!(!at_bottom.can_scroll_down());
}

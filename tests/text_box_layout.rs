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
    let props = new_issue_props(28);
    assert_eq!(
        props.viewport_rows, 4,
        "all four guidance rows stay visible"
    );
    assert_eq!(
        props.composer_rows, 17,
        "composer fills the remaining body rows"
    );
    assert_eq!(props.viewport_rows + props.composer_rows, 21);
}

#[test]
fn constrained_new_issue_composer_keeps_one_editable_row() {
    let props = new_issue_props(8);
    assert_eq!(props.viewport_rows, 0);
    assert_eq!(props.composer_rows, 1);
}

#[test]
fn narrow_new_issue_reserves_wrapped_guidance_rows() {
    let props = new_issue_props_at_width(28, 20);
    assert!(
        props.viewport_rows > 4,
        "wrapped guidance should occupy more than four display rows"
    );
    assert_eq!(props.viewport_rows + props.composer_rows, 21);
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

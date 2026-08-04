//! Behavioral tests for the layout tree editor (issue #388).
//!
//! @requirement CW08-03
//! @requirement CW08-04

use crate::domain::Id;
use crate::workbench::descriptor::{Axis, LayoutNode, ScreenDescriptor};
use crate::workbench::screens::builtin_screens;

use super::{LayoutEditorState, NodeDialog, NodeDialogKind, SizeKind};

/// A compiled screen with at least two panels, and its own layout.
fn screen() -> ScreenDescriptor {
    builtin_screens()
        .unwrap_or_else(|error| panic!("compiled screen table: {error}"))
        .screens()
        .iter()
        .find(|screen| screen.panels.len() >= 2)
        .unwrap_or_else(|| panic!("a screen with two panels"))
        .clone()
}

fn editor(screen: &ScreenDescriptor) -> LayoutEditorState {
    let id =
        Id::parse(screen.id.as_str()).unwrap_or_else(|error| panic!("screen id fixture: {error}"));
    LayoutEditorState::open(id, screen.layout.clone())
}

// ── CW08-03: only a complete valid tree leaves the editor ─────────────────

#[test]
fn an_untouched_tree_completes_as_the_screen_already_declares_it() {
    let screen = screen();
    let editor = editor(&screen);

    let completed = editor
        .complete(&screen)
        .unwrap_or_else(|reason| panic!("the screen's own layout validates: {reason}"));

    assert_eq!(completed, screen.layout);
}

#[test]
fn a_tree_the_validator_refuses_never_completes() {
    let screen = screen();
    let mut editor = editor(&screen);
    editor.tree = LayoutNode::Leaf {
        panel: screen.panels[0].id,
    };

    let Err(reason) = editor.complete(&screen) else {
        panic!("a tree leaving panels unplaced cannot complete");
    };
    assert!(!reason.is_empty(), "the validator says why");
}

// ── CW08-04: an invalid intermediate stays in the node dialog ─────────────

#[test]
fn a_size_that_is_not_a_number_keeps_the_dialog_open_with_its_reason() {
    let screen = screen();
    let mut editor = editor(&screen);
    editor.selected = vec![0];
    let mut dialog = NodeDialog::adding();
    dialog.size = "wide".to_owned();
    let before = editor.tree.clone();
    editor.dialog = Some(dialog);

    editor.apply_dialog(&screen);

    assert_eq!(editor.tree, before, "the tree is untouched");
    let Some(dialog) = editor.dialog.as_ref() else {
        panic!("a refused dialog stays open");
    };
    assert!(
        dialog
            .error
            .as_deref()
            .is_some_and(|error| error.contains("size")),
        "the reason names the field: {:?}",
        dialog.error
    );
}

#[test]
fn a_size_of_zero_is_refused_rather_than_corrected() {
    let screen = screen();
    let mut editor = editor(&screen);
    editor.selected = vec![0];
    let mut dialog = NodeDialog::adding();
    dialog.size_kind = SizeKind::Fixed;
    dialog.size = "0".to_owned();
    editor.dialog = Some(dialog);

    editor.apply_dialog(&screen);

    assert!(
        editor
            .dialog
            .as_ref()
            .and_then(|dialog| dialog.error.as_deref())
            .is_some_and(|error| error.contains("greater than zero")),
        "a fixed size of zero is unrepresentable, not a small size"
    );
}

#[test]
fn a_maximum_below_the_minimum_is_refused() {
    let screen = screen();
    let mut editor = editor(&screen);
    editor.selected = vec![0];
    let mut dialog = NodeDialog::adding();
    dialog.min = "20".to_owned();
    dialog.max = "10".to_owned();
    editor.dialog = Some(dialog);

    editor.apply_dialog(&screen);

    assert!(
        editor
            .dialog
            .as_ref()
            .and_then(|dialog| dialog.error.as_deref())
            .is_some_and(|error| error.contains("max")),
    );
}

#[test]
fn a_collapsible_child_without_a_collapse_order_is_refused() {
    let screen = screen();
    let mut editor = editor(&screen);
    editor.selected = vec![0];
    let mut dialog = NodeDialog::adding();
    dialog.collapsible = true;
    dialog.collapse_priority = String::new();
    editor.dialog = Some(dialog);

    editor.apply_dialog(&screen);

    assert!(
        editor
            .dialog
            .as_ref()
            .and_then(|dialog| dialog.error.as_deref())
            .is_some_and(|error| error.contains("collapse")),
    );
}

#[test]
fn an_add_the_validator_would_refuse_keeps_the_dialog_open() {
    let screen = screen();
    let mut editor = editor(&screen);
    // Every declared panel is already placed, so there is nothing to add and
    // the dialog says so rather than adding a duplicate.
    editor.selected = vec![0];
    editor.dialog = Some(NodeDialog::adding());
    let before = editor.tree.clone();

    editor.apply_dialog(&screen);

    assert_eq!(editor.tree, before);
    assert!(editor.dialog.is_some(), "the dialog stays open");
}

#[test]
fn editing_a_childs_allocation_applies_when_every_field_parses() {
    let screen = screen();
    let mut editor = editor(&screen);
    editor.selected = vec![0];
    let LayoutNode::Split { children, .. } = &editor.tree else {
        panic!("this screen's layout is a split");
    };
    let mut dialog = NodeDialog::editing(&children[0]);
    dialog.size_kind = SizeKind::Weight;
    dialog.size = "3".to_owned();
    dialog.min = "5".to_owned();
    dialog.max.clear();
    editor.dialog = Some(dialog);

    editor.apply_dialog(&screen);

    assert!(
        editor.dialog.is_none(),
        "an applied dialog closes: {:?}",
        editor
            .dialog
            .as_ref()
            .and_then(|dialog| dialog.error.clone())
    );
    let LayoutNode::Split { children, .. } = &editor.tree else {
        panic!("the tree is still a split");
    };
    assert_eq!(children[0].min, 5);
}

// ── tree navigation ───────────────────────────────────────────────────────

#[test]
fn navigation_moves_between_parents_children_and_siblings() {
    let screen = screen();
    let mut editor = editor(&screen);

    editor.select_child();
    assert_eq!(editor.selected, vec![0]);

    editor.select_next();
    assert_eq!(editor.selected, vec![1]);

    editor.select_previous();
    assert_eq!(editor.selected, vec![0]);

    editor.select_parent();
    assert!(editor.selected.is_empty());
}

#[test]
fn stepping_past_the_last_sibling_stays_on_it() {
    let screen = screen();
    let mut editor = editor(&screen);
    let LayoutNode::Split { children, .. } = &editor.tree else {
        panic!("this screen's layout is a split");
    };
    let last = children.len() - 1;
    editor.selected = vec![last];

    editor.select_next();

    assert_eq!(editor.selected, vec![last]);
}

#[test]
fn the_add_chooser_offers_only_panels_the_tree_does_not_place() {
    let screen = screen();
    let mut editor = editor(&screen);

    assert!(
        editor.addable_panels(&screen).is_empty(),
        "a complete layout places every declared panel"
    );

    editor.tree = LayoutNode::Leaf {
        panel: screen.panels[0].id,
    };
    let addable = editor.addable_panels(&screen);
    assert_eq!(addable.len(), screen.panels.len() - 1);
    assert!(!addable.contains(&screen.panels[0].id));
}

// ── removal only when the invariants survive ──────────────────────────────

#[test]
fn removing_a_child_that_would_leave_a_panel_unplaced_is_refused() {
    let screen = screen();
    let mut editor = editor(&screen);
    editor.selected = vec![0];
    let before = editor.tree.clone();

    editor.remove_selected(&screen);

    assert_eq!(editor.tree, before, "the tree is untouched");
    assert!(
        editor.notice.is_some(),
        "the validator's refusal is reported"
    );
}

#[test]
fn the_root_node_cannot_be_removed() {
    let screen = screen();
    let mut editor = editor(&screen);
    let before = editor.tree.clone();

    editor.remove_selected(&screen);

    assert_eq!(editor.tree, before);
    assert!(
        editor
            .notice
            .as_deref()
            .is_some_and(|notice| notice.contains("root"))
    );
}

// ── splitting a node ──────────────────────────────────────────────────────

#[test]
fn splitting_a_node_says_a_second_child_is_still_needed() {
    let screen = screen();
    let mut editor = editor(&screen);
    editor.selected = vec![0];

    editor.split_selected(Axis::Vertical);

    assert!(matches!(
        editor.selected_node(),
        Some(LayoutNode::Split {
            axis: Axis::Vertical,
            ..
        })
    ));
    assert!(
        editor.notice.is_some(),
        "a one-child split is an unfinished edit and says so"
    );
    assert!(
        editor.complete(&screen).is_err(),
        "an unfinished split cannot reach the draft"
    );
}

#[test]
fn a_dialog_reports_which_kind_of_edit_it_is_collecting() {
    let screen = screen();
    let LayoutNode::Split { children, .. } = &screen.layout else {
        panic!("this screen's layout is a split");
    };

    assert_eq!(NodeDialog::adding().kind, NodeDialogKind::AddLeaf);
    assert_eq!(
        NodeDialog::editing(&children[0]).kind,
        NodeDialogKind::EditChild
    );
}

#[test]
fn typing_and_deleting_move_only_the_focused_field() {
    let mut dialog = NodeDialog::adding();
    dialog.field = 1;
    dialog.size.clear();

    dialog.push('4');
    dialog.push('2');
    assert_eq!(dialog.size, "42");

    dialog.backspace();
    assert_eq!(dialog.size, "4");
    assert_eq!(dialog.min, "1", "another field is untouched");
}

#[test]
fn tab_cycles_the_dialog_fields_and_wraps() {
    let mut dialog = NodeDialog::adding();

    for expected in 1..super::DIALOG_FIELDS {
        dialog.next_field();
        assert_eq!(dialog.field, expected);
    }
    dialog.next_field();
    assert_eq!(dialog.field, 0);
}

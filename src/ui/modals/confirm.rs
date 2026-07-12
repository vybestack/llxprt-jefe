//! Confirm modal - confirmation dialogs for destructive actions.
//!
//! @plan PLAN-20260216-FIRSTVERSION-V1.P09
//! @requirement REQ-FUNC-004

use iocraft::prelude::*;

use crate::selection::{SelectablePane, TextSelection};
use crate::state::ConfirmFocus;
use crate::theme::{ResolvedColors, SelectionColors, ThemeColors};
use crate::ui::components::selectable_line;

/// Format the confirm-dialog button row with the focused button visually
/// distinct (issue #228). The focused button uses `( … )` and the unfocused
/// button uses `[ … ]`.
#[must_use]
pub fn confirm_button_row(focus: ConfirmFocus) -> String {
    let cancel = if matches!(focus, ConfirmFocus::Cancel) {
        "( Cancel )"
    } else {
        "[ Cancel ]"
    };
    let confirm = if matches!(focus, ConfirmFocus::Confirm) {
        "( Confirm )"
    } else {
        "[ Confirm ]"
    };
    format!("{cancel}  {confirm}")
}

/// Props for the confirm modal.
#[derive(Default, Props)]
pub struct ConfirmModalProps {
    /// Title of the confirmation dialog.
    pub title: String,
    /// Message to display.
    pub message: String,
    /// Whether to show delete work dir option.
    pub show_delete_work_dir: bool,
    /// Current state of delete work dir toggle.
    pub delete_work_dir: bool,
    /// Which button has keyboard focus (issue #228).
    pub confirm_focus: ConfirmFocus,
    /// Theme colors.
    pub colors: ThemeColors,
    /// Active text selection for drag-highlight (issue #178).
    pub selection: Option<TextSelection>,
}

/// Confirm modal for destructive actions (delete, kill).
#[component]
pub fn ConfirmModal(props: &ConfirmModalProps) -> impl Into<AnyElement<'static>> {
    let rc = ResolvedColors::from_theme(Some(&props.colors));
    let sel = SelectionColors::from_resolved(&rc);
    let pane = SelectablePane::ConfirmModal;
    let selection = props.selection;

    let checkbox_line = if props.show_delete_work_dir {
        let mark = if props.delete_work_dir { "x" } else { " " };
        format!("[{mark}] Delete work directory")
    } else {
        String::new()
    };

    // Focus-aware button row (issue #228): the focused button uses (…) and
    // the unfocused button uses […].
    let button_line = confirm_button_row(props.confirm_focus);

    let lines: Vec<AnyElement<'static>> = vec![
        selectable_line(&props.title, 0, selection, pane, rc.fg, sel),
        selectable_line("", 1, selection, pane, rc.fg, sel),
        selectable_line(&props.message, 2, selection, pane, rc.fg, sel),
        selectable_line(&checkbox_line, 3, selection, pane, rc.fg, sel),
        selectable_line(&button_line, 4, selection, pane, rc.fg, sel),
        selectable_line("", 5, selection, pane, rc.fg, sel),
    ];

    element! {
        Box(
            flex_direction: FlexDirection::Column,
            width: 50u32,
            height: 10u32,
            border_style: BorderStyle::Round,
            border_color: rc.border_focused,
            background_color: rc.bg,
            padding: 1u32,
        ) {
            #(lines)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn confirm_button_row_cancel_focused() {
        assert_eq!(
            confirm_button_row(ConfirmFocus::Cancel),
            "( Cancel )  [ Confirm ]"
        );
    }

    #[test]
    fn confirm_button_row_confirm_focused() {
        assert_eq!(
            confirm_button_row(ConfirmFocus::Confirm),
            "[ Cancel ]  ( Confirm )"
        );
    }
}

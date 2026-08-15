//! S7 RED-first tests for approved mouse action targets.

use super::*;
use jefe::domain::RepositoryId;
use jefe::domain::action_registry::{ActionId, HandlerKey};
use jefe::state::{AppState, ConfirmFocus, ModalState};

fn action_id(value: &str) -> ActionId {
    ActionId::parse(value).unwrap_or_else(|error| panic!("test ActionId {value}: {error}"))
}

fn state_with_confirm_modal() -> AppState {
    let mut state = crate::test_app_state();
    state.modal = ModalState::ConfirmDeleteRepository {
        id: RepositoryId("repo".to_owned()),
        confirm_focus: ConfirmFocus::Confirm,
    };
    state
}

fn click(down: Option<(u16, u16)>, up: (u16, u16), terminal: (u16, u16)) -> MouseClickInput {
    MouseClickInput { down, up, terminal }
}

#[test]
fn confirm_targets_use_existing_pane_geometry_and_emit_action_ids() {
    let state = state_with_confirm_modal();

    let cancel = resolve_action_click(&state, click(Some((2, 6)), (2, 6), (120, 40)))
        .unwrap_or_else(|| panic!("Cancel button must be an action target"));
    assert_eq!(cancel.chord.to_string(), "Esc");
    assert!(matches!(
        cancel.resolution,
        Resolution::Dispatch { action, handler: HandlerKey::ConfirmCancel }
            if action == action_id("confirm.cancel")
    ));

    let confirm = resolve_action_click(&state, click(Some((14, 6)), (14, 6), (120, 40)))
        .unwrap_or_else(|| panic!("Confirm button must be an action target"));
    assert_eq!(confirm.chord.to_string(), "Enter");
    assert!(matches!(
        confirm.resolution,
        Resolution::Dispatch { action, handler: HandlerKey::ConfirmAccept }
            if action == action_id("confirm.accept")
    ));
}

#[test]
fn focus_markers_do_not_change_confirm_button_geometry() {
    for focus in [ConfirmFocus::Cancel, ConfirmFocus::Confirm] {
        let mut state = state_with_confirm_modal();
        if let ModalState::ConfirmDeleteRepository { confirm_focus, .. } = &mut state.modal {
            *confirm_focus = focus;
        }
        assert!(resolve_action_click(&state, click(Some((2, 6)), (2, 6), (120, 40))).is_some());
        assert!(resolve_action_click(&state, click(Some((14, 6)), (14, 6), (120, 40))).is_some());
    }
}

#[test]
fn drag_gap_and_non_approved_modal_are_no_ops() {
    let confirm = state_with_confirm_modal();
    assert!(resolve_action_click(&confirm, click(Some((2, 6)), (3, 6), (120, 40))).is_none());
    assert!(resolve_action_click(&confirm, click(Some((12, 6)), (12, 6), (120, 40))).is_none());
    assert!(resolve_action_click(&confirm, click(None, (2, 6), (120, 40))).is_none());

    let mut help = crate::test_app_state();
    help.modal = ModalState::Help;
    assert!(resolve_action_click(&help, click(Some((2, 6)), (2, 6), (120, 40))).is_none());
}

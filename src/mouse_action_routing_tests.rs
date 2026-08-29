//! S7 RED-first tests for approved mouse action targets.

use super::*;
use jefe::domain::RepositoryId;
use jefe::domain::action_registry::{ActionId, HandlerKey};
use jefe::state::{AppEvent, AppState, transition::TransitionExt};

fn action_id(value: &str) -> ActionId {
    ActionId::parse(value).unwrap_or_else(|error| panic!("test ActionId {value}: {error}"))
}

fn state_with_confirm_modal() -> AppState {
    crate::test_app_state()
        .apply(AppEvent::OpenDeleteRepository(RepositoryId(
            "repo".to_owned(),
        )))
        .committed_pure()
}

fn click(down: Option<(u16, u16)>, up: (u16, u16), terminal: (u16, u16)) -> MouseClickInput {
    MouseClickInput { down, up, terminal }
}

fn projected_action_point(state: &AppState, target: &str) -> ((u16, u16), MouseActionRoute) {
    for row in 0..40 {
        for col in 0..120 {
            let point = (col, row);
            if let Some(route) = resolve_action_click(state, click(Some(point), point, (120, 40)))
                && route.action.as_str() == target
            {
                return (point, route);
            }
        }
    }
    panic!("projected confirmation action {target} must have a hit target");
}

#[test]
fn confirm_targets_follow_projected_form_rows_and_emit_action_ids() {
    let state = state_with_confirm_modal();

    let (_, decision) = projected_action_point(&state, "confirm.cycle-focus");
    assert!(matches!(
        decision.resolution,
        Resolution::Dispatch {
            action,
            handler: HandlerKey::ConfirmCycleFocus
        } if action == action_id("confirm.cycle-focus")
    ));

    let (_, submit) = projected_action_point(&state, "confirm.accept");
    assert_eq!(submit.chord.to_string(), "Enter");
    assert!(matches!(
        submit.resolution,
        Resolution::Dispatch { action, handler: HandlerKey::ConfirmAccept }
            if action == action_id("confirm.accept")
    ));
}

#[test]
fn focus_value_does_not_change_projected_confirm_row_geometry() {
    let cancel = state_with_confirm_modal();
    let confirm = cancel
        .clone()
        .apply(AppEvent::ConfirmCycleFocus)
        .committed_pure();

    assert_eq!(
        projected_action_point(&cancel, "confirm.cycle-focus").0,
        projected_action_point(&confirm, "confirm.cycle-focus").0
    );
    assert_eq!(
        projected_action_point(&cancel, "confirm.accept").0,
        projected_action_point(&confirm, "confirm.accept").0
    );
}

#[test]
fn drag_gap_and_non_approved_modal_are_no_ops() {
    let confirm = state_with_confirm_modal();
    let (decision, _) = projected_action_point(&confirm, "confirm.cycle-focus");
    let drag_end = (decision.0.saturating_add(1), decision.1);
    assert!(resolve_action_click(&confirm, click(Some(decision), drag_end, (120, 40))).is_none());
    assert!(resolve_action_click(&confirm, click(Some((0, 0)), (0, 0), (120, 40))).is_none());
    assert!(resolve_action_click(&confirm, click(None, decision, (120, 40))).is_none());

    let help = crate::test_app_state()
        .apply(jefe::state::AppEvent::OpenHelp)
        .committed_pure();
    assert!(resolve_action_click(&help, click(Some(decision), decision, (120, 40))).is_none());
}

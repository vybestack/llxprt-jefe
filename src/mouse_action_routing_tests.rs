//! S7 RED-first tests for approved mouse action targets.

use super::*;
use jefe::domain::Id;
use jefe::domain::RepositoryId;
use jefe::domain::action_registry::{
    ActionAvailability, ActionId, Availability, AvailabilityGeneration, HandlerKey,
    RegistryCandidate,
};
use jefe::domain::effects::{Correlation, CorrelationId, EffectFamily, SemanticKey};
use jefe::domain::input_context::ContextStack;
use jefe::state::{AppState, ConfirmFocus, ModalState};

fn action_id(value: &str) -> ActionId {
    ActionId::parse(value).unwrap_or_else(|error| panic!("test ActionId {value}: {error}"))
}

fn availability_generation(entries: Vec<ActionAvailability>) -> AvailabilityGeneration {
    let owner = Id::parse("core.keymap").unwrap_or_else(|error| panic!("owner: {error}"));
    AvailabilityGeneration::new(
        Correlation {
            correlation_id: CorrelationId::new(1),
            owner,
            screen_generation: 0,
            activation_generation: 0,
            semantic_key: SemanticKey::new(EffectFamily::Provider, "action-availability"),
        },
        entries,
    )
}

fn compiled_snapshot() -> ActionRegistrySnapshot {
    let inventory = jefe::domain::default_action_inventory::compiled_inventory()
        .unwrap_or_else(|error| panic!("compiled inventory: {error}"));
    let availability = inventory
        .actions
        .iter()
        .map(|action| ActionAvailability::new(action.id.clone(), Availability::Available))
        .collect();
    let generation = availability_generation(availability);
    let stacks = inventory
        .bindings
        .iter()
        .filter_map(|binding| ContextStack::from_ordered([binding.context.as_str()], false).ok())
        .collect();
    RegistryCandidate::new(
        inventory.actions,
        inventory.bindings,
        Vec::new(),
        stacks,
        generation,
    )
    .compose()
    .unwrap_or_else(|error| panic!("compiled snapshot: {error}"))
}

fn state_with_confirm_modal(snapshot: &ActionRegistrySnapshot) -> AppState {
    AppState {
        modal: ModalState::ConfirmDeleteRepository {
            id: RepositoryId("repo".to_owned()),
            confirm_focus: ConfirmFocus::Confirm,
        },
        action_registry_snapshot: Some(snapshot.clone()),
        ..AppState::default()
    }
}

fn click(down: Option<(u16, u16)>, up: (u16, u16), terminal: (u16, u16)) -> MouseClickInput {
    MouseClickInput { down, up, terminal }
}

#[test]
fn confirm_targets_use_existing_pane_geometry_and_emit_action_ids() {
    let snapshot = compiled_snapshot();
    let state = state_with_confirm_modal(&snapshot);

    let cancel = resolve_action_click(&state, &snapshot, click(Some((2, 6)), (2, 6), (120, 40)))
        .unwrap_or_else(|| panic!("Cancel button must be an action target"));
    assert_eq!(cancel.chord.to_string(), "Esc");
    assert!(matches!(
        cancel.resolution,
        Resolution::Dispatch { action, handler: HandlerKey::ConfirmCancel }
            if action == action_id("confirm.cancel")
    ));

    let confirm = resolve_action_click(&state, &snapshot, click(Some((14, 6)), (14, 6), (120, 40)))
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
    let snapshot = compiled_snapshot();
    for focus in [ConfirmFocus::Cancel, ConfirmFocus::Confirm] {
        let mut state = state_with_confirm_modal(&snapshot);
        if let ModalState::ConfirmDeleteRepository { confirm_focus, .. } = &mut state.modal {
            *confirm_focus = focus;
        }
        assert!(
            resolve_action_click(&state, &snapshot, click(Some((2, 6)), (2, 6), (120, 40)))
                .is_some()
        );
        assert!(
            resolve_action_click(&state, &snapshot, click(Some((14, 6)), (14, 6), (120, 40)))
                .is_some()
        );
    }
}

#[test]
fn drag_gap_and_non_approved_modal_are_no_ops() {
    let snapshot = compiled_snapshot();
    let confirm = state_with_confirm_modal(&snapshot);
    assert!(
        resolve_action_click(&confirm, &snapshot, click(Some((2, 6)), (3, 6), (120, 40))).is_none()
    );
    assert!(
        resolve_action_click(
            &confirm,
            &snapshot,
            click(Some((12, 6)), (12, 6), (120, 40))
        )
        .is_none()
    );
    assert!(resolve_action_click(&confirm, &snapshot, click(None, (2, 6), (120, 40))).is_none());

    let help = AppState {
        modal: ModalState::Help,
        action_registry_snapshot: Some(snapshot.clone()),
        ..AppState::default()
    };
    assert!(
        resolve_action_click(&help, &snapshot, click(Some((2, 6)), (2, 6), (120, 40))).is_none()
    );
}

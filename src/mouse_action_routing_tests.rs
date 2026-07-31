//! S7 RED-first tests for approved mouse action targets.

use super::*;
use jefe::domain::Id;
use jefe::domain::RepositoryId;
use jefe::domain::action_registry::{
    Action, ActionAvailability, ActionId, ActionMetadata, Availability, AvailabilityGeneration,
    Binding, HandlerKey, Provenance, RegistryCandidate, Resolution,
};
use jefe::domain::effects::{Correlation, CorrelationId, EffectFamily, SemanticKey};
use jefe::domain::input_context::{ContextId, ContextStack};
use jefe::domain::keymap::Chord;
use jefe::state::{AppState, ConfirmFocus, KeysEditorState, ModalState};

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

fn state_with_keys(snapshot: &ActionRegistrySnapshot) -> AppState {
    AppState {
        modal: ModalState::Keys {
            editor: Box::new(KeysEditorState::from_snapshot(snapshot, None)),
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
fn keys_row_target_uses_projected_action_identity() {
    let snapshot = compiled_snapshot();
    let state = state_with_keys(&snapshot);

    let route = resolve_action_click(&state, &snapshot, click(Some((3, 5)), (3, 5), (120, 36)))
        .unwrap_or_else(|| panic!("second visible Keys row must be clickable"));
    assert!(matches!(
        route.resolution,
        Resolution::Dispatch { action, handler: HandlerKey::OpenKeys }
            if action == action_id("core.open-keys")
    ));
}

#[test]
fn status_lines_shift_keys_targets_with_the_rendered_projection() {
    let snapshot = compiled_snapshot();
    let mut state = state_with_keys(&snapshot);
    let ModalState::Keys { editor } = &mut state.modal else {
        panic!("fixture must open Keys");
    };
    editor.status = Some("status".to_owned());
    editor.recovery = Some("KEY-E401: recovery".to_owned());

    assert!(
        resolve_action_click(&state, &snapshot, click(Some((3, 4)), (3, 4), (120, 36))).is_none()
    );
    let route = resolve_action_click(&state, &snapshot, click(Some((3, 5)), (3, 5), (120, 36)))
        .unwrap_or_else(|| panic!("projected first row must shift below recovery status"));
    assert!(matches!(
        route.resolution,
        Resolution::Dispatch { action, .. } if action == action_id("core.emergency-exit")
    ));
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

#[test]
fn unavailable_target_returns_the_snapshot_reason_without_a_handler() {
    let snapshot = unavailable_snapshot("Shared unavailable reason");
    let state = state_with_keys(&snapshot);
    let route = resolve_action_click(&state, &snapshot, click(Some((3, 4)), (3, 4), (120, 36)))
        .unwrap_or_else(|| panic!("unavailable row remains an action target"));

    assert_eq!(
        route.resolution,
        Resolution::Unavailable {
            action: action_id("test.unavailable"),
            reason: "Shared unavailable reason".to_owned(),
        }
    );
}

fn unavailable_snapshot(reason: &str) -> ActionRegistrySnapshot {
    let context = ContextId::parse("global").unwrap_or_else(|error| panic!("context: {error}"));
    let id = action_id("test.unavailable");
    let action = Action::new(
        ActionMetadata {
            id: id.clone(),
            label: "Unavailable".to_owned(),
            description: "Unavailable test action.".to_owned(),
            category: "test".to_owned(),
            contexts: vec![context.clone()],
        },
        HandlerKey::OpenKeys,
        false,
    )
    .unwrap_or_else(|error| panic!("action: {error}"));
    let chord = Chord::parse("x").unwrap_or_else(|error| panic!("chord: {error}"));
    let binding = Binding::new(context, id.clone(), vec![chord], Provenance::Compiled)
        .unwrap_or_else(|error| panic!("binding: {error}"));
    let generation = availability_generation(vec![ActionAvailability::new(
        id,
        Availability::Unavailable {
            reason: reason.to_owned(),
        },
    )]);
    let stack = ContextStack::from_ordered(["global"], false)
        .unwrap_or_else(|error| panic!("stack: {error}"));
    RegistryCandidate::new(vec![action], vec![binding], vec![], vec![stack], generation)
        .compose()
        .unwrap_or_else(|error| panic!("snapshot: {error}"))
}

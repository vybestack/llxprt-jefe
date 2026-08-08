//! Provider availability survives the host's availability refresh
//! (issue #390 CW-10, row CW10-13).
//!
//! The host recomputes availability for every action in the snapshot on nearly
//! every input. Its reason table describes compiled actions only, so a provider
//! action it has no opinion about must keep whatever startup composition
//! decided — not silently become available.

use super::*;
use crate::domain::action_registry::{
    Action, ActionAvailability, ActionId, ActionMetadata, Availability, HandlerKey,
};
use crate::domain::input_context::ContextId;

fn action_id(value: &str) -> ActionId {
    let Ok(parsed) = ActionId::parse(value) else {
        panic!("action fixture must parse");
    };
    parsed
}

fn provider_action(id: &str) -> Action {
    let Ok(context) = ContextId::parse("dashboard") else {
        panic!("context fixture must parse");
    };
    let metadata = ActionMetadata {
        id: action_id(id),
        label: "Ship release".to_owned(),
        description: "Ship the selected release".to_owned(),
        category: "vendor".to_owned(),
        contexts: vec![context],
    };
    match Action::new(metadata, HandlerKey::ProviderAction, false) {
        Ok(action) => action,
        Err(error) => panic!("provider action fixture must build: {error:?}"),
    }
}

/// A state whose snapshot carries one provider action with `availability`.
fn state_with_provider(availability: Availability) -> AppState {
    let settings = crate::persistence::settings_document::PublishedSettings::default();
    let composed = crate::persistence::keymap_edit::compose_published_with_providers(
        &settings,
        "test",
        vec![provider_action("vendor.deploy.ship")],
        vec![ActionAvailability::new(
            action_id("vendor.deploy.ship"),
            availability,
        )],
    );
    let Ok(composed) = composed else {
        panic!("provider composition must succeed");
    };
    AppState {
        action_registry_snapshot: Some(composed.snapshot().clone()),
        ..AppState::default()
    }
}

/// The regression that matters: an action whose provider has no binary for this
/// host must not be re-published as available by a refresh that never heard of
/// it. Doing so would offer the operator an action that cannot possibly run.
#[test]
fn an_unavailable_provider_action_is_not_made_available_by_a_refresh() {
    let reason = "no binary for x86_64-unknown-linux-gnu";
    let state = state_with_provider(Availability::Unavailable {
        reason: reason.to_owned(),
    });
    let Some(snapshot) = state.action_registry_snapshot.as_ref() else {
        panic!("fixture must carry a snapshot");
    };

    let entries = availability_entries(&state, snapshot.actions());

    let entry = entries
        .iter()
        .find(|entry| entry.action() == &action_id("vendor.deploy.ship"));
    assert_eq!(
        entry.map(ActionAvailability::availability),
        Some(&Availability::Unavailable {
            reason: reason.to_owned()
        }),
        "the refresh must preserve the provider reason composition published"
    );
}

/// An available provider action stays available; preserving is not freezing.
#[test]
fn an_available_provider_action_stays_available() {
    let state = state_with_provider(Availability::Available);
    let Some(snapshot) = state.action_registry_snapshot.as_ref() else {
        panic!("fixture must carry a snapshot");
    };

    let entries = availability_entries(&state, snapshot.actions());

    let entry = entries
        .iter()
        .find(|entry| entry.action() == &action_id("vendor.deploy.ship"));
    assert_eq!(
        entry.map(ActionAvailability::availability),
        Some(&Availability::Available)
    );
}

/// Compiled actions must still be recomputed from host state, or the refresh
/// would stop doing its job.
#[test]
fn compiled_actions_are_still_recomputed_from_host_state() {
    let mut state = state_with_provider(Availability::Available);
    state.nav = crate::state::navigation::NavState::rooted(crate::state::ScreenId::Issues);
    state.issues_state.issue_focus = crate::state::IssueFocus::IssueList;
    let Some(snapshot) = state.action_registry_snapshot.clone() else {
        panic!("fixture must carry a snapshot");
    };

    let entries = availability_entries(&state, snapshot.actions());

    let entry = entries
        .iter()
        .find(|entry| entry.action() == &action_id("issues.list-send-agent"));
    assert_eq!(
        entry.map(ActionAvailability::availability),
        Some(&Availability::Unavailable {
            reason: "No issue selected".to_owned()
        })
    );
}

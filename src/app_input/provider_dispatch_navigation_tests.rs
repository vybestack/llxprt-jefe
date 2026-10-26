//! Navigation outcome lifetime tests sharing the provider-dispatch request fixtures.

use super::*;

#[test]
fn navigate_outcome_row_survives_the_route_push() {
    let mut state = crate::test_app_state();
    let route = Id::parse("actions").unwrap_or_else(|error| panic!("declared route: {error}"));
    let policy = ActionPolicy::new(
        ActionConfirmation::None,
        vec![ActionOutcome::NavigateDeclaredRoute],
        false,
    )
    .with_declared_routes(vec![route.clone()]);
    let key = active_request_with_policy(&mut state, policy);
    state
        .provider_requests
        .record_outcome(
            &key,
            Outcome::Navigate {
                route_id: route,
                activation: TypedMap::new(),
            },
            1,
        )
        .unwrap_or_else(|error| panic!("record navigate outcome: {error}"));
    let origin_instance = state.nav.current().id;
    assert!(
        state
            .latest_current_provider_request()
            .is_some_and(jefe::state::provider_requests::ActiveRequest::is_terminal),
        "the terminal navigate row must exist on the invoking instance before the push"
    );

    apply_provider_host_action_state(
        &mut state,
        &ProviderHostAction::Navigate {
            route: jefe::workbench::RouteId::from_static("actions"),
            values: jefe::workbench::ActivationValues::empty(),
        },
    );

    assert_eq!(state.screen(), ScreenId::Actions);
    assert_ne!(state.nav.current().id, origin_instance);
    let carried = state
        .latest_current_provider_request()
        .unwrap_or_else(|| panic!("terminal navigate row must survive the route push"));
    assert_eq!(carried.key(), &key);
    assert!(carried.is_terminal());

    // The escape path still dismisses the carried row from the pushed instance.
    let transition = state
        .apply_message(jefe::messages::AppMessage::Provider(Box::new(
            ProviderMessage::DismissTerminals,
        )))
        .unwrap_or_else(|error| panic!("dismiss transition: {error}"));
    let state = transition.next_state;
    assert!(state.latest_current_provider_request().is_none());
    assert!(
        state
            .provider_requests
            .requests()
            .iter()
            .all(|request| request.key() != &key)
    );
}

#[test]
fn refused_route_push_keeps_the_terminal_request_on_its_origin_instance() {
    use jefe::state::navigation_dirty::{DraftToken, SaveIntent};

    let mut state = crate::test_app_state();
    let key = active_request(&mut state);
    state
        .provider_requests
        .record_error(&key, "completed".to_owned())
        .unwrap_or_else(|error| panic!("record terminal request: {error}"));
    let origin_instance = state.nav.current().id;
    state.mark_screen_dirty(
        DraftToken::next(),
        SaveIntent::Unavailable {
            reason: "test draft has no save target",
        },
    );

    apply_provider_host_action_state(
        &mut state,
        &ProviderHostAction::Navigate {
            route: jefe::workbench::RouteId::from_static("actions"),
            values: jefe::workbench::ActivationValues::empty(),
        },
    );

    assert_eq!(state.nav.current().id, origin_instance);
    let terminal = state
        .latest_current_provider_request()
        .unwrap_or_else(|| panic!("refused navigation must preserve the origin terminal row"));
    assert_eq!(terminal.key(), &key);
    assert!(terminal.is_terminal());
}

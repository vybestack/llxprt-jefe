//! Push, Replace, Back, and the stack bound (issue #386, CW06-01..CW06-07).

use crate::domain::Id;
use crate::workbench::{
    ActivationError, ActivationValue, ActivationValues, NavCode, RouteId, ScreenId,
    ScreenInstanceId, ScreenRegistry, screen_registry,
};

use super::navigation::{
    Activation, MAX_NAVIGATION_STACK, NavIntent, NavMessage, NavOutcome, NavRefusal, NavState,
    SuspendedInstance, reduce_navigation,
};

fn registry() -> &'static ScreenRegistry {
    screen_registry().unwrap_or_else(|_| unreachable!("the compiled registry must be well formed"))
}

fn route_of(screen: ScreenId) -> RouteId {
    let Some(descriptor) = registry().get(screen) else {
        unreachable!("every compiled screen has a descriptor");
    };
    descriptor.route
}

fn rooted(screen: ScreenId) -> NavState {
    match NavState::rooted(registry(), screen) {
        Ok(state) => state,
        Err(refusal) => unreachable!("a compiled screen must root a session: {refusal}"),
    }
}

/// An activation for `screen`, computed from `state`'s live current instance.
fn request(state: &NavState, screen: ScreenId) -> Activation {
    Activation::from_source(route_of(screen), ActivationValues::empty(), state.current())
}

fn apply(state: NavState, intent: NavIntent) -> (NavState, NavOutcome) {
    let transition = reduce_navigation(state, registry(), NavMessage::Navigate(intent));
    (transition.state, transition.outcome)
}

fn push(state: NavState, screen: ScreenId) -> (NavState, NavOutcome) {
    let intent = NavIntent::Push(request(&state, screen));
    apply(state, intent)
}

fn replace(state: NavState, screen: ScreenId) -> (NavState, NavOutcome) {
    let intent = NavIntent::Replace(request(&state, screen));
    apply(state, intent)
}

// ── CW06-01: Push ───────────────────────────────────────────────────────────

#[test]
fn push_suspends_the_exact_current_instance_and_enters_a_fresh_one() {
    let before = rooted(ScreenId::Dashboard);
    let suspended_instance = before.current().clone();

    let (after, outcome) = push(before, ScreenId::PullRequests);

    assert_eq!(after.current().screen, ScreenId::PullRequests);
    assert_eq!(after.depth(), 1);
    assert_eq!(
        after.suspended().first().map(SuspendedInstance::instance),
        Some(&suspended_instance),
        "the suspended instance must be the exact instance that was current"
    );
    assert_ne!(
        after.current().id,
        suspended_instance.id,
        "a pushed target is a fresh instance, never the suspended one"
    );
    assert!(
        after.current().generation > suspended_instance.generation,
        "screen generations advance so an older instance's completions are stale"
    );
    let NavOutcome::Pushed {
        suspended, entered, ..
    } = outcome
    else {
        panic!("a valid push reports what it suspended and what it entered: {outcome:?}");
    };
    assert_eq!(suspended, suspended_instance.id);
    assert_eq!(entered, after.current().id);
}

#[test]
fn a_pushed_instance_starts_focused_where_its_descriptor_says() {
    let (after, _) = push(rooted(ScreenId::Dashboard), ScreenId::Issues);
    let Some(descriptor) = registry().get(ScreenId::Issues) else {
        unreachable!("every compiled screen has a descriptor");
    };
    assert_eq!(after.current().panel_focus, descriptor.initial_focus);
}

#[test]
fn every_entered_instance_has_an_identity_no_other_instance_had() {
    let mut state = rooted(ScreenId::Dashboard);
    let mut seen = vec![state.current().id];
    for screen in [
        ScreenId::Issues,
        ScreenId::PullRequests,
        ScreenId::Actions,
        ScreenId::Errors,
    ] {
        let (next, _) = push(state, screen);
        state = next;
        let id = state.current().id;
        assert!(!seen.contains(&id), "instance identity {id} was reused");
        seen.push(id);
    }
}

#[test]
fn the_activation_an_instance_was_entered_with_records_which_instance_asked() {
    let before = rooted(ScreenId::Dashboard);
    let source = before.current().id;
    let (after, _) = push(before, ScreenId::Issues);
    assert_eq!(after.current().activation.source_instance, source);
    assert_eq!(after.current().activation.route, route_of(ScreenId::Issues));
}

// ── Validation refusals leave navigation untouched ──────────────────────────

#[test]
fn a_push_to_an_undeclared_route_changes_nothing() {
    let before = rooted(ScreenId::Dashboard);
    let Ok(unknown) = RouteId::parse("nonesuch") else {
        unreachable!("test route id is valid");
    };
    let intent = NavIntent::Push(Activation::from_source(
        unknown,
        ActivationValues::empty(),
        before.current(),
    ));
    let expected = before.clone();

    let (after, outcome) = apply(before, intent);

    assert_eq!(after, expected, "a refused push must not mutate navigation");
    let NavOutcome::Refused(refusal) = outcome else {
        panic!("an undeclared route must be refused: {outcome:?}");
    };
    assert!(matches!(
        refusal,
        NavRefusal::Activation(ActivationError::UnknownRoute { .. })
    ));
    assert_eq!(refusal.code(), NavCode::E001);
}

#[test]
fn a_push_whose_activation_does_not_satisfy_the_schema_changes_nothing() {
    // A compiled screen declares no activation fields, so any supplied value is
    // undeclared and the whole request is refused before anything moves.
    let before = rooted(ScreenId::Dashboard);
    let Ok(name) = Id::parse("number") else {
        unreachable!("test field name is a valid identifier");
    };
    let Ok(values) = ActivationValues::new(vec![(name, ActivationValue::Integer(42))]) else {
        unreachable!("one field is within the bound");
    };
    let intent = NavIntent::Push(Activation::from_source(
        route_of(ScreenId::Issues),
        values,
        before.current(),
    ));
    let expected = before.clone();

    let (after, outcome) = apply(before, intent);

    assert_eq!(after, expected);
    assert!(matches!(
        outcome,
        NavOutcome::Refused(NavRefusal::Activation(ActivationError::UnknownField { .. }))
    ));
}

#[test]
fn a_push_computed_from_an_instance_that_is_no_longer_current_changes_nothing() {
    // The request was computed from a snapshot that has since been replaced,
    // so acting on it would navigate on behalf of a screen that is gone.
    let before = rooted(ScreenId::Dashboard);
    let stale = Activation::from_source(
        route_of(ScreenId::Issues),
        ActivationValues::empty(),
        before.current(),
    );
    let (moved, _) = push(before, ScreenId::PullRequests);
    let expected = moved.clone();

    let (after, outcome) = apply(moved, NavIntent::Push(stale));

    assert_eq!(after, expected);
    assert!(matches!(
        outcome,
        NavOutcome::Refused(NavRefusal::StaleSource { .. })
    ));
}

// ── CW06-02: Replace ────────────────────────────────────────────────────────

#[test]
fn replace_enters_the_target_and_disposes_the_old_instance_without_stacking() {
    let before = rooted(ScreenId::Dashboard);
    let disposed_instance = before.current().id;

    let (after, outcome) = replace(before, ScreenId::Actions);

    assert_eq!(after.current().screen, ScreenId::Actions);
    assert_eq!(after.depth(), 0, "replace never grows the stack");
    let NavOutcome::Replaced { disposed, entered } = outcome else {
        panic!("a valid replace reports what it disposed: {outcome:?}");
    };
    assert_eq!(disposed, disposed_instance);
    assert_eq!(entered, after.current().id);
    assert_ne!(entered, disposed_instance);
}

#[test]
fn a_replace_that_fails_validation_disposes_nothing() {
    let before = push(rooted(ScreenId::Dashboard), ScreenId::Issues).0;
    let Ok(unknown) = RouteId::parse("nonesuch") else {
        unreachable!("test route id is valid");
    };
    let intent = NavIntent::Replace(Activation::from_source(
        unknown,
        ActivationValues::empty(),
        before.current(),
    ));
    let expected = before.clone();

    let (after, outcome) = apply(before, intent);

    assert_eq!(
        after, expected,
        "the target is constructed before the old instance is disposed"
    );
    assert!(matches!(outcome, NavOutcome::Refused(_)));
}

#[test]
fn replace_keeps_the_stack_it_was_given() {
    let (pushed, _) = push(rooted(ScreenId::Dashboard), ScreenId::Issues);
    let suspended = pushed.suspended().to_vec();

    let (after, _) = replace(pushed, ScreenId::Actions);

    assert_eq!(after.suspended(), suspended.as_slice());
    assert_eq!(after.depth(), 1);
}

// ── CW06-03: Back ───────────────────────────────────────────────────────────

#[test]
fn back_restores_the_exact_prior_instance() {
    let before = rooted(ScreenId::Dashboard);
    let original = before.current().clone();
    let (pushed, _) = push(before, ScreenId::PullRequests);
    let disposed_instance = pushed.current().id;

    let (after, outcome) = apply(pushed, NavIntent::Back);

    assert_eq!(
        after.current(),
        &original,
        "the restored instance must be byte-equivalent to the suspended one"
    );
    assert_eq!(after.depth(), 0);
    let NavOutcome::Restored { disposed, restored } = outcome else {
        panic!("back over the stack reports what it disposed: {outcome:?}");
    };
    assert_eq!(disposed, disposed_instance);
    assert_eq!(restored, original.id);
}

#[test]
fn back_unwinds_a_deep_stack_in_reverse_entry_order() {
    let mut state = rooted(ScreenId::Dashboard);
    let order = [ScreenId::Issues, ScreenId::PullRequests, ScreenId::Actions];
    for screen in order {
        state = push(state, screen).0;
    }
    for screen in order.into_iter().rev().skip(1) {
        state = apply(state, NavIntent::Back).0;
        assert_eq!(state.current().screen, screen);
    }
    state = apply(state, NavIntent::Back).0;
    assert_eq!(state.current().screen, ScreenId::Dashboard);
    assert_eq!(state.depth(), 0);
}

#[test]
fn back_at_the_root_changes_nothing_and_reports_nothing_to_do() {
    let before = rooted(ScreenId::Dashboard);
    let expected = before.clone();

    let (after, outcome) = apply(before, NavIntent::Back);

    assert_eq!(after, expected);
    assert_eq!(outcome, NavOutcome::Unchanged);
}

// ── CW06-07: stack bound ────────────────────────────────────────────────────

#[test]
fn the_stack_holds_exactly_the_declared_maximum() {
    let mut state = rooted(ScreenId::Dashboard);
    for _ in 0..MAX_NAVIGATION_STACK {
        let (next, outcome) = push(state, ScreenId::Issues);
        assert!(matches!(outcome, NavOutcome::Pushed { .. }));
        state = next;
    }
    assert_eq!(state.depth(), MAX_NAVIGATION_STACK);
}

#[test]
fn a_push_beyond_the_declared_maximum_is_refused_and_changes_nothing() {
    let mut state = rooted(ScreenId::Dashboard);
    for _ in 0..MAX_NAVIGATION_STACK {
        state = push(state, ScreenId::Issues).0;
    }
    let expected = state.clone();

    let (after, outcome) = push(state, ScreenId::Issues);

    assert_eq!(after, expected, "an overflowing push must not mutate state");
    let NavOutcome::Refused(refusal) = outcome else {
        panic!("the 33rd push must be refused: {outcome:?}");
    };
    assert!(matches!(refusal, NavRefusal::StackDepth { .. }));
    assert_eq!(refusal.code(), NavCode::E001);
    assert!(refusal.to_string().starts_with("NAV-E001: "));
}

#[test]
fn replace_is_still_available_at_the_stack_bound() {
    // Replace does not stack, so a full stack must not strand the session.
    let mut state = rooted(ScreenId::Dashboard);
    for _ in 0..MAX_NAVIGATION_STACK {
        state = push(state, ScreenId::Issues).0;
    }
    let (after, outcome) = replace(state, ScreenId::Actions);
    assert!(matches!(outcome, NavOutcome::Replaced { .. }));
    assert_eq!(after.depth(), MAX_NAVIGATION_STACK);
}

// ── Root construction ───────────────────────────────────────────────────────

#[test]
fn a_rooted_session_has_one_clean_instance_and_no_stack() {
    let state = rooted(ScreenId::Issues);
    assert_eq!(state.current().screen, ScreenId::Issues);
    assert_eq!(state.depth(), 0);
    assert_eq!(state.current().generation, 1);
    assert_eq!(
        state.current().activation.source_instance,
        state.current().id,
        "the root instance was activated by nothing but itself"
    );
}

#[test]
fn every_compiled_screen_can_root_a_session() {
    for screen in ScreenId::ALL {
        let state = rooted(screen);
        assert_eq!(state.current().screen, screen);
    }
}

#[test]
fn instance_identity_is_never_reused_across_states() {
    let first = rooted(ScreenId::Dashboard);
    let second = rooted(ScreenId::Dashboard);
    assert_ne!(first.current().id, second.current().id);
}

#[test]
fn a_suspended_instance_is_not_the_live_one() {
    let (state, _) = push(rooted(ScreenId::Dashboard), ScreenId::Issues);
    let live: ScreenInstanceId = state.current().id;
    assert!(
        !state
            .suspended()
            .iter()
            .any(|entry| entry.instance().id == live)
    );
}

// ── CW06-08: only the live instance's work is answered ──────────────────────

#[test]
fn the_live_generations_are_the_current_instances_own() {
    let state = rooted(ScreenId::Dashboard);
    let current = state.current();
    assert_eq!(
        state.live_generations(),
        (current.generation, current.activation.activation_generation)
    );
    assert!(state.answers_live_work(current.generation, current.activation.activation_generation));
}

#[test]
fn a_suspended_instances_work_is_not_answered_while_it_waits() {
    let before = rooted(ScreenId::Dashboard);
    let (suspended_screen, suspended_activation) = before.live_generations();

    let (after, _) = push(before, ScreenId::Issues);

    assert!(
        !after.answers_live_work(suspended_screen, suspended_activation),
        "a suspended instance's answers must be ignored while it waits"
    );
}

#[test]
fn restoring_an_instance_makes_its_work_answerable_again() {
    let before = rooted(ScreenId::Dashboard);
    let (suspended_screen, suspended_activation) = before.live_generations();
    let (pushed, _) = push(before, ScreenId::Issues);

    let (after, _) = apply(pushed, NavIntent::Back);

    assert!(
        after.answers_live_work(suspended_screen, suspended_activation),
        "back restores the instance's subscriptions along with the instance"
    );
}

#[test]
fn a_disposed_instances_work_is_never_answered_again() {
    let before = rooted(ScreenId::Dashboard);
    let (pushed, _) = push(before, ScreenId::Issues);
    let (disposed_screen, disposed_activation) = pushed.live_generations();

    let (after, _) = apply(pushed, NavIntent::Back);

    assert!(
        !after.answers_live_work(disposed_screen, disposed_activation),
        "the instance back disposed must never receive an answer"
    );
}

#[test]
fn a_replaced_instances_work_is_never_answered_again() {
    let before = rooted(ScreenId::Dashboard);
    let (disposed_screen, disposed_activation) = before.live_generations();

    let (after, _) = replace(before, ScreenId::Actions);

    assert!(!after.answers_live_work(disposed_screen, disposed_activation));
}

#[test]
fn generations_only_ever_move_forward() {
    let mut state = rooted(ScreenId::Dashboard);
    let mut previous = state.live_generations();
    for screen in [ScreenId::Issues, ScreenId::PullRequests, ScreenId::Actions] {
        state = push(state, screen).0;
        let current = state.live_generations();
        assert!(
            current.0 > previous.0 && current.1 > previous.1,
            "generations must advance: {previous:?} -> {current:?}"
        );
        previous = current;
    }
}

#[test]
fn a_refused_navigation_does_not_advance_generations() {
    let mut state = rooted(ScreenId::Dashboard);
    for _ in 0..MAX_NAVIGATION_STACK {
        state = push(state, ScreenId::Issues).0;
    }
    let before = state.live_generations();

    let (after, _) = push(state, ScreenId::Issues);

    assert_eq!(
        after.live_generations(),
        before,
        "a refusal must not invalidate the live instance's in-flight work"
    );
}

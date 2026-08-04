//! The host dirty guard: Save, Discard, Cancel, and recovery
//! (issue #386, CW06-05/CW06-06).

use crate::domain::effects::{Correlation, CorrelationId, EffectError, EffectFamily, SemanticKey};
use crate::domain::{Id, effects::EffectErrorKind};
use crate::workbench::{ActivationValues, PanelId, ScreenId, ScreenRegistry, screen_registry};

use super::navigation::{
    Activation, NavIntent, NavMessage, NavOutcome, NavState, NavTransition, reduce_navigation,
};
use super::navigation_dirty::{
    DirtyChoice, DirtyState, DraftAction, DraftToken, GuardPhase, SaveIntent,
};

fn registry() -> &'static ScreenRegistry {
    screen_registry().unwrap_or_else(|_| unreachable!("the compiled registry must be well formed"))
}

fn rooted(screen: ScreenId) -> NavState {
    NavState::rooted(screen)
}

fn push_to(state: &NavState, screen: ScreenId) -> NavIntent {
    let Some(descriptor) = registry().get(screen) else {
        unreachable!("every compiled screen has a descriptor");
    };
    NavIntent::Push(Activation::from_source(
        descriptor.route,
        ActivationValues::empty(),
        state.current(),
    ))
}

fn send(state: NavState, message: NavMessage) -> NavTransition {
    reduce_navigation(state, registry(), message)
}

fn owner_key() -> SemanticKey {
    SemanticKey::new(EffectFamily::Persistence, "settings-draft")
}

fn owner() -> Id {
    Id::parse("core.settings").unwrap_or_else(|_| unreachable!("test owner is a valid identifier"))
}

/// A dirty session whose owner declares a real save.
fn savable() -> NavState {
    let state = rooted(ScreenId::Dashboard);
    send(
        state,
        NavMessage::MarkDirty {
            draft: DraftToken::next(),
            save: SaveIntent::Owner {
                owner: owner(),
                semantic_key: owner_key(),
            },
        },
    )
    .state
}

/// A dirty session whose draft has nowhere to save to.
fn unsavable() -> NavState {
    let state = rooted(ScreenId::Dashboard);
    send(
        state,
        NavMessage::MarkDirty {
            draft: DraftToken::next(),
            save: SaveIntent::Unavailable {
                reason: "an unsent draft has nowhere to save to",
            },
        },
    )
    .state
}

/// One attempt's identity, at the live generations.
fn attempt(state: &NavState, key: SemanticKey, id: u64) -> Correlation {
    let (screen_generation, activation_generation) = state.live_generations();
    Correlation {
        correlation_id: CorrelationId::new(id),
        owner: owner(),
        screen_generation,
        activation_generation,
        semantic_key: key,
    }
}

/// The completion the guard is waiting for, at the live generations.
fn completion(state: &NavState, key: SemanticKey) -> Correlation {
    attempt(state, key, 1)
}

/// Drive a guard all the way to a registered, running save attempt.
fn saving_attempt(state: NavState, id: u64) -> (NavState, Correlation) {
    let requested = send(state, NavMessage::ResolveDirty(DirtyChoice::Save)).state;
    let correlation = attempt(&requested, owner_key(), id);
    let running = send(
        requested,
        NavMessage::SaveStarted {
            correlation: correlation.clone(),
        },
    )
    .state;
    (running, correlation)
}

/// Assert the owner was asked to save exactly this draft.
fn assert_asked_to_save(action: &DraftAction, expected_draft: Option<DraftToken>) {
    let DraftAction::Save {
        owner: asked_owner,
        semantic_key,
        draft,
    } = action
    else {
        panic!("the guard must ask the owner to save: {action:?}");
    };
    assert_eq!(asked_owner, &owner());
    assert_eq!(semantic_key, &owner_key());
    if let Some(expected) = expected_draft {
        assert_eq!(*draft, expected, "the save must name the held draft");
    }
}

fn failure() -> EffectError {
    EffectError::new(EffectErrorKind::Io, true, "the write could not complete")
}

// ── CW06-05: navigation waits for the draft ─────────────────────────────────

#[test]
fn leaving_a_dirty_screen_raises_the_guard_instead_of_navigating() {
    let state = savable();
    let expected_screen = state.current().screen;
    let intent = push_to(&state, ScreenId::Issues);

    let transition = send(state, NavMessage::Navigate(intent));

    assert_eq!(transition.outcome, NavOutcome::GuardRaised);
    assert_eq!(transition.draft, DraftAction::None);
    assert_eq!(
        transition.state.current().screen,
        expected_screen,
        "the session must not move until the draft is resolved"
    );
    assert_eq!(transition.state.depth(), 0);
    assert!(matches!(
        transition
            .state
            .guard()
            .map(super::navigation_dirty::DirtyGuard::phase),
        Some(GuardPhase::Choosing)
    ));
}

#[test]
fn leaving_a_clean_screen_raises_no_guard() {
    let state = rooted(ScreenId::Dashboard);
    let intent = push_to(&state, ScreenId::Issues);

    let transition = send(state, NavMessage::Navigate(intent));

    assert!(matches!(transition.outcome, NavOutcome::Pushed { .. }));
    assert!(transition.state.guard().is_none());
}

#[test]
fn a_second_request_while_the_guard_is_up_does_not_replace_the_pending_one() {
    let state = savable();
    let first = push_to(&state, ScreenId::Issues);
    let guarded = send(state, NavMessage::Navigate(first.clone())).state;
    let second = push_to(&guarded, ScreenId::Actions);

    let transition = send(guarded, NavMessage::Navigate(second));

    assert_eq!(transition.outcome, NavOutcome::GuardRaised);
    assert_eq!(
        transition
            .state
            .guard()
            .map(|guard| guard.pending().clone()),
        Some(first),
        "the user is being asked about the first request, so it must stand"
    );
}

#[test]
fn choosing_save_asks_the_owner_and_does_not_navigate_yet() {
    let state = savable();
    let intent = push_to(&state, ScreenId::Issues);
    let guarded = send(state, NavMessage::Navigate(intent)).state;

    let transition = send(guarded, NavMessage::ResolveDirty(DirtyChoice::Save));

    assert_asked_to_save(&transition.draft, None);
    assert_eq!(transition.state.current().screen, ScreenId::Dashboard);
    assert!(matches!(
        transition
            .state
            .guard()
            .map(super::navigation_dirty::DirtyGuard::phase),
        Some(GuardPhase::SaveRequested { .. })
    ));
}

#[test]
fn a_matching_successful_completion_performs_the_pending_navigation() {
    let state = savable();
    let intent = push_to(&state, ScreenId::Issues);
    let guarded = send(state, NavMessage::Navigate(intent)).state;
    let (saving, correlation) = saving_attempt(guarded, 1);

    let transition = send(
        saving,
        NavMessage::SaveCompleted {
            correlation,
            result: Ok(()),
        },
    );

    assert!(matches!(transition.outcome, NavOutcome::Pushed { .. }));
    assert_eq!(transition.state.current().screen, ScreenId::Issues);
    assert!(transition.state.guard().is_none());
    assert_eq!(transition.state.current().dirty, DirtyState::Clean);
}

// ── CW06-08 applied to the save: only the live instance is answered ─────────

#[test]
fn a_completion_for_another_operation_changes_nothing() {
    let state = savable();
    let intent = push_to(&state, ScreenId::Issues);
    let guarded = send(state, NavMessage::Navigate(intent)).state;
    let (saving, _started) = saving_attempt(guarded, 1);
    let other = SemanticKey::new(EffectFamily::GitHub, "something-else");
    let correlation = completion(&saving, other);
    let expected = saving.clone();

    let transition = send(
        saving,
        NavMessage::SaveCompleted {
            correlation,
            result: Ok(()),
        },
    );

    assert_eq!(transition.state, expected);
    assert_eq!(transition.outcome, NavOutcome::Unchanged);
}

#[test]
fn a_completion_naming_a_stale_generation_changes_nothing() {
    let state = savable();
    let intent = push_to(&state, ScreenId::Issues);
    let guarded = send(state, NavMessage::Navigate(intent)).state;
    let (saving, _started) = saving_attempt(guarded, 1);
    let mut correlation = completion(&saving, owner_key());
    correlation.screen_generation = correlation.screen_generation.saturating_add(7);
    let expected = saving.clone();

    let transition = send(
        saving,
        NavMessage::SaveCompleted {
            correlation,
            result: Ok(()),
        },
    );

    assert_eq!(transition.state, expected);
    assert_eq!(transition.outcome, NavOutcome::Unchanged);
}

#[test]
fn a_completion_with_no_guard_waiting_changes_nothing() {
    let state = rooted(ScreenId::Dashboard);
    let correlation = completion(&state, owner_key());
    let expected = state.clone();

    let transition = send(
        state,
        NavMessage::SaveCompleted {
            correlation,
            result: Ok(()),
        },
    );

    assert_eq!(transition.state, expected);
    assert_eq!(transition.outcome, NavOutcome::Unchanged);
}

// ── Recovery: a failed save keeps the user with their work ─────────────────

#[test]
fn a_failed_save_retains_the_draft_the_screen_and_the_choices() {
    let state = savable();
    let held = state.current().dirty.clone();
    let intent = push_to(&state, ScreenId::Issues);
    let guarded = send(state, NavMessage::Navigate(intent.clone())).state;
    let (saving, correlation) = saving_attempt(guarded, 1);

    let transition = send(
        saving,
        NavMessage::SaveCompleted {
            correlation,
            result: Err(failure()),
        },
    );

    assert_eq!(transition.outcome, NavOutcome::SaveFailed);
    assert_eq!(transition.state.current().screen, ScreenId::Dashboard);
    assert_eq!(transition.state.current().dirty, held);
    let Some(guard) = transition.state.guard() else {
        panic!("the guard must stay up so recovery choices remain reachable");
    };
    assert!(matches!(guard.phase(), GuardPhase::Failed { .. }));
    assert_eq!(
        guard.pending(),
        &intent,
        "the navigation the user asked for must survive a failed save"
    );
}

#[test]
fn retrying_after_a_failed_save_asks_the_owner_again() {
    let state = savable();
    let intent = push_to(&state, ScreenId::Issues);
    let guarded = send(state, NavMessage::Navigate(intent)).state;
    let (saving, correlation) = saving_attempt(guarded, 1);
    let failed = send(
        saving,
        NavMessage::SaveCompleted {
            correlation,
            result: Err(failure()),
        },
    )
    .state;

    let transition = send(failed, NavMessage::ResolveDirty(DirtyChoice::Save));

    assert_asked_to_save(&transition.draft, None);
    assert!(matches!(
        transition
            .state
            .guard()
            .map(super::navigation_dirty::DirtyGuard::phase),
        Some(GuardPhase::SaveRequested { .. })
    ));
}

#[test]
fn discarding_after_a_failed_save_still_leaves() {
    let state = savable();
    let intent = push_to(&state, ScreenId::Issues);
    let guarded = send(state, NavMessage::Navigate(intent)).state;
    let (saving, correlation) = saving_attempt(guarded, 1);
    let failed = send(
        saving,
        NavMessage::SaveCompleted {
            correlation,
            result: Err(failure()),
        },
    )
    .state;

    let transition = send(failed, NavMessage::ResolveDirty(DirtyChoice::Discard));

    assert!(matches!(transition.outcome, NavOutcome::Pushed { .. }));
    assert_eq!(transition.state.current().screen, ScreenId::Issues);
}

// ── CW06-06: Discard and Cancel ────────────────────────────────────────────

#[test]
fn discarding_abandons_the_draft_and_performs_the_navigation() {
    let state = savable();
    let Some(abandoned) = state.current().dirty.draft() else {
        panic!("the fixture holds a draft");
    };
    let intent = push_to(&state, ScreenId::Issues);
    let guarded = send(state, NavMessage::Navigate(intent)).state;

    let transition = send(guarded, NavMessage::ResolveDirty(DirtyChoice::Discard));

    assert!(matches!(transition.outcome, NavOutcome::Pushed { .. }));
    assert_eq!(transition.state.current().screen, ScreenId::Issues);
    assert_eq!(
        transition.draft,
        DraftAction::RestoreBase { draft: abandoned },
        "the owner must be told to restore the base its draft was taken from"
    );
    assert!(transition.state.guard().is_none());
}

#[test]
fn a_discarded_draft_does_not_follow_the_session_to_the_next_screen() {
    let state = savable();
    let intent = push_to(&state, ScreenId::Issues);
    let guarded = send(state, NavMessage::Navigate(intent)).state;

    let after = send(guarded, NavMessage::ResolveDirty(DirtyChoice::Discard)).state;

    assert_eq!(after.current().dirty, DirtyState::Clean);
    let Some(suspended) = after.suspended().first() else {
        panic!("the previous instance was suspended");
    };
    assert_eq!(
        suspended.instance().dirty,
        DirtyState::Clean,
        "the instance the user left must not still claim unsaved work"
    );
}

#[test]
fn cancelling_keeps_the_draft_and_drops_the_navigation() {
    let state = savable();
    let held = state.current().dirty.clone();
    let intent = push_to(&state, ScreenId::Issues);
    let guarded = send(state, NavMessage::Navigate(intent)).state;

    let transition = send(guarded, NavMessage::ResolveDirty(DirtyChoice::Cancel));

    assert_eq!(transition.outcome, NavOutcome::Cancelled);
    assert_eq!(transition.draft, DraftAction::None);
    assert_eq!(transition.state.current().screen, ScreenId::Dashboard);
    assert_eq!(transition.state.current().dirty, held);
    assert!(
        transition.state.guard().is_none(),
        "cancelling clears the pending navigation rather than deferring it"
    );
}

#[test]
fn cancelling_restores_the_exact_focus_the_guard_interrupted() {
    let mut state = savable();
    let Some(descriptor) = registry().get(ScreenId::Dashboard) else {
        unreachable!("every compiled screen has a descriptor");
    };
    let Some(other) = descriptor
        .focus_order
        .iter()
        .find(|panel| **panel != descriptor.initial_focus)
        .copied()
    else {
        unreachable!("the dashboard cycles focus across more than one panel");
    };
    state.current_mut().panel_focus = other;
    let interrupted: PanelId = other;
    let intent = push_to(&state, ScreenId::Issues);
    let guarded = send(state, NavMessage::Navigate(intent)).state;

    let after = send(guarded, NavMessage::ResolveDirty(DirtyChoice::Cancel)).state;

    assert_eq!(after.current().panel_focus, interrupted);
}

// ── Save when the owner cannot save ────────────────────────────────────────

#[test]
fn a_draft_with_nowhere_to_save_offers_no_save() {
    let state = unsavable();
    let DirtyState::Dirty { save, .. } = &state.current().dirty else {
        panic!("the fixture holds a draft");
    };
    assert!(!save.can_save());
}

#[test]
fn choosing_save_for_a_draft_with_nowhere_to_save_leaves_the_guard_up() {
    let state = unsavable();
    let intent = push_to(&state, ScreenId::Issues);
    let guarded = send(state, NavMessage::Navigate(intent)).state;

    let transition = send(guarded, NavMessage::ResolveDirty(DirtyChoice::Save));

    assert_eq!(transition.outcome, NavOutcome::GuardRaised);
    assert_eq!(transition.draft, DraftAction::None);
    assert_eq!(transition.state.current().screen, ScreenId::Dashboard);
    assert!(
        matches!(
            transition
                .state
                .guard()
                .map(super::navigation_dirty::DirtyGuard::phase),
            Some(GuardPhase::Choosing)
        ),
        "Discard and Cancel must stay reachable"
    );
}

#[test]
fn discarding_a_draft_with_nowhere_to_save_leaves() {
    let state = unsavable();
    let intent = push_to(&state, ScreenId::Issues);
    let guarded = send(state, NavMessage::Navigate(intent)).state;

    let transition = send(guarded, NavMessage::ResolveDirty(DirtyChoice::Discard));

    assert_eq!(transition.state.current().screen, ScreenId::Issues);
    assert!(matches!(transition.draft, DraftAction::RestoreBase { .. }));
}

// ── Bookkeeping ────────────────────────────────────────────────────────────

#[test]
fn resolving_with_no_guard_up_changes_nothing() {
    for choice in [DirtyChoice::Save, DirtyChoice::Discard, DirtyChoice::Cancel] {
        let state = rooted(ScreenId::Dashboard);
        let expected = state.clone();
        let transition = send(state, NavMessage::ResolveDirty(choice));
        assert_eq!(transition.state, expected, "{choice:?} must be inert");
        assert_eq!(transition.outcome, NavOutcome::Unchanged);
    }
}

#[test]
fn marking_clean_lets_the_next_navigation_through() {
    let state = savable();
    let cleaned = send(state, NavMessage::MarkClean).state;
    assert_eq!(cleaned.current().dirty, DirtyState::Clean);
    let intent = push_to(&cleaned, ScreenId::Issues);

    let transition = send(cleaned, NavMessage::Navigate(intent));

    assert!(matches!(transition.outcome, NavOutcome::Pushed { .. }));
}

#[test]
fn a_navigation_that_cannot_commit_never_raises_the_guard() {
    // Asking about unsaved work and then declining to move whatever the user
    // answered would be worse than refusing outright.
    let state = savable();
    let Ok(unknown) = crate::workbench::RouteId::parse("nonesuch") else {
        unreachable!("test route id is valid");
    };
    let intent = NavIntent::Push(Activation::from_source(
        unknown,
        ActivationValues::empty(),
        state.current(),
    ));
    let held = state.current().dirty.clone();

    let transition = send(state, NavMessage::Navigate(intent));

    assert!(matches!(transition.outcome, NavOutcome::Refused(_)));
    assert!(transition.state.guard().is_none());
    assert_eq!(transition.state.current().dirty, held);
}

#[test]
fn asking_to_save_again_while_a_save_is_running_does_not_run_it_twice() {
    let state = savable();
    let intent = push_to(&state, ScreenId::Issues);
    let guarded = send(state, NavMessage::Navigate(intent)).state;
    let (saving, _started) = saving_attempt(guarded, 1);

    let transition = send(saving, NavMessage::ResolveDirty(DirtyChoice::Save));

    assert_eq!(
        transition.draft,
        DraftAction::None,
        "a second Save must not ask the owner to write again"
    );
    assert!(matches!(
        transition
            .state
            .guard()
            .map(super::navigation_dirty::DirtyGuard::phase),
        Some(GuardPhase::Saving { .. })
    ));
}

#[test]
fn a_draft_cannot_be_replaced_while_the_guard_is_saving_it() {
    // The running save's completion would otherwise clear a draft it never saw.
    let state = savable();
    let intent = push_to(&state, ScreenId::Issues);
    let guarded = send(state, NavMessage::Navigate(intent)).state;
    let (saving, _started) = saving_attempt(guarded, 1);
    let held = saving.current().dirty.clone();

    let transition = send(
        saving,
        NavMessage::MarkDirty {
            draft: DraftToken::next(),
            save: SaveIntent::Unavailable { reason: "nowhere" },
        },
    );

    assert_eq!(transition.state.current().dirty, held);
}

#[test]
fn the_instance_left_behind_does_not_carry_a_draft_that_was_abandoned() {
    let state = savable();
    let intent = push_to(&state, ScreenId::Issues);
    let guarded = send(state, NavMessage::Navigate(intent)).state;

    let after = send(guarded, NavMessage::ResolveDirty(DirtyChoice::Discard)).state;

    let Some(suspended) = after.suspended().first() else {
        panic!("the previous instance was suspended");
    };
    assert_eq!(suspended.instance().dirty, DirtyState::Clean);
}

#[test]
fn a_completion_from_a_different_owner_changes_nothing() {
    let state = savable();
    let intent = push_to(&state, ScreenId::Issues);
    let guarded = send(state, NavMessage::Navigate(intent)).state;
    let (saving, _started) = saving_attempt(guarded, 1);
    let mut correlation = completion(&saving, owner_key());
    correlation.owner =
        Id::parse("core.somethingelse").unwrap_or_else(|_| unreachable!("valid identifier"));
    let expected = saving.clone();

    let transition = send(
        saving,
        NavMessage::SaveCompleted {
            correlation,
            result: Ok(()),
        },
    );

    assert_eq!(transition.state, expected);
    assert_eq!(transition.outcome, NavOutcome::Unchanged);
}

#[test]
fn a_completion_for_a_draft_that_has_since_been_replaced_changes_nothing() {
    // The guard has to survive a failed save, a fresh draft, and a late answer
    // about the draft that is gone.
    let state = savable();
    let intent = push_to(&state, ScreenId::Issues);
    let guarded = send(state, NavMessage::Navigate(intent)).state;
    let (saving, correlation) = saving_attempt(guarded, 1);
    let failed = send(
        saving,
        NavMessage::SaveCompleted {
            correlation: correlation.clone(),
            result: Err(failure()),
        },
    )
    .state;
    let replaced = send(
        failed,
        NavMessage::MarkDirty {
            draft: DraftToken::next(),
            save: SaveIntent::Owner {
                owner: owner(),
                semantic_key: owner_key(),
            },
        },
    )
    .state;
    let retrying = send(replaced, NavMessage::ResolveDirty(DirtyChoice::Save)).state;

    // The completion above named the draft that has since been replaced.
    let transition = send(
        retrying,
        NavMessage::SaveCompleted {
            correlation,
            result: Ok(()),
        },
    );

    assert_eq!(
        transition.state.current().screen,
        ScreenId::Dashboard,
        "an answer about a replaced draft must not navigate"
    );
    assert!(transition.state.current().dirty.is_dirty());
}

#[test]
fn the_save_the_owner_is_asked_to_run_names_the_draft_being_held() {
    let state = savable();
    let held = state.current().dirty.draft();
    let intent = push_to(&state, ScreenId::Issues);
    let guarded = send(state, NavMessage::Navigate(intent)).state;

    let transition = send(guarded, NavMessage::ResolveDirty(DirtyChoice::Save));

    assert_asked_to_save(&transition.draft, held);
}

#[test]
fn a_late_answer_from_an_abandoned_attempt_does_not_resolve_the_retry() {
    // Owner, operation, screen, and both generations are identical across
    // attempts, so only the correlation the owner registered tells them apart.
    let state = savable();
    let intent = push_to(&state, ScreenId::Issues);
    let guarded = send(state, NavMessage::Navigate(intent)).state;
    let (first_running, first) = saving_attempt(guarded, 1);
    let failed = send(
        first_running,
        NavMessage::SaveCompleted {
            correlation: first.clone(),
            result: Err(failure()),
        },
    )
    .state;
    let (retrying, second) = saving_attempt(failed, 2);
    assert_ne!(first.correlation_id, second.correlation_id);

    let transition = send(
        retrying,
        NavMessage::SaveCompleted {
            correlation: first,
            result: Ok(()),
        },
    );

    assert_eq!(
        transition.state.current().screen,
        ScreenId::Dashboard,
        "the abandoned attempt must not navigate on the retry's behalf"
    );
    assert!(transition.state.current().dirty.is_dirty());
    assert!(matches!(
        transition
            .state
            .guard()
            .map(super::navigation_dirty::DirtyGuard::phase),
        Some(GuardPhase::Saving { .. })
    ));
}

#[test]
fn nothing_resolves_the_guard_before_the_owner_says_what_it_registered() {
    let state = savable();
    let intent = push_to(&state, ScreenId::Issues);
    let guarded = send(state, NavMessage::Navigate(intent)).state;
    let requested = send(guarded, NavMessage::ResolveDirty(DirtyChoice::Save)).state;
    let correlation = completion(&requested, owner_key());

    let transition = send(
        requested,
        NavMessage::SaveCompleted {
            correlation,
            result: Ok(()),
        },
    );

    assert_eq!(transition.state.current().screen, ScreenId::Dashboard);
    assert!(transition.state.current().dirty.is_dirty());
}

#[test]
fn a_registration_that_answers_no_request_is_ignored() {
    let state = savable();
    let intent = push_to(&state, ScreenId::Issues);
    let guarded = send(state, NavMessage::Navigate(intent)).state;
    let expected = guarded.clone();
    let correlation = attempt(&guarded, owner_key(), 9);

    let transition = send(guarded, NavMessage::SaveStarted { correlation });

    assert_eq!(
        transition.state, expected,
        "no save was asked for, so nothing may claim the guard"
    );
}

#[test]
fn every_draft_identity_is_distinct() {
    let first = DraftToken::next();
    let second = DraftToken::next();
    assert_ne!(first, second);
    assert!(second.get() > first.get());
}

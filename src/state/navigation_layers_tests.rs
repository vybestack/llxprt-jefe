//! What Back does from a real screen (issue #386, CW06-04).

use super::navigation_dirty::{DirtyChoice, DraftToken, SaveIntent};
use super::navigation_unwind::{BackLayer, BackResolution, LocalIntent};
use super::screen_overlays::ConfirmationRequest;
use super::types::{
    AgentChooserState, AppState, ComposerTarget, InlineState, IssueFocus, ModalState,
};
use crate::domain::RepositoryId;
use crate::workbench::{ScreenId, ScreenIdentity};

fn on(screen: impl Into<ScreenIdentity>) -> AppState {
    let mut state = AppState::test_fixture();
    state.restore_navigation_root(screen);
    state
}

fn issues() -> AppState {
    let mut state = on(ScreenId::Issues);
    state.issues_state.active = true;
    state
}

fn composing(state: &mut AppState) {
    state.issues_state.inline_state = InlineState::Composer {
        target: ComposerTarget::NewComment,
        text: "half a thought".to_owned(),
        cursor: 0,
    };
}

#[test]
fn a_screen_with_nothing_open_reports_no_layers() {
    assert!(issues().open_back_layers().is_empty());
}

#[test]
fn back_from_the_home_screen_with_nothing_open_does_nothing() {
    assert_eq!(
        on(crate::workbench::DASHBOARD_IDENTITY).back_resolution(),
        BackResolution::Nothing
    );
}

#[test]
fn back_from_another_screen_with_nothing_open_leaves_it() {
    assert_eq!(issues().back_resolution(), BackResolution::Leave);
}

#[test]
fn a_focused_detail_panel_is_unwound_before_the_screen_is_left() {
    let mut state = issues();
    state.issues_state.issue_focus = IssueFocus::IssueDetail;
    assert_eq!(state.open_back_layers(), vec![BackLayer::PanelTransient]);
    assert_eq!(
        state.back_resolution(),
        BackResolution::Local(LocalIntent::ClearPanelTransient)
    );
}

#[test]
fn an_open_composer_is_unwound_before_the_focused_panel() {
    let mut state = issues();
    state.issues_state.issue_focus = IssueFocus::IssueDetail;
    composing(&mut state);
    assert_eq!(
        state.back_resolution(),
        BackResolution::Local(LocalIntent::CloseEditor)
    );
}

#[test]
fn an_open_chooser_is_unwound_before_the_composer() {
    let mut state = issues();
    composing(&mut state);
    state.issues_state.agent_chooser = Some(AgentChooserState::default());
    assert_eq!(
        state.back_resolution(),
        BackResolution::Local(LocalIntent::CloseChooser)
    );
}

#[test]
fn a_focused_search_is_unwound_before_open_filter_controls() {
    let mut state = issues();
    state.issues_state.search_input_focused = true;
    state.issues_state.filter_ui.controls_open = true;
    assert_eq!(
        state.back_resolution(),
        BackResolution::Local(LocalIntent::CloseSearch)
    );
}

#[test]
fn open_filter_controls_are_unwound_before_the_screen_is_left() {
    let mut state = issues();
    state.issues_state.filter_ui.controls_open = true;
    assert_eq!(
        state.back_resolution(),
        BackResolution::Local(LocalIntent::CloseFilterControls)
    );
}

#[test]
fn the_dirty_guard_outranks_everything_but_a_host_confirmation() {
    let mut state = issues();
    composing(&mut state);
    state.issues_state.agent_chooser = Some(AgentChooserState::default());
    let _ = state.mark_screen_dirty(
        DraftToken::next(),
        SaveIntent::Unavailable {
            reason: "an unsent draft has nowhere to save to",
        },
    );
    let _ = state.leave_screen();

    assert_eq!(
        state.back_resolution(),
        BackResolution::Local(LocalIntent::ResolveDirty(DirtyChoice::Cancel)),
        "the user is being asked about unsaved work, so Back answers that"
    );
}

#[test]
fn a_host_confirmation_outranks_the_dirty_guard() {
    let mut state = issues();
    let _ = state.mark_screen_dirty(
        DraftToken::next(),
        SaveIntent::Unavailable { reason: "nowhere" },
    );
    let _ = state.leave_screen();
    assert!(
        state.open_confirmation_payload(ConfirmationRequest::DeleteRepository {
            id: RepositoryId("repo".to_owned()),
        })
    );

    assert_eq!(
        state.back_resolution(),
        BackResolution::Local(LocalIntent::CloseHostConfirmation)
    );
}

#[test]
fn a_host_confirmation_is_not_also_counted_as_a_plain_overlay() {
    let mut state = issues();
    assert!(
        state.open_confirmation_payload(ConfirmationRequest::DeleteRepository {
            id: RepositoryId("repo".to_owned()),
        })
    );
    assert_eq!(state.open_back_layers(), vec![BackLayer::HostConfirmation]);
}

#[test]
fn a_plain_overlay_is_unwound_before_the_screen_is_left() {
    let mut state = issues();
    state.nav.current_mut().overlays_mut().open_help();
    assert_eq!(
        state.back_resolution(),
        BackResolution::Local(LocalIntent::CloseOverlay)
    );
}

#[test]
fn a_dirty_screen_with_no_guard_up_still_leaves_normally() {
    // Marking a draft does not by itself trap the user; the guard only appears
    // once something actually tries to leave.
    let mut state = issues();
    let _ = state.mark_screen_dirty(
        DraftToken::next(),
        SaveIntent::Unavailable { reason: "nowhere" },
    );
    assert_eq!(state.back_resolution(), BackResolution::Leave);
}

#[test]
fn typed_back_applies_exactly_one_resolved_layer_per_transition() {
    use crate::state::transition::TransitionExt;

    let mut state = issues();
    let screen = state.screen();
    let _ = state.mark_screen_dirty(
        DraftToken::next(),
        SaveIntent::Unavailable { reason: "nowhere" },
    );
    let _ = state.leave_screen();
    assert!(
        state.open_confirmation_payload(ConfirmationRequest::DeleteRepository {
            id: RepositoryId("repo".to_owned()),
        })
    );

    let state = state.apply(super::AppEvent::Back).committed_pure();
    assert!(matches!(state.modal, ModalState::None));
    assert!(
        state.nav.guard().is_some(),
        "Back must not also dismiss the guard"
    );
    assert_eq!(state.screen(), screen);

    let state = state.apply(super::AppEvent::Back).committed_pure();
    assert!(state.nav.guard().is_none());
    assert_eq!(
        state.screen(),
        screen,
        "dirty Cancel must preserve navigation"
    );
}

#[test]
fn shared_back_applies_each_issues_owner_without_falling_through() {
    use crate::state::transition::TransitionExt;

    let mut state = issues();
    state.issues_state.issue_focus = IssueFocus::IssueDetail;
    state.issues_state.filter_ui.controls_open = true;
    state.issues_state.committed_filter.author = "octocat".to_owned();
    state.issues_state.search_input_focused = true;
    state.issues_state.search_query = "typed query".to_owned();
    composing(&mut state);
    state.issues_state.agent_chooser = Some(AgentChooserState::default());

    let state = state.apply(super::AppEvent::Back).committed_pure();
    assert!(state.issues_state.agent_chooser.is_none());
    assert_ne!(state.issues_state.inline_state, InlineState::None);

    let state = state.apply(super::AppEvent::Back).committed_pure();
    assert_eq!(state.issues_state.inline_state, InlineState::None);
    assert!(state.issues_state.search_input_focused);

    let state = state.apply(super::AppEvent::Back).committed_pure();
    assert!(state.issues_state.search_query.is_empty());
    assert!(state.issues_state.search_input_focused);

    let state = state.apply(super::AppEvent::Back).committed_pure();
    assert!(!state.issues_state.search_input_focused);
    assert!(state.issues_state.filter_ui.controls_open);

    let state = state.apply(super::AppEvent::Back).committed_pure();
    assert!(!state.issues_state.filter_ui.controls_open);
    assert_eq!(state.issues_state.committed_filter.author, "octocat");
    assert_eq!(state.issues_state.issue_focus, IssueFocus::IssueDetail);

    let state = state.apply(super::AppEvent::Back).committed_pure();
    assert_eq!(state.issues_state.issue_focus, IssueFocus::IssueList);
    assert_eq!(state.screen(), ScreenId::Issues);

    let state = state.apply(super::AppEvent::Back).committed_pure();
    assert_eq!(state.screen(), crate::workbench::DASHBOARD_IDENTITY);
}

#[test]
fn dirty_interception_does_not_finalize_a_compiled_screen() {
    use crate::state::transition::TransitionExt;

    let mut state = issues();
    let _ = state.mark_screen_dirty(
        DraftToken::next(),
        SaveIntent::Unavailable { reason: "nowhere" },
    );

    let state = state.apply(super::AppEvent::Back).committed_pure();
    assert!(state.nav.guard().is_some());
    assert_eq!(state.screen(), ScreenId::Issues);
    assert!(state.issues_state.active);
}

#[test]
fn repositories_back_preserves_one_transition_exit_while_grabbing() {
    use crate::state::transition::TransitionExt;

    let mut state = on(crate::workbench::REPOSITORIES_IDENTITY);
    state.split_grab_index = Some(0);

    let state = state.apply(super::AppEvent::Back).committed_pure();
    assert!(state.split_grab_index.is_none());
    assert_eq!(state.screen(), crate::workbench::DASHBOARD_IDENTITY);
}

#[test]
fn a_mode_the_user_left_cannot_change_what_back_does_here() {
    // Issues keeps its composer, chooser, search, and filter state after the
    // user moves on. None of it belongs to the screen they are now looking at.
    let mut state = on(ScreenId::PullRequests);
    state.issues_state.active = true;
    composing(&mut state);
    state.issues_state.agent_chooser = Some(AgentChooserState::default());
    state.issues_state.search_input_focused = true;
    state.issues_state.filter_ui.controls_open = true;

    assert!(
        state.open_back_layers().is_empty(),
        "stale issues state leaked into pull requests: {:?}",
        state.open_back_layers()
    );
    assert_eq!(state.back_resolution(), BackResolution::Leave);
}

#[test]
fn leaving_a_screen_makes_its_in_flight_work_unanswerable() {
    // Navigation decides which work is still wanted. Advancing the counters
    // alone would not be enough: a pending record carries the correlation it
    // was registered with, so its own completion would still match it exactly.
    use crate::domain::effects::{Effect, EffectFamily, RetryPolicy, SemanticKey, TimerEffect};

    let mut state = issues();
    let (screen_generation, activation_generation) = state.nav.live_generations();
    state.pending_effects.screen_generation = screen_generation;
    state.pending_effects.activation_generation = activation_generation;
    let owner = crate::domain::Id::parse("github.issues")
        .unwrap_or_else(|_| unreachable!("valid identifier"));
    let key = SemanticKey::new(EffectFamily::Timer, "issue-refresh");
    let Ok(correlation) = state.register_pending_effect(
        owner,
        key,
        Effect::Timer(TimerEffect::Wakeup { after_ms: 10 }),
        RetryPolicy::Never,
    ) else {
        panic!("the pending store has room");
    };
    assert_eq!(state.pending_effects.len(), 1);

    let _ = state.enter_screen(ScreenId::PullRequests);

    assert_eq!(
        state.pending_effects.len(),
        0,
        "work started on the screen the session left must not still be pending"
    );
    assert_eq!(
        state.apply_effect_completion(&correlation),
        crate::state::transition::CompletionOutcome::StaleIgnored,
        "its own completion must no longer be applied to whatever replaced it"
    );
}

#[test]
fn every_layer_is_reachable_from_some_real_screen_state() {
    // A layer nothing can open is a layer the precedence cannot be trusted
    // about, so each one has to be produced by an actual state.
    let mut produced: Vec<BackLayer> = Vec::new();

    let mut confirmation = issues();
    assert!(
        confirmation.open_confirmation_payload(ConfirmationRequest::DeleteRepository {
            id: RepositoryId("repo".to_owned()),
        })
    );
    produced.extend(confirmation.open_back_layers());

    let mut guarded = issues();
    let _ = guarded.mark_screen_dirty(
        DraftToken::next(),
        SaveIntent::Unavailable { reason: "nowhere" },
    );
    let _ = guarded.leave_screen();
    produced.extend(guarded.open_back_layers());

    let mut chooser = issues();
    chooser.issues_state.agent_chooser = Some(AgentChooserState::default());
    produced.extend(chooser.open_back_layers());

    let mut editor = issues();
    composing(&mut editor);
    produced.extend(editor.open_back_layers());

    let mut search = issues();
    search.issues_state.search_input_focused = true;
    produced.extend(search.open_back_layers());

    let mut filter = issues();
    filter.issues_state.filter_ui.controls_open = true;
    produced.extend(filter.open_back_layers());

    let mut overlay = issues();
    overlay.nav.current_mut().overlays_mut().open_help();
    produced.extend(overlay.open_back_layers());

    let mut transient = issues();
    transient.issues_state.issue_focus = IssueFocus::IssueDetail;
    produced.extend(transient.open_back_layers());

    for layer in BackLayer::PRECEDENCE {
        assert!(
            produced.contains(&layer),
            "no real screen state produces {layer:?}"
        );
    }
}

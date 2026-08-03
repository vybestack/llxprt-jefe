//! What Back does from a real screen (issue #386, CW06-04).

use super::navigation::NavState;
use super::navigation_dirty::{DirtyChoice, DraftToken, SaveIntent};
use super::navigation_unwind::{BackLayer, BackResolution, LocalIntent};
use super::types::{
    AgentChooserState, AppState, ComposerTarget, ConfirmFocus, InlineState, IssueFocus, ModalState,
};
use crate::domain::RepositoryId;
use crate::workbench::ScreenId;

fn on(screen: ScreenId) -> AppState {
    AppState {
        nav: NavState::rooted(screen),
        ..AppState::default()
    }
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
        on(ScreenId::Dashboard).back_resolution(),
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
        BackResolution::Local(LocalIntent::ClearFilter)
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
    state.modal = ModalState::ConfirmDeleteRepository {
        id: RepositoryId("repo".to_owned()),
        confirm_focus: ConfirmFocus::Cancel,
    };

    assert_eq!(
        state.back_resolution(),
        BackResolution::Local(LocalIntent::CloseHostConfirmation)
    );
}

#[test]
fn a_host_confirmation_is_not_also_counted_as_a_plain_overlay() {
    let mut state = issues();
    state.modal = ModalState::ConfirmDeleteRepository {
        id: RepositoryId("repo".to_owned()),
        confirm_focus: ConfirmFocus::Cancel,
    };
    assert_eq!(state.open_back_layers(), vec![BackLayer::HostConfirmation]);
}

#[test]
fn a_plain_overlay_is_unwound_before_the_screen_is_left() {
    let mut state = issues();
    state.modal = ModalState::Help;
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
fn every_layer_is_reachable_from_some_real_screen_state() {
    // A layer nothing can open is a layer the precedence cannot be trusted
    // about, so each one has to be produced by an actual state.
    let mut produced: Vec<BackLayer> = Vec::new();

    let mut confirmation = issues();
    confirmation.modal = ModalState::ConfirmDeleteRepository {
        id: RepositoryId("repo".to_owned()),
        confirm_focus: ConfirmFocus::Cancel,
    };
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
    overlay.modal = ModalState::Help;
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

//! Esc-key precedence reducer tests for issues mode: inline editors, agent
//! chooser, search, filter controls, exit, and inline exclusivity.

use crate::state::AppState;
use crate::state::events::AppEvent;
use crate::state::transition::TransitionExt;
use crate::state::types::{AgentChooserState, ComposerTarget, EditorTarget, InlineState, ScreenId};

fn dashboard_issues_state() -> AppState {
        let mut state = AppState::test_fixture();
    state.nav = crate::state::navigation::NavState::rooted(ScreenId::Issues);
    state
}

/// Test 17: InlineCancelOrEsc clears inline editor state.
/// @plan PLAN-20260329-ISSUES-MODE.P04
/// @requirement REQ-ISS-010
/// @pseudocode component-001 lines 135-140
#[test]
fn test_esc_cancels_inline_editor() {
    let mut state = dashboard_issues_state();
    state.issues_state.inline_state = InlineState::Editor {
        target: EditorTarget::IssueBody,
        text: "draft content".to_string(),
        cursor: 5,
    };

    let new_state = state.apply(AppEvent::InlineCancelOrEsc).committed_pure();
    assert_eq!(new_state.issues_state.inline_state, InlineState::None);
}

/// Test 18: AgentChooserCancel clears agent chooser state.
/// @plan PLAN-20260329-ISSUES-MODE.P04
/// @requirement REQ-ISS-011
/// @pseudocode component-001 lines 141-145
#[test]
fn test_esc_cancels_agent_chooser() {
    let mut state = dashboard_issues_state();
    state.issues_state.agent_chooser = Some(AgentChooserState::default());
    state.issues_state.inline_state = InlineState::None;

    let new_state = state.apply(AppEvent::AgentChooserCancel).committed_pure();
    assert!(new_state.issues_state.agent_chooser.is_none());
}

/// Test 19: ClearSearch clears non-empty search query.
/// @plan PLAN-20260329-ISSUES-MODE.P04
/// @requirement REQ-ISS-007
/// @pseudocode component-001 lines 146-150
#[test]
fn test_esc_clears_nonempty_search() {
    let mut state = dashboard_issues_state();
    state.issues_state.search_input_focused = true;
    state.issues_state.search_query = "bug".to_string();
    state.issues_state.inline_state = InlineState::None;
    state.issues_state.agent_chooser = None;

    let new_state = state.apply(AppEvent::ClearSearch).committed_pure();
    assert!(new_state.issues_state.search_query.is_empty());
    assert!(new_state.issues_state.search_input_focused);
}

/// Test 20: BlurSearchInput blurs empty search input.
/// @plan PLAN-20260329-ISSUES-MODE.P04
/// @requirement REQ-ISS-007
/// @pseudocode component-001 lines 151-155
#[test]
fn test_esc_blurs_empty_search() {
    let mut state = dashboard_issues_state();
    state.issues_state.search_input_focused = true;
    state.issues_state.search_query = String::new();

    let new_state = state.apply(AppEvent::BlurSearchInput).committed_pure();
    assert!(!new_state.issues_state.search_input_focused);
}

/// Test 21: CloseFilterControls closes filter controls.
/// @plan PLAN-20260329-ISSUES-MODE.P04
/// @requirement REQ-ISS-008
/// @pseudocode component-001 lines 156-160
#[test]
fn test_esc_closes_filter_controls() {
    let mut state = dashboard_issues_state();
    state.issues_state.filter_ui.controls_open = true;

    let new_state = state.apply(AppEvent::CloseFilterControls).committed_pure();
    assert!(!new_state.issues_state.filter_ui.controls_open);
}

/// Test 22: ExitIssuesMode when no inner controls are active.
/// @plan PLAN-20260329-ISSUES-MODE.P04
/// @requirement REQ-ISS-001
/// @pseudocode component-001 lines 161-165
#[test]
fn test_esc_exits_issues_mode() {
    let mut state = dashboard_issues_state();
    state.issues_state.active = true;
    state.issues_state.inline_state = InlineState::None;
    state.issues_state.agent_chooser = None;
    state.issues_state.filter_ui.controls_open = false;
    state.issues_state.search_input_focused = false;

    let new_state = state.apply(AppEvent::ExitIssuesMode).committed_pure();
    assert_eq!(new_state.screen(), ScreenId::Dashboard);
}

/// Test 23: OpenInlineEditor is blocked when another inline control is active.
/// @plan PLAN-20260329-ISSUES-MODE.P04
/// @requirement REQ-ISS-010
/// @pseudocode component-001 lines 170-175
#[test]
fn test_inline_exclusivity_blocks_second_control() {
    let mut state = dashboard_issues_state();

    // Set active Composer
    state.issues_state.inline_state = InlineState::Composer {
        target: ComposerTarget::NewComment,
        text: "hello".to_string(),
        cursor: 5,
    };

    // Try to open Editor while Composer is active
    let new_state = state
        .apply(AppEvent::OpenInlineEditor {
            target: EditorTarget::IssueBody,
        })
        .committed_pure();

    // Should still be Composer, not changed to Editor
    assert!(
        matches!(
            &new_state.issues_state.inline_state,
            InlineState::Composer {
                target: ComposerTarget::NewComment,
                ..
            }
        ),
        "Expected Composer state to remain, but got {:?}",
        new_state.issues_state.inline_state
    );
}

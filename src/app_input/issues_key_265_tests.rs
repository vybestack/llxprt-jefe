//! Issue #265: Linux keyboard behavior key-routing tests.
//!
//! Extracted from `issues_key_tests.rs` to keep that file under the
//! source-size hard limit. Compiled as a submodule via
//! `#[path = "..."] mod ...;`, so `use super::*;` re-imports the parent
//! module's helpers (`issues_state_with_inline`, `issues_state_with_focus`,
//! `key`, `key_with_mods`, `resolve_issues_key_event`).

use super::*;

/// Alt+Enter when inline active dispatches InlineSubmit — the advertised
/// terminal-portable submit key (issue #265).
#[test]
fn test_alt_enter_submits_inline() {
    let state = issues_state_with_inline(InlineState::Composer {
        target: ComposerTarget::NewComment,
        text: String::from("hello"),
        cursor: 5,
    });
    let event = resolve_issues_key_event(&state, &key_with_mods(KeyCode::Enter, KeyModifiers::ALT));
    assert!(
        matches!(event, Some(AppEvent::InlineSubmit)),
        "Alt+Enter must dispatch InlineSubmit, got {event:?}"
    );
}

/// Bare Enter (no modifiers) when inline active dispatches InlineNewline —
/// never submit (issue #265).
#[test]
fn test_bare_enter_inserts_newline_not_submit() {
    let state = issues_state_with_inline(InlineState::Composer {
        target: ComposerTarget::NewComment,
        text: String::from("hello"),
        cursor: 5,
    });
    let event = resolve_issues_key_event(&state, &key(KeyCode::Enter));
    assert!(
        matches!(event, Some(AppEvent::InlineNewline)),
        "bare Enter must dispatch InlineNewline, got {event:?}"
    );
}

/// Shift+S (`KeyCode::Char('S')`) from IssueDetail with no global agents
/// still dispatches `OpenAgentChooser` — the input layer must always
/// express intent; the reducer decides eligibility and surfaces
/// `No agents available` when no eligible agent exists (issue #265).
///
/// The fixture is made explicit: `state.agents` is cleared and asserted
/// empty so the "no agents available" precondition is not merely an
/// accident of `AppState::default()`.
///
/// @plan PLAN-20260329-ISSUES-MODE.P10
/// @requirement REQ-ISS-011
/// @pseudocode component-003 lines 102-111
#[test]
fn shift_s_no_global_agents_still_dispatches_open_agent_chooser() {
    let mut state = issues_state_with_focus(IssueFocus::IssueDetail);
    // Explicitly guarantee the "no global agents" precondition instead of
    // relying on AppState::default() happening to produce an empty vec.
    state.agents.clear();
    assert!(
        state.agents.is_empty(),
        "fixture must start with no global agents"
    );

    // Shift+S is delivered as KeyCode::Char('S') (uppercase) with no
    // modifier bit; the input layer must still emit the intent.
    let event = resolve_issues_key_event(&state, &key(KeyCode::Char('S')));
    assert!(
        matches!(event, Some(AppEvent::OpenAgentChooser)),
        "Shift+S must always dispatch OpenAgentChooser even with no agents, got {event:?}"
    );
}

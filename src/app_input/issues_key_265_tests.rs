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

/// `S` from IssueDetail with no global agents still dispatches
/// `OpenAgentChooser` — the input layer must always express intent; the
/// reducer decides eligibility and surfaces `No agents available` when no
/// eligible agent exists (issue #265).
///
/// @plan PLAN-20260329-ISSUES-MODE.P10
/// @requirement REQ-ISS-011
/// @pseudocode component-003 lines 102-111
#[test]
fn test_s_no_agents_still_dispatches_open_agent_chooser() {
    let state = issues_state_with_focus(IssueFocus::IssueDetail);
    let event = resolve_issues_key_event(&state, &key(KeyCode::Char('S')));
    assert!(
        matches!(event, Some(AppEvent::OpenAgentChooser)),
        "S must always dispatch OpenAgentChooser even with no agents, got {event:?}"
    );
}

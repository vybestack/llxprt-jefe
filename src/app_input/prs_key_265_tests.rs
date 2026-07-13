//! Issue #265: Linux keyboard behavior tests for PR inline input.

use super::*;

#[test]
fn test_alt_enter_submits_pr_inline() {
    let state = prs_state_with_inline(InlineState::Composer {
        target: ComposerTarget::NewComment,
        text: String::from("hello"),
        cursor: 5,
    });
    let event = resolve_prs_key_event(&state, &key_with_mods(KeyCode::Enter, KeyModifiers::ALT));
    assert!(matches!(event, Some(AppEvent::PrInlineSubmit)));
}

#[test]
fn test_ctrl_enter_submits_pr_inline() {
    let state = prs_state_with_inline(InlineState::Composer {
        target: ComposerTarget::NewComment,
        text: String::from("hello"),
        cursor: 5,
    });
    let event = resolve_prs_key_event(
        &state,
        &key_with_mods(KeyCode::Enter, KeyModifiers::CONTROL),
    );
    assert!(matches!(event, Some(AppEvent::PrInlineSubmit)));
}

#[test]
fn test_bare_enter_inserts_pr_newline_not_submit() {
    let state = prs_state_with_inline(InlineState::Composer {
        target: ComposerTarget::NewComment,
        text: String::from("hello"),
        cursor: 5,
    });
    let event = resolve_prs_key_event(&state, &key(KeyCode::Enter));
    assert!(matches!(event, Some(AppEvent::PrInlineNewline)));
}

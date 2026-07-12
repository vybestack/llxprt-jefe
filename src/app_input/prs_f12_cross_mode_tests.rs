//! F12 mode-aware behavior + cross-mode `i` navigation in PR mode (issue #164).
//!
//! Extracted from `prs_key_tests.rs` to keep file sizes under the project's
//! source-file-size hard limit. These tests exercise the pure
//! `resolve_prs_key_event` resolver — they assert which `AppEvent` (if any)
//! a given key produces for a given PR-mode state.

use super::*;

// ═══════════════════════════════════════════════════════════════════════
// F12 mode-aware behavior + cross-mode `i` (issue #164)
// ═══════════════════════════════════════════════════════════════════════

/// F12 in PrDetail focus returns to the PR list (issue #164).
#[test]
fn f12_in_pr_detail_returns_to_list() {
    let state = prs_state_with_focus(PrFocus::PrDetail);
    let event = resolve_prs_key_event(&state, &key(KeyCode::F(12)));
    assert!(
        matches!(event, Some(AppEvent::RefocusPrList)),
        "F12 in PrDetail must yield RefocusPrList, got {event:?}"
    );
}

/// F12 at the PR list with the terminal unfocused is a no-op (issue #164).
#[test]
fn f12_in_pr_list_is_noop() {
    let mut state = prs_base_state();
    state.terminal_focused = false;
    let event = resolve_prs_key_event(&state, &key(KeyCode::F(12)));
    assert!(
        event.is_none(),
        "F12 at PrList (terminal unfocused) must be None, got {event:?}"
    );
}

/// F12 while the terminal is focused defocuses it (issue #164).
#[test]
fn f12_while_terminal_focused_defocuses() {
    let mut state = prs_base_state();
    state.terminal_focused = true;
    let event = resolve_prs_key_event(&state, &key(KeyCode::F(12)));
    assert!(
        matches!(event, Some(AppEvent::ToggleTerminalFocus)),
        "F12 with terminal focused must yield ToggleTerminalFocus, got {event:?}"
    );
}

/// F12 does not fire when the inline composer is open (overlay owns the key).
#[test]
fn f12_does_not_fire_when_inline_composer_open() {
    let state = prs_state_with_inline(InlineState::Composer {
        target: ComposerTarget::NewComment,
        text: String::new(),
        cursor: 0,
    });
    let event = resolve_prs_key_event(&state, &key(KeyCode::F(12)));
    assert!(
        event.is_none(),
        "F12 must be suppressed by inline composer, got {event:?}"
    );
}

/// `i` from PR mode enters Issues mode (issue #164 cross-mode navigation).
#[test]
fn i_from_prs_enters_issues_mode() {
    let state = prs_base_state();
    let event = resolve_prs_key_event(&state, &key(KeyCode::Char('i')));
    assert!(
        matches!(event, Some(AppEvent::EnterIssuesMode)),
        "'i' from PRs must yield EnterIssuesMode, got {event:?}"
    );
}

/// `p` from PrDetail still refocuses the PR list (regression, issue #164).
#[test]
fn p_from_prs_still_refocuses_list() {
    let state = prs_state_with_focus(PrFocus::PrDetail);
    let event = resolve_prs_key_event(&state, &key(KeyCode::Char('p')));
    assert!(
        matches!(event, Some(AppEvent::RefocusPrList)),
        "'p' in PrDetail must yield RefocusPrList, got {event:?}"
    );
}

// ─── Overlay precedence for cross-mode keys (issue #164 review Finding 4) ──

/// F12 while the terminal is focused AND in PrDetail defocuses the terminal
/// first (one-layer-at-a-time). The detail view stays — only the terminal
/// defocus wins.
#[test]
fn f12_while_terminal_focused_and_in_detail_defocuses_terminal_first() {
    let mut state = prs_state_with_focus(PrFocus::PrDetail);
    state.terminal_focused = true;
    let event = resolve_prs_key_event(&state, &key(KeyCode::F(12)));
    assert!(
        matches!(event, Some(AppEvent::ToggleTerminalFocus)),
        "F12 with terminal focused must yield ToggleTerminalFocus even in PrDetail, got {event:?}"
    );
}

/// `i` while the search input is focused types into the query — it must NOT
/// switch to Issues mode (overlay owns the key before the global tier).
#[test]
fn i_in_search_input_does_not_switch_modes() {
    let state = prs_state_with_search_focused();
    let event = resolve_prs_key_event(&state, &key(KeyCode::Char('i')));
    assert!(
        matches!(event, Some(AppEvent::PrSetSearchQuery { .. })),
        "'i' with search focused must yield PrSetSearchQuery, got {event:?}"
    );
    assert!(
        !matches!(event, Some(AppEvent::EnterIssuesMode)),
        "'i' with search focused must NOT yield EnterIssuesMode"
    );
}

/// `i` while the inline composer is active types the character into the
/// composer — it must NOT switch to Issues mode.
#[test]
fn i_in_inline_composer_types_char() {
    let state = prs_state_with_inline(InlineState::Composer {
        target: ComposerTarget::NewComment,
        text: String::new(),
        cursor: 0,
    });
    let event = resolve_prs_key_event(&state, &key(KeyCode::Char('i')));
    assert!(
        matches!(event, Some(AppEvent::PrInlineChar('i'))),
        "'i' with inline composer must yield PrInlineChar('i'), got {event:?}"
    );
}

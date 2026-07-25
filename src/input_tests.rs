//! Unit tests for input key predicates and quit-sequence helpers.
//!
//! Extracted from `input.rs` to keep that module under the 1000-line hard limit
//! after Caps Lock Ctrl-C coverage (issue #333).

use super::*;
use iocraft::prelude::KeyEventKind;

fn key(code: KeyCode) -> KeyEvent {
    KeyEvent::new(KeyEventKind::Press, code)
}

fn key_mods(code: KeyCode, modifiers: KeyModifiers) -> KeyEvent {
    let mut event = KeyEvent::new(KeyEventKind::Press, code);
    event.modifiers = modifiers;
    event
}

/// Fixed base instant; tests pass `base + ms` so timing is deterministic.
fn at(base: Instant, millis: u64) -> Instant {
    base + Duration::from_millis(millis)
}

// ── is_quit_key ────────────────────────────────────────────────────────

#[test]
fn is_quit_key_accepts_ctrl_q() {
    assert!(is_quit_key(&key_mods(
        KeyCode::Char('q'),
        KeyModifiers::CONTROL
    )));
}

#[test]
fn is_quit_key_accepts_ctrl_q_under_caps_lock() {
    assert!(is_quit_key(&key_mods(
        KeyCode::Char('Q'),
        KeyModifiers::CONTROL
    )));
}

#[test]
fn is_quit_key_rejects_bare_q() {
    assert!(!is_quit_key(&key(KeyCode::Char('q'))));
}

#[test]
fn is_quit_key_rejects_ctrl_shift_q() {
    assert!(!is_quit_key(&key_mods(
        KeyCode::Char('Q'),
        KeyModifiers::CONTROL | KeyModifiers::SHIFT
    )));
}

#[test]
fn is_quit_key_rejects_ctrl_alt_q() {
    assert!(!is_quit_key(&key_mods(
        KeyCode::Char('q'),
        KeyModifiers::CONTROL | KeyModifiers::ALT
    )));
}

#[test]
fn is_quit_key_rejects_unrelated_keys() {
    assert!(!is_quit_key(&key(KeyCode::Enter)));
    assert!(!is_quit_key(&key(KeyCode::Char('a'))));
}

// Ctrl-C must NEVER quit jefe. jefe owns its quit policy (Ctrl-Q / rapid
// qqq) and forwards Ctrl-C to the embedded agent terminal so runtimes like
// Code Puppy can use it to kill running shells / cancel an agent run. The
// vendored iocraft terminal layer used to hardcode Ctrl-C as an exit
// signal and swallow the event before it reached the app; that interception
// was removed, so this guard is now the authoritative "Ctrl-C is not quit"
// contract. See issue #200.
#[test]
fn is_quit_key_rejects_ctrl_c() {
    assert!(!is_quit_key(&key_mods(
        KeyCode::Char('c'),
        KeyModifiers::CONTROL
    )));
}

// ── is_bare_ctrl_c / should_forward_ctrl_c_to_attached_terminal ────────
//
// Ctrl-C (byte 0x03) must reach the attached agent terminal to interrupt
// the agent's foreground shell / cancel a run (issue #200). These predicates
// drive the dashboard-level passthrough that fires regardless of pane focus
// so the interrupt survives the F12 toggle trap.

#[test]
fn is_bare_ctrl_c_accepts_ctrl_c() {
    assert!(is_bare_ctrl_c(&key_mods(
        KeyCode::Char('c'),
        KeyModifiers::CONTROL
    )));
}

#[test]
fn is_bare_ctrl_c_accepts_caps_lock_uppercase_c() {
    // Caps Lock yields Char('C') with CONTROL only (mirrors is_quit_key).
    assert!(is_bare_ctrl_c(&key_mods(
        KeyCode::Char('C'),
        KeyModifiers::CONTROL
    )));
}

#[test]
fn is_bare_ctrl_c_rejects_ctrl_shift_c_chord() {
    // Ctrl-Shift-C is a host copy shortcut on some platforms and must not
    // be treated as an interrupt.
    assert!(!is_bare_ctrl_c(&key_mods(
        KeyCode::Char('C'),
        KeyModifiers::CONTROL | KeyModifiers::SHIFT
    )));
    assert!(!is_bare_ctrl_c(&key_mods(
        KeyCode::Char('c'),
        KeyModifiers::CONTROL | KeyModifiers::SHIFT
    )));
}

#[test]
fn is_bare_ctrl_c_rejects_extra_modifiers() {
    assert!(!is_bare_ctrl_c(&key_mods(
        KeyCode::Char('c'),
        KeyModifiers::CONTROL | KeyModifiers::SHIFT
    )));
    assert!(!is_bare_ctrl_c(&key_mods(
        KeyCode::Char('c'),
        KeyModifiers::CONTROL | KeyModifiers::ALT
    )));
    assert!(!is_bare_ctrl_c(&key_mods(
        KeyCode::Char('C'),
        KeyModifiers::CONTROL | KeyModifiers::ALT
    )));
    assert!(!is_bare_ctrl_c(&key_mods(
        KeyCode::Char('c'),
        KeyModifiers::CONTROL | KeyModifiers::SUPER
    )));
    assert!(!is_bare_ctrl_c(&key_mods(
        KeyCode::Char('c'),
        KeyModifiers::CONTROL | KeyModifiers::META
    )));
}

#[test]
fn is_bare_ctrl_c_rejects_bare_c_and_other_keys() {
    assert!(!is_bare_ctrl_c(&key(KeyCode::Char('c'))));
    assert!(!is_bare_ctrl_c(&key(KeyCode::Enter)));
}

#[test]
fn ctrl_c_forward_requires_normal_mode_and_attached_terminal() {
    let ctrl_c = key_mods(KeyCode::Char('c'), KeyModifiers::CONTROL);

    // Happy path: plain dashboard + attached terminal.
    assert!(should_forward_ctrl_c_to_attached_terminal(
        &ctrl_c,
        InputMode::Normal,
        true
    ));

    // No terminal attached → never forward (nothing to forward to).
    assert!(!should_forward_ctrl_c_to_attached_terminal(
        &ctrl_c,
        InputMode::Normal,
        false
    ));

    // A modal/form/search/terminal-capture mode owns the key instead.
    assert!(!should_forward_ctrl_c_to_attached_terminal(
        &ctrl_c,
        InputMode::Form,
        true
    ));
    assert!(!should_forward_ctrl_c_to_attached_terminal(
        &ctrl_c,
        InputMode::Search,
        true
    ));
    assert!(!should_forward_ctrl_c_to_attached_terminal(
        &ctrl_c,
        InputMode::Confirm,
        true
    ));
    assert!(!should_forward_ctrl_c_to_attached_terminal(
        &ctrl_c,
        InputMode::TerminalCapture,
        true
    ));
}

#[test]
fn ctrl_c_forward_accepts_caps_lock_ctrl_c() {
    let caps_ctrl_c = key_mods(KeyCode::Char('C'), KeyModifiers::CONTROL);
    assert!(should_forward_ctrl_c_to_attached_terminal(
        &caps_ctrl_c,
        InputMode::Normal,
        true
    ));
}

#[test]
fn ctrl_c_forward_rejects_non_ctrl_c_keys() {
    // Even on the dashboard with a terminal attached, only Ctrl-C qualifies.
    assert!(!should_forward_ctrl_c_to_attached_terminal(
        &key(KeyCode::Char('c')),
        InputMode::Normal,
        true
    ));
    assert!(!should_forward_ctrl_c_to_attached_terminal(
        &key_mods(KeyCode::Char('x'), KeyModifiers::CONTROL),
        InputMode::Normal,
        true
    ));
}

// ── is_qqq_press ───────────────────────────────────────────────────────

#[test]
fn qqq_press_accepts_bare_q() {
    assert!(is_qqq_press(&key(KeyCode::Char('q'))));
}

#[test]
fn qqq_press_accepts_bare_q_under_caps_lock() {
    assert!(is_qqq_press(&key(KeyCode::Char('Q'))));
}

#[test]
fn qqq_press_rejects_any_modifier() {
    assert!(!is_qqq_press(&key_mods(
        KeyCode::Char('q'),
        KeyModifiers::SHIFT
    )));
    assert!(!is_qqq_press(&key_mods(
        KeyCode::Char('q'),
        KeyModifiers::CONTROL
    )));
    assert!(!is_qqq_press(&key_mods(
        KeyCode::Char('q'),
        KeyModifiers::ALT
    )));
}

#[test]
fn qqq_press_rejects_non_q() {
    assert!(!is_qqq_press(&key(KeyCode::Enter)));
    assert!(!is_qqq_press(&key(KeyCode::Char('a'))));
}

// ── observe_quit_sequence ──────────────────────────────────────────────

#[test]
fn ctrl_q_quits_immediately_from_idle() {
    let mut seq = QuitSequenceState::default();
    let base = Instant::now();
    assert_eq!(
        observe_quit_sequence(
            &mut seq,
            &key_mods(KeyCode::Char('q'), KeyModifiers::CONTROL),
            base
        ),
        QuitOutcome::Quit
    );
    assert_eq!(seq, QuitSequenceState::default());
}

#[test]
fn first_q_starts_sequence_without_quitting() {
    let mut seq = QuitSequenceState::default();
    let base = Instant::now();
    assert_eq!(
        observe_quit_sequence(&mut seq, &key(KeyCode::Char('q')), base),
        QuitOutcome::Continue
    );
    assert_eq!(seq.presses, 1);
    assert_eq!(seq.last_press, Some(base));
}

#[test]
fn two_rapid_qs_do_not_quit() {
    let mut seq = QuitSequenceState::default();
    let base = Instant::now();
    assert_eq!(
        observe_quit_sequence(&mut seq, &key(KeyCode::Char('q')), at(base, 0)),
        QuitOutcome::Continue
    );
    assert_eq!(
        observe_quit_sequence(&mut seq, &key(KeyCode::Char('q')), at(base, 100)),
        QuitOutcome::Continue
    );
    assert_eq!(seq.presses, 2);
}

#[test]
fn three_rapid_qs_quit_and_reset() {
    let mut seq = QuitSequenceState::default();
    let base = Instant::now();
    assert_eq!(
        observe_quit_sequence(&mut seq, &key(KeyCode::Char('q')), at(base, 0)),
        QuitOutcome::Continue
    );
    assert_eq!(
        observe_quit_sequence(&mut seq, &key(KeyCode::Char('q')), at(base, 100)),
        QuitOutcome::Continue
    );
    assert_eq!(
        observe_quit_sequence(&mut seq, &key(KeyCode::Char('q')), at(base, 200)),
        QuitOutcome::Quit
    );
    assert_eq!(seq, QuitSequenceState::default());
}

#[test]
fn slow_third_q_does_not_quit_and_restarts_count() {
    let mut seq = QuitSequenceState::default();
    let base = Instant::now();
    assert_eq!(
        observe_quit_sequence(&mut seq, &key(KeyCode::Char('q')), at(base, 0)),
        QuitOutcome::Continue
    );
    assert_eq!(
        observe_quit_sequence(&mut seq, &key(KeyCode::Char('q')), at(base, 100)),
        QuitOutcome::Continue
    );
    // Third q lands after the 1s window: the run restarts at 1, no quit.
    assert_eq!(
        observe_quit_sequence(&mut seq, &key(KeyCode::Char('q')), at(base, 1500)),
        QuitOutcome::Continue
    );
    assert_eq!(seq.presses, 1);
}

#[test]
fn q_at_exact_window_boundary_still_counts() {
    let mut seq = QuitSequenceState::default();
    let base = Instant::now();
    assert_eq!(
        observe_quit_sequence(&mut seq, &key(KeyCode::Char('q')), at(base, 0)),
        QuitOutcome::Continue
    );
    // The window is inclusive (`elapsed <= WINDOW`): a second `q` landing at
    // exactly 1000ms still counts toward the sequence rather than resetting.
    assert_eq!(
        observe_quit_sequence(&mut seq, &key(KeyCode::Char('q')), at(base, 1000)),
        QuitOutcome::Continue
    );
    assert_eq!(seq.presses, 2);
}

#[test]
fn non_q_key_resets_pending_sequence() {
    let mut seq = QuitSequenceState::default();
    let base = Instant::now();
    assert_eq!(
        observe_quit_sequence(&mut seq, &key(KeyCode::Char('q')), at(base, 0)),
        QuitOutcome::Continue
    );
    assert_eq!(
        observe_quit_sequence(&mut seq, &key(KeyCode::Char('q')), at(base, 100)),
        QuitOutcome::Continue
    );
    assert_eq!(
        observe_quit_sequence(&mut seq, &key(KeyCode::Enter), at(base, 150)),
        QuitOutcome::Reset
    );
    assert_eq!(seq, QuitSequenceState::default());
    // After reset, three fresh rapid q's are required to quit.
    assert_eq!(
        observe_quit_sequence(&mut seq, &key(KeyCode::Char('q')), at(base, 200)),
        QuitOutcome::Continue
    );
    assert_eq!(
        observe_quit_sequence(&mut seq, &key(KeyCode::Char('q')), at(base, 250)),
        QuitOutcome::Continue
    );
    assert_eq!(
        observe_quit_sequence(&mut seq, &key(KeyCode::Char('q')), at(base, 300)),
        QuitOutcome::Quit
    );
}

#[test]
fn ctrl_q_quits_even_mid_sequence() {
    let mut seq = QuitSequenceState::default();
    let base = Instant::now();
    assert_eq!(
        observe_quit_sequence(&mut seq, &key(KeyCode::Char('q')), at(base, 0)),
        QuitOutcome::Continue
    );
    assert_eq!(
        observe_quit_sequence(&mut seq, &key(KeyCode::Char('q')), at(base, 100)),
        QuitOutcome::Continue
    );
    assert_eq!(
        observe_quit_sequence(
            &mut seq,
            &key_mods(KeyCode::Char('q'), KeyModifiers::CONTROL),
            at(base, 150)
        ),
        QuitOutcome::Quit
    );
    assert_eq!(seq, QuitSequenceState::default());
}

// ── should_intercept_for_scrollback (issue #198) ───────────────────────
//
// The `kennel_mode` parameter gates whether Jefe's scrollback viewport
// intercepts scroll keys (issue #245). Kennel agents (Code Puppy) use
// Jefe's scrollback; non-kennel agents (llxprt) handle their own
// scrolling. The tests below use `kennel_mode = true` to preserve the
// existing #198 behavior. Separate tests (at the end) assert the
// non-kennel forwarding contract.

#[test]
fn scrollback_pageup_intercepts_from_follow_tail() {
    let evt = should_intercept_for_scrollback(&key(KeyCode::PageUp), false, true);
    assert!(matches!(evt, Some(AppEvent::TerminalScrollPageUp)));
}

#[test]
fn scrollback_pagedown_intercepts_from_follow_tail() {
    let evt = should_intercept_for_scrollback(&key(KeyCode::PageDown), false, true);
    assert!(matches!(evt, Some(AppEvent::TerminalScrollPageDown)));
}

#[test]
fn scrollback_pageup_intercepts_when_scrolled_back() {
    let evt = should_intercept_for_scrollback(&key(KeyCode::PageUp), true, true);
    assert!(matches!(evt, Some(AppEvent::TerminalScrollPageUp)));
}

#[test]
fn scrollback_pagedown_intercepts_when_scrolled_back() {
    let evt = should_intercept_for_scrollback(&key(KeyCode::PageDown), true, true);
    assert!(matches!(evt, Some(AppEvent::TerminalScrollPageDown)));
}

// ── Modifier chords go to the PTY ─────────────────────────

#[test]
fn scrollback_ctrl_end_forwards_to_pty() {
    let evt =
        should_intercept_for_scrollback(&key_mods(KeyCode::End, KeyModifiers::CONTROL), true, true);
    assert!(evt.is_none(), "Ctrl+End must be forwarded to the PTY");
}

#[test]
fn scrollback_alt_pageup_forwards_to_pty() {
    let evt =
        should_intercept_for_scrollback(&key_mods(KeyCode::PageUp, KeyModifiers::ALT), false, true);
    assert!(evt.is_none(), "Alt+PageUp must be forwarded to the PTY");
}

#[test]
fn scrollback_shift_pageup_forwards_to_pty() {
    let evt = should_intercept_for_scrollback(
        &key_mods(KeyCode::PageUp, KeyModifiers::SHIFT),
        false,
        true,
    );
    assert!(evt.is_none(), "Shift+PageUp must be forwarded to the PTY");
}

#[test]
fn scrollback_ctrl_pagedown_forwards_to_pty() {
    let evt = should_intercept_for_scrollback(
        &key_mods(KeyCode::PageDown, KeyModifiers::CONTROL),
        false,
        true,
    );
    assert!(evt.is_none(), "Ctrl+PageDown must be forwarded to the PTY");
}

#[test]
fn scrollback_ctrl_home_forwards_to_pty() {
    let evt = should_intercept_for_scrollback(
        &key_mods(KeyCode::Home, KeyModifiers::CONTROL),
        true,
        true,
    );
    assert!(evt.is_none(), "Ctrl+Home must be forwarded to the PTY");
}

#[test]
fn scrollback_ctrl_up_forwards_to_pty_even_when_scrolled() {
    let evt =
        should_intercept_for_scrollback(&key_mods(KeyCode::Up, KeyModifiers::CONTROL), true, true);
    assert!(
        evt.is_none(),
        "Ctrl+Up must be forwarded to the PTY even when scrolled"
    );
}

// ── End only intercepts when scrolled back ────────────────

#[test]
fn scrollback_end_at_follow_tail_forwards_to_pty() {
    let evt = should_intercept_for_scrollback(&key(KeyCode::End), false, true);
    assert!(
        evt.is_none(),
        "End at follow-tail must be forwarded to the PTY"
    );
}

#[test]
fn scrollback_end_while_scrolled_returns_follow_tail() {
    let evt = should_intercept_for_scrollback(&key(KeyCode::End), true, true);
    assert!(matches!(evt, Some(AppEvent::TerminalFollowTail)));
}

// ── Home intercepts from BOTH states ──────────────────────

#[test]
fn scrollback_home_intercepts_when_scrolled_back() {
    let evt = should_intercept_for_scrollback(&key(KeyCode::Home), true, true);
    assert!(
        matches!(evt, Some(AppEvent::TerminalScrollToTop)),
        "Home must map to TerminalScrollToTop (scroll to top of history)"
    );
}

#[test]
fn scrollback_home_intercepts_from_follow_tail() {
    let evt = should_intercept_for_scrollback(&key(KeyCode::Home), false, true);
    assert!(
        matches!(evt, Some(AppEvent::TerminalScrollToTop)),
        "Home must intercept from follow-tail too (enter scrollback from anywhere)"
    );
}

#[test]
fn scrollback_up_intercepts_when_scrolled_back() {
    let evt = should_intercept_for_scrollback(&key(KeyCode::Up), true, true);
    assert!(matches!(evt, Some(AppEvent::TerminalScrollUp)));
}

#[test]
fn scrollback_down_intercepts_when_scrolled_back() {
    let evt = should_intercept_for_scrollback(&key(KeyCode::Down), true, true);
    assert!(matches!(evt, Some(AppEvent::TerminalScrollDown)));
}

#[test]
fn scrollback_up_not_intercepted_when_following() {
    // When at follow-tail (offset None), Up goes to the PTY.
    let evt = should_intercept_for_scrollback(&key(KeyCode::Up), false, true);
    assert!(evt.is_none());
}

#[test]
fn scrollback_down_not_intercepted_when_following() {
    let evt = should_intercept_for_scrollback(&key(KeyCode::Down), false, true);
    assert!(evt.is_none());
}

#[test]
fn scrollback_regular_keys_not_intercepted() {
    // Regular character keys are never intercepted.
    assert!(should_intercept_for_scrollback(&key(KeyCode::Char('a')), true, true).is_none());
    assert!(should_intercept_for_scrollback(&key(KeyCode::Enter), true, true).is_none());
    assert!(should_intercept_for_scrollback(&key(KeyCode::Tab), true, true).is_none());
    assert!(should_intercept_for_scrollback(&key(KeyCode::Backspace), true, true).is_none());
}

#[test]
fn scrollback_left_right_not_intercepted() {
    // Left/Right go to the PTY even when scrolled back.
    assert!(should_intercept_for_scrollback(&key(KeyCode::Left), true, true).is_none());
    assert!(should_intercept_for_scrollback(&key(KeyCode::Right), true, true).is_none());
}

// ── Issue #245: non-kennel agents forward ALL scroll keys to the PTY ────
//
// When `kennel_mode == false` (llxprt), the helper must return `None`
// for every scroll key so the child TUI's native scrolling is not
// stolen by Jefe's scrollback viewport. This is true even when the
// viewport is scrolled back (`offset_is_some == true`).

#[test]
fn non_kennel_pageup_forwards_to_pty() {
    let evt = should_intercept_for_scrollback(&key(KeyCode::PageUp), true, false);
    assert!(evt.is_none(), "non-kennel PageUp must forward to the PTY");
}

#[test]
fn non_kennel_pagedown_forwards_to_pty() {
    let evt = should_intercept_for_scrollback(&key(KeyCode::PageDown), true, false);
    assert!(evt.is_none(), "non-kennel PageDown must forward to the PTY");
}

#[test]
fn non_kennel_home_forwards_to_pty() {
    let evt = should_intercept_for_scrollback(&key(KeyCode::Home), true, false);
    assert!(evt.is_none(), "non-kennel Home must forward to the PTY");
}

#[test]
fn non_kennel_end_forwards_to_pty_when_scrolled() {
    let evt = should_intercept_for_scrollback(&key(KeyCode::End), true, false);
    assert!(
        evt.is_none(),
        "non-kennel End must forward to the PTY even when scrolled back"
    );
}

#[test]
fn non_kennel_up_forwards_to_pty_when_scrolled() {
    let evt = should_intercept_for_scrollback(&key(KeyCode::Up), true, false);
    assert!(
        evt.is_none(),
        "non-kennel Up must forward to the PTY even when scrolled back"
    );
}

#[test]
fn non_kennel_down_forwards_to_pty_when_scrolled() {
    let evt = should_intercept_for_scrollback(&key(KeyCode::Down), true, false);
    assert!(
        evt.is_none(),
        "non-kennel Down must forward to the PTY even when scrolled back"
    );
}

#[test]
fn non_kennel_pageup_forwards_from_follow_tail() {
    let evt = should_intercept_for_scrollback(&key(KeyCode::PageUp), false, false);
    assert!(
        evt.is_none(),
        "non-kennel PageUp must forward from follow-tail"
    );
}

#[test]
fn non_kennel_pagedown_forwards_from_follow_tail() {
    let evt = should_intercept_for_scrollback(&key(KeyCode::PageDown), false, false);
    assert!(
        evt.is_none(),
        "non-kennel PageDown must forward from follow-tail"
    );
}

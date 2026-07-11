//! Input-mode and key-routing helpers.

use std::time::{Duration, Instant};

use iocraft::prelude::{KeyCode, KeyEvent, KeyModifiers};

use crate::state::{AppState, InlineState, ModalState, PaneFocus, QuitSequenceState, ScreenMode};

/// High-level mode used to route keyboard events.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InputMode {
    Normal,
    TerminalCapture,
    Help,
    Search,
    Form,
    Confirm,
    /// Theme picker overlay.
    ThemePicker,
    /// @plan PLAN-20260329-ISSUES-MODE.P03
    /// @requirement REQ-ISS-002
    IssuesNormal,
    /// @plan PLAN-20260329-ISSUES-MODE.P03
    /// @requirement REQ-ISS-002
    IssuesInline,
    /// @plan PLAN-20260329-ISSUES-MODE.P03
    /// @requirement REQ-ISS-002
    IssuesSearch,
    /// @plan PLAN-20260329-ISSUES-MODE.P03
    /// @requirement REQ-ISS-002
    IssuesFilter,
    /// @plan PLAN-20260329-ISSUES-MODE.P03
    /// @requirement REQ-ISS-002
    IssuesChooser,
    /// @plan PLAN-20260624-PR-MODE.P03
    /// @requirement REQ-PR-002
    PrsNormal,
    /// @plan PLAN-20260624-PR-MODE.P03
    /// @requirement REQ-PR-002
    PrsInline,
    /// @plan PLAN-20260624-PR-MODE.P03
    /// @requirement REQ-PR-002
    PrsSearch,
    /// @plan PLAN-20260624-PR-MODE.P03
    /// @requirement REQ-PR-002
    PrsFilter,
    /// @plan PLAN-20260624-PR-MODE.P03
    /// @requirement REQ-PR-002
    PrsChooser,
    ActionsNormal,
    ActionsFilter,
    ActionsSearch,
}

/// Search-mode key routing result.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SearchKeyRoute {
    CloseAndConsume,
    EditQueryChar(char),
    Backspace,
    CloseAndReroute,
    Ignore,
}

/// Resolve the active input mode from current app state.
#[must_use]
pub fn input_mode_for_state(state: &AppState) -> InputMode {
    match state.modal {
        ModalState::Help => return InputMode::Help,
        ModalState::Search { .. } => return InputMode::Search,
        ModalState::ThemePicker { .. } => return InputMode::ThemePicker,
        ModalState::NewRepository { .. }
        | ModalState::EditRepository { .. }
        | ModalState::NewAgent { .. }
        | ModalState::EditAgent { .. }
        | ModalState::WorkflowDispatch { .. } => return InputMode::Form,
        ModalState::ConfirmDeleteRepository { .. }
        | ModalState::ConfirmDeleteAgent { .. }
        | ModalState::ConfirmKillAgent { .. }
        | ModalState::PreflightPrompt { .. }
        | ModalState::ConfirmIssueDirtyCopy { .. } => return InputMode::Confirm,
        ModalState::None => {}
    }

    // Issues mode detection — must be before Normal fallback
    // @plan PLAN-20260329-ISSUES-MODE.P03
    // @requirement REQ-ISS-002
    // @pseudocode component-003 lines 01-02
    if state.screen_mode == ScreenMode::DashboardIssues {
        if state.issues_state.inline_state != InlineState::None {
            return InputMode::IssuesInline;
        }
        if state.issues_state.agent_chooser.is_some() {
            return InputMode::IssuesChooser;
        }
        if state.issues_state.search_input_focused {
            return InputMode::IssuesSearch;
        }
        if state.issues_state.filter_ui.controls_open {
            return InputMode::IssuesFilter;
        }
        return InputMode::IssuesNormal;
    }

    // PR mode detection — real precedence routing (Inline > Chooser > Search >
    // Filter > Normal), mirroring the DashboardIssues block above.
    // @plan PLAN-20260624-PR-MODE.P11
    // @requirement REQ-PR-002
    // @requirement REQ-PR-004
    // @pseudocode component-003 lines 07,51
    if state.screen_mode == ScreenMode::DashboardPullRequests {
        if state.prs_state.inline_state != InlineState::None {
            return InputMode::PrsInline;
        }
        if state.prs_state.agent_chooser.is_some() {
            return InputMode::PrsChooser;
        }
        if state.prs_state.search_input_focused {
            return InputMode::PrsSearch;
        }
        if state.prs_state.filter_ui.controls_open {
            return InputMode::PrsFilter;
        }
        return InputMode::PrsNormal;
    }

    // Actions mode detection
    if state.screen_mode == ScreenMode::DashboardActions {
        if state.actions_state.ui.search_input_focused {
            return InputMode::ActionsSearch;
        }
        if state.actions_state.ui.filter_ui_open {
            return InputMode::ActionsFilter;
        }
        return InputMode::ActionsNormal;
    }

    if state.terminal_focused && state.pane_focus == PaneFocus::Terminal {
        InputMode::TerminalCapture
    } else {
        InputMode::Normal
    }
}

/// Whether a key event is a bare `Ctrl-C` (byte `0x03`).
///
/// Used by [`should_forward_ctrl_c_to_attached_terminal`] so the recognition
/// stays in one place. Shift/Alt/Super/Meta modifiers are excluded because they
/// change the meaning of the key (e.g. `Ctrl-Shift-C` is a host copy shortcut
/// on some platforms) and must not be treated as an interrupt.
#[must_use]
pub fn is_bare_ctrl_c(key: &KeyEvent) -> bool {
    key.code == KeyCode::Char('c') && key.modifiers == KeyModifiers::CONTROL
}

/// Whether `Ctrl-C` should be forwarded to the currently-attached agent
/// terminal even when the terminal pane is not in dedicated capture mode.
///
/// `Ctrl-C`'s only sensible meaning when an agent terminal is attached is
/// "interrupt the agent's foreground shell / cancel the run" (issue #200).
/// Routing it to the agent terminal regardless of pane focus makes the
/// interrupt reliable and side-steps the F12 toggle trap: creating/selecting
/// an agent auto-focuses the terminal, so a user pressing F12 (advertised as
/// "terminal focus") can inadvertently *unfocus* it, after which a raw
/// `TerminalCapture`-gated forward would silently drop `Ctrl-C` (issue #200).
///
/// The forward is constrained so it never fights an active modal/form/search:
/// only the plain dashboard (`Normal` mode) qualifies — when no overlay owns
/// the keystroke and an agent terminal is attached. The caller supplies
/// `has_attached_terminal` (from the runtime's `attached_agent()` probe).
#[must_use]
pub fn should_forward_ctrl_c_to_attached_terminal(
    key: &KeyEvent,
    input_mode: InputMode,
    has_attached_terminal: bool,
) -> bool {
    has_attached_terminal && input_mode == InputMode::Normal && is_bare_ctrl_c(key)
}

/// Route a key while search mode is active.
#[must_use]
pub fn route_search_key(key: &KeyEvent) -> SearchKeyRoute {
    match key.code {
        KeyCode::Esc | KeyCode::Enter => SearchKeyRoute::CloseAndConsume,
        KeyCode::Backspace => SearchKeyRoute::Backspace,
        KeyCode::Char(c)
            if !key.modifiers.intersects(
                iocraft::prelude::KeyModifiers::CONTROL | iocraft::prelude::KeyModifiers::ALT,
            ) =>
        {
            SearchKeyRoute::EditQueryChar(c)
        }
        KeyCode::Char(_) | KeyCode::Up | KeyCode::Down | KeyCode::Left | KeyCode::Right => {
            SearchKeyRoute::CloseAndReroute
        }
        _ => SearchKeyRoute::Ignore,
    }
}

// ──────────────────────────────────────────────────────────────────────────
// Quit policy: instant `Ctrl-Q` plus the rapid `qqq` sequence fallback.
//
// The quit key is a deliberate two-modifier-free chord (`Ctrl-Q`) so a stray
// keystroke can never drop unsent composer/inline text. As a terminal-portable
// fallback that preserves the `q` muscle memory, three rapid bare-`q` presses
// (`qqq`) within a short window also quit — guarding against terminals that
// swallow `Ctrl-Q` for XON/XOFF flow control.
// ──────────────────────────────────────────────────────────────────────────

/// Number of rapid `q` presses required to quit via the `qqq` sequence.
const QUIT_SEQUENCE_THRESHOLD: u8 = 3;
/// Window within which consecutive `q` presses count toward `qqq`.
const QUIT_SEQUENCE_WINDOW: Duration = Duration::from_secs(1);

/// Outcome of observing a key against the quit policy.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum QuitOutcome {
    /// The quit trigger fired (`Ctrl-Q`, or the rapid `qqq` sequence completed).
    Quit,
    /// A bare `q` was registered toward a `qqq` sequence; the key should be
    /// swallowed (consumed) but the app must not quit yet.
    Continue,
    /// An unrelated key arrived; the pending sequence resets and the key should
    /// be routed normally.
    Reset,
}

/// Returns `true` for the instant `Ctrl-Q` quit chord.
///
/// Accepts both `q` and `Q` so Caps Lock (which can make the terminal report an
/// uppercase glyph) still quits, while requiring the *only* modifier to be
/// `CONTROL` — excluding `Ctrl-Shift-Q` and `Ctrl-Alt-Q`.
#[must_use]
pub fn is_quit_key(key: &KeyEvent) -> bool {
    matches!(key.code, KeyCode::Char('q' | 'Q')) && key.modifiers == KeyModifiers::CONTROL
}

/// Returns `true` for a bare, unmodified `q`/`Q` — a single press toward the
/// `qqq` rapid-quit sequence.
///
/// Accepts both `q` and `Q` for Caps-Lock tolerance, mirroring [`is_quit_key`].
/// Any modifier (Shift, Ctrl, Alt, …) disqualifies it, so chords such as
/// `Ctrl-Q` or `Shift-Q` are never miscounted as sequence presses.
#[must_use]
pub fn is_qqq_press(key: &KeyEvent) -> bool {
    matches!(key.code, KeyCode::Char('q' | 'Q')) && key.modifiers == KeyModifiers::NONE
}

/// Observe a key against the quit policy, advancing the rapid-`qqq` sequence
/// state stored in `seq`.
///
/// - `Ctrl-Q` ([`is_quit_key`]) quits immediately and resets the sequence.
/// - A bare `q` ([`is_qqq_press`]) increments the counter when it lands within
///   [`QUIT_SEQUENCE_WINDOW`] of the previous `q`; reaching
///   [`QUIT_SEQUENCE_THRESHOLD`] consecutive rapid presses quits. A lone or
///   slow `q` yields [`QuitOutcome::Continue`] (the key is swallowed).
/// - Any other key resets the sequence and yields [`QuitOutcome::Reset`].
#[must_use]
pub fn observe_quit_sequence(
    seq: &mut QuitSequenceState,
    key: &KeyEvent,
    now: Instant,
) -> QuitOutcome {
    if is_quit_key(key) {
        *seq = QuitSequenceState::default();
        return QuitOutcome::Quit;
    }
    if is_qqq_press(key) {
        let within_window = seq.last_press.is_some_and(|pressed| {
            now.checked_duration_since(pressed)
                .is_some_and(|elapsed| elapsed <= QUIT_SEQUENCE_WINDOW)
        });
        seq.presses = if within_window {
            seq.presses.saturating_add(1)
        } else {
            1
        };
        seq.last_press = Some(now);
        if seq.presses >= QUIT_SEQUENCE_THRESHOLD {
            *seq = QuitSequenceState::default();
            return QuitOutcome::Quit;
        }
        return QuitOutcome::Continue;
    }
    // Any other key breaks the rapid-`q` run.
    *seq = QuitSequenceState::default();
    QuitOutcome::Reset
}

#[cfg(test)]
mod tests {
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
    fn is_bare_ctrl_c_accepts_lowercase_only_not_uppercase() {
        // Ctrl-Shift-C (uppercase) is a host copy shortcut on some platforms
        // and must not be treated as an interrupt.
        assert!(!is_bare_ctrl_c(&key_mods(
            KeyCode::Char('C'),
            KeyModifiers::CONTROL
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
}

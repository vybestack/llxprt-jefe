//! Input-mode and key-routing helpers.

use std::time::{Duration, Instant};

use iocraft::prelude::{KeyCode, KeyEvent, KeyModifiers};

use crate::state::{
    AppEvent, AppState, InlineState, ModalState, PaneFocus, QuitSequenceState, ScreenMode,
};

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
    /// In-app device-code auth dialog (issue #244).
    Auth,
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
/// Resolve the input mode from the active modal, if any.
///
/// Returns `None` when no modal is active (fall through to screen-mode
/// detection).
fn modal_input_mode(modal: &ModalState) -> Option<InputMode> {
    match modal {
        ModalState::Help => Some(InputMode::Help),
        ModalState::Search { .. } => Some(InputMode::Search),
        ModalState::ThemePicker { .. } => Some(InputMode::ThemePicker),
        ModalState::NewRepository { .. }
        | ModalState::EditRepository { .. }
        | ModalState::NewAgent { .. }
        | ModalState::EditAgent { .. }
        | ModalState::WorkflowDispatch { .. } => Some(InputMode::Form),
        ModalState::ConfirmDeleteRepository { .. }
        | ModalState::ConfirmDeleteAgent { .. }
        | ModalState::ConfirmKillAgent { .. }
        | ModalState::PreflightPrompt { .. }
        | ModalState::ConfirmIssueDirtyCopy { .. }
        | ModalState::ConfirmIssueOriginMismatch { .. } => Some(InputMode::Confirm),
        ModalState::Auth { .. } => Some(InputMode::Auth),
        ModalState::None => None,
    }
}

#[must_use]
pub fn input_mode_for_state(state: &AppState) -> InputMode {
    if let Some(mode) = modal_input_mode(&state.modal) {
        return mode;
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
    matches!(key.code, KeyCode::Char('c' | 'C')) && key.modifiers == KeyModifiers::CONTROL
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
// Terminal scrollback key interception (issue #198).
//
// When the terminal pane is focused (InputMode::TerminalCapture), certain keys
// move the Jefe scrollback viewport instead of being forwarded to the PTY.
// This pure helper translates a key event + scroll state into an optional
// AppEvent so the app-shell can dispatch the scroll event before PTY forwarding.
// ──────────────────────────────────────────────────────────────────────────

/// Determine whether a key event should be intercepted for terminal scrollback
/// control instead of forwarded to the PTY (issue #198).
///
/// Parameters:
/// - `key_event`: The keyboard event to evaluate.
/// - `offset_is_some`: Whether the terminal is currently scrolled back
///   (`terminal_history_offset.is_some()`). When `true`, arrow keys also move
///   the viewport; when `false` (follow-tail), only PageUp/PageDown/Home
///   trigger scroll interception.
/// - `kennel_mode`: Whether the focused terminal belongs to a kennel (Code
///   Puppy) agent. When `false` (llxprt), ALL scroll keys are forwarded to
///   the PTY so the child TUI's native scrolling is not stolen by Jefe's
///   scrollback viewport (issue #245).
///
/// Returns the `AppEvent` to dispatch, or `None` when the key should be
/// forwarded to the PTY as normal terminal input.
///
/// ## Modifier policy
///
/// Scroll keys are ONLY intercepted when the key has NO modifiers
/// (`KeyModifiers::NONE`). Any modifier chord (Ctrl, Alt, Shift, or a
/// combination) is forwarded to the PTY so child TUIs that bind those chords
/// (e.g. Ctrl+End, Alt+PageUp) are not broken.
///
/// ## End key
///
/// `End` ONLY intercepts when the viewport is scrolled back
/// (`offset_is_some == true`): it returns the user to follow-tail. At
/// follow-tail, `End` is forwarded to the PTY (so shell line editing works).
///
/// ## Home key
///
/// `Home` intercepts from BOTH states (follow-tail and scrolled-back): it
/// jumps to the top of history, matching PageUp's "enter scrollback from
/// anywhere" behavior.
#[must_use]
pub fn should_intercept_for_scrollback(
    key_event: &KeyEvent,
    offset_is_some: bool,
    kennel_mode: bool,
) -> Option<AppEvent> {
    // Non-kennel agents (llxprt) handle their own scrolling — forward all
    // scroll keys to the PTY so the child TUI's native scrollback works
    // (issue #245).
    if !kennel_mode {
        return None;
    }
    // Modifier chords always go to the PTY (so child TUI key bindings work).
    if key_event.modifiers != KeyModifiers::NONE {
        return None;
    }
    match key_event.code {
        // PageUp/PageDown enter/continue scrollback from both states.
        KeyCode::PageUp => Some(AppEvent::TerminalScrollPageUp),
        KeyCode::PageDown => Some(AppEvent::TerminalScrollPageDown),
        // End only intercepts when scrolled back (return to follow-tail).
        // At follow-tail, End goes to the PTY (shell line editing).
        KeyCode::End if offset_is_some => Some(AppEvent::TerminalFollowTail),
        // Arrow keys only scroll the viewport when already scrolled back.
        // When at follow-tail, arrows go to the PTY (so the child TUI works).
        KeyCode::Up if offset_is_some => Some(AppEvent::TerminalScrollUp),
        KeyCode::Down if offset_is_some => Some(AppEvent::TerminalScrollDown),
        // Home scrolls to the top of history from BOTH states.
        KeyCode::Home => Some(AppEvent::TerminalScrollToTop),
        _ => None,
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
#[path = "input_tests.rs"]
mod tests;

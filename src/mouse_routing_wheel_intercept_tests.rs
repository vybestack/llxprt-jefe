//! Tests for the wheel-intercept agent-kind gating (issue #245).
//!
//! Extracted from `mouse_routing_tests.rs` to keep that file under the 1000-line
//! size limit. These tests exercise the pure helper
//! [`super::wheel_intercept_active_for_agent`] which encapsulates the decision
//! of whether Jefe's scrollback viewport should own wheel events for the
//! focused terminal agent. The wheel-vs-non-wheel and over-pane-vs-not checks
//! are performed by the caller via `is_wheel_event` / `is_event_over_terminal_pane`
//! (already unit-tested in `mouse_routing_tests.rs`), so this helper focuses on
//! the agent-kind gating that #245 introduced.

use super::wheel_intercept_active_for_agent;
use jefe::selection::{
    GestureAction, GestureEvent, GestureEventKind, GestureState, SelectablePane,
};

// ── wheel_intercept_active_for_agent truth table ─────────────────────────────
//
// The wheel-intercept pre-check in `route_terminal_gesture` must be gated on
// `is_kennel_mode()`. Non-kennel agents (llxprt) handle their own scrolling
// via SGR mouse reporting, so the wheel must fall through to the gesture state
// machine which forwards it to the PTY. These tests exercise the pure helper
// that encapsulates that agent-kind decision.

#[test]
fn wheel_intercept_active_for_kennel_no_shift() {
    // Code Puppy scrollback: Jefe may intercept the wheel (subject to the
    // caller's wheel + over-pane checks).
    assert!(
        wheel_intercept_active_for_agent(true, false),
        "kennel + no shift must allow scrollback intercept"
    );
}

#[test]
fn wheel_intercept_inactive_for_non_kennel_no_shift() {
    // THIS IS THE REGRESSION TEST (issue #245): llxprt is non-kennel, so the
    // wheel must NOT be intercepted — it falls through to the gesture state
    // machine which forwards it to the PTY via SGR mouse reporting.
    assert!(
        !wheel_intercept_active_for_agent(false, false),
        "non-kennel + no shift must NOT intercept (llxprt owns scrolling)"
    );
}

#[test]
fn wheel_intercept_inactive_for_kennel_shift() {
    // Shift+wheel is host passthrough — never intercepted, regardless of kind.
    assert!(
        !wheel_intercept_active_for_agent(true, true),
        "kennel + shift must NOT intercept (host passthrough)"
    );
}

#[test]
fn wheel_intercept_inactive_for_non_kennel_shift() {
    // Non-kennel + shift: doubly excluded (non-kennel AND shift passthrough).
    assert!(
        !wheel_intercept_active_for_agent(false, true),
        "non-kennel + shift must NOT intercept"
    );
}

/// Composite assertion (issue #245) validating the two pure components that
/// the production router (`route_terminal_gesture`) composes for a non-kennel
/// wheel event:
///
/// 1. the routing-layer gate helper `wheel_intercept_active_for_agent` returns
///    `false` for a non-kennel agent, so the router does NOT intercept the
///    wheel for Jefe scrollback;
/// 2. the gesture state machine (the fallback the router delegates to when the
///    gate is false) forwards the wheel to the PTY when mouse reporting is
///    active.
///
/// This validates the helper and the gesture machine in isolation. The router
/// itself takes an iocraft `HookState<AppState>` that cannot be constructed
/// outside a hook context, so it is not unit-testable here; the router's
/// composition of these two pure components is instead covered by the truth
/// table above and the existing `wheel_forwards_when_mouse_reporting_active`
/// integration test in `tests/runtime/terminal_focus_routing.rs`.
#[test]
fn non_kennel_wheel_gate_and_gesture_machine_forward_to_pty() {
    use jefe::selection::SelectionPoint;

    // The routing-layer gate: non-kennel + no shift → NOT intercepted. The
    // wheel falls through to the gesture state machine.
    assert!(
        !wheel_intercept_active_for_agent(false, false),
        "non-kennel wheel must NOT be intercepted by Jefe scrollback (issue #245)"
    );

    // The gesture state machine (the fallback path) forwards the wheel to the
    // PTY when mouse reporting is active.
    let wheel_event = GestureEvent {
        kind: GestureEventKind::ScrollDown,
        shift_held: false,
        col: 5,
        row: 5,
        mouse_reporting_active: true,
        kennel_mode: false,
    };
    let resolver = |col: u16, row: u16| -> Option<SelectionPoint> {
        if col < 2 || row < 2 {
            return None;
        }
        Some(SelectionPoint::new(SelectablePane::TerminalView, 0, 0))
    };
    let (action, _state) = GestureState::default().process(wheel_event, &resolver);
    match action {
        GestureAction::ForwardToPty(replays) => {
            assert_eq!(replays.len(), 1);
            assert_eq!(replays[0].kind, GestureEventKind::ScrollDown);
        }
        other => panic!(
            "non-kennel reporting wheel must forward to PTY via gesture machine, got {other:?}"
        ),
    }
}

// ── Issue #296: mouse-mode recovery ties observed mode to routing outcome ──
//
// The root cause of #296 is that a freshly spawned AttachedViewer reports
// `mouse_reporting_active() == false` until the child re-emits DEC private
// mouse modes through the PTY stream. When false, the gesture machine treats a
// non-kennel (LLxprt) child as non-reporting and routes its wheel/click to
// Jefe selection instead of the PTY. The post-attach mode-recovery nudge
// restores the observed mode so the routing below (ForwardToPty) is reached.
// These tests pin the routing consequence of the observed-mode state.

/// When `mouse_reporting_active` is FALSE (pre-recovery), a non-kennel wheel
/// event is NOT forwarded to the PTY — it falls to app-level scroll. This is
/// the regression surface #296's mode-recovery nudge must overcome.
#[test]
fn non_kennel_wheel_not_forwarded_when_mouse_reporting_inactive() {
    use jefe::selection::SelectionPoint;

    let wheel_event = GestureEvent {
        kind: GestureEventKind::ScrollDown,
        shift_held: false,
        col: 5,
        row: 5,
        mouse_reporting_active: false,
        kennel_mode: false,
    };
    let resolver = |col: u16, row: u16| -> Option<SelectionPoint> {
        if col < 2 || row < 2 {
            return None;
        }
        Some(SelectionPoint::new(SelectablePane::TerminalView, 0, 0))
    };
    let (action, _state) = GestureState::default().process(wheel_event, &resolver);
    // Non-reporting child wheel: NOT ForwardToPty (Noop → app-level scroll).
    assert!(
        !matches!(action, GestureAction::ForwardToPty(_)),
        "non-kennel non-reporting wheel must NOT forward to PTY (pre-recovery state): {action:?}"
    );
}

/// When `mouse_reporting_active` is TRUE (post-recovery), a non-kennel wheel
/// event IS forwarded to the PTY. This is the routing outcome the #296
/// mode-recovery nudge restores.
#[test]
fn non_kennel_wheel_forwarded_when_mouse_reporting_active() {
    use jefe::selection::SelectionPoint;

    let wheel_event = GestureEvent {
        kind: GestureEventKind::ScrollDown,
        shift_held: false,
        col: 5,
        row: 5,
        mouse_reporting_active: true,
        kennel_mode: false,
    };
    let resolver = |col: u16, row: u16| -> Option<SelectionPoint> {
        if col < 2 || row < 2 {
            return None;
        }
        Some(SelectionPoint::new(SelectablePane::TerminalView, 0, 0))
    };
    let (action, _state) = GestureState::default().process(wheel_event, &resolver);
    match action {
        GestureAction::ForwardToPty(replays) => {
            assert_eq!(replays.len(), 1);
            assert_eq!(replays[0].kind, GestureEventKind::ScrollDown);
        }
        other => panic!(
            "non-kennel reporting wheel must forward to PTY (post-recovery state): {other:?}"
        ),
    }
}

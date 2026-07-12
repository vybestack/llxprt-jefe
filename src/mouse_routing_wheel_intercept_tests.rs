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

/// Integration assertion (issue #245): for a non-kennel agent with mouse
/// reporting active, the wheel is NOT intercepted by Jefe scrollback (the pure
/// helper returns false), so it falls through to the gesture state machine
/// which forwards it to the PTY. This proves the end-to-end routing contract:
/// non-kennel wheel → helper says no → gesture machine forwards to PTY.
///
/// This mirrors the existing `wheel_forwards_when_mouse_reporting_active`
/// test in `tests/runtime/terminal_focus_routing.rs` but adds the routing-layer
/// gate assertion that was missing.
#[test]
fn non_kennel_wheel_not_intercepted_and_forwards_to_pty() {
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

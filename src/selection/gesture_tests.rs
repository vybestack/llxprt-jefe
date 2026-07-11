//! Tests for the pure gesture-ownership state machine (issue #197).
//!
//! These exercise every sequence the spec requires without iocraft or a
//! runtime: the state machine is pure, and the point resolver is injected.

use crate::selection::gesture::{
    GestureAction, GestureEvent, GestureEventKind, GestureState, PtyReplay,
};
use crate::selection::{SelectablePane, SelectionPoint};

fn ev(kind: GestureEventKind, shift: bool, reporting: bool) -> GestureEvent {
    GestureEvent {
        kind,
        shift_held: shift,
        col: 10,
        row: 5,
        mouse_reporting_active: reporting,
    }
}

fn ev_at(kind: GestureEventKind, shift: bool, reporting: bool, col: u16, row: u16) -> GestureEvent {
    GestureEvent {
        kind,
        shift_held: shift,
        col,
        row,
        mouse_reporting_active: reporting,
    }
}

/// Resolver that returns a valid point inside the terminal content area and
/// `None` for coordinates on/inside the chrome (col/row < 2), mirroring the
/// real content-origin math so the `Option` return is genuinely exercised.
fn resolver(col: u16, row: u16) -> Option<SelectionPoint> {
    if col < 2 || row < 2 {
        return None;
    }
    let content_line = usize::from(row.saturating_sub(2));
    let content_col = usize::from(col.saturating_sub(2));
    Some(SelectionPoint::new(
        SelectablePane::TerminalView,
        content_line,
        content_col,
    ))
}

/// Resolver that always returns None (coordinate not over the terminal pane).
fn no_resolver(_col: u16, _row: u16) -> Option<SelectionPoint> {
    None
}

// ── Sequence (1): reporting down→drag→up creates selection + finalizes copy ──

#[test]
fn reporting_down_drag_up_creates_selection_and_copies() {
    let state = GestureState::default();
    let (action, state) = state.process(ev(GestureEventKind::LeftDown, false, true), &resolver);
    // Pending: no action.
    assert_eq!(action, GestureAction::Noop);
    assert!(matches!(state, GestureState::Pending { .. }));

    let (action, state) = state.process(ev(GestureEventKind::LeftDrag, false, true), &resolver);
    // Drag resolves pending to Jefe-owned: begin a selection spanning the
    // buffered down (anchor) through the drag (focus). Issue #197 review: the
    // first drag must not discard the drag coordinate (was collapsed).
    assert!(matches!(action, GestureAction::BeginSelectionRange { .. }));
    assert!(matches!(state, GestureState::JefeOwned { .. }));

    let (action, state) = state.process(ev(GestureEventKind::LeftUp, false, true), &resolver);
    // Jefe owns through release: finalize + copy.
    assert_eq!(action, GestureAction::FinalizeAndCopy);
    assert_eq!(state, GestureState::Idle);
}

// ── Sequence (2): reporting click (down→up, no drag) forwards to PTY ──────────

#[test]
fn reporting_click_down_up_forwards_to_pty() {
    let state = GestureState::default();
    let (action, state) = state.process(
        ev_at(GestureEventKind::LeftDown, false, true, 10, 5),
        &resolver,
    );
    assert_eq!(action, GestureAction::Noop);
    assert!(matches!(
        state,
        GestureState::Pending {
            down_col: 10,
            down_row: 5
        }
    ));

    let (action, state) = state.process(
        ev_at(GestureEventKind::LeftUp, false, true, 12, 6),
        &resolver,
    );
    // Pure click → replay down + up to PTY, each at its real coordinate and in
    // order (issue #197 review: the up was never emitted, leaving the child's
    // button stuck).
    match action {
        GestureAction::ForwardToPty(replays) => {
            assert_eq!(
                replays,
                vec![
                    PtyReplay {
                        col: 10,
                        row: 5,
                        kind: GestureEventKind::LeftDown,
                    },
                    PtyReplay {
                        col: 12,
                        row: 6,
                        kind: GestureEventKind::LeftUp,
                    },
                ],
                "reporting click must replay down + up at real coordinates, in order"
            );
        }
        other => panic!("expected ForwardToPty, got {other:?}"),
    }
    assert_eq!(state, GestureState::Idle);
}

// ── Sequence (3): non-reporting down→drag→up selects ─────────────────────────

#[test]
fn non_reporting_down_drag_up_selects() {
    let state = GestureState::default();
    let (action, state) = state.process(ev(GestureEventKind::LeftDown, false, false), &resolver);
    assert!(matches!(action, GestureAction::BeginSelection(_)));
    assert!(matches!(state, GestureState::JefeOwned { .. }));

    let (action, state) = state.process(ev(GestureEventKind::LeftDrag, false, false), &resolver);
    assert!(matches!(action, GestureAction::UpdateSelection(_)));
    assert!(matches!(state, GestureState::JefeOwned { .. }));

    let (action, state) = state.process(ev(GestureEventKind::LeftUp, false, false), &resolver);
    assert_eq!(action, GestureAction::FinalizeAndCopy);
    assert_eq!(state, GestureState::Idle);
}

// ── Sequence (4): shift-down→drag→up selects ─────────────────────────────────

#[test]
fn shift_down_drag_up_selects() {
    let state = GestureState::default();
    let (action, state) = state.process(ev(GestureEventKind::LeftDown, true, true), &resolver);
    // Shift at down → Jefe owns immediately even with reporting.
    assert!(matches!(action, GestureAction::BeginSelection(_)));
    assert!(matches!(state, GestureState::JefeOwned { .. }));

    let (action, state) = state.process(ev(GestureEventKind::LeftDrag, true, true), &resolver);
    assert!(matches!(action, GestureAction::UpdateSelection(_)));

    let (action, _state) = state.process(ev(GestureEventKind::LeftUp, true, true), &resolver);
    assert_eq!(action, GestureAction::FinalizeAndCopy);
}

// ── Sequence (5): reporting changes mid-gesture does not split ownership ──────

#[test]
fn reporting_change_mid_gesture_does_not_split_ownership() {
    let state = GestureState::default();
    // Start non-reporting → Jefe owns.
    let (action, state) = state.process(ev(GestureEventKind::LeftDown, false, false), &resolver);
    assert!(matches!(action, GestureAction::BeginSelection(_)));

    // Reporting turns on mid-gesture — Jefe still owns.
    let (action, state) = state.process(ev(GestureEventKind::LeftDrag, false, true), &resolver);
    assert!(matches!(action, GestureAction::UpdateSelection(_)));
    assert!(matches!(state, GestureState::JefeOwned { .. }));

    // Release → finalize + copy (not forwarded to PTY).
    let (action, _) = state.process(ev(GestureEventKind::LeftUp, false, true), &resolver);
    assert_eq!(action, GestureAction::FinalizeAndCopy);
}

// ── Sequence (6): shift+wheel passes through (host passthrough) ───────────────

#[test]
fn shift_wheel_passes_through() {
    let state = GestureState::default();
    let (action, state) = state.process(ev(GestureEventKind::ScrollUp, true, true), &resolver);
    // Shift+wheel → Noop (host passthrough; caller returns early).
    assert_eq!(action, GestureAction::Noop);
    assert_eq!(state, GestureState::Idle);

    // Non-shift wheel over reporting child → forward.
    let (action, _) = state.process(ev(GestureEventKind::ScrollDown, false, true), &resolver);
    assert!(matches!(action, GestureAction::ForwardToPty(_)));

    // Non-shift wheel over non-reporting → Noop (caller handles scroll).
    let (action, _) = state.process(ev(GestureEventKind::ScrollUp, false, false), &resolver);
    assert_eq!(action, GestureAction::Noop);
}

// ── Sequence (7): shift+right/middle passes through ──────────────────────────

#[test]
fn shift_other_button_passes_through() {
    let state = GestureState::default();
    let (action, _) = state.process(ev(GestureEventKind::OtherButton, true, true), &resolver);
    assert_eq!(action, GestureAction::Noop);
}

// ── Pending → non-left event flushes the buffered down to PTY ─────────────────

#[test]
fn pending_flushed_on_non_left_event() {
    let state = GestureState::default();
    let (_action, state) = state.process(
        ev_at(GestureEventKind::LeftDown, false, true, 10, 5),
        &resolver,
    );
    assert!(matches!(state, GestureState::Pending { .. }));

    // A scroll event while pending → flush: replay the down, then the scroll
    // goes through the wheel path. The state resets to idle for the wheel.
    let (action, state) = state.process(ev(GestureEventKind::ScrollUp, false, true), &resolver);
    // The wheel itself either forwards (reporting) or noops (non-reporting);
    // the key assertion is the state is no longer pending.
    assert_eq!(
        state,
        GestureState::Idle,
        "pending must be flushed by a non-left event"
    );
    // The wheel over a reporting child forwards, and the flush must include
    // the buffered down replay (not just the scroll).
    match action {
        GestureAction::ForwardToPty(replays) => {
            assert!(
                replays
                    .iter()
                    .any(|r| r.kind == GestureEventKind::LeftDown && r.col == 10 && r.row == 5),
                "pending must be flushed: buffered down @(10,5) replayed"
            );
        }
        other => panic!("expected ForwardToPty (down flush + wheel), got {other:?}"),
    }
}

// ── Drag without preceding down is a no-op ───────────────────────────────────

#[test]
fn drag_without_down_is_noop() {
    let state = GestureState::default();
    let (action, state) = state.process(ev(GestureEventKind::LeftDrag, false, false), &resolver);
    assert_eq!(action, GestureAction::Noop);
    assert_eq!(state, GestureState::Idle);
}

// ── Coordinate not over terminal pane → no selection ─────────────────────────

#[test]
fn left_down_not_over_terminal_is_noop() {
    let state = GestureState::default();
    let (action, state) = state.process(ev(GestureEventKind::LeftDown, false, false), &no_resolver);
    assert_eq!(action, GestureAction::Noop);
    assert_eq!(state, GestureState::Idle);
}

// ── Pending flush preserves a shift-initiated selection (Composite) ─────────
//
// A stray second LeftDown while Pending flushes the buffered down to the PTY.
// When that second down is shift+LeftDown (a Jefe selection), both the flush
// AND the BeginSelection must happen — previously merge_actions silently
// dropped the selection (issue #197 review).

#[test]
fn shift_left_down_while_pending_emits_composite_flush_plus_selection() {
    use crate::selection::SelectablePane;
    let down = ev_at(GestureEventKind::LeftDown, false, true, 5, 5);
    let (_, pending) = GestureState::default().process(down, &resolver);
    let shift_down = ev_at(GestureEventKind::LeftDown, true, true, 8, 8);
    let (action, state) = pending.process(shift_down, &resolver);
    match action {
        GestureAction::Composite { first, second } => {
            // First: the buffered down replayed to the PTY.
            match *first {
                GestureAction::ForwardToPty(replays) => {
                    assert_eq!(replays.len(), 1, "flush replays the buffered down");
                    assert_eq!(replays[0].kind, GestureEventKind::LeftDown);
                    assert_eq!((replays[0].col, replays[0].row), (5, 5));
                }
                other => panic!("composite first must be ForwardToPty, got {other:?}"),
            }
            // Second: the shift-initiated selection begins.
            match *second {
                GestureAction::BeginSelection(point) => {
                    assert_eq!(point.pane, SelectablePane::TerminalView);
                    assert_eq!(
                        (point.line, point.col),
                        (6, 6),
                        "selection at shift-down coord"
                    );
                }
                other => panic!("composite second must be BeginSelection, got {other:?}"),
            }
        }
        other => panic!("expected Composite, got {other:?}"),
    }
    assert!(
        matches!(state, GestureState::JefeOwned { .. }),
        "state latches Jefe ownership for the shift gesture"
    );
}

// ── Pending→drag with unresolvable buffered anchor forwards the down ────────
//
// If the buffered down coordinate no longer resolves when the drag arrives
// (layout/scroll change), the buffered down is forwarded to the PTY rather
// than silently dropped (issue #197 review: every other Pending→Idle exit
// replays the down; this path was the sole exception).

#[test]
fn pending_drag_with_unresolvable_anchor_forwards_buffered_down() {
    // down@(0,0) is out-of-pane (< 2) → the anchor won't resolve. drag@(5,5)
    // is in-pane, so the Pending→drag path hits the unresolvable-anchor
    // branch: the buffered down AND the drag are forwarded to the PTY rather
    // than dropped (issue #197 review).
    let down = ev_at(GestureEventKind::LeftDown, false, true, 0, 0);
    let (_, pending) = GestureState::default().process(down, &resolver);
    // down@(0,0) is out-of-pane (< 2) so the anchor won't resolve.
    let drag = ev_at(GestureEventKind::LeftDrag, false, true, 5, 5);
    let (action, state) = pending.process(drag, &resolver);
    match action {
        GestureAction::ForwardToPty(replays) => {
            assert_eq!(
                replays.len(),
                2,
                "buffered down AND the drag forwarded (complete press+move)"
            );
            assert_eq!(replays[0].kind, GestureEventKind::LeftDown);
            assert_eq!((replays[0].col, replays[0].row), (0, 0));
            assert_eq!(replays[1].kind, GestureEventKind::LeftDrag);
            assert_eq!((replays[1].col, replays[1].row), (5, 5));
        }
        other => panic!("expected ForwardToPty of buffered down + drag, got {other:?}"),
    }
    assert_eq!(state, GestureState::Idle);
}

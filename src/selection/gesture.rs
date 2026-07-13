//! Pure gesture-ownership state machine for terminal mouse routing (issue #197).
//!
//! A left-button gesture (down → drags → up) has a single owner: either Jefe
//! (paints a selection over the snapshot and copies on release) or the PTY
//! (forwards the events to the child). The owner is decided at gesture START
//! and latched for the whole gesture — per-event classification is wrong
//! because a reporting child's transient menu needs left CLICKS but not drags.
//!
//! # Decision table (at left-down)
//!
//! | shift | reporting | kennel | owner decision                                              |
//! |:-----:|:---------:|:------:|-------------------------------------------------------------|
//! | yes   | any       | any    | Jefe owns (shift means "I want a Jefe selection")           |
//! | no    | no        | any    | Jefe owns (selection over snapshot)                         |
//! | no    | yes       | no     | **PTY owns** — forward down immediately, latch PtyOwned (#245) |
//! | no    | yes       | yes    | **pending** — buffer the down, decide on the next event (#197) |
//!
//! # Pending resolution
//!
//! While **pending** (reporting child, non-shift left-down buffered):
//! - A left-button **drag** → Jefe owns: begin a selection spanning the
//!   buffered down coordinate (anchor) through the drag coordinate (focus),
//!   latch Jefe through release.
//! - Left-button **up** with no preceding drag (pure click) → PTY owns:
//!   replay the buffered down + the up (at its real release coordinate) to the
//!   PTY, discard the buffer.
//! - Any **non-left-button** event (wheel/right/middle) → flush: replay the
//!   buffered down to the PTY, then process the current event from idle.
//! - A stray second **left-down** (rare with well-formed event sequences) →
//!   flush the buffered down to the PTY first, then start the new gesture, so
//!   the reporting app never loses a press.
//!
//! # PTY-owned resolution (non-kennel, issue #245)
//!
//! While **pty-owned** (non-kennel reporting child, non-shift left-down
//! forwarded immediately):
//! - A left-button **drag** → forward the drag to the PTY (scrollbar drag,
//!   text selection in the child TUI, etc.).
//! - Left-button **up** → forward the up to the PTY, then reset to idle.
//! - Any **non-left-button** event (wheel/right/middle) or a stray second
//!   **left-down** → reset to idle and process the event from idle (nothing is
//!   buffered in PtyOwned — the down was already forwarded — so no flush is
//!   needed).
//!
//! # Other gestures
//!
//! Wheel, right, and middle button events are NOT part of a left-button
//! gesture. They forward to a reporting child immediately, else fall through
//! to app selection (unchanged behavior). Shift-modified non-left-button
//! events (shift+wheel, shift+right/middle) are host passthrough — they never
//! start a Jefe selection.
//!
//! All types and functions here are pure, iocraft-free, and side-effect-free
//! so the full contract is unit-testable without the runtime.

use crate::selection::SelectionPoint;

/// The kind of mouse event the state machine processes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GestureEventKind {
    /// Left mouse button pressed down.
    LeftDown,
    /// Left mouse button dragged (moved while held).
    LeftDrag,
    /// Left mouse button released.
    LeftUp,
    /// Mouse wheel scroll up.
    ScrollUp,
    /// Mouse wheel scroll down.
    ScrollDown,
    /// Right or middle mouse button event (down/drag/up).
    OtherButton,
}

/// A single input event to the state machine.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct GestureEvent {
    /// What kind of mouse event this is.
    pub kind: GestureEventKind,
    /// Whether SHIFT is held.
    pub shift_held: bool,
    /// The screen column (for left-button events that may start/update a
    /// selection). Ignored for wheel/other-button events.
    pub col: u16,
    /// The screen row (same as `col`).
    pub row: u16,
    /// Whether the child currently advertises mouse reporting. This is injected
    /// (not read from the runtime) so the state machine is pure — the router
    /// reads it once per event under a single lock acquisition (issue #197
    /// TOCTOU fix).
    pub mouse_reporting_active: bool,
    /// Whether the agent is a kennel (Code Puppy) agent. Injected per-event by
    /// the router so the state machine stays pure. Kennel agents use Jefe's
    /// text selection over reporting terminals (issue #197); non-kennel agents
    /// (llxprt) own their left-button mouse events over the PTY (issue #245).
    pub kennel_mode: bool,
}

/// The action the router should take in response to an event.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum GestureAction {
    /// Begin a Jefe selection at the given point (collapsed).
    BeginSelection(SelectionPoint),
    /// Begin a Jefe selection spanning `anchor` through `focus`. Emitted when a
    /// pending gesture resolves to Jefe ownership on the first drag: the anchor
    /// is the buffered down coordinate and the focus is the current drag
    /// coordinate, so the selection immediately covers the dragged range
    /// instead of starting collapsed (issue #197 review: first-drag gap).
    BeginSelectionRange {
        anchor: SelectionPoint,
        focus: SelectionPoint,
    },
    /// Update the selection focus (drag) to the given point.
    UpdateSelection(SelectionPoint),
    /// Finalize the selection and copy the highlighted text via OSC 52.
    FinalizeAndCopy,
    /// Forward the given raw event bytes to the PTY.
    ///
    /// Each element is a (col, row, kind) triple to encode and write. When a
    /// buffered down is replayed alongside an up, both are included with their
    /// real coordinates.
    ForwardToPty(Vec<PtyReplay>),
    /// Run `first`, then `second`. Emitted when a pending gesture is flushed
    /// (buffered down replayed to the PTY) and the event that triggered the
    /// flush itself starts a Jefe selection (shift+LeftDown): both the flush
    /// and the new selection must happen. Without this composite, the
    /// selection would be silently swallowed by [`merge_actions`] (issue #197
    /// review: shift-select after a pending press must not be lost).
    Composite {
        /// The flush action (run first).
        first: Box<Self>,
        /// The follow-up action (run second).
        second: Box<Self>,
    },
    /// No action (event consumed / irrelevant).
    Noop,
}

/// A PTY replay event: the router encodes this as SGR mouse bytes and writes
/// them to the child PTY.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PtyReplay {
    /// Column relative to the terminal pane.
    pub col: u16,
    /// Row relative to the terminal pane.
    pub row: u16,
    /// The event kind to encode.
    pub kind: GestureEventKind,
}

/// The gesture-ownership state, persisted between events within a single
/// left-button gesture.
///
/// Construct with [`GestureState::default`] (idle). Feed events via
/// [`GestureState::process`]. The state resets to idle on left-up or when a
/// non-left event flushes a pending gesture.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub enum GestureState {
    /// No gesture in progress.
    #[default]
    Idle,
    /// A non-shift left-down happened over a reporting child; the down
    /// coordinate is buffered pending a drag-vs-click decision.
    Pending {
        /// The buffered down coordinate (screen space).
        down_col: u16,
        down_row: u16,
    },
    /// Jefe owns the gesture: a selection is active from `anchor` through the
    /// current drag position. Persists until left-up.
    JefeOwned {
        /// The anchor point (content coordinates).
        anchor: SelectionPoint,
    },
    /// The PTY owns the gesture: a non-kennel reporting child's left-button
    /// down was forwarded immediately, and subsequent drags/up are forwarded
    /// too (issue #245: non-kennel scrollbar drags/clicks must reach the PTY,
    /// not Jefe's text selection). Nothing is buffered — the down was already
    /// forwarded — so a non-left event or stray left-down simply resets to
    /// Idle.
    PtyOwned,
}

impl GestureState {
    /// Process a single event, returning the action to take and the new state.
    ///
    /// `resolve_point` is an injected function that maps a screen `(col, row)`
    /// to a content [`SelectionPoint`] within the terminal pane. This keeps the
    /// state machine pure (no app-state reads). The resolver returns `None`
    /// when the coordinate is not over the terminal pane.
    #[must_use]
    pub fn process<R>(&self, event: GestureEvent, resolve_point: &R) -> (GestureAction, Self)
    where
        R: Fn(u16, u16) -> Option<SelectionPoint>,
    {
        // A non-left event (wheel/right/middle) or a stray second LeftDown
        // while PtyOwned → reset to Idle and process the event from Idle.
        // PtyOwned has nothing buffered (the down was already forwarded), so
        // no flush is needed — unlike Pending (issue #245).
        if matches!(self, Self::PtyOwned)
            && matches!(
                event.kind,
                GestureEventKind::LeftDown
                    | GestureEventKind::ScrollUp
                    | GestureEventKind::ScrollDown
                    | GestureEventKind::OtherButton
            )
        {
            return Self::Idle.process(event, resolve_point);
        }

        // Any non-drag left-button-terminating event while pending → flush:
        // replay the buffered down to the PTY, then process the current event
        // from idle. This prevents a reporting app from being starved of its
        // press if the user switches context mid-gesture (issue #197 review:
        // stray second LeftDown or a wheel/right/middle mid-gesture).
        if let Self::Pending { down_col, down_row } = self {
            match event.kind {
                GestureEventKind::LeftDown
                | GestureEventKind::ScrollUp
                | GestureEventKind::ScrollDown
                | GestureEventKind::OtherButton => {
                    let flush = GestureAction::ForwardToPty(vec![PtyReplay {
                        col: *down_col,
                        row: *down_row,
                        kind: GestureEventKind::LeftDown,
                    }]);
                    let (sub_action, new_state) = Self::Idle.process(event, resolve_point);
                    return (merge_actions(flush, sub_action), new_state);
                }
                _ => {}
            }
        }

        match event.kind {
            GestureEventKind::LeftDown => self.process_left_down(event, resolve_point),
            GestureEventKind::LeftDrag => self.process_left_drag(event, resolve_point),
            GestureEventKind::LeftUp => self.process_left_up(event),
            GestureEventKind::ScrollUp | GestureEventKind::ScrollDown => self.process_wheel(event),
            GestureEventKind::OtherButton => self.process_other_button(event),
        }
    }

    fn process_left_down<R>(&self, event: GestureEvent, resolve_point: &R) -> (GestureAction, Self)
    where
        R: Fn(u16, u16) -> Option<SelectionPoint>,
    {
        // A new left-down always starts a fresh gesture. A stray LeftDown
        // while still mid-gesture (shouldn't happen with well-formed event
        // sequences) is implicitly reset by the state transitions below.
        let _ = self;

        // Shift at left-down → Jefe owns the whole gesture.
        if event.shift_held {
            return match resolve_point(event.col, event.row) {
                Some(point) => (
                    GestureAction::BeginSelection(point),
                    Self::JefeOwned { anchor: point },
                ),
                None => (GestureAction::Noop, Self::Idle),
            };
        }

        // Non-reporting child at left-down → Jefe owns (selection over snapshot).
        if !event.mouse_reporting_active {
            return match resolve_point(event.col, event.row) {
                Some(point) => (
                    GestureAction::BeginSelection(point),
                    Self::JefeOwned { anchor: point },
                ),
                None => (GestureAction::Noop, Self::Idle),
            };
        }

        // Reporting child, non-shift:
        // - Non-kennel (llxprt) → PTY owns: forward the down immediately and
        //   latch PtyOwned so the whole gesture (scrollbar drag / click) reaches
        //   the child (issue #245).
        // - Kennel (Code Puppy) → pending: buffer the down, decide on the next
        //   event (unchanged #197 behavior).
        if !event.kennel_mode {
            return (
                GestureAction::ForwardToPty(vec![PtyReplay {
                    col: event.col,
                    row: event.row,
                    kind: GestureEventKind::LeftDown,
                }]),
                Self::PtyOwned,
            );
        }

        (
            GestureAction::Noop,
            Self::Pending {
                down_col: event.col,
                down_row: event.row,
            },
        )
    }

    fn process_left_drag<R>(&self, event: GestureEvent, resolve_point: &R) -> (GestureAction, Self)
    where
        R: Fn(u16, u16) -> Option<SelectionPoint>,
    {
        match self {
            Self::JefeOwned { anchor } => {
                // Jefe owns: update the selection focus.
                match resolve_point(event.col, event.row) {
                    Some(point) => (
                        GestureAction::UpdateSelection(point),
                        Self::JefeOwned { anchor: *anchor },
                    ),
                    None => (GestureAction::Noop, self.clone()),
                }
            }
            Self::Pending { down_col, down_row } => {
                // Pending → drag resolves to Jefe: begin a selection spanning
                // the buffered down coordinate (anchor) through the drag
                // coordinate (focus). If the drag coordinate does not resolve
                // (cursor left the pane), fall back to a collapsed selection at
                // the anchor so the gesture still latches Jefe ownership —
                // subsequent in-pane drags will extend it (issue #197 review:
                // first-drag must not discard the drag coordinate).
                let Some(anchor) = resolve_point(*down_col, *down_row) else {
                    // The buffered down no longer resolves (layout/scroll
                    // changed between the down and the drag). Forward the
                    // buffered down AND this drag to the PTY so the reporting
                    // child sees a complete press+move (not an orphan press),
                    // then reset — consistent with every other Pending→Idle
                    // exit (issue #197 review).
                    return (
                        GestureAction::ForwardToPty(vec![
                            PtyReplay {
                                col: *down_col,
                                row: *down_row,
                                kind: GestureEventKind::LeftDown,
                            },
                            PtyReplay {
                                col: event.col,
                                row: event.row,
                                kind: GestureEventKind::LeftDrag,
                            },
                        ]),
                        Self::Idle,
                    );
                };
                let focus = resolve_point(event.col, event.row).unwrap_or(anchor);
                (
                    GestureAction::BeginSelectionRange { anchor, focus },
                    Self::JefeOwned { anchor },
                )
            }
            Self::Idle => {
                // Drag without a preceding down — ignore.
                (GestureAction::Noop, Self::Idle)
            }
            Self::PtyOwned => {
                // PTY owns the gesture: forward the drag to the child.
                (
                    GestureAction::ForwardToPty(vec![PtyReplay {
                        col: event.col,
                        row: event.row,
                        kind: GestureEventKind::LeftDrag,
                    }]),
                    Self::PtyOwned,
                )
            }
        }
    }

    fn process_left_up(&self, event: GestureEvent) -> (GestureAction, Self) {
        match self {
            Self::JefeOwned { .. } => {
                // Jefe owns: finalize and copy.
                (GestureAction::FinalizeAndCopy, Self::Idle)
            }
            Self::Pending { down_col, down_row } => {
                // Pending → pure click (no drag): replay down + up to the PTY,
                // each at its real coordinate, so the reporting child gets a
                // complete press/release (issue #197 review: the up was never
                // emitted, leaving the child's button stuck).
                (
                    GestureAction::ForwardToPty(vec![
                        PtyReplay {
                            col: *down_col,
                            row: *down_row,
                            kind: GestureEventKind::LeftDown,
                        },
                        PtyReplay {
                            col: event.col,
                            row: event.row,
                            kind: GestureEventKind::LeftUp,
                        },
                    ]),
                    Self::Idle,
                )
            }
            Self::Idle => (GestureAction::Noop, Self::Idle),
            Self::PtyOwned => {
                // PTY owns the gesture: forward the up to the child, then reset.
                (
                    GestureAction::ForwardToPty(vec![PtyReplay {
                        col: event.col,
                        row: event.row,
                        kind: GestureEventKind::LeftUp,
                    }]),
                    Self::Idle,
                )
            }
        }
    }

    fn process_wheel(&self, event: GestureEvent) -> (GestureAction, Self) {
        // Shift-modified wheel is host passthrough (never a Jefe selection).
        if event.shift_held {
            return (GestureAction::Noop, self.clone());
        }
        // Non-shift wheel over a reporting child → forward. Otherwise fall
        // through to app selection (Noop here — the caller handles scroll).
        if event.mouse_reporting_active {
            return (
                GestureAction::ForwardToPty(vec![forward_event(event.col, event.row, event.kind)]),
                self.clone(),
            );
        }
        (GestureAction::Noop, self.clone())
    }

    fn process_other_button(&self, event: GestureEvent) -> (GestureAction, Self) {
        // Shift-modified right/middle is host passthrough.
        if event.shift_held {
            return (GestureAction::Noop, self.clone());
        }
        // Non-shift right/middle over a reporting child → forward.
        if event.mouse_reporting_active {
            return (
                GestureAction::ForwardToPty(vec![forward_event(event.col, event.row, event.kind)]),
                self.clone(),
            );
        }
        (GestureAction::Noop, self.clone())
    }
}

fn forward_event(col: u16, row: u16, kind: GestureEventKind) -> PtyReplay {
    PtyReplay { col, row, kind }
}

/// Merge two actions: if both forward to the PTY, combine their replay lists.
/// If the second is a no-op, the flush alone stands. Otherwise (the flush must
/// run AND the second action — e.g. a shift-initiated `BeginSelection` — must
/// also run), wrap them in a [`GestureAction::Composite`] so neither is
/// silently dropped (issue #197 review: shift-select after a pending press).
fn merge_actions(first: GestureAction, second: GestureAction) -> GestureAction {
    match (first, second) {
        (GestureAction::ForwardToPty(mut a), GestureAction::ForwardToPty(b)) => {
            a.extend(b);
            GestureAction::ForwardToPty(a)
        }
        (flush, GestureAction::Noop) => flush,
        (first, second) => GestureAction::Composite {
            first: Box::new(first),
            second: Box::new(second),
        },
    }
}

#[cfg(test)]
#[path = "gesture_tests.rs"]
mod gesture_tests;

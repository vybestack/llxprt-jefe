//! Keeping an Enter chord out of the preceding keystroke's burst (issue #627).
//!
//! jefe receives terminal events from a queue that is drained in full whenever
//! the UI gets a chance to poll it. Every keystroke that arrived while a frame
//! was being rendered is therefore handed to the key handler back to back, and
//! each one is written straight into the attached child's PTY. The user's
//! typing rhythm is destroyed: characters typed a tenth of a second apart can
//! reach the child in the same instant.
//!
//! That matters because agent TUIs cannot ask a terminal whether a `CR` came
//! from a keypress or from pasted text, so they infer it from arrival timing —
//! a `CR` that follows another key within a few tens of milliseconds is treated
//! as pasted content and inserted as a newline instead of submitting. Collapsed
//! timing turns a deliberate Enter into a newline and makes modified Enter
//! chords unreachable.
//!
//! jefe cannot restore the original rhythm, but it can stop destroying the
//! separation: an Enter is held back until the guard interval has passed since
//! the last byte jefe wrote to that child.

use std::time::{Duration, Instant};

/// Minimum separation jefe guarantees between the previous byte written to a
/// child's PTY input and an Enter chord.
///
/// Comfortably above the burst windows agent TUIs use (a few tens of
/// milliseconds) while staying short enough to be imperceptible on a keypress
/// that the child then has to act on anyway.
pub const ENTER_INPUT_GAP: Duration = Duration::from_millis(45);

/// What kind of input is being written into a child's PTY.
///
/// Only Enter needs separating; everything else is written immediately and
/// merely marks the moment for the next Enter to measure against.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum PtyInputKind {
    /// An Enter chord, whatever its modifiers. Modified Enter chords are
    /// reclassified by the same burst heuristics as a bare submit, so they are
    /// separated too.
    Enter,
    /// Characters, navigation keys, mouse reports, pastes, query replies.
    #[default]
    Other,
}

/// When the last byte was written to a child's PTY input.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct KeyWritePacing {
    last_write: Option<Instant>,
}

impl KeyWritePacing {
    /// A pacing state that has never written anything.
    #[must_use]
    pub const fn new() -> Self {
        Self { last_write: None }
    }

    /// How long to hold this write back so the child sees it separated from the
    /// previous one.
    ///
    /// Zero for everything except an Enter that follows a write more recently
    /// than [`ENTER_INPUT_GAP`].
    #[must_use]
    pub fn delay_before(&self, kind: PtyInputKind, now: Instant) -> Duration {
        if kind != PtyInputKind::Enter {
            return Duration::ZERO;
        }
        let Some(last_write) = self.last_write else {
            return Duration::ZERO;
        };
        ENTER_INPUT_GAP.saturating_sub(now.saturating_duration_since(last_write))
    }

    /// Record that bytes reached the child at `now`.
    pub fn record(&mut self, now: Instant) {
        self.last_write = Some(now);
    }
}

#[cfg(test)]
#[path = "key_pacing_tests.rs"]
mod tests;

//! Internal chord canonicalization and terminal-intercept helpers extracted to
//! keep `action_registry.rs` below the source-size hard limit.

use super::{Action, HandlerKey};
use crate::domain::keymap::{Chord, Key, Modifier, TerminalClass};

pub(super) fn chords_equivalent(first: &Chord, second: &Chord) -> bool {
    canonical_key(first) == canonical_key(second)
        && [
            Modifier::Ctrl,
            Modifier::Alt,
            Modifier::Shift,
            Modifier::Super,
        ]
        .into_iter()
        .all(|modifier| canonical_modifier(first, modifier) == canonical_modifier(second, modifier))
}

pub(super) fn canonical_key(chord: &Chord) -> Key {
    if chord.key == Key::Tab && chord.modifiers.contains(Modifier::Shift) {
        Key::BackTab
    } else {
        chord.key
    }
}

pub(super) fn canonical_modifier(chord: &Chord, modifier: Modifier) -> bool {
    if modifier == Modifier::Shift && canonical_key(chord) == Key::BackTab {
        false
    } else {
        chord.modifiers.contains(modifier)
    }
}

pub(super) fn terminal_intercepts(action: &Action, chord: &Chord) -> bool {
    matches!(
        action.handler,
        HandlerKey::EmergencyExit | HandlerKey::LeaveTerminal
    ) || matches!(
        action.handler,
        HandlerKey::TerminalScrollPageUp
            | HandlerKey::TerminalScrollPageDown
            | HandlerKey::TerminalScrollTop
            | HandlerKey::TerminalScrollTail
            | HandlerKey::TerminalScrollUp
            | HandlerKey::TerminalScrollDown
    ) && chord.terminal_class() == TerminalClass::ScrollbackCandidate
}

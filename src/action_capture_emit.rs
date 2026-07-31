//! Binary-private emission of strict-harness action captures (issue #383 S8).
//!
//! This is the one place that translates production routing values into the
//! private harness record. It observes; it never decides. Every entry point is
//! a no-op unless the contained schema-1 runner activated capture, so ordinary
//! runs pay only an atomic load.
//!
//! The four keyboard values are captured independently: the original platform
//! event, the canonical chord, the resolution class, and — separately — the
//! exact PTY bytes taken from the same encoder the forwarder uses.

use iocraft::prelude::KeyEvent;

use jefe::domain::action_registry::Resolution;
use jefe::domain::keymap::Chord;
use jefe::harness::v1::action_capture::{
    ActionCaptureRecord, KeyCapture, MouseCapture, OriginalKeyEvent, ResolutionClass,
};
use jefe::harness::v1::action_capture_sink as sink;

fn class_of(resolution: &Resolution) -> ResolutionClass {
    match resolution {
        Resolution::Dispatch { .. } => ResolutionClass::Dispatch,
        Resolution::Unavailable { .. } => ResolutionClass::Unavailable,
        Resolution::ForwardToPty => ResolutionClass::ForwardToPty,
        Resolution::Unbound => ResolutionClass::Unbound,
    }
}

fn original_of(key_event: &KeyEvent) -> OriginalKeyEvent {
    OriginalKeyEvent {
        code: format!("{:?}", key_event.code),
        modifiers: key_event.modifiers.bits(),
    }
}

/// Record one routed keyboard input.
///
/// `pty_bytes` carries the literal encoder output for forwarded input and is
/// empty otherwise; it is never derived from the chord's display text.
pub fn record_key(key_event: &KeyEvent, chord: &Chord, resolution: &Resolution) {
    if !sink::is_active() {
        return;
    }
    let (action, handler) = match resolution {
        Resolution::Dispatch {
            action, handler, ..
        } => (
            Some(action.as_str().to_owned()),
            Some(format!("{handler:?}")),
        ),
        Resolution::Unavailable { action, .. } => (Some(action.as_str().to_owned()), None),
        Resolution::ForwardToPty | Resolution::Unbound => (None, None),
    };
    let pty_bytes = if matches!(resolution, Resolution::ForwardToPty | Resolution::Unbound) {
        crate::pty_encoding::key_to_bytes(key_event, false).unwrap_or_default()
    } else {
        Vec::new()
    };
    let record = ActionCaptureRecord::Key(KeyCapture {
        original: original_of(key_event),
        canonical_chord: chord.to_string(),
        resolution: class_of(resolution),
        action,
        handler,
        pty_bytes,
    });
    sink::record(|| record);
}

/// Record a keyboard input that had no canonical chord translation. The
/// original event and exact forwarded bytes are still observable.
pub fn record_untranslatable(key_event: &KeyEvent, forwarded: bool) {
    if !sink::is_active() {
        return;
    }
    let pty_bytes = if forwarded {
        crate::pty_encoding::key_to_bytes(key_event, false).unwrap_or_default()
    } else {
        Vec::new()
    };
    let record = ActionCaptureRecord::Key(KeyCapture {
        original: original_of(key_event),
        canonical_chord: String::new(),
        resolution: ResolutionClass::ForwardToPty,
        action: None,
        handler: None,
        pty_bytes,
    });
    sink::record(|| record);
}

/// Record one mouse activation: frame, cell, hit surface, and action identity.
pub fn record_mouse(column: u16, row: u16, hit: &str, action: &str, resolution: &Resolution) {
    if !sink::is_active() {
        return;
    }
    let record = ActionCaptureRecord::Mouse(MouseCapture {
        frame: sink::next_frame(),
        column,
        row,
        hit: hit.to_owned(),
        action: action.to_owned(),
        resolution: class_of(resolution),
    });
    sink::record(|| record);
}

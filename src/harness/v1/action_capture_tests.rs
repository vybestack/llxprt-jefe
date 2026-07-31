//! RED-first tests for the private strict-harness action capture protocol
//! (issue #383 S8, decision D9).
//!
//! The capture record must keep four separately observable values for one
//! keyboard input: the original platform event, the canonical chord, the
//! resolution, and the exact PTY bytes. The PTY byte field is never derived
//! from the chord text; it carries the literal encoder output.

use super::action_capture::{
    ActionCaptureRecord, KeyCapture, MouseCapture, OriginalKeyEvent, ResolutionClass,
    decode_records, encode_record,
};

fn forwarded_ctrl_c() -> KeyCapture {
    KeyCapture {
        original: OriginalKeyEvent {
            code: "Char(c)".to_string(),
            modifiers: 0b0000_0010,
        },
        canonical_chord: "Ctrl+c".to_string(),
        resolution: ResolutionClass::ForwardToPty,
        action: None,
        handler: None,
        pty_bytes: vec![0x03],
    }
}

#[test]
fn key_capture_keeps_original_chord_resolution_and_bytes_separate() {
    let capture = forwarded_ctrl_c();

    // Four independently observable values, not one merged blob.
    assert_eq!(capture.original.code, "Char(c)");
    assert_eq!(capture.original.modifiers, 0b0000_0010);
    assert_eq!(capture.canonical_chord, "Ctrl+c");
    assert_eq!(capture.resolution, ResolutionClass::ForwardToPty);
    assert_eq!(capture.pty_bytes, vec![0x03]);

    // The byte field is not a re-encoding of the canonical chord text.
    assert_ne!(capture.pty_bytes, capture.canonical_chord.as_bytes());
}

#[test]
fn dispatched_key_capture_records_action_and_handler_without_pty_bytes() {
    let capture = KeyCapture {
        original: OriginalKeyEvent {
            code: "Char(q)".to_string(),
            modifiers: 0b0000_0010,
        },
        canonical_chord: "Ctrl+q".to_string(),
        resolution: ResolutionClass::Dispatch,
        action: Some("core.emergency-exit".to_string()),
        handler: Some("EmergencyExit".to_string()),
        pty_bytes: Vec::new(),
    };

    assert_eq!(capture.resolution, ResolutionClass::Dispatch);
    assert_eq!(capture.action.as_deref(), Some("core.emergency-exit"));
    assert_eq!(capture.handler.as_deref(), Some("EmergencyExit"));
    assert!(
        capture.pty_bytes.is_empty(),
        "a dispatched action writes no PTY bytes"
    );
}

#[test]
fn function_key_capture_records_exact_xterm_bytes() {
    let capture = KeyCapture {
        original: OriginalKeyEvent {
            code: "F(12)".to_string(),
            modifiers: 0,
        },
        canonical_chord: "F12".to_string(),
        resolution: ResolutionClass::ForwardToPty,
        action: None,
        handler: None,
        pty_bytes: b"\x1b[24~".to_vec(),
    };

    assert_eq!(capture.pty_bytes, b"\x1b[24~".to_vec());
    assert_ne!(capture.pty_bytes, capture.canonical_chord.as_bytes());
}

#[test]
fn mouse_capture_records_frame_cell_hit_and_action() {
    let capture = MouseCapture {
        frame: 7,
        column: 42,
        row: 11,
        hit: "keys.row".to_string(),
        action: "core.open-keys".to_string(),
        resolution: ResolutionClass::Dispatch,
    };

    assert_eq!(capture.frame, 7);
    assert_eq!((capture.column, capture.row), (42, 11));
    assert_eq!(capture.hit, "keys.row");
    assert_eq!(capture.action, "core.open-keys");
    assert_eq!(capture.resolution, ResolutionClass::Dispatch);
}

#[test]
fn records_round_trip_through_the_private_line_protocol() {
    let key = ActionCaptureRecord::Key(forwarded_ctrl_c());
    let mouse = ActionCaptureRecord::Mouse(MouseCapture {
        frame: 3,
        column: 4,
        row: 5,
        hit: "confirm.button".to_string(),
        action: "confirm.accept".to_string(),
        resolution: ResolutionClass::Unavailable,
    });

    let mut buffer = String::new();
    buffer.push_str(&encode_record(&key).unwrap_or_else(|err| panic!("encode key: {err}")));
    buffer.push_str(&encode_record(&mouse).unwrap_or_else(|err| panic!("encode mouse: {err}")));

    let decoded = decode_records(&buffer).unwrap_or_else(|err| panic!("decode records: {err}"));
    assert_eq!(decoded.len(), 2);
    assert_eq!(decoded[0], key);
    assert_eq!(decoded[1], mouse);
}

#[test]
fn malformed_capture_lines_are_rejected_without_panicking() {
    let err = decode_records("{not json}\n")
        .err()
        .unwrap_or_else(|| panic!("malformed capture must fail"));
    assert_eq!(err.code(), super::error::HarCode::E001);
}

#[test]
fn blank_lines_are_ignored_by_the_decoder() {
    let record = ActionCaptureRecord::Key(forwarded_ctrl_c());
    let line = encode_record(&record).unwrap_or_else(|err| panic!("encode: {err}"));
    let decoded =
        decode_records(&format!("\n{line}\n\n")).unwrap_or_else(|err| panic!("decode: {err}"));
    assert_eq!(decoded, vec![record]);
}

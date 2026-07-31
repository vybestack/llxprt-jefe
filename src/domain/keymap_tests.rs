//! Unit tests for the CW-03 S0 single-chord grammar, formatting, bounds,
//! canonical crossterm translation, and terminal PTY-byte classification.

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

use super::keymap::{
    Chord, ChordError, Key, Modifier, ModifierSet, TerminalClass, pty_bytes_for_chord,
};

/// Parse a canonical chord text, panicking with context on grammar failure.
fn parsed(text: &str) -> Chord {
    let Ok(chord) = Chord::parse(text) else {
        panic!("chord {text:?} should parse");
    };
    chord
}

/// Translate a crossterm key event, panicking with context on failure.
fn translated(event: &KeyEvent) -> Chord {
    let Ok(chord) = Chord::from_crossterm(event) else {
        panic!("key event {event:?} should translate");
    };
    chord
}

/// Encode a chord for the PTY, panicking with context on failure.
fn pty_bytes(chord: &Chord) -> Vec<u8> {
    let Ok(bytes) = pty_bytes_for_chord(chord) else {
        panic!("chord {chord} should encode for the PTY");
    };
    bytes
}

// ── ModifierSet ────────────────────────────────────────────────────────────

#[test]
fn modifier_set_is_empty_by_default() {
    let set = ModifierSet::default();
    assert!(set.is_empty());
    assert!(!set.contains(Modifier::Ctrl));
}

#[test]
fn modifier_set_from_bits_round_trips_each_modifier() {
    for modifier in [
        Modifier::Ctrl,
        Modifier::Alt,
        Modifier::Shift,
        Modifier::Super,
    ] {
        let set = ModifierSet::from_modifier(modifier);
        assert!(set.contains(modifier), "{modifier:?} should be present");
        assert_eq!(set.iter().count(), 1);
    }
}

#[test]
fn modifier_set_rejects_duplicate_modifier_insert() {
    let mut set = ModifierSet::from_modifier(Modifier::Ctrl);
    let result = set.insert(Modifier::Ctrl);
    assert!(result.is_err(), "duplicate modifier insert must error");
}

// ── Chord grammar: parse ───────────────────────────────────────────────────

#[test]
fn chord_parse_single_char_lowercase() {
    let chord = parsed("q");
    assert_eq!(chord.key, Key::Char('q'));
    assert!(chord.modifiers.is_empty());
}

#[test]
fn chord_parse_uppercase_char_preserves_scalar_without_inventing_shift() {
    let chord = parsed("S");
    assert_eq!(chord.key, Key::Char('S'));
    assert!(
        !chord.modifiers.contains(Modifier::Shift),
        "Shift provenance must come from an explicit modifier"
    );

    let shifted = parsed("Shift+S");
    assert_eq!(shifted.key, Key::Char('S'));
    assert!(shifted.modifiers.contains(Modifier::Shift));
}

#[test]
fn chord_parse_ctrl_alt_shift_super_then_key_in_canonical_order() {
    let chord = parsed("Ctrl+Alt+Shift+Super+a");
    assert!(chord.modifiers.contains(Modifier::Ctrl));
    assert!(chord.modifiers.contains(Modifier::Alt));
    assert!(chord.modifiers.contains(Modifier::Shift));
    assert!(chord.modifiers.contains(Modifier::Super));
    assert_eq!(chord.key, Key::Char('a'));
}

#[test]
fn chord_parse_accepts_any_modifier_order_and_normalizes() {
    let a = parsed("Shift+Ctrl+c");
    let b = parsed("Ctrl+Shift+c");
    assert_eq!(a, b, "modifier order must normalize");
}

#[test]
fn chord_parse_named_keys_use_exact_enum_spelling() {
    let chord = parsed("Enter");
    assert_eq!(chord.key, Key::Enter);
    assert!(chord.modifiers.is_empty());

    let pagedown = parsed("PageDown");
    assert_eq!(pagedown.key, Key::PageDown);
}

#[test]
fn chord_parse_backtab_named_key() {
    let chord = parsed("BackTab");
    assert_eq!(chord.key, Key::BackTab);
}

#[test]
fn chord_parse_function_keys_f1_through_f24() {
    for n in 1..=24u8 {
        let text = format!("F{n}");
        let chord = Chord::parse(&text).unwrap_or_else(|err| panic!("F{n} should parse: {err}"));
        assert_eq!(chord.key, Key::Function(n));
    }
}

#[test]
fn chord_parse_rejects_function_key_out_of_range() {
    assert!(Chord::parse("F0").is_err(), "F0 must be rejected");
    assert!(Chord::parse("F25").is_err(), "F25 must be rejected");
}

#[test]
fn chord_parse_rejects_modifier_only() {
    for modifier_only in [
        "Ctrl",
        "Alt",
        "Shift",
        "Super",
        "Ctrl+Alt",
        "Ctrl+Shift+Alt+Super",
    ] {
        let result = Chord::parse(modifier_only);
        assert!(
            matches!(result, Err(ChordError::ModifierOnly)),
            "{modifier_only:?} must be modifier-only"
        );
    }
}

#[test]
fn chord_parse_rejects_duplicate_modifier() {
    let result = Chord::parse("Ctrl+Ctrl+a");
    assert!(
        matches!(result, Err(ChordError::DuplicateModifier)),
        "duplicate modifier must be rejected"
    );
}

#[test]
fn chord_parse_rejects_unknown_named_key() {
    let result = Chord::parse("Space");
    assert!(
        matches!(result, Err(ChordError::UnknownKey)),
        "unknown named key must be rejected"
    );
}

#[test]
fn chord_parse_rejects_multiple_scalars_or_sequence() {
    // A sequence / multi-scalar is outside the closed single-chord grammar.
    assert!(Chord::parse("ab").is_err());
    assert!(Chord::parse("Ctrl+a b").is_err());
}

#[test]
fn chord_parse_rejects_non_unicode_scalar_value() {
    // The grammar accepts one Unicode scalar; a two-scalar grapheme cluster is
    // rejected. Using a combining sequence (e + combining acute).
    let result = Chord::parse("e\u{0301}");
    assert!(result.is_err(), "multi-scalar input must be rejected");
}

// ── Chord formatting ───────────────────────────────────────────────────────

#[test]
fn chord_format_emits_canonical_modifier_order() {
    let chord = parsed("Shift+Ctrl+c");
    assert_eq!(chord.to_canonical_text(), "Ctrl+Shift+c");
}

#[test]
fn chord_format_plain_named_key() {
    let chord = parsed("PageUp");
    assert_eq!(chord.to_canonical_text(), "PageUp");
}

#[test]
fn chord_format_function_key() {
    let chord = parsed("F12");
    assert_eq!(chord.to_canonical_text(), "F12");
}

#[test]
fn chord_parse_and_format_round_trip() {
    for text in [
        "q",
        "S",
        "Ctrl+C",
        "Ctrl+Shift+S",
        "Alt+1",
        "Alt+9",
        "F1",
        "F24",
        "Enter",
        "Esc",
        "Tab",
        "BackTab",
        "Backspace",
        "Delete",
        "Insert",
        "Home",
        "End",
        "PageUp",
        "PageDown",
        "Up",
        "Down",
        "Left",
        "Right",
        "Ctrl+Q",
    ] {
        let chord = Chord::parse(text).unwrap_or_else(|err| panic!("parse {text:?}: {err}"));
        assert_eq!(
            chord.to_canonical_text(),
            text,
            "round-trip must preserve canonical text for {text:?}"
        );
    }
}

// ── Chord equality preserves explicit Shift provenance ─────────────────────

#[test]
fn chord_uppercase_scalar_is_distinct_from_explicit_shift() {
    let scalar = parsed("A");
    let explicit = parsed("Shift+A");
    assert_ne!(scalar, explicit);
    assert_eq!(scalar.to_canonical_text(), "A");
    assert_eq!(explicit.to_canonical_text(), "Shift+A");
}

#[test]
fn chord_ctrl_c_distinct_from_bare_c() {
    let ctrl_c = parsed("Ctrl+C");
    let bare_c = parsed("c");
    assert_ne!(ctrl_c, bare_c);
}

// ── crossterm KeyEvent canonical translation (CW03-07/08) ──────────────────

fn event(code: KeyCode, modifiers: KeyModifiers) -> KeyEvent {
    KeyEvent::new(code, modifiers)
}

#[test]
fn from_crossterm_bare_char_preserves_scalar() {
    let key_event = event(KeyCode::Char('c'), KeyModifiers::NONE);
    let chord = translated(&key_event);
    assert_eq!(chord.key, Key::Char('c'));
    assert!(chord.modifiers.is_empty());
}

#[test]
fn from_crossterm_uppercase_char_preserves_scalar_and_shift_provenance() {
    let key_event = event(KeyCode::Char('S'), KeyModifiers::SHIFT);
    let chord = translated(&key_event);
    assert_eq!(chord.key, Key::Char('S'));
    assert!(
        chord.modifiers.contains(Modifier::Shift),
        "uppercase scalar must record explicit Shift provenance"
    );
}

#[test]
fn from_crossterm_ctrl_c() {
    let key_event = event(KeyCode::Char('c'), KeyModifiers::CONTROL);
    let chord = translated(&key_event);
    assert_eq!(chord.key, Key::Char('c'));
    assert!(chord.modifiers.contains(Modifier::Ctrl));
    assert_eq!(chord.to_canonical_text(), "Ctrl+C");
}

#[test]
fn from_crossterm_ctrl_shift_c_does_not_double_shift() {
    // A lowercase 'c' with Ctrl+Shift records Shift but keeps the lowercase
    // scalar (no uppercase folding when the scalar is already lowercase).
    let key_event = event(
        KeyCode::Char('c'),
        KeyModifiers::CONTROL | KeyModifiers::SHIFT,
    );
    let chord = translated(&key_event);
    assert_eq!(chord.key, Key::Char('c'));
    assert!(chord.modifiers.contains(Modifier::Ctrl));
    assert!(chord.modifiers.contains(Modifier::Shift));
}

#[test]
fn from_crossterm_alt_digit() {
    let key_event = event(KeyCode::Char('1'), KeyModifiers::ALT);
    let chord = translated(&key_event);
    assert_eq!(chord.key, Key::Char('1'));
    assert!(chord.modifiers.contains(Modifier::Alt));
}

#[test]
fn from_crossterm_function_key() {
    let key_event = event(KeyCode::F(12), KeyModifiers::NONE);
    let chord = translated(&key_event);
    assert_eq!(chord.key, Key::Function(12));
}

#[test]
fn from_crossterm_named_keys() {
    for (code, expected) in [
        (KeyCode::Enter, Key::Enter),
        (KeyCode::Esc, Key::Esc),
        (KeyCode::Tab, Key::Tab),
        (KeyCode::BackTab, Key::BackTab),
        (KeyCode::Backspace, Key::Backspace),
        (KeyCode::Delete, Key::Delete),
        (KeyCode::Insert, Key::Insert),
        (KeyCode::Home, Key::Home),
        (KeyCode::End, Key::End),
        (KeyCode::PageUp, Key::PageUp),
        (KeyCode::PageDown, Key::PageDown),
        (KeyCode::Up, Key::Up),
        (KeyCode::Down, Key::Down),
        (KeyCode::Left, Key::Left),
        (KeyCode::Right, Key::Right),
    ] {
        let key_event = event(code, KeyModifiers::NONE);
        let chord =
            Chord::from_crossterm(&key_event).unwrap_or_else(|err| panic!("{code:?}: {err}"));
        assert_eq!(chord.key, expected);
    }
}

#[test]
fn from_crossterm_meta_modifier_is_unsupported_typed_error() {
    let key_event = event(KeyCode::Char('a'), KeyModifiers::META);
    let result = Chord::from_crossterm(&key_event);
    assert!(
        result.is_err(),
        "META must fail as a typed error, not be silently accepted"
    );
    assert!(
        matches!(result, Err(ChordError::UnsupportedModifier)),
        "META must classify as an unsupported modifier"
    );
}

#[test]
fn from_crossterm_hyper_modifier_is_unsupported_typed_error() {
    let key_event = event(KeyCode::Char('a'), KeyModifiers::HYPER);
    let result = Chord::from_crossterm(&key_event);
    assert!(
        matches!(result, Err(ChordError::UnsupportedModifier)),
        "HYPER must classify as an unsupported modifier"
    );
}

#[test]
fn from_crossterm_super_modifiers_supported() {
    let key_event = event(KeyCode::Char('a'), KeyModifiers::SUPER);
    let chord = translated(&key_event);
    assert!(chord.modifiers.contains(Modifier::Super));
}

#[test]
fn from_crossterm_null_key_is_unsupported() {
    let key_event = event(KeyCode::Null, KeyModifiers::NONE);
    assert!(Chord::from_crossterm(&key_event).is_err());
}

// ── Terminal PTY-byte classification (CW03-08 groundwork) ──────────────────

#[test]
fn terminal_class_plain_ctrl_c_is_forward_to_pty() {
    let chord = parsed("Ctrl+C");
    let class = chord.terminal_class();
    assert_eq!(class, TerminalClass::ForwardToPty);
}

#[test]
fn terminal_class_scrollback_keys_when_unmodified() {
    for text in ["PageUp", "PageDown", "Home", "End", "Up", "Down"] {
        let chord = Chord::parse(text).unwrap_or_else(|err| panic!("parse {text}: {err}"));
        // Unmodified scrollback keys are candidate interception; the exact
        // gating conditions live in the runtime. S0 classifies the canonical
        // chord family.
        assert!(
            matches!(chord.terminal_class(), TerminalClass::ScrollbackCandidate),
            "{text} unmodified is a scrollback candidate"
        );
    }
}

#[test]
fn terminal_class_modified_scrollback_key_forwards() {
    let chord = parsed("Ctrl+End");
    // Modified scrollback keys forward to the PTY (matches input.rs policy).
    assert!(
        matches!(chord.terminal_class(), TerminalClass::ForwardToPty),
        "modified scrollback key forwards"
    );
}

#[test]
fn pty_bytes_for_ctrl_c_is_byte_0x03() {
    let chord = parsed("Ctrl+C");
    let bytes = pty_bytes(&chord);
    assert_eq!(bytes, vec![0x03]);
}

#[test]
fn pty_bytes_for_enter_is_cr() {
    let chord = parsed("Enter");
    let bytes = pty_bytes(&chord);
    assert_eq!(bytes, vec![b'\r']);
}

#[test]
fn pty_bytes_for_alt_x_esc_prefixed() {
    let chord = parsed("Alt+X");
    let bytes = pty_bytes(&chord);
    assert_eq!(bytes, vec![0x1b, b'X']);
}

#[test]
fn uppercase_without_shift_preserves_scalar_only() {
    let chord = translated(&event(KeyCode::Char('S'), KeyModifiers::NONE));
    assert_eq!(chord.key, Key::Char('S'));
    assert!(!chord.modifiers.contains(Modifier::Shift));
    assert_eq!(chord.to_canonical_text(), "S");
}

#[test]
fn parsed_ctrl_c_equals_ordinary_terminal_ctrl_c_event() {
    let compiled = parsed("Ctrl+C");
    let terminal = translated(&event(KeyCode::Char('c'), KeyModifiers::CONTROL));
    assert_eq!(compiled, terminal);
}

#[test]
fn pty_bytes_match_shift_and_alt_shift_enter() {
    let shift = parsed("Shift+Enter");
    let alt_shift = parsed("Alt+Shift+Enter");
    assert_eq!(pty_bytes_for_chord(&shift), Ok(b"\\\r".to_vec()));
    assert_eq!(pty_bytes_for_chord(&alt_shift), Ok(b"\\\x1b\r".to_vec()));
}

#[test]
fn pty_bytes_match_modified_edit_key_parameters() {
    let cases = [
        ("Ctrl+PageUp", b"\x1b[5;5~".as_slice()),
        ("Alt+PageDown", b"\x1b[6;3~".as_slice()),
        ("Shift+Delete", b"\x1b[3;2~".as_slice()),
        ("Ctrl+Alt+Insert", b"\x1b[2;7~".as_slice()),
        ("Shift+Home", b"\x1b[1;2H".as_slice()),
        ("Ctrl+End", b"\x1b[1;5F".as_slice()),
    ];
    for (text, expected) in cases {
        let chord = Chord::parse(text).unwrap_or_else(|error| panic!("{text}: {error}"));
        assert_eq!(
            pty_bytes_for_chord(&chord).as_deref(),
            Ok(expected),
            "{text}"
        );
    }
}

#[test]
fn pty_bytes_do_not_double_prefix_alt_function_keys() {
    let chord = parsed("Alt+F1");
    assert_eq!(pty_bytes_for_chord(&chord), Ok(b"\x1b[1;3P".to_vec()));
}

#[test]
fn pty_bytes_reject_values_production_does_not_encode() {
    for text in ["BackTab", "F13", "F24"] {
        let chord = Chord::parse(text).unwrap_or_else(|error| panic!("{text}: {error}"));
        assert!(pty_bytes_for_chord(&chord).is_err(), "{text}");
    }
}

#[test]
fn backtab_translation_removes_redundant_shift_bit() {
    let result = translated(&event(KeyCode::BackTab, KeyModifiers::SHIFT));
    assert_eq!(result, parsed("BackTab"));
}

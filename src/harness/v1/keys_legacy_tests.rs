//! RED-first tests for legacy tmux key-spelling translation (issue #383 S8).
//!
//! Converted scenarios keep the legacy tmux vocabulary. The runner translates
//! those spellings to the existing closed schema-1 key table before encoding,
//! so every produced byte sequence is exactly what the current encoder already
//! emits. No driver byte changes.

use super::contract::Modifier;
use super::keys::encode;
use super::keys_legacy::translate;

fn translated(spelling: &str) -> (String, Vec<Modifier>) {
    translate(spelling).unwrap_or_else(|| panic!("'{spelling}' must translate"))
}

#[test]
fn named_legacy_spellings_map_to_canonical_keys() {
    for (legacy, key) in [
        ("Esc", "escape"),
        ("Escape", "escape"),
        ("BSpace", "backspace"),
        ("Backspace", "backspace"),
        ("Space", "space"),
        ("Enter", "enter"),
        ("Tab", "tab"),
        ("BTab", "backtab"),
        ("PageUp", "pageup"),
        ("PageDown", "pagedown"),
        ("Home", "home"),
        ("End", "end"),
        ("Up", "up"),
        ("Down", "down"),
        ("Left", "left"),
        ("Right", "right"),
        ("Delete", "delete"),
        ("F1", "f1"),
        ("F12", "f12"),
    ] {
        let (name, modifiers) = translated(legacy);
        assert_eq!(name, key, "{legacy}");
        assert!(modifiers.is_empty(), "{legacy} takes no modifier");
    }
}

#[test]
fn control_and_alt_prefixes_become_modifiers() {
    let (name, modifiers) = translated("C-q");
    assert_eq!(name, "q");
    assert_eq!(modifiers, vec![Modifier::Control]);

    let (name, modifiers) = translated("M-3");
    assert_eq!(name, "3");
    assert_eq!(modifiers, vec![Modifier::Alt]);

    let (name, modifiers) = translated("M-Enter");
    assert_eq!(name, "enter");
    assert_eq!(modifiers, vec![Modifier::Alt]);
}

#[test]
fn back_tab_is_its_own_key_and_uppercase_letters_use_shift() {
    // Back-tab is a distinct terminal key (CSI Z), not a shifted tab.
    let (name, modifiers) = translated("BTab");
    assert_eq!(name, "backtab");
    assert!(modifiers.is_empty());

    let (name, modifiers) = translated("N");
    assert_eq!(name, "n");
    assert_eq!(modifiers, vec![Modifier::Shift]);
}

#[test]
fn translation_preserves_exact_encoder_bytes() {
    for (legacy, expected) in [
        ("Esc", b"\x1b".to_vec()),
        ("BSpace", b"\x7f".to_vec()),
        ("BTab", b"\x1b[Z".to_vec()),
        ("Space", b" ".to_vec()),
        ("Enter", b"\r".to_vec()),
        ("PageUp", b"\x1b[5~".to_vec()),
        ("F12", b"\x1b[24~".to_vec()),
        ("C-q", vec![0x11]),
        ("C-c", vec![0x03]),
        ("M-3", vec![0x1b, b'3']),
        ("N", b"N".to_vec()),
    ] {
        let (name, modifiers) = translated(legacy);
        let bytes = encode("legacy", &name, &modifiers)
            .unwrap_or_else(|err| panic!("{legacy} should encode: {err}"));
        assert_eq!(bytes, expected, "{legacy}");
    }
}

#[test]
fn canonical_spellings_are_not_claimed_by_the_translator() {
    for canonical in ["escape", "enter", "a", "f5", "pagedown"] {
        assert!(
            translate(canonical).is_none(),
            "'{canonical}' is already canonical"
        );
    }
}

#[test]
fn unknown_spellings_do_not_translate() {
    for unknown in ["C-", "M-", "Bogus", "F13", ""] {
        assert!(translate(unknown).is_none(), "{unknown:?}");
    }
}

//! Closed key-to-PTY-byte encoding (issue #380).
//!
//! `key` steps name a key from a closed table plus at most three modifiers.
//! Encoding is validated at parse time so an unknown key fails before any
//! workspace or launch work, and the runner reuses the same single encoder
//! for delivery (one owner, no drift).

use super::contract::Modifier;
use super::error::HarnessError;

/// Encode a key plus modifiers into the bytes written to the PTY master.
///
/// # Errors
///
/// `HAR-E001` for unknown key names or unsupported modifier combinations.
pub fn encode(field: &str, key: &str, modifiers: &[Modifier]) -> Result<Vec<u8>, HarnessError> {
    let alt = modifiers.contains(&Modifier::Alt);
    let control = modifiers.contains(&Modifier::Control);
    let shift = modifiers.contains(&Modifier::Shift);
    let mut bytes = encode_base(field, key, control, shift)?;
    if alt {
        let mut prefixed = vec![0x1b];
        prefixed.append(&mut bytes);
        bytes = prefixed;
    }
    Ok(bytes)
}

fn encode_base(
    field: &str,
    key: &str,
    control: bool,
    shift: bool,
) -> Result<Vec<u8>, HarnessError> {
    if let Some(bytes) = encode_named(key) {
        if control || shift {
            return Err(HarnessError::syntax(format!(
                "{field}: control/shift modifiers are not supported for named key '{key}'"
            )));
        }
        return Ok(bytes);
    }
    let mut chars = key.chars();
    let (Some(ch), None) = (chars.next(), chars.next()) else {
        return Err(HarnessError::syntax(format!(
            "{field}: unknown key '{key}'"
        )));
    };
    encode_char(field, ch, control, shift)
}

fn encode_char(field: &str, ch: char, control: bool, shift: bool) -> Result<Vec<u8>, HarnessError> {
    if control {
        let lower = ch.to_ascii_lowercase();
        if !lower.is_ascii_lowercase() {
            return Err(HarnessError::syntax(format!(
                "{field}: control modifier requires a letter, got '{ch}'"
            )));
        }
        return Ok(vec![(lower as u8) - b'a' + 1]);
    }
    let effective = if shift { ch.to_ascii_uppercase() } else { ch };
    let mut buffer = [0u8; 4];
    Ok(effective.encode_utf8(&mut buffer).as_bytes().to_vec())
}

fn encode_named(key: &str) -> Option<Vec<u8>> {
    let bytes: &[u8] = match key {
        "enter" => b"\r",
        "tab" => b"\t",
        "escape" => b"\x1b",
        "backspace" => b"\x7f",
        "space" => b" ",
        "up" => b"\x1b[A",
        "down" => b"\x1b[B",
        "right" => b"\x1b[C",
        "left" => b"\x1b[D",
        "home" => b"\x1b[H",
        "end" => b"\x1b[F",
        "pageup" => b"\x1b[5~",
        "pagedown" => b"\x1b[6~",
        "insert" => b"\x1b[2~",
        "delete" => b"\x1b[3~",
        "f1" => b"\x1bOP",
        "f2" => b"\x1bOQ",
        "f3" => b"\x1bOR",
        "f4" => b"\x1bOS",
        "f5" => b"\x1b[15~",
        "f6" => b"\x1b[17~",
        "f7" => b"\x1b[18~",
        "f8" => b"\x1b[19~",
        "f9" => b"\x1b[20~",
        "f10" => b"\x1b[21~",
        "f11" => b"\x1b[23~",
        "f12" => b"\x1b[24~",
        _ => return None,
    };
    Some(bytes.to_vec())
}

#[cfg(test)]
mod tests {
    use super::super::contract::Modifier;
    use super::super::error::HarCode;
    use super::encode;

    #[test]
    fn named_keys_encode_to_expected_sequences() {
        let cases: &[(&str, &[u8])] = &[
            ("enter", b"\r"),
            ("escape", b"\x1b"),
            ("up", b"\x1b[A"),
            ("f5", b"\x1b[15~"),
            ("f12", b"\x1b[24~"),
            ("pagedown", b"\x1b[6~"),
        ];
        for (key, expected) in cases {
            let bytes =
                encode("f", key, &[]).unwrap_or_else(|err| panic!("{key} should encode: {err}"));
            assert_eq!(&bytes, expected, "{key}");
        }
    }

    #[test]
    fn characters_and_modifiers_encode() {
        let plain = encode("f", "a", &[]).unwrap_or_else(|err| panic!("should encode: {err}"));
        assert_eq!(plain, b"a");
        let shifted = encode("f", "a", &[Modifier::Shift])
            .unwrap_or_else(|err| panic!("should encode: {err}"));
        assert_eq!(shifted, b"A");
        let control = encode("f", "c", &[Modifier::Control])
            .unwrap_or_else(|err| panic!("should encode: {err}"));
        assert_eq!(control, vec![3]);
        let alt =
            encode("f", "x", &[Modifier::Alt]).unwrap_or_else(|err| panic!("should encode: {err}"));
        assert_eq!(alt, vec![0x1b, b'x']);
        let alt_named = encode("f", "enter", &[Modifier::Alt])
            .unwrap_or_else(|err| panic!("should encode: {err}"));
        assert_eq!(alt_named, vec![0x1b, b'\r']);
    }

    #[test]
    fn unknown_keys_and_bad_modifiers_are_e001() {
        for (key, modifiers) in [
            ("f13", vec![]),
            ("bogus", vec![]),
            ("", vec![]),
            ("enter", vec![Modifier::Control]),
            ("5", vec![Modifier::Control]),
        ] {
            let err = encode("f", key, &modifiers)
                .err()
                .unwrap_or_else(|| panic!("{key:?} must fail"));
            assert_eq!(err.code(), HarCode::E001, "{key:?}");
        }
    }
}

//! Legacy tmux key-spelling translation (issue #383 S8).
//!
//! Converted scenarios keep the legacy tmux key vocabulary (`Esc`, `BTab`,
//! `C-q`, `M-3`, `N`). This module is a pure name-to-name mapping onto the
//! existing closed schema-1 key table plus its modifier list. Encoding stays
//! with [`super::keys::encode`], so every produced byte sequence is exactly
//! what the current encoder already emits: translation never changes a
//! driver byte.

use super::contract::Modifier;

/// Translate a legacy spelling into a canonical key name plus modifiers.
///
/// Returns `None` when `spelling` is already canonical or is not a legacy
/// spelling, so callers fall through to the existing encoder behavior
/// (including its `HAR-E001` diagnostics for unknown keys).
#[must_use]
pub fn translate(spelling: &str) -> Option<(String, Vec<Modifier>)> {
    if let Some(rest) = spelling.strip_prefix("C-") {
        return prefixed(rest, Modifier::Control);
    }
    if let Some(rest) = spelling.strip_prefix("M-") {
        return prefixed(rest, Modifier::Alt);
    }
    if let Some(name) = named(spelling) {
        return Some((name.to_string(), Vec::new()));
    }
    uppercase_letter(spelling)
}

/// Translate the payload of a `C-`/`M-` prefix, keeping any nested naming.
fn prefixed(rest: &str, modifier: Modifier) -> Option<(String, Vec<Modifier>)> {
    if rest.is_empty() {
        return None;
    }
    let (name, mut modifiers) = match translate(rest) {
        Some(translated) => translated,
        None => (rest.to_string(), Vec::new()),
    };
    if modifiers.contains(&modifier) {
        return None;
    }
    modifiers.push(modifier);
    Some((name, modifiers))
}

/// A single uppercase ASCII letter is the legacy spelling for Shift+letter.
fn uppercase_letter(spelling: &str) -> Option<(String, Vec<Modifier>)> {
    let mut chars = spelling.chars();
    let (Some(character), None) = (chars.next(), chars.next()) else {
        return None;
    };
    if !character.is_ascii_uppercase() {
        return None;
    }
    Some((
        character.to_ascii_lowercase().to_string(),
        vec![Modifier::Shift],
    ))
}

/// Legacy named keys that differ from the canonical schema-1 spelling only
/// by capitalization or by a historical tmux alias.
fn named(spelling: &str) -> Option<&'static str> {
    let canonical = match spelling {
        "Esc" | "Escape" => "escape",
        "BSpace" | "Backspace" => "backspace",
        "Space" => "space",
        "Enter" => "enter",
        "Tab" => "tab",
        "BTab" => "backtab",
        "PageUp" => "pageup",
        "PageDown" => "pagedown",
        "Home" => "home",
        "End" => "end",
        "Up" => "up",
        "Down" => "down",
        "Left" => "left",
        "Right" => "right",
        "Insert" => "insert",
        "Delete" => "delete",
        "F1" => "f1",
        "F2" => "f2",
        "F3" => "f3",
        "F4" => "f4",
        "F5" => "f5",
        "F6" => "f6",
        "F7" => "f7",
        "F8" => "f8",
        "F9" => "f9",
        "F10" => "f10",
        "F11" => "f11",
        "F12" => "f12",
        _ => return None,
    };
    Some(canonical)
}

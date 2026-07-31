//! CW-03 closed single-chord grammar and platform translation (issue #383).
//!
//! Chords preserve the reported Unicode scalar and explicit modifier bits. The
//! one deliberate textual normalization is the conventional control-letter
//! spelling: `Ctrl+C` denotes the same value as an ordinary terminal event
//! containing `CONTROL + Char('c')`. Multi-chord sequences are not part of this
//! grammar.

use std::fmt;

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

/// Maximum chords in one effective action/context binding.
pub const MAX_CHORDS_PER_BINDING: usize = 8;
/// Maximum bindings in one composed keymap.
pub const MAX_EFFECTIVE_BINDINGS: usize = 2048;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Modifier {
    Ctrl,
    Alt,
    Shift,
    Super,
}

impl Modifier {
    const fn as_str(self) -> &'static str {
        match self {
            Self::Ctrl => "Ctrl",
            Self::Alt => "Alt",
            Self::Shift => "Shift",
            Self::Super => "Super",
        }
    }

    fn parse(value: &str) -> Option<Self> {
        match value {
            "Ctrl" => Some(Self::Ctrl),
            "Alt" => Some(Self::Alt),
            "Shift" => Some(Self::Shift),
            "Super" => Some(Self::Super),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
pub struct ModifierSet(u8);

impl ModifierSet {
    const CTRL: u8 = 1;
    const ALT: u8 = 2;
    const SHIFT: u8 = 4;
    const SUPER: u8 = 8;

    #[must_use]
    pub const fn empty() -> Self {
        Self(0)
    }

    #[must_use]
    pub const fn from_modifier(modifier: Modifier) -> Self {
        Self(Self::bit(modifier))
    }

    const fn bit(modifier: Modifier) -> u8 {
        match modifier {
            Modifier::Ctrl => Self::CTRL,
            Modifier::Alt => Self::ALT,
            Modifier::Shift => Self::SHIFT,
            Modifier::Super => Self::SUPER,
        }
    }

    #[must_use]
    pub const fn is_empty(self) -> bool {
        self.0 == 0
    }

    #[must_use]
    pub const fn contains(self, modifier: Modifier) -> bool {
        self.0 & Self::bit(modifier) != 0
    }

    pub fn insert(&mut self, modifier: Modifier) -> Result<(), ChordError> {
        if self.contains(modifier) {
            return Err(ChordError::DuplicateModifier);
        }
        self.0 |= Self::bit(modifier);
        Ok(())
    }

    fn remove(&mut self, modifier: Modifier) {
        self.0 &= !Self::bit(modifier);
    }

    #[must_use]
    pub fn iter(self) -> ModifierIter {
        ModifierIter {
            order: [
                (Modifier::Ctrl, self.contains(Modifier::Ctrl)),
                (Modifier::Alt, self.contains(Modifier::Alt)),
                (Modifier::Shift, self.contains(Modifier::Shift)),
                (Modifier::Super, self.contains(Modifier::Super)),
            ],
            index: 0,
        }
    }
}

pub struct ModifierIter {
    order: [(Modifier, bool); 4],
    index: usize,
}

impl Iterator for ModifierIter {
    type Item = Modifier;

    fn next(&mut self) -> Option<Self::Item> {
        while self.index < self.order.len() {
            let (modifier, present) = self.order[self.index];
            self.index += 1;
            if present {
                return Some(modifier);
            }
        }
        None
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Key {
    Char(char),
    Enter,
    Esc,
    Tab,
    BackTab,
    Backspace,
    Delete,
    Insert,
    Home,
    End,
    PageUp,
    PageDown,
    Up,
    Down,
    Left,
    Right,
    Function(u8),
}

impl Key {
    fn parse(value: &str) -> Result<Self, ChordError> {
        if let Some(digits) = value.strip_prefix('F')
            && !digits.is_empty()
            && digits.bytes().all(|byte| byte.is_ascii_digit())
        {
            let number = digits.parse::<u8>().map_err(|_| ChordError::UnknownKey)?;
            return if (1..=24).contains(&number) {
                Ok(Self::Function(number))
            } else {
                Err(ChordError::UnknownKey)
            };
        }
        let named = match value {
            "Enter" => Some(Self::Enter),
            "Esc" => Some(Self::Esc),
            "Tab" => Some(Self::Tab),
            "BackTab" => Some(Self::BackTab),
            "Backspace" => Some(Self::Backspace),
            "Delete" => Some(Self::Delete),
            "Insert" => Some(Self::Insert),
            "Home" => Some(Self::Home),
            "End" => Some(Self::End),
            "PageUp" => Some(Self::PageUp),
            "PageDown" => Some(Self::PageDown),
            "Up" => Some(Self::Up),
            "Down" => Some(Self::Down),
            "Left" => Some(Self::Left),
            "Right" => Some(Self::Right),
            _ => None,
        };
        if let Some(key) = named {
            return Ok(key);
        }

        let mut scalars = value.chars();
        let Some(first) = scalars.next() else {
            return Err(ChordError::UnknownKey);
        };
        if scalars.next().is_none() {
            return Ok(Self::Char(first));
        }
        if looks_like_named_key(value) {
            Err(ChordError::UnknownKey)
        } else {
            Err(ChordError::MultipleScalars)
        }
    }

    fn text(self, modifiers: ModifierSet) -> String {
        match self {
            Self::Char(character)
                if modifiers.contains(Modifier::Ctrl)
                    && !modifiers.contains(Modifier::Shift)
                    && character.is_ascii_lowercase() =>
            {
                character.to_ascii_uppercase().to_string()
            }
            Self::Char(character) => character.to_string(),
            Self::Enter => "Enter".to_owned(),
            Self::Esc => "Esc".to_owned(),
            Self::Tab => "Tab".to_owned(),
            Self::BackTab => "BackTab".to_owned(),
            Self::Backspace => "Backspace".to_owned(),
            Self::Delete => "Delete".to_owned(),
            Self::Insert => "Insert".to_owned(),
            Self::Home => "Home".to_owned(),
            Self::End => "End".to_owned(),
            Self::PageUp => "PageUp".to_owned(),
            Self::PageDown => "PageDown".to_owned(),
            Self::Up => "Up".to_owned(),
            Self::Down => "Down".to_owned(),
            Self::Left => "Left".to_owned(),
            Self::Right => "Right".to_owned(),
            Self::Function(number) => format!("F{number}"),
        }
    }
}

fn looks_like_named_key(value: &str) -> bool {
    value
        .chars()
        .next()
        .is_some_and(|character| character.is_ascii_uppercase())
        && value
            .chars()
            .all(|character| character.is_ascii_alphanumeric())
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Chord {
    pub modifiers: ModifierSet,
    pub key: Key,
}

impl Chord {
    #[must_use]
    pub const fn new(modifiers: ModifierSet, key: Key) -> Self {
        Self { modifiers, key }
    }

    /// Parse one canonical chord.
    ///
    /// # Errors
    /// Returns a typed grammar error when the complete value is invalid.
    pub fn parse(text: &str) -> Result<Self, ChordError> {
        if text.is_empty() {
            return Err(ChordError::UnknownKey);
        }
        if text.chars().count() == 1 {
            return Ok(Self::new(ModifierSet::empty(), Key::parse(text)?));
        }

        let (modifier_text, key_text) = if text == "+" {
            (None, "+")
        } else if let Some(prefix) = text.strip_suffix("++") {
            (Some(prefix), "+")
        } else if let Some((prefix, key)) = text.rsplit_once('+') {
            (Some(prefix), key)
        } else if Modifier::parse(text).is_some() {
            return Err(ChordError::ModifierOnly);
        } else {
            (None, text)
        };

        let mut modifiers = ModifierSet::empty();
        if let Some(prefix) = modifier_text {
            if prefix.is_empty() {
                return Err(ChordError::UnknownKey);
            }
            for token in prefix.split('+') {
                let modifier = Modifier::parse(token).ok_or(ChordError::UnknownKey)?;
                modifiers.insert(modifier)?;
            }
        }
        if Modifier::parse(key_text).is_some() {
            return Err(ChordError::ModifierOnly);
        }
        let key = normalize_control_character(modifiers, Key::parse(key_text)?);
        Ok(Self { modifiers, key })
    }

    #[must_use]
    pub fn to_canonical_text(&self) -> String {
        let mut value = String::new();
        for modifier in self.modifiers.iter() {
            value.push_str(modifier.as_str());
            value.push('+');
        }
        value.push_str(&self.key.text(self.modifiers));
        value
    }

    /// Translate a platform key event without inventing modifier provenance.
    ///
    /// `BackTab` already carries its Shift meaning in the key code, so a Shift
    /// bit on that platform event is removed from the canonical value.
    ///
    /// # Errors
    /// META/HYPER and key codes outside the closed grammar are rejected.
    pub fn from_crossterm(event: &KeyEvent) -> Result<Self, ChordError> {
        if event
            .modifiers
            .intersects(KeyModifiers::META | KeyModifiers::HYPER)
        {
            return Err(ChordError::UnsupportedModifier);
        }
        let mut modifiers = ModifierSet::empty();
        if event.modifiers.contains(KeyModifiers::CONTROL) {
            modifiers.insert(Modifier::Ctrl)?;
        }
        if event.modifiers.contains(KeyModifiers::ALT) {
            modifiers.insert(Modifier::Alt)?;
        }
        if event.modifiers.contains(KeyModifiers::SHIFT) {
            modifiers.insert(Modifier::Shift)?;
        }
        if event.modifiers.contains(KeyModifiers::SUPER) {
            modifiers.insert(Modifier::Super)?;
        }
        let key = translate_code(event.code)?;
        if key == Key::BackTab {
            modifiers.remove(Modifier::Shift);
        }
        Ok(Self {
            modifiers,
            key: normalize_control_character(modifiers, key),
        })
    }

    #[must_use]
    pub fn terminal_class(&self) -> TerminalClass {
        match self.key {
            Key::PageUp | Key::PageDown | Key::Home | Key::End | Key::Up | Key::Down
                if self.modifiers.is_empty() =>
            {
                TerminalClass::ScrollbackCandidate
            }
            _ => TerminalClass::ForwardToPty,
        }
    }
}

fn normalize_control_character(modifiers: ModifierSet, key: Key) -> Key {
    match key {
        Key::Char(character)
            if modifiers.contains(Modifier::Ctrl)
                && !modifiers.contains(Modifier::Shift)
                && character.is_ascii_uppercase() =>
        {
            Key::Char(character.to_ascii_lowercase())
        }
        _ => key,
    }
}

fn translate_code(code: KeyCode) -> Result<Key, ChordError> {
    match code {
        KeyCode::Char(character) => Ok(Key::Char(character)),
        KeyCode::Enter => Ok(Key::Enter),
        KeyCode::Esc => Ok(Key::Esc),
        KeyCode::Tab => Ok(Key::Tab),
        KeyCode::BackTab => Ok(Key::BackTab),
        KeyCode::Backspace => Ok(Key::Backspace),
        KeyCode::Delete => Ok(Key::Delete),
        KeyCode::Insert => Ok(Key::Insert),
        KeyCode::Home => Ok(Key::Home),
        KeyCode::End => Ok(Key::End),
        KeyCode::PageUp => Ok(Key::PageUp),
        KeyCode::PageDown => Ok(Key::PageDown),
        KeyCode::Up => Ok(Key::Up),
        KeyCode::Down => Ok(Key::Down),
        KeyCode::Left => Ok(Key::Left),
        KeyCode::Right => Ok(Key::Right),
        KeyCode::F(number) if (1..=24).contains(&number) => Ok(Key::Function(number)),
        _ => Err(ChordError::UnknownKey),
    }
}

impl fmt::Display for Chord {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.to_canonical_text())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChordError {
    DuplicateModifier,
    ModifierOnly,
    UnknownKey,
    MultipleScalars,
    UnsupportedModifier,
}

impl fmt::Display for ChordError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::DuplicateModifier => "duplicate modifier in chord",
            Self::ModifierOnly => "chord is modifier-only with no key",
            Self::UnknownKey => "unknown key in chord grammar",
            Self::MultipleScalars => "chord key must be exactly one Unicode scalar",
            Self::UnsupportedModifier => "META/HYPER modifiers are unsupported",
        })
    }
}

impl std::error::Error for ChordError {}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TerminalClass {
    ScrollbackCandidate,
    ForwardToPty,
}

/// Encode exactly as the current terminal forwarding path does with
/// `passthrough_enter = false`.
///
/// # Errors
/// Returns [`PtyEncodeError::Unencodable`] for `BackTab`, F13-F24, and other
/// values that production `key_to_bytes` does not encode.
pub fn pty_bytes_for_chord(chord: &Chord) -> Result<Vec<u8>, PtyEncodeError> {
    let (mut bytes, alt_encoded) = basic_key_bytes(chord)
        .or_else(|| nav_key_bytes(chord))
        .or_else(|| function_key_bytes(chord))
        .ok_or(PtyEncodeError::Unencodable)?;
    if chord.modifiers.contains(Modifier::Alt) && !alt_encoded {
        let mut prefixed = Vec::with_capacity(bytes.len() + 1);
        prefixed.push(0x1b);
        prefixed.extend_from_slice(&bytes);
        bytes = prefixed;
    }
    Ok(bytes)
}

fn basic_key_bytes(chord: &Chord) -> Option<(Vec<u8>, bool)> {
    let ctrl = chord.modifiers.contains(Modifier::Ctrl);
    let alt = chord.modifiers.contains(Modifier::Alt);
    let shift = chord.modifiers.contains(Modifier::Shift);
    match chord.key {
        Key::Char(character) if ctrl => Some((vec![ctrl_char_to_byte(character)?], false)),
        Key::Char(character) => {
            let mut buffer = [0; 4];
            Some((
                character.encode_utf8(&mut buffer).as_bytes().to_vec(),
                false,
            ))
        }
        Key::Enter if shift && alt => Some((b"\\\x1b\r".to_vec(), true)),
        Key::Enter if shift => Some((b"\\\r".to_vec(), false)),
        Key::Enter if ctrl => Some((vec![b'\n'], false)),
        Key::Enter => Some((vec![b'\r'], false)),
        Key::Backspace => Some((vec![0x7f], false)),
        Key::Tab => Some((vec![b'\t'], false)),
        Key::Esc => Some((vec![0x1b], false)),
        _ => None,
    }
}

fn nav_key_bytes(chord: &Chord) -> Option<(Vec<u8>, bool)> {
    let (base, parameter_base, suffix) = match chord.key {
        Key::Up => ("\x1b[A", 1, 'A'),
        Key::Down => ("\x1b[B", 1, 'B'),
        Key::Right => ("\x1b[C", 1, 'C'),
        Key::Left => ("\x1b[D", 1, 'D'),
        Key::Home => ("\x1b[H", 1, 'H'),
        Key::End => ("\x1b[F", 1, 'F'),
        Key::PageUp => ("\x1b[5~", 5, '~'),
        Key::PageDown => ("\x1b[6~", 6, '~'),
        Key::Delete => ("\x1b[3~", 3, '~'),
        Key::Insert => ("\x1b[2~", 2, '~'),
        _ => return None,
    };
    if let Some(modifier) = modifiers_to_param(chord.modifiers) {
        Some((
            format!("\x1b[{parameter_base};{modifier}{suffix}").into_bytes(),
            true,
        ))
    } else {
        Some((base.as_bytes().to_vec(), false))
    }
}

fn function_key_bytes(chord: &Chord) -> Option<(Vec<u8>, bool)> {
    let Key::Function(number) = chord.key else {
        return None;
    };
    let modifier = modifiers_to_param(chord.modifiers);
    let bytes = match (number, modifier) {
        (1..=4, None) => format!("\x1bO{}", ['P', 'Q', 'R', 'S'][usize::from(number - 1)]),
        (1..=4, Some(parameter)) => format!(
            "\x1b[1;{parameter}{}",
            ['P', 'Q', 'R', 'S'][usize::from(number - 1)]
        ),
        (5..=12, value) => {
            let code = [15, 17, 18, 19, 20, 21, 23, 24][usize::from(number - 5)];
            value.map_or_else(
                || format!("\x1b[{code}~"),
                |parameter| format!("\x1b[{code};{parameter}~"),
            )
        }
        _ => return None,
    };
    Some((bytes.into_bytes(), modifier.is_some()))
}

fn modifiers_to_param(modifiers: ModifierSet) -> Option<u8> {
    let parameter = 1
        + u8::from(modifiers.contains(Modifier::Shift))
        + 2 * u8::from(modifiers.contains(Modifier::Alt))
        + 4 * u8::from(modifiers.contains(Modifier::Ctrl));
    (parameter > 1).then_some(parameter)
}

fn ctrl_char_to_byte(character: char) -> Option<u8> {
    let character = character.to_ascii_lowercase();
    match character {
        '@' | ' ' | '2' => Some(0),
        '[' | '3' => Some(0x1b),
        '\\' | '4' => Some(0x1c),
        ']' | '5' => Some(0x1d),
        '^' | '6' => Some(0x1e),
        '_' | '7' | '/' => Some(0x1f),
        '?' | '8' => Some(0x7f),
        _ if character.is_ascii_alphabetic() => {
            Some((character as u8).wrapping_sub(b'a').wrapping_add(1))
        }
        _ if character.is_ascii() => Some((character as u8) & 0x1f),
        _ => None,
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PtyEncodeError {
    Unencodable,
}

impl fmt::Display for PtyEncodeError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("chord cannot be encoded for the PTY")
    }
}

impl std::error::Error for PtyEncodeError {}

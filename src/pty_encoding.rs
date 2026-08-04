//! PTY input encoding: converts key events and mouse events to raw bytes for
//! terminal passthrough.

use std::time::{Duration, Instant};

use iocraft::prelude::{KeyCode, KeyEvent, KeyModifiers};

use jefe::input::InputMode;

/// Maximum time after a paste shortcut during which a synthetic Enter is
/// suppressed.
///
/// Some terminals emit a spurious Enter key event immediately after a paste
/// shortcut (Cmd-V / Ctrl-V). That synthetic Enter arrives within a few
/// milliseconds of the paste. A real, human-pressed submit Enter always arrives
/// much later — even fast typists need well over 100 ms to move from Cmd-V to
/// Enter. Bounding suppression to this window ensures a delayed paste-shortcut
/// key event (reordered under load) or a paste shortcut with no corresponding
/// paste event (empty clipboard) can never swallow a genuine submit Enter,
/// regardless of event ordering or system load (issue #286).
pub const PASTE_ENTER_SUPPRESSION_WINDOW: Duration = Duration::from_millis(80);

pub fn ctrl_char_to_byte(c: char) -> Option<u8> {
    let c = c.to_ascii_lowercase();
    match c {
        '@' | ' ' | '2' => Some(0x00),
        '[' | '3' => Some(0x1b),
        '\\' | '4' => Some(0x1c),
        ']' | '5' => Some(0x1d),
        '^' | '6' => Some(0x1e),
        '_' | '7' | '/' => Some(0x1f),
        '?' | '8' => Some(0x7f),
        _ if c.is_ascii_alphabetic() => {
            let byte = (c as u8).wrapping_sub(b'a').wrapping_add(1);
            Some(byte)
        }
        _ if c.is_ascii() => Some((c as u8) & 0x1f),
        _ => None,
    }
}

/// Compute the xterm modifier parameter for a key event.
///
/// Returns `None` when no PTY-relevant modifier (Shift/Alt/Ctrl) is held so that
/// unmodified keys keep their base sequences. Super/Meta are intentionally
/// excluded: they are host/window-manager concerns (e.g. macOS Cmd), not input
/// that should be forwarded into the managed PTY, and the xterm "meta" param
/// bit is not what the OS Super key represents.
fn modifiers_to_param(modifiers: KeyModifiers) -> Option<u8> {
    let shift = u8::from(modifiers.contains(KeyModifiers::SHIFT));
    let alt = u8::from(modifiers.contains(KeyModifiers::ALT)) * 2;
    let ctrl = u8::from(modifiers.contains(KeyModifiers::CONTROL)) * 4;
    let val = 1 + shift + alt + ctrl;
    if val > 1 { Some(val) } else { None }
}

fn function_key_to_bytes(n: u8, modifier: Option<u8>) -> Option<Vec<u8>> {
    if let Some(param) = modifier {
        Some(match n {
            1 => format!("\x1b[1;{param}P").into_bytes(),
            2 => format!("\x1b[1;{param}Q").into_bytes(),
            3 => format!("\x1b[1;{param}R").into_bytes(),
            4 => format!("\x1b[1;{param}S").into_bytes(),
            5 => format!("\x1b[15;{param}~").into_bytes(),
            6 => format!("\x1b[17;{param}~").into_bytes(),
            7 => format!("\x1b[18;{param}~").into_bytes(),
            8 => format!("\x1b[19;{param}~").into_bytes(),
            9 => format!("\x1b[20;{param}~").into_bytes(),
            10 => format!("\x1b[21;{param}~").into_bytes(),
            11 => format!("\x1b[23;{param}~").into_bytes(),
            12 => format!("\x1b[24;{param}~").into_bytes(),
            _ => return None,
        })
    } else {
        Some(match n {
            1 => b"\x1bOP".to_vec(),
            2 => b"\x1bOQ".to_vec(),
            3 => b"\x1bOR".to_vec(),
            4 => b"\x1bOS".to_vec(),
            5 => b"\x1b[15~".to_vec(),
            6 => b"\x1b[17~".to_vec(),
            7 => b"\x1b[18~".to_vec(),
            8 => b"\x1b[19~".to_vec(),
            9 => b"\x1b[20~".to_vec(),
            10 => b"\x1b[21~".to_vec(),
            11 => b"\x1b[23~".to_vec(),
            12 => b"\x1b[24~".to_vec(),
            _ => return None,
        })
    }
}

/// Encode an Enter chord for the multiplexer client on the other end.
///
/// `Ctrl+Enter` used to be a bare `LF`, which is byte-identical to `Ctrl+J`;
/// there was no byte sequence that could express the chord, so agents that bind
/// `Ctrl+Enter` could never see it. It is now sent in CSI-u form
/// (`CSI 13 ; <mods> u`), which the multiplexer parses as a modified Enter and
/// then delivers to the pane child in whatever form that child negotiated —
/// the extended form for a child that asked for extended keys, and a plain `CR`
/// for one that did not. Jefe therefore does not have to guess what the child
/// understands (issue #627).
///
/// The other chords keep their existing encodings: unmodified Enter is `CR`
/// because that is what "submit" means everywhere, and `Shift+Enter` keeps the
/// backslash-CR form that made it distinguishable before extended keys were
/// available (issue #1).
fn enter_bytes(modifiers: KeyModifiers) -> (Vec<u8>, bool) {
    if modifiers.contains(KeyModifiers::SHIFT) {
        if modifiers.contains(KeyModifiers::ALT) {
            (b"\\\x1b\r".to_vec(), true)
        } else {
            (b"\\\r".to_vec(), false)
        }
    } else if modifiers.contains(KeyModifiers::CONTROL) {
        match modifiers_to_param(modifiers) {
            Some(param) => (format!("\x1b[13;{param}u").into_bytes(), true),
            None => (vec![b'\r'], false),
        }
    } else {
        (vec![b'\r'], false)
    }
}

/// Convert a key event to raw bytes for PTY input.
fn basic_key_bytes(code: KeyCode, modifiers: KeyModifiers) -> Option<(Vec<u8>, bool)> {
    let ctrl = modifiers.contains(KeyModifiers::CONTROL);

    match code {
        KeyCode::Char(c) if ctrl => {
            let byte = ctrl_char_to_byte(c)?;
            Some((vec![byte], false))
        }
        KeyCode::Char(c) => {
            let mut buf = [0u8; 4];
            let s = c.encode_utf8(&mut buf);
            Some((s.as_bytes().to_vec(), false))
        }
        KeyCode::Enter => Some(enter_bytes(modifiers)),
        KeyCode::Backspace => Some((vec![0x7f], false)),
        KeyCode::Tab => Some((vec![b'\t'], false)),
        KeyCode::Esc => Some((vec![0x1b], false)),
        _ => None,
    }
}

fn nav_key_bytes(code: KeyCode, modifiers: KeyModifiers) -> Option<(Vec<u8>, bool)> {
    fn encode(
        base: &str,
        with_param: impl Fn(u8) -> String,
        modifiers: KeyModifiers,
    ) -> (Vec<u8>, bool) {
        if let Some(param) = modifiers_to_param(modifiers) {
            (with_param(param).into_bytes(), true)
        } else {
            (base.as_bytes().to_vec(), false)
        }
    }

    match code {
        KeyCode::Up => Some(encode("\x1b[A", |p| format!("\x1b[1;{p}A"), modifiers)),
        KeyCode::Down => Some(encode("\x1b[B", |p| format!("\x1b[1;{p}B"), modifiers)),
        KeyCode::Right => Some(encode("\x1b[C", |p| format!("\x1b[1;{p}C"), modifiers)),
        KeyCode::Left => Some(encode("\x1b[D", |p| format!("\x1b[1;{p}D"), modifiers)),
        KeyCode::Home => Some(encode("\x1b[H", |p| format!("\x1b[1;{p}H"), modifiers)),
        KeyCode::End => Some(encode("\x1b[F", |p| format!("\x1b[1;{p}F"), modifiers)),
        KeyCode::PageUp => Some(encode("\x1b[5~", |p| format!("\x1b[5;{p}~"), modifiers)),
        KeyCode::PageDown => Some(encode("\x1b[6~", |p| format!("\x1b[6;{p}~"), modifiers)),
        KeyCode::Delete => Some(encode("\x1b[3~", |p| format!("\x1b[3;{p}~"), modifiers)),
        KeyCode::Insert => Some(encode("\x1b[2~", |p| format!("\x1b[2;{p}~"), modifiers)),
        _ => None,
    }
}

fn fkey_bytes(n: u8, modifiers: KeyModifiers) -> Option<(Vec<u8>, bool)> {
    let param = modifiers_to_param(modifiers);
    Some((function_key_to_bytes(n, param)?, param.is_some()))
}

pub fn key_to_bytes(key: &KeyEvent) -> Option<Vec<u8>> {
    let modifiers = key.modifiers;

    let (mut out, alt_encoded) = basic_key_bytes(key.code, modifiers)
        .or_else(|| nav_key_bytes(key.code, modifiers))
        .or_else(|| match key.code {
            KeyCode::F(n) => fkey_bytes(n, modifiers),
            _ => None,
        })?;

    // Alt that was not already embedded in a CSI modifier param is represented
    // as a leading ESC prefix.
    if modifiers.contains(KeyModifiers::ALT) && !alt_encoded {
        let mut prefixed = Vec::with_capacity(out.len() + 1);
        prefixed.push(0x1b);
        prefixed.extend_from_slice(&out);
        out = prefixed;
    }

    Some(out)
}

/// Time-bounded paste-Enter suppression state (issue #286).
///
/// Tracks *when* the suppression was armed so it can expire automatically. A
/// synthetic Enter emitted by the terminal immediately after a paste arrives
/// within [`PASTE_ENTER_SUPPRESSION_WINDOW`] of the paste shortcut; a genuine
/// human-pressed submit Enter always arrives later. This makes suppression
/// immune to key/paste event reordering under load and to a paste shortcut with
/// no corresponding paste event (empty clipboard).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct PasteEnterSuppression {
    armed_at: Option<Instant>,
}

impl PasteEnterSuppression {
    /// Create a disarmed (empty) suppression state.
    #[must_use]
    pub const fn new() -> Self {
        Self { armed_at: None }
    }

    /// Arm the suppression, recording the supplied time as the arming instant.
    pub fn arm(&mut self, now: Instant) {
        self.armed_at = Some(now);
    }

    /// Whether suppression is currently active (armed and within the window).
    #[must_use]
    pub fn is_active(&self, now: Instant) -> bool {
        match self.armed_at {
            Some(armed) => now
                .checked_duration_since(armed)
                .is_some_and(|elapsed| elapsed <= PASTE_ENTER_SUPPRESSION_WINDOW),
            None => false,
        }
    }
}

/// Whether an Enter key event should be suppressed as a synthetic paste
/// artifact. The suppression must be active (armed and within the window) and
/// the key must be an Enter.
#[must_use]
pub fn should_suppress_synthetic_enter(
    suppression: PasteEnterSuppression,
    key_event: &KeyEvent,
    now: Instant,
) -> bool {
    suppression.is_active(now) && key_event.code == KeyCode::Enter
}

/// Whether a non-Enter key event should disarm the paste-Enter suppression.
/// Suppression is disarmed by any key that is not Enter while active, so a
/// subsequent real key press (e.g. typing) resets the state even before the
/// window elapses.
#[must_use]
pub fn should_disarm_paste_enter_suppression(
    suppression: PasteEnterSuppression,
    key_event: &KeyEvent,
    now: Instant,
) -> bool {
    suppression.is_active(now) && key_event.code != KeyCode::Enter
}

pub fn should_arm_paste_enter_suppression(key_event: &KeyEvent, input_mode: InputMode) -> bool {
    input_mode == InputMode::TerminalCapture
        && key_event
            .modifiers
            .intersects(KeyModifiers::CONTROL | KeyModifiers::SUPER | KeyModifiers::META)
        && matches!(key_event.code, KeyCode::Char('v' | 'V'))
}

/// Convert a fullscreen mouse event into xterm SGR mouse reporting bytes.
pub fn mouse_event_to_bytes(event: &iocraft::FullscreenMouseEvent) -> Option<Vec<u8>> {
    use iocraft::MouseEventKind;

    // Hold Shift for host-side selection/copy gestures.
    // This mirrors typical terminal behavior where Shift bypasses app mouse reporting.
    if event.modifiers.contains(iocraft::KeyModifiers::SHIFT) {
        return None;
    }

    let (cb, release) = match event.kind {
        MouseEventKind::Down(button) => {
            let code = match button {
                crossterm::event::MouseButton::Left => 0,
                crossterm::event::MouseButton::Middle => 1,
                crossterm::event::MouseButton::Right => 2,
            };
            (code, false)
        }
        MouseEventKind::Up(button) => {
            let code = match button {
                crossterm::event::MouseButton::Left => 0,
                crossterm::event::MouseButton::Middle => 1,
                crossterm::event::MouseButton::Right => 2,
            };
            (code, true)
        }
        MouseEventKind::Drag(button) => {
            let base = match button {
                crossterm::event::MouseButton::Left => 0,
                crossterm::event::MouseButton::Middle => 1,
                crossterm::event::MouseButton::Right => 2,
            };
            (base + 32, false)
        }
        MouseEventKind::Moved => return None,
        MouseEventKind::ScrollDown => (65, false),
        MouseEventKind::ScrollUp => (64, false),
        MouseEventKind::ScrollLeft => (66, false),
        MouseEventKind::ScrollRight => (67, false),
    };

    let mut cb_with_mods = cb;
    if event.modifiers.contains(iocraft::KeyModifiers::ALT) {
        cb_with_mods += 8;
    }
    if event.modifiers.contains(iocraft::KeyModifiers::CONTROL) {
        cb_with_mods += 16;
    }

    let cx = event.column.saturating_add(1);
    let cy = event.row.saturating_add(1);
    let suffix = if release { 'm' } else { 'M' };
    let seq = format!("\x1b[<{cb_with_mods};{cx};{cy}{suffix}");
    Some(seq.into_bytes())
}

#[cfg(test)]
#[path = "pty_encoding_tests.rs"]
mod tests;

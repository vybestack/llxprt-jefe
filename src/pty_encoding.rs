//! PTY input encoding: converts key events and mouse events to raw bytes for
//! terminal passthrough.

use iocraft::prelude::{KeyCode, KeyEvent, KeyModifiers};

use jefe::input::InputMode;

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

fn function_key_to_bytes(n: u8) -> Option<Vec<u8>> {
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

/// Convert a key event to raw bytes for PTY input.
///
/// When `passthrough_enter` is true, Enter maps directly to CR regardless of
/// modifiers, so terminal-focus mode stays close to raw passthrough.
pub fn key_to_bytes(key: &KeyEvent, passthrough_enter: bool) -> Option<Vec<u8>> {
    let ctrl = key.modifiers.contains(KeyModifiers::CONTROL);
    let alt = key.modifiers.contains(KeyModifiers::ALT);
    let shift = key.modifiers.contains(KeyModifiers::SHIFT);

    let mut alt_encoded = false;

    let mut out = match key.code {
        KeyCode::Char(c) if ctrl => {
            let byte = ctrl_char_to_byte(c)?;
            vec![byte]
        }
        KeyCode::Char(c) => {
            let mut buf = [0u8; 4];
            let s = c.encode_utf8(&mut buf);
            s.as_bytes().to_vec()
        }
        KeyCode::Enter => {
            if passthrough_enter {
                vec![b'\r']
            } else if shift {
                // llxprt handles multiline Enter via Shift+Return key state and
                // also via VSCode fallback sequence backslash+carriage-return.
                // The fallback survives tmux attach paths more reliably.
                alt_encoded = alt;
                if alt {
                    b"\\\x1b\r".to_vec()
                } else {
                    b"\\\r".to_vec()
                }
            } else if ctrl {
                // llxprt accepts Ctrl+J as newline.
                vec![b'\n']
            } else {
                vec![b'\r']
            }
        }
        KeyCode::Backspace => vec![0x7f],
        KeyCode::Tab => vec![b'\t'],
        KeyCode::Esc => vec![0x1b],
        KeyCode::Up => b"\x1b[A".to_vec(),
        KeyCode::Down => b"\x1b[B".to_vec(),
        KeyCode::Right => b"\x1b[C".to_vec(),
        KeyCode::Left => b"\x1b[D".to_vec(),
        KeyCode::Home => b"\x1b[H".to_vec(),
        KeyCode::End => b"\x1b[F".to_vec(),
        KeyCode::PageUp => b"\x1b[5~".to_vec(),
        KeyCode::PageDown => b"\x1b[6~".to_vec(),
        KeyCode::Delete => b"\x1b[3~".to_vec(),
        KeyCode::Insert => b"\x1b[2~".to_vec(),
        KeyCode::F(n) => function_key_to_bytes(n)?,
        _ => return None,
    };

    if alt && !alt_encoded {
        let mut prefixed = Vec::with_capacity(out.len() + 1);
        prefixed.push(0x1b);
        prefixed.extend_from_slice(&out);
        out = prefixed;
    }

    Some(out)
}

pub fn should_suppress_synthetic_enter(armed: bool, key_event: &KeyEvent) -> bool {
    armed && key_event.code == KeyCode::Enter
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
mod key_tests {
    use super::{
        ctrl_char_to_byte, key_to_bytes, should_arm_paste_enter_suppression,
        should_suppress_synthetic_enter,
    };
    use iocraft::{KeyCode, KeyEvent, KeyEventKind, KeyModifiers};
    use jefe::input::InputMode;

    fn key_event(code: KeyCode, modifiers: KeyModifiers) -> KeyEvent {
        let mut event = KeyEvent::new(KeyEventKind::Press, code);
        event.modifiers = modifiers;
        event
    }

    #[test]
    fn plain_enter_maps_to_cr() {
        let key = key_event(KeyCode::Enter, KeyModifiers::NONE);
        assert_eq!(key_to_bytes(&key, false), Some(vec![b'\r']));
    }

    #[test]
    fn shift_enter_maps_to_backslash_cr() {
        let key = key_event(KeyCode::Enter, KeyModifiers::SHIFT);
        assert_eq!(key_to_bytes(&key, false), Some(b"\\\r".to_vec()));
    }

    #[test]
    fn synthetic_enter_is_only_suppressed_when_armed() {
        let enter = key_event(KeyCode::Enter, KeyModifiers::NONE);
        assert!(should_suppress_synthetic_enter(true, &enter));
        assert!(!should_suppress_synthetic_enter(false, &enter));
    }

    #[test]
    fn paste_shortcut_arming_only_applies_in_terminal_capture() {
        let ctrl_v = key_event(KeyCode::Char('v'), KeyModifiers::CONTROL);
        assert!(should_arm_paste_enter_suppression(
            &ctrl_v,
            InputMode::TerminalCapture
        ));
        assert!(!should_arm_paste_enter_suppression(
            &ctrl_v,
            InputMode::Normal
        ));

        let cmd_v = key_event(KeyCode::Char('v'), KeyModifiers::SUPER);
        assert!(should_arm_paste_enter_suppression(
            &cmd_v,
            InputMode::TerminalCapture
        ));

        let meta_v = key_event(KeyCode::Char('v'), KeyModifiers::META);
        assert!(should_arm_paste_enter_suppression(
            &meta_v,
            InputMode::TerminalCapture
        ));

        let alt_v = key_event(KeyCode::Char('v'), KeyModifiers::ALT);
        assert!(!should_arm_paste_enter_suppression(
            &alt_v,
            InputMode::TerminalCapture
        ));

        let plain_v = key_event(KeyCode::Char('v'), KeyModifiers::NONE);
        assert!(!should_arm_paste_enter_suppression(
            &plain_v,
            InputMode::TerminalCapture
        ));
    }

    #[test]
    fn passthrough_enter_keeps_cr_for_common_newline_modifiers() {
        let plain_enter = key_event(KeyCode::Enter, KeyModifiers::NONE);
        let shift_enter = key_event(KeyCode::Enter, KeyModifiers::SHIFT);
        let ctrl_enter = key_event(KeyCode::Enter, KeyModifiers::CONTROL);

        assert_eq!(key_to_bytes(&plain_enter, true), Some(vec![b'\r']));
        assert_eq!(key_to_bytes(&shift_enter, true), Some(vec![b'\r']));
        assert_eq!(key_to_bytes(&ctrl_enter, true), Some(vec![b'\r']));
    }

    #[test]
    fn passthrough_enter_with_alt_preserves_escape_prefix() {
        let alt_enter = key_event(KeyCode::Enter, KeyModifiers::ALT);
        assert_eq!(key_to_bytes(&alt_enter, true), Some(vec![0x1b, b'\r']));
    }

    #[test]
    fn alt_char_prefixes_escape() {
        let alt_x = key_event(KeyCode::Char('x'), KeyModifiers::ALT);
        assert_eq!(key_to_bytes(&alt_x, false), Some(b"\x1bx".to_vec()));
    }

    #[test]
    fn alt_shift_enter_does_not_double_prefix_escape() {
        let key = key_event(KeyCode::Enter, KeyModifiers::ALT | KeyModifiers::SHIFT);
        assert_eq!(key_to_bytes(&key, false), Some(b"\\\x1b\r".to_vec()));
    }

    #[test]
    fn shift_alt_enter_maps_to_backslash_esc_cr() {
        let key = key_event(KeyCode::Enter, KeyModifiers::SHIFT | KeyModifiers::ALT);
        assert_eq!(key_to_bytes(&key, false), Some(b"\\\x1b\r".to_vec()));
    }

    #[test]
    fn ctrl_backslash_maps_to_fs() {
        let key = key_event(KeyCode::Char('\\'), KeyModifiers::CONTROL);
        assert_eq!(ctrl_char_to_byte('\\'), Some(0x1c));
        assert_eq!(key_to_bytes(&key, false), Some(vec![0x1c]));
    }

    #[test]
    fn ctrl_underscore_maps_to_us() {
        let key = key_event(KeyCode::Char('_'), KeyModifiers::CONTROL);
        assert_eq!(ctrl_char_to_byte('_'), Some(0x1f));
        assert_eq!(key_to_bytes(&key, false), Some(vec![0x1f]));
    }

    #[test]
    fn ctrl_enter_maps_to_lf() {
        let key = key_event(KeyCode::Enter, KeyModifiers::CONTROL);
        assert_eq!(key_to_bytes(&key, false), Some(vec![b'\n']));
    }

    #[test]
    fn function_keys_use_expected_xterm_sequences() {
        let f1 = key_event(KeyCode::F(1), KeyModifiers::NONE);
        let f2 = key_event(KeyCode::F(2), KeyModifiers::NONE);
        let f12 = key_event(KeyCode::F(12), KeyModifiers::NONE);
        let insert = key_event(KeyCode::Insert, KeyModifiers::NONE);

        assert_eq!(key_to_bytes(&f1, false), Some(b"\x1bOP".to_vec()));
        assert_eq!(key_to_bytes(&f2, false), Some(b"\x1bOQ".to_vec()));
        assert_eq!(key_to_bytes(&f12, false), Some(b"\x1b[24~".to_vec()));
        assert_ne!(key_to_bytes(&f2, false), key_to_bytes(&insert, false));
    }
}

#[cfg(test)]
mod mouse_tests {
    use super::mouse_event_to_bytes;
    use crossterm::event::MouseButton;
    use iocraft::{FullscreenMouseEvent, KeyModifiers, MouseEventKind};

    #[test]
    fn shift_mouse_events_are_not_forwarded_to_pty() {
        let mut event = FullscreenMouseEvent::new(MouseEventKind::Down(MouseButton::Left), 9, 4);
        event.modifiers = KeyModifiers::SHIFT;
        assert_eq!(mouse_event_to_bytes(&event), None);
    }

    #[test]
    fn left_click_uses_sgr_press_encoding() {
        let event = FullscreenMouseEvent::new(MouseEventKind::Down(MouseButton::Left), 9, 4);
        assert_eq!(
            mouse_event_to_bytes(&event),
            Some(b"\x1b[<0;10;5M".to_vec())
        );
    }

    #[test]
    fn right_release_uses_sgr_release_suffix() {
        let event = FullscreenMouseEvent::new(MouseEventKind::Up(MouseButton::Right), 3, 7);
        assert_eq!(mouse_event_to_bytes(&event), Some(b"\x1b[<2;4;8m".to_vec()));
    }

    #[test]
    fn drag_with_alt_and_ctrl_sets_modifier_bits() {
        let mut event = FullscreenMouseEvent::new(MouseEventKind::Drag(MouseButton::Left), 0, 0);
        event.modifiers = KeyModifiers::ALT | KeyModifiers::CONTROL;
        assert_eq!(
            mouse_event_to_bytes(&event),
            Some(b"\x1b[<56;1;1M".to_vec())
        );
    }
}

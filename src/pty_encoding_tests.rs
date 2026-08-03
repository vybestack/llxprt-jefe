//! Encoding tests for PTY key and mouse passthrough (extracted from
//! `pty_encoding.rs` to keep that file inside the source-size gate).

#[cfg(test)]
mod key_tests {
    use crate::pty_encoding::{
        PASTE_ENTER_SUPPRESSION_WINDOW, PasteEnterSuppression, ctrl_char_to_byte, key_to_bytes,
        should_arm_paste_enter_suppression, should_disarm_paste_enter_suppression,
        should_suppress_synthetic_enter,
    };
    use iocraft::{KeyCode, KeyEvent, KeyEventKind, KeyModifiers};
    use jefe::input::InputMode;
    use std::time::{Duration, Instant};

    fn key_event(code: KeyCode, modifiers: KeyModifiers) -> KeyEvent {
        let mut event = KeyEvent::new(KeyEventKind::Press, code);
        event.modifiers = modifiers;
        event
    }

    #[test]
    fn plain_enter_maps_to_cr() {
        let key = key_event(KeyCode::Enter, KeyModifiers::NONE);
        assert_eq!(key_to_bytes(&key), Some(vec![b'\r']));
    }

    #[test]
    fn shift_enter_maps_to_backslash_cr() {
        let key = key_event(KeyCode::Enter, KeyModifiers::SHIFT);
        assert_eq!(key_to_bytes(&key), Some(b"\\\r".to_vec()));
    }

    #[test]
    fn synthetic_enter_is_only_suppressed_when_armed_and_within_window() {
        let enter = key_event(KeyCode::Enter, KeyModifiers::NONE);
        let base = Instant::now();

        // Disarmed — never suppresses.
        let mut suppression = PasteEnterSuppression::new();
        assert!(!should_suppress_synthetic_enter(suppression, &enter, base));

        // Armed — suppresses within the window.
        suppression.arm(base);
        assert!(should_suppress_synthetic_enter(
            suppression,
            &enter,
            base + Duration::from_millis(10)
        ));

        // After the window — a real submit Enter is forwarded normally.
        assert!(!should_suppress_synthetic_enter(
            suppression,
            &enter,
            base + PASTE_ENTER_SUPPRESSION_WINDOW + Duration::from_millis(1)
        ));
    }

    #[test]
    fn non_enter_key_disarms_paste_suppression_when_active() {
        let key = key_event(KeyCode::Char('x'), KeyModifiers::NONE);
        let enter = key_event(KeyCode::Enter, KeyModifiers::NONE);
        let base = Instant::now();

        // Disarmed — nothing to disarm.
        let mut suppression = PasteEnterSuppression::new();
        assert!(!should_disarm_paste_enter_suppression(
            suppression,
            &key,
            base
        ));

        // Armed and active — a non-Enter key disarms.
        suppression.arm(base);
        assert!(should_disarm_paste_enter_suppression(
            suppression,
            &key,
            base + Duration::from_millis(5)
        ));

        // Enter never disarms (it is either suppressed or forwarded).
        suppression.arm(base);
        assert!(!should_disarm_paste_enter_suppression(
            suppression,
            &enter,
            base + Duration::from_millis(5)
        ));
    }

    // ── Issue #286: paste-suppression race regression tests ──────────────────
    //
    // These tests prove the suppression can never swallow a real submit Enter
    // regardless of event ordering, delay, or missing paste event.

    /// Cmd-V key event arrives, then the user presses Enter well after the
    /// window. The Enter must NOT be suppressed (it is a real submit).
    #[test]
    fn real_submit_enter_after_paste_window_is_not_suppressed() {
        let enter = key_event(KeyCode::Enter, KeyModifiers::NONE);
        let base = Instant::now();

        let mut suppression = PasteEnterSuppression::new();
        suppression.arm(base);

        // 500ms later — far beyond the window — a real Enter is forwarded.
        assert!(!should_suppress_synthetic_enter(
            suppression,
            &enter,
            base + Duration::from_millis(500)
        ));
    }

    /// Paste event clears suppression, then a delayed Cmd-V key event re-arms
    /// it. A real Enter arriving later must NOT be suppressed (issue #286
    /// scenario: event reordering under load).
    #[test]
    fn delayed_re_arm_after_paste_does_not_swallow_later_enter() {
        let enter = key_event(KeyCode::Enter, KeyModifiers::NONE);
        let base = Instant::now();

        let mut suppression = PasteEnterSuppression::new();
        // Paste shortcut arms at time 0.
        suppression.arm(base);
        // Paste event clears at time 5ms.
        suppression = PasteEnterSuppression::new();
        // A *delayed* Cmd-V key event re-arms at time 200ms (event reordered
        // under load — arrives long after the paste event).
        suppression.arm(base + Duration::from_millis(200));

        // The user's real Enter arrives at 600ms — well past the re-arm window.
        assert!(!should_suppress_synthetic_enter(
            suppression,
            &enter,
            base + Duration::from_millis(600)
        ));
    }

    /// Cmd-V with no corresponding Paste event (empty clipboard). The
    /// suppression arms but must expire before a real Enter arrives.
    #[test]
    fn no_paste_event_after_cmd_v_still_expires_before_real_enter() {
        let enter = key_event(KeyCode::Enter, KeyModifiers::NONE);
        let base = Instant::now();

        let mut suppression = PasteEnterSuppression::new();
        // Cmd-V armed, no paste event ever arrives.
        suppression.arm(base);

        // Synthetic Enter within window — suppressed (this is the intended
        // behavior: swallow the spurious Enter some terminals send).
        assert!(should_suppress_synthetic_enter(
            suppression,
            &enter,
            base + Duration::from_millis(5)
        ));

        // After suppression consumed the synthetic Enter, it is disarmed.
        suppression = PasteEnterSuppression::new();

        // A later real Enter is forwarded.
        assert!(!should_suppress_synthetic_enter(
            suppression,
            &enter,
            base + Duration::from_millis(300)
        ));
    }

    /// Interleaved key/paste events modeling event-loop load: the suppression
    /// only fires for an Enter within the window of the most recent arm.
    #[test]
    fn interleaved_events_only_suppress_within_recent_window() {
        let enter = key_event(KeyCode::Enter, KeyModifiers::NONE);
        let base = Instant::now();

        let mut suppression = PasteEnterSuppression::new();
        suppression.arm(base);
        // Simulate a paste event at 3ms clearing it.
        suppression = PasteEnterSuppression::new();
        // A second paste shortcut at 10ms re-arms.
        suppression.arm(base + Duration::from_millis(10));
        // Synthetic Enter at 15ms (within window of second arm) — suppressed.
        assert!(should_suppress_synthetic_enter(
            suppression,
            &enter,
            base + Duration::from_millis(15)
        ));
    }

    /// Suppression at the exact window boundary is still active (inclusive).
    #[test]
    fn suppression_active_at_exact_window_boundary() {
        let enter = key_event(KeyCode::Enter, KeyModifiers::NONE);
        let base = Instant::now();

        let mut suppression = PasteEnterSuppression::new();
        suppression.arm(base);
        assert!(should_suppress_synthetic_enter(
            suppression,
            &enter,
            base + PASTE_ENTER_SUPPRESSION_WINDOW
        ));
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

    /// Legacy `Alt+Enter` has no CSI form, so the Alt bit is carried as the
    /// usual ESC prefix in front of the CR.
    #[test]
    fn legacy_alt_enter_prefixes_escape_before_cr() {
        let alt_enter = key_event(KeyCode::Enter, KeyModifiers::ALT);
        assert_eq!(key_to_bytes(&alt_enter), Some(vec![0x1b, b'\r']));
    }

    #[test]
    fn alt_char_prefixes_escape() {
        let alt_x = key_event(KeyCode::Char('x'), KeyModifiers::ALT);
        assert_eq!(key_to_bytes(&alt_x), Some(b"\x1bx".to_vec()));
    }

    #[test]
    fn alt_shift_enter_does_not_double_prefix_escape() {
        let key = key_event(KeyCode::Enter, KeyModifiers::ALT | KeyModifiers::SHIFT);
        assert_eq!(key_to_bytes(&key), Some(b"\\\x1b\r".to_vec()));
    }

    #[test]
    fn shift_alt_enter_maps_to_backslash_esc_cr() {
        let key = key_event(KeyCode::Enter, KeyModifiers::SHIFT | KeyModifiers::ALT);
        assert_eq!(key_to_bytes(&key), Some(b"\\\x1b\r".to_vec()));
    }

    #[test]
    fn ctrl_backslash_maps_to_fs() {
        let key = key_event(KeyCode::Char('\\'), KeyModifiers::CONTROL);
        assert_eq!(ctrl_char_to_byte('\\'), Some(0x1c));
        assert_eq!(key_to_bytes(&key), Some(vec![0x1c]));
    }

    // ── Control-chord passthrough bytes (issue #200) ───────────────────────
    //
    // Code Puppy's shell-control chords depend on these exact single-byte
    // encodings reaching the child through jefe's PTY transport. Locking them
    // here guards the encoding contract independently of the tmux transport.

    #[test]
    fn ctrl_x_maps_to_can_byte() {
        let key = key_event(KeyCode::Char('x'), KeyModifiers::CONTROL);
        assert_eq!(ctrl_char_to_byte('x'), Some(0x18));
        assert_eq!(key_to_bytes(&key), Some(vec![0x18]));
    }

    #[test]
    fn ctrl_b_maps_to_stx_byte() {
        let key = key_event(KeyCode::Char('b'), KeyModifiers::CONTROL);
        assert_eq!(ctrl_char_to_byte('b'), Some(0x02));
        assert_eq!(key_to_bytes(&key), Some(vec![0x02]));
    }

    #[test]
    fn ctrl_c_maps_to_etx_byte() {
        let key = key_event(KeyCode::Char('c'), KeyModifiers::CONTROL);
        assert_eq!(ctrl_char_to_byte('c'), Some(0x03));
        assert_eq!(key_to_bytes(&key), Some(vec![0x03]));
    }

    #[test]
    fn ctrl_caps_c_maps_to_etx_byte() {
        let key = key_event(KeyCode::Char('C'), KeyModifiers::CONTROL);
        assert_eq!(ctrl_char_to_byte('C'), Some(0x03));
        assert_eq!(key_to_bytes(&key), Some(vec![0x03]));
    }

    /// A Ctrl-X Ctrl-B chord encodes to the two raw bytes `0x18 0x02` in
    /// order, matching what Code Puppy's `command_runner` listens for.
    #[test]
    fn ctrl_x_ctrl_b_chord_encodes_to_ordered_bytes() {
        let x = key_event(KeyCode::Char('x'), KeyModifiers::CONTROL);
        let b = key_event(KeyCode::Char('b'), KeyModifiers::CONTROL);
        let x_bytes = key_to_bytes(&x);
        let b_bytes = key_to_bytes(&b);
        assert!(x_bytes.is_some(), "Ctrl-X must encode");
        assert!(b_bytes.is_some(), "Ctrl-B must encode");
        let mut encoded = Vec::<u8>::new();
        encoded.extend(x_bytes.unwrap_or_default());
        encoded.extend(b_bytes.unwrap_or_default());
        assert_eq!(encoded, [0x18u8, 0x02]);
    }

    /// A Ctrl-X Ctrl-X chord encodes to `0x18 0x18` in order.
    #[test]
    fn ctrl_x_ctrl_x_chord_encodes_to_ordered_bytes() {
        let x = key_event(KeyCode::Char('x'), KeyModifiers::CONTROL);
        let x_bytes = key_to_bytes(&x);
        assert!(x_bytes.is_some(), "Ctrl-X must encode");
        let bytes = x_bytes.unwrap_or_default();
        let mut encoded = Vec::<u8>::new();
        encoded.extend(&bytes);
        encoded.extend(&bytes);
        assert_eq!(encoded, [0x18u8, 0x18]);
    }

    #[test]
    fn ctrl_underscore_maps_to_us() {
        let key = key_event(KeyCode::Char('_'), KeyModifiers::CONTROL);
        assert_eq!(ctrl_char_to_byte('_'), Some(0x1f));
        assert_eq!(key_to_bytes(&key), Some(vec![0x1f]));
    }

    // ── Enter chords through the multiplexer (issue #627) ──────────────────
    //
    // `LF` is byte-identical to `Ctrl+J`, so while Jefe sent it there was no
    // byte sequence that could express `Ctrl+Enter` at all. It is now sent in
    // CSI-u form, which the multiplexer parses as a modified Enter and then
    // delivers to each pane child in the form that child negotiated.

    /// `Ctrl+Enter` is expressible at all, and is not the `Ctrl+J` alias that
    /// made agents insert a newline instead of steering.
    #[test]
    fn ctrl_enter_is_distinguishable_from_ctrl_j() {
        let ctrl_enter = key_event(KeyCode::Enter, KeyModifiers::CONTROL);
        let ctrl_j = key_event(KeyCode::Char('j'), KeyModifiers::CONTROL);

        assert_eq!(key_to_bytes(&ctrl_enter), Some(b"\x1b[13;5u".to_vec()));
        assert_ne!(
            key_to_bytes(&ctrl_enter),
            key_to_bytes(&ctrl_j),
            "Ctrl+Enter must not collapse onto Ctrl+J"
        );
        assert_eq!(
            key_to_bytes(&ctrl_j),
            Some(vec![b'\n']),
            "Ctrl+J itself keeps its control byte"
        );
    }

    /// Combined modifiers accumulate into the one CSI-u parameter.
    #[test]
    fn ctrl_alt_enter_combines_modifier_bits_in_one_parameter() {
        let key = key_event(KeyCode::Enter, KeyModifiers::CONTROL | KeyModifiers::ALT);
        assert_eq!(key_to_bytes(&key), Some(b"\x1b[13;7u".to_vec()));
    }

    /// Unmodified Enter stays a bare CR: "submit" means CR everywhere, and the
    /// multiplexer passes it through untouched whatever the pane negotiated.
    #[test]
    fn plain_enter_stays_carriage_return() {
        let key = key_event(KeyCode::Enter, KeyModifiers::NONE);
        assert_eq!(key_to_bytes(&key), Some(vec![b'\r']));
    }

    /// The `Shift+Enter` compatibility form (issue #1) is untouched, so agents
    /// relying on it keep working.
    #[test]
    fn shift_enter_keeps_its_compatibility_form() {
        let shift = key_event(KeyCode::Enter, KeyModifiers::SHIFT);
        let shift_alt = key_event(KeyCode::Enter, KeyModifiers::SHIFT | KeyModifiers::ALT);

        assert_eq!(key_to_bytes(&shift), Some(b"\\\r".to_vec()));
        assert_eq!(key_to_bytes(&shift_alt), Some(b"\\\x1b\r".to_vec()));
    }

    /// Only `Ctrl+Enter` changes. Every other key keeps the bytes it had, so
    /// no hosted agent that never asked for anything can regress.
    #[test]
    fn no_other_key_encoding_changes() {
        let cases: [(KeyEvent, Vec<u8>); 9] = [
            (key_event(KeyCode::Enter, KeyModifiers::NONE), vec![b'\r']),
            (
                key_event(KeyCode::Enter, KeyModifiers::ALT),
                vec![0x1b, b'\r'],
            ),
            (
                key_event(KeyCode::Char('x'), KeyModifiers::CONTROL),
                vec![0x18],
            ),
            (
                key_event(KeyCode::Char('a'), KeyModifiers::NONE),
                b"a".to_vec(),
            ),
            (key_event(KeyCode::Tab, KeyModifiers::NONE), vec![b'\t']),
            (
                key_event(KeyCode::Backspace, KeyModifiers::NONE),
                vec![0x7f],
            ),
            (key_event(KeyCode::Esc, KeyModifiers::NONE), vec![0x1b]),
            (
                key_event(KeyCode::Up, KeyModifiers::CONTROL),
                b"\x1b[1;5A".to_vec(),
            ),
            (
                key_event(KeyCode::F(5), KeyModifiers::ALT),
                b"\x1b[15;3~".to_vec(),
            ),
        ];

        for (key, expected) in cases {
            assert_eq!(
                key_to_bytes(&key),
                Some(expected),
                "{:?} with {:?} must keep its encoding",
                key.code,
                key.modifiers
            );
        }
    }

    #[test]
    fn function_keys_use_expected_xterm_sequences() {
        let f1 = key_event(KeyCode::F(1), KeyModifiers::NONE);
        let f2 = key_event(KeyCode::F(2), KeyModifiers::NONE);
        let f12 = key_event(KeyCode::F(12), KeyModifiers::NONE);
        let insert = key_event(KeyCode::Insert, KeyModifiers::NONE);

        assert_eq!(key_to_bytes(&f1), Some(b"\x1bOP".to_vec()));
        assert_eq!(key_to_bytes(&f2), Some(b"\x1bOQ".to_vec()));
        assert_eq!(key_to_bytes(&f12), Some(b"\x1b[24~".to_vec()));
        assert_ne!(key_to_bytes(&f2), key_to_bytes(&insert));
    }

    #[test]
    fn modified_arrow_keys_use_xterm_sequences() {
        let ctrl_up = key_event(KeyCode::Up, KeyModifiers::CONTROL);
        let alt_down = key_event(KeyCode::Down, KeyModifiers::ALT);
        let shift_right = key_event(KeyCode::Right, KeyModifiers::SHIFT);
        let ctrl_alt_left = key_event(KeyCode::Left, KeyModifiers::CONTROL | KeyModifiers::ALT);

        // ctrl parameter = 5
        assert_eq!(key_to_bytes(&ctrl_up), Some(b"\x1b[1;5A".to_vec()));
        // alt parameter = 3
        assert_eq!(key_to_bytes(&alt_down), Some(b"\x1b[1;3B".to_vec()));
        // shift parameter = 2
        assert_eq!(key_to_bytes(&shift_right), Some(b"\x1b[1;2C".to_vec()));
        // ctrl + alt parameter = 7
        assert_eq!(key_to_bytes(&ctrl_alt_left), Some(b"\x1b[1;7D".to_vec()));
    }

    #[test]
    fn modified_edit_keys_use_xterm_sequences() {
        let ctrl_pageup = key_event(KeyCode::PageUp, KeyModifiers::CONTROL);
        let alt_pagedown = key_event(KeyCode::PageDown, KeyModifiers::ALT);
        let shift_delete = key_event(KeyCode::Delete, KeyModifiers::SHIFT);
        let ctrl_alt_insert = key_event(KeyCode::Insert, KeyModifiers::CONTROL | KeyModifiers::ALT);
        let shift_home = key_event(KeyCode::Home, KeyModifiers::SHIFT);
        let ctrl_end = key_event(KeyCode::End, KeyModifiers::CONTROL);

        assert_eq!(key_to_bytes(&ctrl_pageup), Some(b"\x1b[5;5~".to_vec()));
        assert_eq!(key_to_bytes(&alt_pagedown), Some(b"\x1b[6;3~".to_vec()));
        assert_eq!(key_to_bytes(&shift_delete), Some(b"\x1b[3;2~".to_vec()));
        assert_eq!(key_to_bytes(&ctrl_alt_insert), Some(b"\x1b[2;7~".to_vec()));
        assert_eq!(key_to_bytes(&shift_home), Some(b"\x1b[1;2H".to_vec()));
        assert_eq!(key_to_bytes(&ctrl_end), Some(b"\x1b[1;5F".to_vec()));
    }

    #[test]
    fn modified_function_keys_use_xterm_sequences() {
        let ctrl_f1 = key_event(KeyCode::F(1), KeyModifiers::CONTROL);
        let alt_f5 = key_event(KeyCode::F(5), KeyModifiers::ALT);
        let ctrl_alt_f12 = key_event(KeyCode::F(12), KeyModifiers::CONTROL | KeyModifiers::ALT);

        assert_eq!(key_to_bytes(&ctrl_f1), Some(b"\x1b[1;5P".to_vec()));
        assert_eq!(key_to_bytes(&alt_f5), Some(b"\x1b[15;3~".to_vec()));
        assert_eq!(key_to_bytes(&ctrl_alt_f12), Some(b"\x1b[24;7~".to_vec()));
    }

    #[test]
    fn alt_encoding_is_consistent_and_not_double_encoded() {
        // Alt-up modified should be \x1b[1;3A, not double ESC-prefixed (e.g. not \x1b\x1b[1;3A)
        let alt_up = key_event(KeyCode::Up, KeyModifiers::ALT);
        assert_eq!(key_to_bytes(&alt_up), Some(b"\x1b[1;3A".to_vec()));

        // Alt-F1 modified should be \x1b[1;3P, not \x1b\x1b[1;3P
        let alt_f1 = key_event(KeyCode::F(1), KeyModifiers::ALT);
        assert_eq!(key_to_bytes(&alt_f1), Some(b"\x1b[1;3P".to_vec()));
    }
}

#[cfg(test)]
mod mouse_tests {
    use crate::pty_encoding::mouse_event_to_bytes;
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

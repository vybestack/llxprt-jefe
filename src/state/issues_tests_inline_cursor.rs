//! Inline-editor vertical cursor movement tests: line navigation, column
//! clamping, and Unicode char-boundary correctness for
//! [`crate::state::util::inline_cursor_vertical`].

/// InlineCursorUp/Down move the cursor between lines in multi-line text.
#[test]
fn test_inline_cursor_vertical_navigation() {
    use crate::state::inline_cursor_vertical;

    // 3 lines: abc, def, ghi — offsets [0..3], [4..7], [8..11]
    let text = ["abc", "def", "ghi"].join(&String::from(char::from(0x0Au8)));

    // Down from line 0 col 1 to line 1 col 1
    let mut cursor = 1;
    inline_cursor_vertical(&text, &mut cursor, 1);
    assert_eq!(cursor, 5);

    // Down from line 1 col 1 to line 2 col 1
    inline_cursor_vertical(&text, &mut cursor, 1);
    assert_eq!(cursor, 9);

    // Down from last line stays
    inline_cursor_vertical(&text, &mut cursor, 1);
    assert_eq!(cursor, 9);

    // Up from line 2 col 1 to line 1 col 1
    inline_cursor_vertical(&text, &mut cursor, -1);
    assert_eq!(cursor, 5);

    // Up from line 1 col 1 to line 0 col 1
    inline_cursor_vertical(&text, &mut cursor, -1);
    assert_eq!(cursor, 1);

    // Up from first line stays
    inline_cursor_vertical(&text, &mut cursor, -1);
    assert_eq!(cursor, 1);
}

/// InlineCursorUp/Down clamp column when target line is shorter.
#[test]
fn test_inline_cursor_vertical_column_clamping() {
    use crate::state::inline_cursor_vertical;

    // 3 lines: abcdef (len 6), xy (len 2), z (len 1)
    let nl = String::from(char::from(0x0Au8));
    let text = ["abcdef", "xy", "z"].join(&nl);

    // Cursor at col 5 of line 0 -> down to line 1 (len 2) -> clamp to col 2
    let mut cursor = 5;
    inline_cursor_vertical(&text, &mut cursor, 1);
    assert_eq!(cursor, 9); // line 1 start=7, col clamped to 2 = byte 9
}

/// InlineCursorUp/Down compute columns in characters for multi-byte (Unicode) text.
/// Without this fix, byte-based column math lands on invalid positions.
#[test]
fn test_inline_cursor_vertical_unicode_columns() {
    use crate::state::inline_cursor_vertical;

    let nl = String::from(char::from(0x0Au8));
    // Line 0: 3 emoji (4 bytes each = 12 bytes, 3 chars)
    // Line 1: 2 emoji (4 bytes each = 8 bytes, 2 chars)
    let emoji = "\u{1F600}\u{1F601}\u{1F602}";
    let emoji_short = "\u{1F600}\u{1F601}";
    let text = [emoji, emoji_short].join(&nl);
    // Byte layout:
    //   [0..4)   emoji 1 (line 0)
    //   [4..8)   emoji 2 (line 0)
    //   [8..12)  emoji 3 (line 0)
    //   [12]     newline
    //   [13..17) emoji 1 (line 1)
    //   [17..21) emoji 2 (line 1)

    // Place cursor after the 2nd emoji on line 0 (char col 2, byte 8).
    let mut cursor = 8;
    // Move down: char col 2 on line 1 (end of line 1) = byte 21.
    // The old byte-based code would compute col=8 and clamp to 8, landing
    // at byte 13+8=21 only by coincidence; for col 1 the bug is visible.
    inline_cursor_vertical(&text, &mut cursor, 1);
    assert_eq!(
        cursor, 21,
        "Unicode down: should land at char col 2 on line 1"
    );

    // Move back up: char col 2 on line 0 = byte 8 again.
    inline_cursor_vertical(&text, &mut cursor, -1);
    assert_eq!(cursor, 8, "Unicode up: should land at char col 2 on line 0");

    // Place cursor after 1st emoji on line 0 (char col 1, byte 4).
    let mut cursor = 4;
    // Down: char col 1 on line 1 = byte 17.
    // With the old byte-based code, col=4 would land at byte 13+4=17, which
    // is the middle of emoji 2 on line 1 (bytes 17..21) — an invalid char
    // boundary. The fix lands exactly on the boundary at byte 17.
    inline_cursor_vertical(&text, &mut cursor, 1);
    assert_eq!(
        cursor, 17,
        "Unicode down: should land at char col 1 on line 1"
    );
}

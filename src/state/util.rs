//! Text-editing utility functions for form field cursors.
//!
//! Field normalization helpers (`normalize_profile`, `normalize_sandbox_flags`,
//! `normalize_llxprt_debug`, `expand_tilde`) and `generate_id` now live in the
//! [`crate::services`] layer alongside the canonical agent-creation use-case.

pub(super) fn clamp_cursor(s: &str, cursor: usize) -> usize {
    cursor.min(s.chars().count())
}

pub(super) fn byte_index_at_char(s: &str, char_idx: usize) -> usize {
    if char_idx == 0 {
        return 0;
    }

    s.char_indices()
        .nth(char_idx)
        .map_or_else(|| s.len(), |(idx, _)| idx)
}

pub(super) fn insert_char_at(s: &mut String, cursor: usize, ch: char) -> usize {
    let clamped = clamp_cursor(s, cursor);
    let byte_idx = byte_index_at_char(s, clamped);
    s.insert(byte_idx, ch);
    clamped + 1
}

pub(super) fn delete_char_before(s: &mut String, cursor: usize) -> usize {
    let clamped = clamp_cursor(s, cursor);
    if clamped == 0 {
        return 0;
    }

    let start = byte_index_at_char(s, clamped - 1);
    let end = byte_index_at_char(s, clamped);
    s.replace_range(start..end, "");
    clamped - 1
}

pub(super) fn delete_char_at(s: &mut String, cursor: usize) {
    let clamped = clamp_cursor(s, cursor);
    let len = s.chars().count();
    if clamped >= len {
        return;
    }

    let start = byte_index_at_char(s, clamped);
    let end = byte_index_at_char(s, clamped + 1);
    s.replace_range(start..end, "");
}

pub(super) fn move_cursor_left(cursor: usize) -> usize {
    cursor.saturating_sub(1)
}

pub(super) fn move_cursor_right(s: &str, cursor: usize) -> usize {
    let len = s.chars().count();
    clamp_cursor(s, cursor).saturating_add(1).min(len)
}

/// Move the inline editor cursor up or down by `direction` lines (-1 = up, 1 = down).
/// Attempts to land on the same column in the target line, clamping to line length.
///
/// Column positions are computed in **characters** (Unicode scalar values), not
/// bytes, so multi-byte text does not cause cursor jumps to invalid positions.
pub fn inline_cursor_vertical(text: &str, cursor: &mut usize, direction: i32) {
    // Split into lines (as &str slices) preserving byte offsets.
    let mut line_byte_starts: Vec<usize> = vec![0];
    for (byte_idx, ch) in text.char_indices() {
        if ch == char::from(0x0Au8) {
            line_byte_starts.push(byte_idx + ch.len_utf8());
        }
    }

    // Clamp the cursor to a valid char boundary. Defensively walks a
    // mid-codepoint offset DOWN so slicing cannot panic.
    let mut clamped_cursor = (*cursor).min(text.len());
    while clamped_cursor > 0 && !text.is_char_boundary(clamped_cursor) {
        clamped_cursor -= 1;
    }
    let before_cursor = &text[..clamped_cursor];

    // Find which line the cursor is on (by byte offset).
    let mut current_line = 0;
    for (i, &byte_start) in line_byte_starts.iter().enumerate() {
        if clamped_cursor >= byte_start {
            current_line = i;
        }
    }

    // Compute the current column in CHARACTERS, not bytes.
    let line_byte_start = line_byte_starts[current_line];
    let col_chars = before_cursor[line_byte_start..].chars().count();

    let target_line = if direction < 0 {
        current_line.saturating_sub(1)
    } else {
        (current_line + 1).min(line_byte_starts.len() - 1)
    };

    if target_line == current_line {
        return; // already at first/last line
    }

    // Slice the target line (excluding its trailing newline) and convert the
    // desired character column back to a byte offset within the target line.
    let target_byte_start = line_byte_starts[target_line];
    let target_line_end_byte = if target_line + 1 < line_byte_starts.len() {
        line_byte_starts[target_line + 1] - 1
    } else {
        text.len()
    };
    let target_slice = &text[target_byte_start..target_line_end_byte];
    let target_byte_offset = target_slice
        .char_indices()
        .nth(col_chars)
        .map_or(target_slice.len(), |(byte_idx, _)| byte_idx);

    *cursor = target_byte_start + target_byte_offset;
}

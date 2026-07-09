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

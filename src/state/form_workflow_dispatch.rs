//! WorkflowDispatch form field-handling free functions.
//!
//! Mirrors the `form_cursor.rs` pattern: pure functions that take
//! `(&mut fields, &mut cursor, focus)` and are called as thin delegations
//! from `form_ops.rs` match arms. Keeping these here avoids re-bloating
//! `form_ops.rs` past its architecture boundary limit.

use super::types::{
    WorkflowDispatchFormCursor, WorkflowDispatchFormFields, WorkflowDispatchFormFocus,
};
use super::util::{delete_char_at, delete_char_before, insert_char_at, move_cursor_left};

/// Insert a character at the cursor position in the focused WorkflowDispatch
/// text field.
pub(super) fn handle_field_char(
    fields: &mut WorkflowDispatchFormFields,
    cursor: &mut WorkflowDispatchFormCursor,
    focus: WorkflowDispatchFormFocus,
    c: char,
) {
    match focus {
        WorkflowDispatchFormFocus::RefName => {
            cursor.ref_name = insert_char_at(&mut fields.ref_name, cursor.ref_name, c);
        }
        WorkflowDispatchFormFocus::Inputs => {
            cursor.inputs = insert_char_at(&mut fields.inputs, cursor.inputs, c);
        }
        WorkflowDispatchFormFocus::Submit | WorkflowDispatchFormFocus::Cancel => {}
    }
}

/// Delete the character before the cursor in the focused WorkflowDispatch
/// text field.
pub(super) fn delete_field_before_cursor(
    fields: &mut WorkflowDispatchFormFields,
    cursor: &mut WorkflowDispatchFormCursor,
    focus: WorkflowDispatchFormFocus,
) {
    match focus {
        WorkflowDispatchFormFocus::RefName => {
            cursor.ref_name = delete_char_before(&mut fields.ref_name, cursor.ref_name);
        }
        WorkflowDispatchFormFocus::Inputs => {
            cursor.inputs = delete_char_before(&mut fields.inputs, cursor.inputs);
        }
        WorkflowDispatchFormFocus::Submit | WorkflowDispatchFormFocus::Cancel => {}
    }
}

/// Delete the character at the cursor in the focused WorkflowDispatch text
/// field.
pub(super) fn delete_field_at_cursor(
    fields: &mut WorkflowDispatchFormFields,
    cursor: &WorkflowDispatchFormCursor,
    focus: WorkflowDispatchFormFocus,
) {
    match focus {
        WorkflowDispatchFormFocus::RefName => {
            delete_char_at(&mut fields.ref_name, cursor.ref_name);
        }
        WorkflowDispatchFormFocus::Inputs => {
            delete_char_at(&mut fields.inputs, cursor.inputs);
        }
        WorkflowDispatchFormFocus::Submit | WorkflowDispatchFormFocus::Cancel => {}
    }
}

/// Move the cursor left in the focused WorkflowDispatch text field.
pub(super) fn move_cursor_field_left(
    cursor: &mut WorkflowDispatchFormCursor,
    focus: WorkflowDispatchFormFocus,
) {
    match focus {
        WorkflowDispatchFormFocus::RefName => {
            cursor.ref_name = move_cursor_left(cursor.ref_name);
        }
        WorkflowDispatchFormFocus::Inputs => {
            cursor.inputs = move_cursor_left(cursor.inputs);
        }
        WorkflowDispatchFormFocus::Submit | WorkflowDispatchFormFocus::Cancel => {}
    }
}

/// Parse the free-text WorkflowDispatch `inputs` field into typed key/value
/// pairs. Inputs are parsed as newline-separated `key=value` pairs; lines
/// without `=` (or empty lines) are skipped. Lines with an empty key (e.g.
/// `=value`) are also skipped so an invalid empty input key is never forwarded
/// to the GitHub Actions API. Both sides are trimmed.
#[must_use]
pub fn parse_inputs(inputs: &str) -> Vec<(String, String)> {
    inputs
        .lines()
        .filter_map(|line| {
            let line = line.trim();
            let (key, val) = line.split_once('=')?;
            let key = key.trim();
            if key.is_empty() {
                return None;
            }
            Some((key.to_string(), val.trim().to_string()))
        })
        .collect()
}

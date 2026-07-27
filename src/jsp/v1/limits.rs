//! Inclusive JSP/1 bounds (issue #476, J1 slice, decision 13).
//!
//! Every bound is inclusive: an at-limit input is accepted and a
//! limit-plus-one input fails before a contract value is returned. Limits are
//! applied during deserialization and validation.

/// Maximum total snapshot document size in bytes (256 KiB).
pub const MAX_DOCUMENT_BYTES: usize = 256 * 1024;
/// Maximum length in bytes of an opaque identifier (agent id, source epoch).
pub const MAX_ID_BYTES: usize = 128;
/// Maximum number of todo entries.
pub const MAX_TODOS: usize = 256;
/// Maximum length in bytes of a todo text string.
pub const MAX_TODO_TEXT_BYTES: usize = 2 * 1024;
/// Maximum length in bytes of displayed assistant content.
pub const MAX_DISPLAYED_CONTENT_BYTES: usize = 16 * 1024;
/// Maximum length in bytes of a source diagnostic summary.
pub const MAX_DIAGNOSTIC_SUMMARY_BYTES: usize = 2 * 1024;
/// Maximum length in bytes of a tool label.
pub const MAX_TOOL_LABEL_BYTES: usize = 256;
/// Maximum length in bytes of a repository reference.
pub const MAX_REPOSITORY_BYTES: usize = 256;
/// Maximum length in bytes of a path reference.
pub const MAX_PATH_BYTES: usize = 4 * 1024;
/// Maximum length in bytes of an agent-kind label.
pub const MAX_AGENT_KIND_BYTES: usize = 64;
/// Maximum length in bytes of a display name.
pub const MAX_DISPLAY_NAME_BYTES: usize = 256;
/// Maximum length in bytes of a diagnostic code.
pub const MAX_DIAGNOSTIC_CODE_BYTES: usize = 128;
/// Maximum length in bytes of a bounded free-text error code field.
pub const MAX_ERROR_CODE_BYTES: usize = 128;

/// The single accepted schema version.
pub const ACCEPTED_SCHEMA: u64 = 1;
/// The single accepted top-level kind for a snapshot document.
pub const SNAPSHOT_KIND: &str = "snapshot";

/// Validate that a byte length is within the inclusive bound.
///
/// # Errors
///
/// Returns a `JSP-E002` bound error if `len > max`.
pub fn check_bound(
    path: &str,
    len: usize,
    max: usize,
) -> Result<(), crate::jsp::v1::error::JspError> {
    if len > max {
        Err(crate::jsp::v1::error::JspError::bound(format!(
            "{path}: length {len} exceeds maximum {max}"
        )))
    } else {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::jsp::v1::error::JspCode;

    #[test]
    fn at_limit_accepted_over_limit_rejected() {
        assert!(check_bound("x", MAX_ID_BYTES, MAX_ID_BYTES).is_ok());
        let error = check_bound("x", MAX_ID_BYTES + 1, MAX_ID_BYTES)
            .err()
            .unwrap_or_else(|| panic!("over limit must fail"));
        assert_eq!(error.code(), JspCode::EBound);
    }

    #[test]
    fn zero_length_accepted() {
        assert!(check_bound("x", 0, MAX_ID_BYTES).is_ok());
    }
}

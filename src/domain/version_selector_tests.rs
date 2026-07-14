//! Behavioral tests for LLxprt version-selector validation (issue #269).
//!
//! These tests prove the pure [`validate_version_selector`] predicate rejects
//! structurally unrepresentable input (embedded NUL) while accepting all
//! npm-supported selectors (exact versions, nightlies, dist tags, and blank).

use super::{VersionSelectorError, validate_version_selector};

#[test]
fn npm_supported_selectors_are_valid() {
    for selector in [
        "",
        "   ",
        "0.9.0",
        "0.10.0-nightly.260712.21cb698b6",
        "latest",
        "next",
        "  0.9.0  ",
    ] {
        assert!(
            validate_version_selector(selector).is_ok(),
            "npm-supported selector must be valid: {selector:?}"
        );
    }
}

#[test]
fn nul_in_any_position_is_rejected() {
    for selector in ["\x00", "  \x00  ", "0.9.0\x00", "0.9.0\x00; rm -rf /"] {
        let result = validate_version_selector(selector);
        assert!(
            matches!(result, Err(VersionSelectorError::EmbeddedNul)),
            "NUL-containing selector must be rejected: {selector:?}"
        );
    }
}

#[test]
fn shell_metacharacters_without_nul_are_valid() {
    for selector in ["0.9.0; rm -rf /", "'; whoami; '", "$(whoami)", "`whoami`"] {
        assert!(
            validate_version_selector(selector).is_ok(),
            "safe argv/shell boundaries permit selector: {selector:?}"
        );
    }
}

#[test]
fn error_message_is_actionable() {
    let result = validate_version_selector("0.9.0\x00");
    let Err(error) = result else {
        panic!("expected error");
    };
    let message = error.to_string();
    assert!(
        message.contains("NUL"),
        "error message must mention NUL: {message}"
    );
}

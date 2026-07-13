//! Behavioral tests for LLxprt version-selector validation (issue #269).
//!
//! These tests prove the pure [`validate_version_selector`] predicate rejects
//! structurally unrepresentable input (embedded NUL) while accepting all
//! npm-supported selectors (exact versions, nightlies, dist tags, and blank).

use super::{VersionSelectorError, validate_version_selector};

#[test]
fn blank_selector_is_valid() {
    assert!(validate_version_selector("").is_ok());
}

#[test]
fn whitespace_only_selector_is_valid_for_normalizer() {
    // Whitespace-only is accepted by the validator so callers can trim it to
    // blank at the normalization boundary.
    assert!(validate_version_selector("   ").is_ok());
}

#[test]
fn stable_version_is_valid() {
    assert!(validate_version_selector("0.9.0").is_ok());
}

#[test]
fn nightly_selector_is_valid() {
    assert!(
        validate_version_selector("0.10.0-nightly.260712.21cb698b6").is_ok(),
        "nightly selectors must be preserved exactly"
    );
}

#[test]
fn dist_tag_is_valid() {
    assert!(validate_version_selector("latest").is_ok());
    assert!(validate_version_selector("next").is_ok());
}

#[test]
fn surrounding_whitespace_is_valid() {
    assert!(validate_version_selector("  0.9.0  ").is_ok());
}

#[test]
fn embedded_nul_is_rejected() {
    let result = validate_version_selector("0.9.0\x00; rm -rf /");
    let Err(error) = result else {
        panic!("embedded NUL must be rejected");
    };
    assert!(
        matches!(error, VersionSelectorError::EmbeddedNul),
        "error must be EmbeddedNul: {error:?}"
    );
}

#[test]
fn leading_nul_is_rejected() {
    let result = validate_version_selector("\x00");
    assert!(matches!(result, Err(VersionSelectorError::EmbeddedNul)));
}

#[test]
fn nul_in_whitespace_is_rejected() {
    // Even surrounded by whitespace, a NUL byte is structurally
    // unrepresentable and must be rejected before any trim.
    let result = validate_version_selector("  \x00  ");
    assert!(matches!(result, Err(VersionSelectorError::EmbeddedNul)));
}

#[test]
fn nul_at_end_is_rejected() {
    let result = validate_version_selector("0.9.0\x00");
    assert!(matches!(result, Err(VersionSelectorError::EmbeddedNul)));
}

#[test]
fn metacharacters_without_nul_are_valid() {
    // Adversarial shell metacharacters without NUL are valid — they are
    // carried as a single argv token (local) or shell-escaped (remote), so
    // they never reach the shell as syntax.
    assert!(validate_version_selector("0.9.0; rm -rf /").is_ok());
    assert!(validate_version_selector("'; whoami; '").is_ok());
    assert!(validate_version_selector("$(whoami)").is_ok());
    assert!(validate_version_selector("`whoami`").is_ok());
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

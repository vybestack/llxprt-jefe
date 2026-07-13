//! Pure validation for LLxprt version selectors (issue #269).
//!
//! A version selector is an npm package version specifier appended to
//! `@vybestack/llxprt-code@` in an `npm exec` invocation. Most inputs are
//! valid — npm supports exact versions, prereleases/nightlies, and dist tags.
//! The only structurally unrepresentable input is an embedded NUL byte, which
//! cannot be passed as a process argument or shell-escaped safely on any
//! platform. This module rejects that before persistence or launch.
//!
//! The validator is intentionally permissive: it does NOT enforce semver or a
//! known allow-list. Restricting to a schema would reject npm-supported
//! selectors and break the copy-on-create flow from repository defaults. The
//! security boundary is structural argv construction and shell escaping, not
//! selector content validation.

use std::fmt;

/// Error returned when a version selector is structurally unrepresentable.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum VersionSelectorError {
    /// The selector contains an embedded NUL byte (`\0`). NUL cannot appear
    /// in a process argument (it terminates the C-string on Unix) and cannot
    /// be safely shell-escaped.
    EmbeddedNul,
}

impl fmt::Display for VersionSelectorError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::EmbeddedNul => write!(
                f,
                "Version selector contains an embedded NUL byte, which cannot be passed to a \
                 process or shell-escaped. Remove the NUL character from the Version field."
            ),
        }
    }
}

impl std::error::Error for VersionSelectorError {}

/// Validate a version selector before persistence or launch.
///
/// Accepts blank, whitespace-only, and any npm-supported selector (exact
/// versions, prereleases/nightlies, dist tags). Rejects only embedded NUL.
///
/// # Errors
///
/// Returns [`VersionSelectorError::EmbeddedNul`] when the selector contains
/// a NUL byte (`\0`).
pub fn validate_version_selector(selector: &str) -> Result<(), VersionSelectorError> {
    if selector.contains('\0') {
        return Err(VersionSelectorError::EmbeddedNul);
    }
    Ok(())
}

/// Normalize a version selector: validate, then trim surrounding whitespace.
///
/// Returns the trimmed selector on success, or the validation error. A
/// whitespace-only or blank input yields an empty string (the direct-launch
/// invariant).
///
/// # Errors
///
/// Returns [`VersionSelectorError`] when the selector is structurally
/// unrepresentable (embedded NUL).
pub fn normalize_version_selector(selector: &str) -> Result<String, VersionSelectorError> {
    validate_version_selector(selector)?;
    Ok(selector.trim().to_owned())
}

#[cfg(test)]
#[path = "version_selector_tests.rs"]
mod tests;

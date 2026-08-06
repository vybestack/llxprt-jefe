//! Manifest value types: package-relative paths, host triples, and secret
//! references (issue #389 CW-09, acceptance rows D6 and D8).
//!
//! Each is a parse-don't-validate value with a private field, so an
//! unvalidated string can never reach the install transaction or the provider
//! launch path.

use std::fmt;

use super::limits::{PACKAGE_PATH_BYTE_LIMIT, PACKAGE_PATH_DEPTH_LIMIT, SECRET_ENV_BYTE_LIMIT};

/// The host triple this executable was built for.
///
/// Emitted by `build.rs` from Cargo's `TARGET`, because a target triple is a
/// build-time fact that `std` does not expose at runtime — `env::consts` gives
/// only the architecture and OS, which cannot distinguish `-gnu` from `-musl`
/// or `-msvc` from `-gnu` on Windows.
const BUILD_HOST_TRIPLE: &str = env!("JEFE_HOST_TRIPLE");

/// A validated path relative to a package directory.
///
/// The rules are the containment rules the archive transaction enforces, held
/// in one place so a manifest-declared path and an archive entry path cannot
/// disagree about what is contained.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct RelativePath {
    text: String,
    components: Vec<String>,
}

impl RelativePath {
    /// Parse and validate a package-relative path.
    ///
    /// # Errors
    ///
    /// Returns [`RelativePathError`] when the path is absolute, contains a
    /// backslash or NUL, has an empty, `.`, or `..` component, or exceeds the
    /// depth or byte bound.
    pub fn parse(value: &str) -> Result<Self, RelativePathError> {
        let reject = |reason| {
            Err(RelativePathError {
                raw: value.to_owned(),
                reason,
            })
        };
        if value.len() > PACKAGE_PATH_BYTE_LIMIT {
            return reject(RelativePathErrorReason::Length);
        }
        if value.contains('\\') {
            return reject(RelativePathErrorReason::Backslash);
        }
        if value.contains('\u{0}') {
            return reject(RelativePathErrorReason::Nul);
        }
        if value.starts_with('/') {
            return reject(RelativePathErrorReason::Absolute);
        }
        let components: Vec<String> = value.split('/').map(ToOwned::to_owned).collect();
        if components
            .iter()
            .any(|component| matches!(component.as_str(), "" | "." | ".."))
        {
            return reject(RelativePathErrorReason::Component);
        }
        if components.len() > PACKAGE_PATH_DEPTH_LIMIT {
            return reject(RelativePathErrorReason::Depth);
        }
        Ok(Self {
            text: value.to_owned(),
            components,
        })
    }

    /// Borrow the exact declared text.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.text
    }

    /// Borrow the validated path components.
    #[must_use]
    pub fn components(&self) -> &[String] {
        &self.components
    }

    /// How many components the path carries.
    #[must_use]
    pub fn depth(&self) -> usize {
        self.components.len()
    }
}

impl fmt::Display for RelativePath {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.as_str())
    }
}

/// A rejected package-relative path and why it failed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RelativePathError {
    /// The raw value that failed validation.
    pub raw: String,
    /// Why it was rejected.
    pub reason: RelativePathErrorReason,
}

/// Categorized reason a package-relative path failed validation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RelativePathErrorReason {
    /// Longer than [`PACKAGE_PATH_BYTE_LIMIT`] bytes.
    Length,
    /// Rooted at `/`.
    Absolute,
    /// Contains a backslash, which would name different files per host.
    Backslash,
    /// Contains a NUL byte.
    Nul,
    /// Has an empty, `.`, or `..` component.
    Component,
    /// More than [`PACKAGE_PATH_DEPTH_LIMIT`] components.
    Depth,
}

impl RelativePathErrorReason {
    /// Human-readable reason text.
    #[must_use]
    pub const fn message(self) -> &'static str {
        match self {
            Self::Length => "longer than 1024 bytes",
            Self::Absolute => "absolute paths are not package-relative",
            Self::Backslash => "a backslash is never a path separator",
            Self::Nul => "contains a NUL byte",
            Self::Component => "has an empty, '.', or '..' component",
            Self::Depth => "more than 16 components deep",
        }
    }
}

impl fmt::Display for RelativePathError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            formatter,
            "invalid package path {:?}: {}",
            self.raw,
            self.reason.message()
        )
    }
}

impl std::error::Error for RelativePathError {}

/// Number of components in `arch-vendor-os`.
const TRIPLE_MIN_PARTS: usize = 3;

/// Number of components in `arch-vendor-os-env`.
const TRIPLE_MAX_PARTS: usize = 4;

/// An exact build host triple, such as `aarch64-apple-darwin`.
///
/// A provider binary is keyed by the exact triple it was built for. Matching is
/// exact and never approximate: an architecture-and-OS guess cannot tell
/// `-gnu` from `-musl`, and running the wrong one is a crash rather than a
/// diagnosable "unsupported platform".
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct HostTriple(String);

impl HostTriple {
    /// Parse and validate an exact host triple.
    ///
    /// # Errors
    ///
    /// Returns [`HostTripleError`] when the value is not three or four
    /// non-empty `[a-z0-9_]` components separated by `-`.
    pub fn parse(value: &str) -> Result<Self, HostTripleError> {
        let parts: Vec<&str> = value.split('-').collect();
        let shaped = (TRIPLE_MIN_PARTS..=TRIPLE_MAX_PARTS).contains(&parts.len())
            && parts.iter().all(|part| {
                !part.is_empty()
                    && part.bytes().all(|byte| {
                        byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'_'
                    })
            });
        if shaped {
            Ok(Self(value.to_owned()))
        } else {
            Err(HostTripleError {
                raw: value.to_owned(),
            })
        }
    }

    /// The triple this executable was built for.
    #[must_use]
    pub fn current() -> Self {
        Self(BUILD_HOST_TRIPLE.to_owned())
    }

    /// Borrow the exact triple text.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for HostTriple {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.as_str())
    }
}

/// A rejected host triple.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HostTripleError {
    /// The raw value that failed validation.
    pub raw: String,
}

impl fmt::Display for HostTripleError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            formatter,
            "invalid host triple {:?}: expected arch-vendor-os[-env]",
            self.raw
        )
    }
}

impl std::error::Error for HostTripleError {}

/// A reference to a secret held in an environment variable.
///
/// The manifest names the variable; it never carries the value. Nothing in this
/// type reads the environment, so a manifest cannot leak a secret by being
/// parsed, rendered, or logged.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct SecretReference(String);

impl SecretReference {
    /// Parse and validate an environment-variable name.
    ///
    /// # Errors
    ///
    /// Returns [`SecretReferenceError`] when the value does not match
    /// `[A-Z_][A-Z0-9_]{0,127}`.
    pub fn parse(value: &str) -> Result<Self, SecretReferenceError> {
        let mut bytes = value.bytes();
        let valid = value.len() <= SECRET_ENV_BYTE_LIMIT
            && bytes
                .next()
                .is_some_and(|byte| byte.is_ascii_uppercase() || byte == b'_')
            && bytes.all(|byte| byte.is_ascii_uppercase() || byte.is_ascii_digit() || byte == b'_');
        if valid {
            Ok(Self(value.to_owned()))
        } else {
            Err(SecretReferenceError {
                raw: value.to_owned(),
            })
        }
    }

    /// Borrow the environment-variable name.
    #[must_use]
    pub fn env(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for SecretReference {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.env())
    }
}

/// A rejected secret reference.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SecretReferenceError {
    /// The raw value that failed validation.
    pub raw: String,
}

impl fmt::Display for SecretReferenceError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            formatter,
            "invalid secret reference {:?}: expected [A-Z_][A-Z0-9_]{{0,127}}",
            self.raw
        )
    }
}

impl std::error::Error for SecretReferenceError {}

#[cfg(test)]
#[path = "values_tests.rs"]
mod tests;

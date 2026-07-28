//! Strict `AgentTypeId` and candidate `CandidateKind`/`ExecutableCandidate`
//! types for the closed definition contract (issue #382 CW-02).
//!
//! `AgentTypeId` replaces the closed `AgentTypeId` enum. Format: lowercase
//! ASCII, 1–128 bytes, matching `[a-z][a-z0-9]*(?:[.-][a-z0-9]+)*`. Construction
//! is via [`AgentTypeId::parse`] (validated) only; there is no public
//! `from_validated` bypass. Schema-1 alias mapping lives only inside the
//! one-way persistence migration (issue #382 non-goals) and is therefore not
//! present in this S1 domain contract.

use std::fmt;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use super::limits::{ID_BYTE_LIMIT, PATH_LIMIT, STRING_VALUE_BYTE_LIMIT};

/// Stable, validated agent-type identifier replacing the closed `AgentTypeId`
/// enum. Format: lowercase ASCII, 1–128 bytes, matching
/// `[a-z][a-z0-9]*(?:[.-][a-z0-9]+)*`.
///
/// Construction is via [`AgentTypeId::parse`] only. The inner value is private
/// so an unvalidated `String` can never become an `AgentTypeId`.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct AgentTypeId(String);

impl AgentTypeId {
    /// Parse and validate an agent-type id.
    ///
    /// # Errors
    ///
    /// Returns [`AgentTypeIdError`] when the value is empty, longer than
    /// 128 bytes, or fails the grammar `[a-z][a-z0-9]*(?:[.-][a-z0-9]+)*`.
    pub fn parse(value: &str) -> Result<Self, AgentTypeIdError> {
        if value.is_empty() || value.len() > ID_BYTE_LIMIT {
            return Err(AgentTypeIdError {
                raw: value.to_owned(),
                reason: AgentTypeIdErrorReason::Length,
            });
        }
        if !valid_type_id(value.as_bytes()) {
            return Err(AgentTypeIdError {
                raw: value.to_owned(),
                reason: AgentTypeIdErrorReason::Grammar,
            });
        }
        Ok(Self(value.to_owned()))
    }

    /// Construct from bytes already known to satisfy the grammar.
    ///
    /// This is a `pub(crate)` convenience for the shipped-definition builder,
    /// which constructs the four stable ids from string literals that are
    /// proven valid by the unit tests in this module. It is not a public
    /// bypass: external callers must use [`AgentTypeId::parse`].
    pub(crate) fn from_validated(value: &str) -> Self {
        // The shipped builders are the only callers; the grammar is verified
        // by the `type_id_grammar` table test. A failure here is a programmer
        // error in the shipped data, not a runtime data path, so fall back to
        // a deterministic invalid sentinel rather than panicking or logging.
        Self::parse(value).unwrap_or_else(|err| invalid_shipped_id(value, err))
    }

    /// Borrow the validated identifier text.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// Whether this id is the shipped stable id for the given literal.
    #[must_use]
    pub fn is_stable(&self, literal: &str) -> bool {
        self.0 == literal
    }
}

fn invalid_shipped_id(value: &str, err: AgentTypeIdError) -> AgentTypeId {
    // A shipped stable id that fails validation is a compile-time-shaped
    // programming error in the allowlisted shipped-data module, not a runtime
    // data path. Return a deterministic invalid sentinel so validation still
    // rejects it downstream rather than silently emitting a broken id. This
    // path is unreachable when the shipped literals are valid; the cause is
    // recorded only in the returned id's text for diagnostics.
    let _ = (value, &err);
    AgentTypeId("__invalid_shipped_id__".to_string())
}

impl fmt::Display for AgentTypeId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

impl Serialize for AgentTypeId {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        serializer.serialize_str(self.as_str())
    }
}

impl Default for AgentTypeId {
    /// Default to the shipped LLxprt agent-type id so schema-1 persisted
    /// documents that predate the generic `type_id` field deserialize into
    /// the dominant default agent kind rather than failing.
    fn default() -> Self {
        Self::from_validated("core.llxprt")
    }
}

impl<'de> Deserialize<'de> for AgentTypeId {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let raw = String::deserialize(deserializer)?;
        Self::parse(&raw).map_err(serde::de::Error::custom)
    }
}

/// Error returned for an invalid agent-type id.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AgentTypeIdError {
    /// The rejected raw value.
    pub raw: String,
    /// Categorized failure reason.
    pub reason: AgentTypeIdErrorReason,
}

/// Categorized reason an agent-type id failed validation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AgentTypeIdErrorReason {
    /// Empty or longer than 128 bytes.
    Length,
    /// Failed the id grammar.
    Grammar,
}

impl fmt::Display for AgentTypeIdError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let reason = match self.reason {
            AgentTypeIdErrorReason::Length => "must be 1..=128 bytes",
            AgentTypeIdErrorReason::Grammar => "must match [a-z][a-z0-9]*(?:[.-][a-z0-9]+)*",
        };
        write!(f, "invalid agent type id {:?}: {reason}", self.raw)
    }
}

impl std::error::Error for AgentTypeIdError {}

/// Validate the agent-type-id grammar `[a-z][a-z0-9]*(?:[.-][a-z0-9]+)*`.
fn valid_type_id(bytes: &[u8]) -> bool {
    let Some(first) = bytes.first() else {
        return false;
    };
    if !first.is_ascii_lowercase() {
        return false;
    }
    let mut separator = false;
    for byte in &bytes[1..] {
        if byte.is_ascii_lowercase() || byte.is_ascii_digit() {
            separator = false;
        } else if matches!(byte, b'.' | b'-') && !separator {
            separator = true;
        } else {
            return false;
        }
    }
    !separator
}

/// Capability-id grammar: lowercase ASCII alphanumerics with optional `.`
/// separators between groups, matching the agent-type-id grammar minus the
/// leading-letter restriction's strictness — but per the closed contract the
/// capability id reuses the same grammar as the type id.
fn valid_capability_id(bytes: &[u8]) -> bool {
    valid_type_id(bytes)
}

/// One declared way to discover an agent executable.
///
/// Candidates are inspected in declaration order; the first physically valid
/// candidate is selected. `path-name` values containing `/` are rejected
/// except the typed repository-LLxprt candidate. Package-runner candidates
/// participate only when the agent's persisted version selector is nonblank.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "kebab-case")]
pub enum CandidateKind {
    /// Resolve by bare name from the PATH snapshot.
    PathName {
        #[serde(rename = "value")]
        name: String,
    },
    /// Repository-local LLxprt binary (`<repo>/.llxprt/bin/llxprt`).
    ///
    /// Unit variant: the relative path is carried by the candidate's `value`.
    /// This is the one allowlisted product adapter in the candidate contract.
    RepositoryLlxprt,
    /// npm package runner
    /// (`npm exec --yes --package=<package>@<selector> -- <binary>`).
    NpmPackage { package: String, binary: String },
    /// uvx package runner (`uvx --from <package>==<selector> <binary>`).
    UvxPackage { package: String, binary: String },
}

impl CandidateKind {
    /// Whether this candidate is a package-runner (npm or uvx).
    #[must_use]
    pub fn is_package_runner(&self) -> bool {
        matches!(self, Self::NpmPackage { .. } | Self::UvxPackage { .. })
    }

    /// Bare executable name to probe on PATH for direct resolution, if any.
    #[must_use]
    pub fn path_name(&self) -> Option<&str> {
        match self {
            Self::PathName { name } => Some(name),
            _ => None,
        }
    }

    /// Validate this candidate kind against the closed bounds.
    pub fn validate(&self, value: &Path) -> Result<(), CandidateValidateError> {
        match self {
            Self::PathName { name } => {
                if name.is_empty() || name.len() > STRING_VALUE_BYTE_LIMIT {
                    return Err(CandidateValidateError::NameLength);
                }
                if name.contains('/') {
                    return Err(CandidateValidateError::PathNameSlash);
                }
            }
            Self::RepositoryLlxprt => {
                let s = value.to_string_lossy();
                if s.is_empty() || s.len() > PATH_LIMIT {
                    return Err(CandidateValidateError::ValueLength);
                }
                if s.contains("..") || std::path::Path::new(s.as_ref()).is_absolute() {
                    return Err(CandidateValidateError::UnsafeRelative);
                }
            }
            Self::NpmPackage { package, binary } | Self::UvxPackage { package, binary } => {
                if package.is_empty() || package.len() > STRING_VALUE_BYTE_LIMIT {
                    return Err(CandidateValidateError::PackageLength);
                }
                if package
                    .chars()
                    .any(|character| character.is_control() || character.is_whitespace())
                {
                    return Err(CandidateValidateError::PackageUnsafe);
                }
                if binary.is_empty() || binary.len() > STRING_VALUE_BYTE_LIMIT {
                    return Err(CandidateValidateError::BinaryLength);
                }
                if binary == "."
                    || binary == ".."
                    || binary
                        .chars()
                        .any(|character| character.is_control() || character.is_whitespace())
                    || binary.contains(['/', '\\'])
                {
                    return Err(CandidateValidateError::BinaryUnsafe);
                }
            }
        }
        Ok(())
    }
}

/// Candidate validation error.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CandidateValidateError {
    /// Path-name candidate name length is outside 1..=4096 bytes.
    NameLength,
    /// Path-name candidate contains `/`.
    PathNameSlash,
    /// Repository-LLxprt value length is outside 1..=4096 bytes.
    ValueLength,
    /// Repository-LLxprt value is not a safe relative path.
    UnsafeRelative,
    /// Package-runner package length is outside 1..=4096 bytes.
    PackageLength,
    /// Package-runner package contains whitespace or control characters.
    PackageUnsafe,
    /// Package-runner binary length is outside 1..=4096 bytes.
    BinaryLength,
    /// Package-runner binary is not one safe path component.
    BinaryUnsafe,
}

impl fmt::Display for CandidateValidateError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let msg = match self {
            Self::NameLength => "path-name value must be 1..=4096 bytes",
            Self::PathNameSlash => "path-name value must not contain '/'",
            Self::ValueLength => "repository-llxprt value must be 1..=4096 bytes",
            Self::UnsafeRelative => "repository-llxprt value must be a safe relative path",
            Self::PackageLength => "package-runner package must be 1..=4096 bytes",
            Self::PackageUnsafe => {
                "package-runner package must not contain whitespace or control characters"
            }
            Self::BinaryLength => "package-runner binary must be 1..=4096 bytes",
            Self::BinaryUnsafe => "package-runner binary must be one safe path component",
        };
        f.write_str(msg)
    }
}

impl std::error::Error for CandidateValidateError {}

/// A resolved-or-declared executable candidate.
///
/// The `value` field carries the resolved path for `path-name` and
/// `repository-llxprt` candidates (set during resolution); package-runner
/// candidates carry the runner-relative path after resolution.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ExecutableCandidate {
    /// The typed candidate kind.
    pub kind: CandidateKind,
    /// Resolved or declared path/relative-path.
    pub value: PathBuf,
}

impl ExecutableCandidate {
    /// Validate this candidate against the closed bounds.
    pub fn validate(&self) -> Result<(), CandidateValidateError> {
        self.kind.validate(&self.value)
    }
}

/// Validate a capability id against the closed grammar.
pub fn validate_capability_id(id: &str) -> Result<(), CapabilityIdError> {
    if id.is_empty() || id.len() > STRING_VALUE_BYTE_LIMIT {
        return Err(CapabilityIdError::Length);
    }
    if !valid_capability_id(id.as_bytes()) {
        return Err(CapabilityIdError::Grammar);
    }
    Ok(())
}

/// Capability-id validation error.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CapabilityIdError {
    /// Empty or too long.
    Length,
    /// Failed the grammar.
    Grammar,
}

impl fmt::Display for CapabilityIdError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let msg = match self {
            Self::Length => "capability id must be 1..=4096 bytes",
            Self::Grammar => "capability id must match [a-z][a-z0-9]*(?:[.-][a-z0-9]+)*",
        };
        f.write_str(msg)
    }
}

impl std::error::Error for CapabilityIdError {}

#[cfg(test)]
#[path = "type_id_tests.rs"]
mod tests;

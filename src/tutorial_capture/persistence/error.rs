//! Error type for persistence operations.
//!
//! @requirement REQ-TUTORIAL-CAPTURE-007

use std::path::PathBuf;

/// Error returned by persistence operations.
///
/// @requirement REQ-TUTORIAL-CAPTURE-007
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PersistenceError {
    /// The manifest schema version is unknown or incompatible.
    SchemaVersion { found: u32, expected: u32 },
    /// The manifest JSON is malformed.
    Json { reason: String },
    /// A file I/O operation failed.
    Io { path: String, reason: String },
    /// A run ID failed domain validation during reconstruction.
    InvalidRunId { value: String },
    /// An owned path is not contained within the run root.
    PathNotContained { path: PathBuf, run_root: PathBuf },
    /// An owned path does not match any expected resource-kind sub-directory.
    UnexpectedSubdir { path: PathBuf },
    /// A symlink was found where a real directory/file was expected.
    SymlinkFound { path: PathBuf },
    /// A duplicate owned path was detected.
    DuplicatePath { path: PathBuf },
    /// The run root already exists (exclusive creation collision).
    RunRootCollision { path: PathBuf },
    /// The run root is inside a production/current checkout.
    ProductionCheckout { path: PathBuf, reason: String },
    /// A path component contains a NUL byte (path injection).
    NulInPath { path: String },
    /// A required field is missing from the DTO.
    MissingField { field: String },
    /// A field value failed custom validation.
    InvalidField { field: String, reason: String },
}

impl std::fmt::Display for PersistenceError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::SchemaVersion { found, expected } => {
                write!(
                    f,
                    "manifest schema version mismatch: found {found}, expected {expected}"
                )
            }
            Self::Json { reason } => write!(f, "manifest JSON error: {reason}"),
            Self::Io { path, reason } => write!(f, "I/O error at '{path}': {reason}"),
            Self::InvalidRunId { value } => {
                write!(
                    f,
                    "invalid run ID '{value}': must be 1-64 ASCII alphanumeric or hyphen chars"
                )
            }
            Self::PathNotContained { path, run_root } => write!(
                f,
                "path '{}' is not contained within run root '{}'",
                path.display(),
                run_root.display()
            ),
            Self::UnexpectedSubdir { path } => {
                write!(
                    f,
                    "path '{}' does not match any expected resource-kind sub-directory",
                    path.display()
                )
            }
            Self::SymlinkFound { path } => {
                write!(
                    f,
                    "symlink found where real directory/file expected: '{}'",
                    path.display()
                )
            }
            Self::DuplicatePath { path } => {
                write!(f, "duplicate owned path: '{}'", path.display())
            }
            Self::RunRootCollision { path } => {
                write!(
                    f,
                    "run root already exists (exclusive collision): '{}'",
                    path.display()
                )
            }
            Self::ProductionCheckout { path, reason } => {
                write!(
                    f,
                    "run root '{}' is inside a production/current checkout: {reason}",
                    path.display()
                )
            }
            Self::NulInPath { path } => {
                write!(f, "NUL byte found in path: '{path}'")
            }
            Self::MissingField { field } => write!(f, "missing field '{field}' in manifest"),
            Self::InvalidField { field, reason } => {
                write!(f, "invalid field '{field}': {reason}")
            }
        }
    }
}

impl std::error::Error for PersistenceError {}

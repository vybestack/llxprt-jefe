//! Deterministic, bounded discovery of user screen definition files
//! (issue #385, CW05-01).
//!
//! This is the only place the definitions directory is enumerated, and the only
//! place a screen definition's bytes are read. It performs no parsing: it
//! answers "which files is Jefe willing to look at, in what order, and what
//! bytes do they hold", and hands that to the workbench, which is I/O-free.
//!
//! The acceptance rule is exactness rather than tolerance. A definitions
//! directory is a place a user drops files into, so the discovery surface is
//! also the attack surface, and everything that is not precisely one direct
//! regular file named `<member>.screen.toml` is simply not a candidate:
//!
//! - no recursion, so a nested tree cannot smuggle definitions in;
//! - no symbolic links, so a definition cannot name bytes outside the directory;
//! - no hidden files and no extension aliases, so `review.screen.toml.bak` and
//!   `review.screen.tml` are editor leftovers rather than live screens;
//! - no non-UTF-8 names, because the file-name stem *is* the screen's member and
//!   has to be quotable in diagnostics;
//! - nothing over [`FILE_LIMIT`] bytes reaches a parser.
//!
//! Order is by canonical path bytes so two machines holding the same directory
//! compose the same registry, and so a diagnostic naming "the first offending
//! file" names the same file twice.

use std::ffi::OsStr;
use std::io::Read;
use std::path::{Path, PathBuf};

use super::diagnostic::FILE_LIMIT;

/// Suffix that marks a screen definition file.
pub const SCREEN_FILE_SUFFIX: &str = ".screen.toml";

/// Why one otherwise well-named candidate produced no bytes.
///
/// A candidate that fails here is still a candidate: its member names an owner,
/// so composition still has to decide whether that owner is active before it
/// decides whether the failure is fatal.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ScreenFileRejection {
    /// The file is larger than [`FILE_LIMIT`] bytes.
    TooLarge {
        /// Size reported before reading, in bytes.
        bytes: u64,
    },
    /// The bytes are not valid UTF-8.
    NotUtf8,
    /// The file could not be read.
    Unreadable {
        /// Redacted operating-system reason.
        reason: String,
    },
}

impl std::fmt::Display for ScreenFileRejection {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::TooLarge { bytes } => {
                write!(formatter, "file is {bytes} bytes (max {FILE_LIMIT})")
            }
            Self::NotUtf8 => formatter.write_str("file is not valid UTF-8"),
            Self::Unreadable { reason } => write!(formatter, "file could not be read: {reason}"),
        }
    }
}

/// One discovered screen definition candidate.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ScreenFileCandidate {
    /// Full path, for diagnostics.
    pub path: PathBuf,
    /// File-name stem, which is also the screen's `local.<member>` member.
    pub member: String,
    /// The file text, or why there is none.
    pub text: Result<String, ScreenFileRejection>,
}

/// The definitions directory itself could not be enumerated.
///
/// This is separate from a per-file rejection because it is not attributable to
/// any one owner: an unreadable directory could be hiding an active screen, so
/// composition cannot safely continue.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DefinitionsUnreadable {
    /// The directory that could not be enumerated.
    pub path: PathBuf,
    /// Redacted operating-system reason.
    pub reason: String,
}

impl std::fmt::Display for DefinitionsUnreadable {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            formatter,
            "definitions directory {} could not be read: {}",
            self.path.display(),
            self.reason
        )
    }
}

impl std::error::Error for DefinitionsUnreadable {}

/// Enumerate every screen definition candidate under `root`, in canonical order.
///
/// A missing directory yields no candidates rather than an error: running
/// without any custom screens is the ordinary case, not a misconfiguration.
///
/// # Errors
///
/// Returns [`DefinitionsUnreadable`] when the directory exists but cannot be
/// enumerated, because an unreadable directory cannot be distinguished from one
/// holding an active screen.
pub fn discover(root: &Path) -> Result<Vec<ScreenFileCandidate>, DefinitionsUnreadable> {
    let entries = match std::fs::read_dir(root) {
        Ok(entries) => entries,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(error) => {
            return Err(DefinitionsUnreadable {
                path: root.to_path_buf(),
                reason: error.kind().to_string(),
            });
        }
    };

    let mut paths: Vec<(PathBuf, String)> = Vec::new();
    for entry in entries {
        let entry = entry.map_err(|error| DefinitionsUnreadable {
            path: root.to_path_buf(),
            reason: error.kind().to_string(),
        })?;
        let path = entry.path();
        if let Some(member) = candidate_member(&path) {
            paths.push((path, member));
        }
    }
    paths.sort_by_key(|(path, _)| canonical_key(path));

    Ok(paths
        .into_iter()
        .map(|(path, member)| ScreenFileCandidate {
            text: read_bounded(&path),
            path,
            member,
        })
        .collect())
}

/// Sort key: the encoded bytes of the whole path.
fn canonical_key(path: &Path) -> Vec<u8> {
    path.as_os_str().as_encoded_bytes().to_vec()
}

/// The member this path declares, if the path is an acceptable candidate.
///
/// Returns `None` for anything that is not a direct, non-symlink, regular file
/// whose name is exactly `<member>.screen.toml`.
fn candidate_member(path: &Path) -> Option<String> {
    // `symlink_metadata` does not traverse, so a link to a regular file is
    // rejected as the link it is rather than accepted as its target.
    let metadata = std::fs::symlink_metadata(path).ok()?;
    if !metadata.is_file() {
        return None;
    }
    let name = path.file_name().and_then(OsStr::to_str)?;
    let member = name.strip_suffix(SCREEN_FILE_SUFFIX)?;
    crate::workbench::ids::check_custom_member(member).ok()?;
    Some(member.to_owned())
}

/// Read at most [`FILE_LIMIT`] bytes of UTF-8 text from an already-vetted path.
fn read_bounded(path: &Path) -> Result<String, ScreenFileRejection> {
    let file = std::fs::File::open(path).map_err(|error| ScreenFileRejection::Unreadable {
        reason: error.kind().to_string(),
    })?;
    // Re-check the opened handle rather than trusting the earlier stat: between
    // the directory scan and the open, the name could have been replaced by a
    // directory, a device, or a link to one.
    let metadata = file
        .metadata()
        .map_err(|error| ScreenFileRejection::Unreadable {
            reason: error.kind().to_string(),
        })?;
    if !metadata.is_file() {
        return Err(ScreenFileRejection::Unreadable {
            reason: "not a regular file".to_owned(),
        });
    }
    if metadata.len() > FILE_LIMIT as u64 {
        return Err(ScreenFileRejection::TooLarge {
            bytes: metadata.len(),
        });
    }
    // Read one byte past the limit so a file that grew between the stat and the
    // read is rejected instead of silently truncated into a parseable prefix.
    let mut bytes = Vec::new();
    file.take(FILE_LIMIT as u64 + 1)
        .read_to_end(&mut bytes)
        .map_err(|error| ScreenFileRejection::Unreadable {
            reason: error.kind().to_string(),
        })?;
    if bytes.len() > FILE_LIMIT {
        return Err(ScreenFileRejection::TooLarge {
            bytes: bytes.len() as u64,
        });
    }
    String::from_utf8(bytes).map_err(|_| ScreenFileRejection::NotUtf8)
}

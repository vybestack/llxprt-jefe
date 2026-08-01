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
//! - at most [`MAX_SCREENS`] candidates, and nothing over [`FILE_LIMIT`] bytes
//!   reaches a parser, so the bytes resident during startup are bounded by a
//!   declared limit rather than by how many files someone left in a directory.
//!
//! Order is by canonical path bytes so two machines holding the same directory
//! compose the same registry, and so a diagnostic naming "the first offending
//! file" names the same file twice.

use std::ffi::OsStr;
use std::io::Read;
use std::path::{Path, PathBuf};

use crate::workbench::ids::MAX_SCREENS;

use super::diagnostic::FILE_LIMIT;

/// Suffix that marks a screen definition file.
pub const SCREEN_FILE_SUFFIX: &str = ".screen.toml";

/// Why one otherwise well-named candidate produced no bytes.
///
/// A candidate that fails here is still a candidate: its member names an owner,
/// so composition still has to decide whether that owner is active before it
/// decides whether the failure is fatal. That is why an unreadable file is
/// reported rather than skipped — skipping it would let a file that an operator
/// enabled disappear silently.
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
    /// What was opened is not the regular file that was enumerated.
    Replaced,
}

impl std::fmt::Display for ScreenFileRejection {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::TooLarge { bytes } => {
                write!(formatter, "file is {bytes} bytes (max {FILE_LIMIT})")
            }
            Self::NotUtf8 => formatter.write_str("file is not valid UTF-8"),
            Self::Unreadable { reason } => write!(formatter, "file could not be read: {reason}"),
            Self::Replaced => {
                formatter.write_str("file was replaced between discovery and reading")
            }
        }
    }
}

impl std::error::Error for ScreenFileRejection {}

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

/// The definitions directory itself could not be enumerated or is overfull.
///
/// This is separate from a per-file rejection because it is not attributable to
/// any one owner: an unreadable or overfull directory could be hiding an active
/// screen, so composition cannot safely continue.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DefinitionsUnreadable {
    /// The directory that could not be used.
    pub path: PathBuf,
    /// Redacted reason.
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
/// enumerated, or holds more than [`MAX_SCREENS`] candidates. Neither can be
/// distinguished from a directory holding an active screen, so neither is safe
/// to continue past.
pub fn discover(root: &Path) -> Result<Vec<ScreenFileCandidate>, DefinitionsUnreadable> {
    let mut paths = enumerate(root)?;
    if paths.len() > MAX_SCREENS {
        return Err(DefinitionsUnreadable {
            path: root.to_path_buf(),
            reason: format!("{} definitions declared (max {MAX_SCREENS})", paths.len()),
        });
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

/// Collect every exactly named entry, without reading any of them.
fn enumerate(root: &Path) -> Result<Vec<(PathBuf, String)>, DefinitionsUnreadable> {
    let entries = match std::fs::read_dir(root) {
        Ok(entries) => entries,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(error) => return Err(unreadable(root, error)),
    };
    let mut paths = Vec::new();
    for entry in entries {
        let path = entry.map_err(|error| unreadable(root, error))?.path();
        if let Some(member) = candidate_member(&path) {
            paths.push((path, member));
        }
    }
    Ok(paths)
}

fn unreadable(root: &Path, error: std::io::Error) -> DefinitionsUnreadable {
    DefinitionsUnreadable {
        path: root.to_path_buf(),
        reason: error.kind().to_string(),
    }
}

/// Sort key: the encoded bytes of the whole path.
fn canonical_key(path: &Path) -> Vec<u8> {
    path.as_os_str().as_encoded_bytes().to_vec()
}

/// The member this path declares, if the path is an acceptable candidate.
///
/// The name is checked before the filesystem is, so an entry that is not a
/// definition costs nothing and an entry that *is* named like one is never
/// dropped for a transient metadata failure — it becomes an unreadable
/// candidate instead, which composition can refuse if its owner is enabled.
fn candidate_member(path: &Path) -> Option<String> {
    let name = path.file_name().and_then(OsStr::to_str)?;
    let member = name.strip_suffix(SCREEN_FILE_SUFFIX)?;
    crate::workbench::ids::check_custom_member(member).ok()?;
    // `symlink_metadata` does not traverse, so a link to a regular file is
    // rejected as the link it is rather than accepted as its target. A metadata
    // failure is deliberately *not* a rejection of the name: the file is kept
    // as a candidate and the read reports why it produced nothing.
    match std::fs::symlink_metadata(path) {
        Ok(metadata) if !metadata.is_file() => None,
        _ => Some(member.to_owned()),
    }
}

/// Read at most [`FILE_LIMIT`] bytes of UTF-8 text from an already-named path.
pub(super) fn read_bounded(path: &Path) -> Result<String, ScreenFileRejection> {
    let before = std::fs::symlink_metadata(path).map_err(unreadable_file)?;
    if !before.is_file() {
        return Err(ScreenFileRejection::Replaced);
    }
    let file = std::fs::File::open(path).map_err(unreadable_file)?;
    // Opening by name follows a symlink, so the handle is compared against the
    // entry that was enumerated. If the name was swapped for a link, a device,
    // or another file between the two calls, the open landed somewhere else and
    // the read is refused rather than trusted.
    let after = file.metadata().map_err(unreadable_file)?;
    if !after.is_file() || !same_file(&before, &after) {
        return Err(ScreenFileRejection::Replaced);
    }
    if after.len() > FILE_LIMIT as u64 {
        return Err(ScreenFileRejection::TooLarge { bytes: after.len() });
    }
    // Read one byte past the limit so a file that grew between the stat and the
    // read is rejected instead of silently truncated into a parseable prefix.
    let mut bytes = Vec::new();
    file.take(FILE_LIMIT as u64 + 1)
        .read_to_end(&mut bytes)
        .map_err(unreadable_file)?;
    if bytes.len() > FILE_LIMIT {
        return Err(ScreenFileRejection::TooLarge {
            bytes: bytes.len() as u64,
        });
    }
    String::from_utf8(bytes).map_err(|_| ScreenFileRejection::NotUtf8)
}

fn unreadable_file(error: std::io::Error) -> ScreenFileRejection {
    ScreenFileRejection::Unreadable {
        reason: error.kind().to_string(),
    }
}

/// Whether an opened handle names the same file the directory scan saw.
///
/// On Unix the device and inode settle it exactly. Elsewhere the comparison
/// falls back to file type and modification time, which still catches a name
/// swapped for a directory, a link to a different file, or a rewritten file,
/// and is why the length is re-read from the handle rather than the scan.
#[cfg(unix)]
pub(super) fn same_file(before: &std::fs::Metadata, after: &std::fs::Metadata) -> bool {
    use std::os::unix::fs::MetadataExt;
    before.dev() == after.dev() && before.ino() == after.ino()
}

#[cfg(not(unix))]
pub(super) fn same_file(before: &std::fs::Metadata, after: &std::fs::Metadata) -> bool {
    before.file_type() == after.file_type() && before.modified().ok() == after.modified().ok()
}

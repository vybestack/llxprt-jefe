//! Physical fingerprint of a resolved executable candidate (issue #382 CW-02 S2).
//!
//! Per the issue's deterministic algorithm #2, an executable is canonicalized,
//! opened, and fingerprinted as `(canonical path, platform-native file key,
//! size, mtime)` before probe. This module owns that pure value type; the
//! resolver ([`crate::agent_candidate`]) populates it after canonicalization
//! and metadata capture. The fingerprint participates in launch-generation
//! rechecks (a fingerprint change triggers reprobe under a new generation) and
//! is therefore `Eq`/`Hash`-stable. It deliberately excludes any
//! installation-specific identity or capability: those live in the probe
//! result, not the physical fingerprint.

use std::fmt;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use thiserror::Error;

/// Stable physical fingerprint of one resolved executable.
///
/// Constructed by the candidate resolver after canonicalization and metadata
/// capture. The type is intentionally narrow: every field is a copy type, so
/// the fingerprint is cheap to clone, compare, and hash inside generation
/// reconciliation without borrowing resolver state.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct CandidateFingerprint {
    /// Canonical absolute path of the resolved executable.
    canonical_path: PathBuf,
    /// Device or volume id of the file's containing device.
    #[serde(default)]
    dev: Option<u64>,
    /// Inode or Windows file-index identity of the physical file.
    #[serde(default)]
    ino: Option<u64>,
    /// File size in bytes at fingerprint time.
    size: u64,
    /// Last-modification time, in seconds since the Unix epoch.
    mtime_secs: i64,
    /// Subsecond nanoseconds retained so rapid in-place rewrites are distinct.
    #[serde(default)]
    mtime_nanos: u32,
}

impl CandidateFingerprint {
    /// Construct a fingerprint from captured physical metadata.
    ///
    /// `dev`/`ino` are platform-conditional: callers pass `Some` only where the
    /// underlying `std::fs::Metadata` extension is available. The canonical
    /// path is the resolver's canonicalized absolute path.
    #[must_use]
    pub fn new(
        canonical_path: PathBuf,
        dev: Option<u64>,
        ino: Option<u64>,
        size: u64,
        mtime_secs: i64,
    ) -> Self {
        Self::with_mtime_nanos(canonical_path, dev, ino, size, mtime_secs, 0)
    }

    fn with_mtime_nanos(
        canonical_path: PathBuf,
        dev: Option<u64>,
        ino: Option<u64>,
        size: u64,
        mtime_secs: i64,
        mtime_nanos: u32,
    ) -> Self {
        Self {
            canonical_path,
            dev,
            ino,
            size,
            mtime_secs,
            mtime_nanos,
        }
    }

    /// Canonical absolute path of the resolved executable.
    #[must_use]
    pub fn canonical_path(&self) -> &std::path::Path {
        &self.canonical_path
    }

    /// Device id where available, else `None`.
    #[must_use]
    pub const fn dev(&self) -> Option<u64> {
        self.dev
    }

    /// Inode where available, else `None`.
    #[must_use]
    pub const fn ino(&self) -> Option<u64> {
        self.ino
    }

    /// File size in bytes at fingerprint time.
    #[must_use]
    pub const fn size(&self) -> u64 {
        self.size
    }

    /// Last-modification time in seconds since the Unix epoch.
    #[must_use]
    pub const fn mtime_secs(&self) -> i64 {
        self.mtime_secs
    }

    /// Subsecond nanoseconds of the last-modification time.
    #[must_use]
    pub const fn mtime_nanos(&self) -> u32 {
        self.mtime_nanos
    }

    /// Whether the platform-native physical file key is present.
    #[must_use]
    pub fn has_dev_ino(&self) -> bool {
        self.dev.is_some() && self.ino.is_some()
    }
}

/// Failure to capture authoritative physical executable evidence.
#[derive(Debug, Error)]
pub(crate) enum FingerprintCaptureError {
    /// The supplied executable path could not be canonicalized.
    #[error("canonicalize executable {}: {source}", path.display())]
    Canonicalize {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
    /// Metadata for the canonical executable could not be read.
    #[error("read executable metadata {}: {source}", path.display())]
    Metadata {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
    /// The executable modification time could not be read.
    #[error("read executable modification time {}: {source}", path.display())]
    Modified {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
    /// Windows physical-file identity capture failed.
    #[cfg(windows)]
    #[error("capture Windows file identity for {}: {detail}", path.display())]
    WindowsFileKey { path: PathBuf, detail: String },
}

/// Capture one executable fingerprint using the platform's physical file key.
pub(crate) fn capture_candidate_fingerprint(
    path: &Path,
) -> Result<CandidateFingerprint, FingerprintCaptureError> {
    let canonical =
        std::fs::canonicalize(path).map_err(|source| FingerprintCaptureError::Canonicalize {
            path: path.to_path_buf(),
            source,
        })?;
    let metadata =
        std::fs::metadata(&canonical).map_err(|source| FingerprintCaptureError::Metadata {
            path: canonical.clone(),
            source,
        })?;
    let modified = metadata
        .modified()
        .map_err(|source| FingerprintCaptureError::Modified {
            path: canonical.clone(),
            source,
        })?;
    let (mtime_secs, mtime_nanos) = timestamp_parts(modified);
    #[cfg(unix)]
    let (dev, ino) = capture_file_key(&metadata, &canonical);
    #[cfg(windows)]
    let (dev, ino) = capture_file_key(&metadata, &canonical)?;
    Ok(CandidateFingerprint::with_mtime_nanos(
        canonical,
        dev,
        ino,
        metadata.len(),
        mtime_secs,
        mtime_nanos,
    ))
}

fn timestamp_parts(time: std::time::SystemTime) -> (i64, u32) {
    match time.duration_since(std::time::UNIX_EPOCH) {
        Ok(duration) => (
            i64::try_from(duration.as_secs()).unwrap_or(i64::MAX),
            duration.subsec_nanos(),
        ),
        Err(error) => {
            let duration = error.duration();
            let seconds = i64::try_from(duration.as_secs()).unwrap_or(i64::MAX);
            if duration.subsec_nanos() == 0 {
                (seconds.saturating_neg(), 0)
            } else {
                (
                    seconds.saturating_neg().saturating_sub(1),
                    1_000_000_000 - duration.subsec_nanos(),
                )
            }
        }
    }
}
#[cfg(unix)]
fn capture_file_key(metadata: &std::fs::Metadata, _path: &Path) -> (Option<u64>, Option<u64>) {
    use std::os::unix::fs::MetadataExt;
    (Some(metadata.dev()), Some(metadata.ino()))
}

#[cfg(windows)]
fn capture_file_key(
    _metadata: &std::fs::Metadata,
    path: &Path,
) -> Result<(Option<u64>, Option<u64>), FingerprintCaptureError> {
    use winsafe::{HFILE, co};

    let path_text = path
        .to_str()
        .ok_or_else(|| FingerprintCaptureError::WindowsFileKey {
            path: path.to_path_buf(),
            detail: "canonical executable path is not Unicode".to_owned(),
        })?;
    let (handle, _) = HFILE::CreateFile(
        path_text,
        co::GENERIC::READ,
        Some(co::FILE_SHARE::READ | co::FILE_SHARE::WRITE | co::FILE_SHARE::DELETE),
        None,
        co::DISPOSITION::OPEN_EXISTING,
        co::FILE_ATTRIBUTE::NORMAL,
        None,
        None,
        None,
    )
    .map_err(|error| FingerprintCaptureError::WindowsFileKey {
        path: path.to_path_buf(),
        detail: error.to_string(),
    })?;
    let information = handle.GetFileInformationByHandle().map_err(|error| {
        FingerprintCaptureError::WindowsFileKey {
            path: path.to_path_buf(),
            detail: error.to_string(),
        }
    })?;
    Ok((
        Some(u64::from(information.dwVolumeSerialNumber)),
        Some(information.nFileIndex()),
    ))
}

impl fmt::Display for CandidateFingerprint {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        // Diagnostic surface only; never includes secret-bearing content.
        write!(
            f,
            "{}(size={}, mtime={}.{:09}",
            self.canonical_path.display(),
            self.size,
            self.mtime_secs,
            self.mtime_nanos,
        )?;
        if let (Some(dev), Some(ino)) = (self.dev, self.ino) {
            write!(f, ", dev={dev}, ino={ino}")?;
        }
        f.write_str(")")
    }
}

#[cfg(test)]
#[path = "agent_candidate_fingerprint_tests.rs"]
mod tests;

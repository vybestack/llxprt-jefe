//! Physical fingerprint of a resolved executable candidate (issue #382 CW-02 S2).
//!
//! Per the issue's deterministic algorithm #2, an executable is canonicalized,
//! opened, and fingerprinted as `(canonical path, device/inode where available,
//! size, mtime)` before probe. This module owns that pure value type; the
//! resolver ([`crate::agent_candidate`]) populates it after canonicalization
//! and metadata capture. The fingerprint participates in launch-generation
//! rechecks (a fingerprint change triggers reprobe under a new generation) and
//! is therefore `Eq`/`Hash`-stable. It deliberately excludes any
//! installation-specific identity or capability: those live in the probe
//! result, not the physical fingerprint.
//!
//! `dev`/`ino` are captured only where the platform exposes them (Unix).
//! Where unavailable the field is `None`; the remaining `(canonical path,
//! size, mtime)` triple is still a stable per-file fingerprint within a host.

use std::fmt;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};

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
    /// Device id of the file's containing device, where the platform exposes
    /// it (Unix). `None` on platforms without a stable device id.
    #[serde(default)]
    dev: Option<u64>,
    /// Inode number of the file, where the platform exposes it (Unix). `None`
    /// on platforms without a stable inode.
    #[serde(default)]
    ino: Option<u64>,
    /// File size in bytes at fingerprint time.
    size: u64,
    /// Last-modification time, in seconds since the Unix epoch, at fingerprint
    /// time. Captured as a plain integer so the type stays `Hash`/`Eq` without
    /// a `SystemTime` platform representation.
    mtime_secs: i64,
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
        Self {
            canonical_path,
            dev,
            ino,
            size,
            mtime_secs,
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

    /// Whether the dev/inode pair is present.
    ///
    /// On Unix the pair is the strongest identity signal; on platforms without
    /// it the resolver relies on the `(canonical path, size, mtime)` triple.
    #[must_use]
    pub fn has_dev_ino(&self) -> bool {
        self.dev.is_some() && self.ino.is_some()
    }
}

impl fmt::Display for CandidateFingerprint {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        // Diagnostic surface only; never includes secret-bearing content.
        write!(
            f,
            "{}(size={}, mtime={}",
            self.canonical_path.display(),
            self.size,
            self.mtime_secs,
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

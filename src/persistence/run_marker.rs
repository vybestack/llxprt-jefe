//! Durable "a run is in progress" markers.
//!
//! A run that is killed outright cannot report anything about itself, so the
//! only record that survives is one written *before* the death. Each live run
//! owns exactly one marker file next to the durable state file, refreshes it
//! while it runs, and removes it when it ends for a recorded reason. A marker
//! still present at the next start therefore names a run that ended without
//! saying why.
//!
//! Markers are keyed by pid so two concurrent jefe instances sharing a config
//! directory never overwrite or retire each other's record.

use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};

use crate::domain::RunMarker;

use super::writer::{atomic_replace, open_user_only, remove_owned_temp, sync_parent};

const MARKER_DIR: &str = "runs";
const MARKER_PREFIX: &str = "run-";
const MARKER_SUFFIX: &str = ".json";
const TEMP_SUFFIX: &str = ".jefe-tmp";

/// A marker read back from disk, paired with the file it came from.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StoredMarker {
    /// The file the marker was read from.
    pub path: PathBuf,
    /// The recorded run.
    pub marker: RunMarker,
}

/// The directory holding run markers for the given durable state file.
#[must_use]
pub fn run_marker_dir(state_path: &Path) -> PathBuf {
    match state_path.parent() {
        Some(parent) => parent.join(MARKER_DIR),
        None => PathBuf::from(MARKER_DIR),
    }
}

/// Write (or replace) the marker owned by `marker`'s pid, returning its path.
///
/// # Errors
///
/// Returns the underlying I/O error when the directory cannot be created or
/// the marker cannot be written and put in place.
pub fn write_marker(dir: &Path, marker: &RunMarker) -> std::io::Result<PathBuf> {
    fs::create_dir_all(dir)?;

    let pid = marker.identity.pid;
    let target = marker_path(dir, pid);
    let temp = dir.join(format!("{MARKER_PREFIX}{pid}{TEMP_SUFFIX}"));
    let bytes = serde_json::to_vec(marker).map_err(std::io::Error::other)?;

    // A run refreshes its own marker repeatedly; a temp file left behind by a
    // previous refresh that died mid-write must not block this one.
    remove_owned_temp(&temp);
    if let Err(error) = draft(&temp, &bytes) {
        remove_owned_temp(&temp);
        return Err(error);
    }
    if let Err(error) = atomic_replace(&temp, &target) {
        remove_owned_temp(&temp);
        return Err(error);
    }
    sync_parent(dir)?;
    Ok(target)
}

/// Read every readable run marker in `dir`.
///
/// A missing directory reads as no prior runs. A marker file that cannot be
/// interpreted is removed rather than reported on every future start, which is
/// safe because markers are only ever put in place by an atomic replacement.
/// Files that are not run markers are left untouched.
#[must_use]
pub fn read_markers(dir: &Path) -> Vec<StoredMarker> {
    let Ok(entries) = fs::read_dir(dir) else {
        return Vec::new();
    };

    let mut found = Vec::new();
    for entry in entries.flatten() {
        let path = entry.path();
        if !is_marker_file(&path) {
            continue;
        }
        let Ok(bytes) = fs::read(&path) else {
            continue;
        };
        let Ok(marker) = serde_json::from_slice::<RunMarker>(&bytes) else {
            let _ = fs::remove_file(&path);
            continue;
        };
        found.push(StoredMarker { path, marker });
    }
    found
}

/// Retire the marker owned by `pid`, ignoring one that is already gone.
pub fn remove_marker(dir: &Path, pid: u32) {
    let _ = fs::remove_file(marker_path(dir, pid));
}

fn marker_path(dir: &Path, pid: u32) -> PathBuf {
    dir.join(format!("{MARKER_PREFIX}{pid}{MARKER_SUFFIX}"))
}

fn is_marker_file(path: &Path) -> bool {
    path.file_name()
        .and_then(std::ffi::OsStr::to_str)
        .is_some_and(|name| name.starts_with(MARKER_PREFIX) && name.ends_with(MARKER_SUFFIX))
}

fn draft(temp: &Path, bytes: &[u8]) -> std::io::Result<()> {
    let mut file = open_user_only(temp)?;
    file.write_all(bytes)?;
    file.sync_all()
}

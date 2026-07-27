//! Read-only persistence probe for `jefe doctor` (issue #264, AC-08 / D-05).
//!
//! [`probe_persistence`] determines whether a candidate config directory is
//! usable for persistence *without initializing it*: a missing directory is
//! reported [`PersistenceProbeOutcome::Absent`] and never created; an existing
//! writable directory is reported [`PersistenceProbeOutcome::Writable`] and the
//! transient writability probe is cleaned up before return. Existing
//! `settings.toml` / `state.json` contents are never read or modified.
//!
//! This deliberately does **not** reuse `persistence::validate_config_dir`,
//! which creates a missing directory — `doctor` must be read-only.

use std::path::Path;

/// The outcome of probing a candidate persistence directory.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum PersistenceProbeOutcome {
    /// The directory does not exist (and was not created).
    Absent,
    /// The directory exists and is writable.
    Writable,
    /// The directory exists but is not writable.
    NotWritable,
}

/// Probe a candidate config directory for persistence readiness.
///
/// - Missing directory → [`PersistenceProbeOutcome::Absent`] (never created).
/// - Existing directory → writes and removes a uniquely named transient probe
///   file, reporting `Writable` on success or `NotWritable` on failure.
/// - A path that exists but is not a directory is reported `NotWritable`.
///
/// # Errors
///
/// Returns an error only when the filesystem itself cannot be queried
/// (e.g. a permission-denied metadata read). A directory that exists but is
/// inaccessible must not be reported `Absent`, since that would mislead the
/// user into believing initialization will succeed; such a stat failure is
/// surfaced as `Err` instead. Normal writability failures map to
/// [`PersistenceProbeOutcome::NotWritable`], not `Err`.
pub fn probe_persistence(dir: &Path) -> Result<PersistenceProbeOutcome, std::io::Error> {
    // Use `metadata` rather than the bool `exists`/`is_dir` helpers so a stat
    // failure (e.g. permission denied) is distinguishable from a genuinely
    // missing path. `exists` swallows all errors into `false`, which would
    // report an unreadable directory as `Absent`.
    match std::fs::metadata(dir) {
        Ok(metadata) if metadata.is_dir() => {}
        Ok(_) => return Ok(PersistenceProbeOutcome::NotWritable),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            return Ok(PersistenceProbeOutcome::Absent);
        }
        Err(error) => return Err(error),
    }
    match write_transient_probe(dir) {
        Ok(()) => Ok(PersistenceProbeOutcome::Writable),
        Err(error) => {
            // Any error from the transient probe (including cleanup failure) is
            // treated as NotWritable to avoid false-positive writability claims.
            let _ = error;
            Ok(PersistenceProbeOutcome::NotWritable)
        }
    }
}

/// Write and remove a uniquely-named transient probe file under `dir`.
///
/// Uses `create_new(true)` and a process + counter suffix so it never
/// truncates or overwrites an existing file, and only ever removes the exact
/// path it created.
fn write_transient_probe(dir: &Path) -> Result<(), std::io::Error> {
    use std::io::Write;
    let probe_path = dir.join(unique_probe_name());
    let mut file = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&probe_path)?;
    if let Err(error) = file.write_all(b"jefe doctor probe") {
        let _ = std::fs::remove_file(&probe_path);
        return Err(error);
    }
    // Best-effort sync; an unsupported fsync is not a writability failure.
    let _ = file.sync_all();
    drop(file);
    std::fs::remove_file(&probe_path)
}

/// Build a process- and counter-unique transient probe filename.
fn unique_probe_name() -> String {
    use std::sync::atomic::{AtomicU64, Ordering};
    static COUNTER: AtomicU64 = AtomicU64::new(0);
    let n = COUNTER.fetch_add(1, Ordering::Relaxed);
    format!(".jefe-doctor-probe-{}-{}", std::process::id(), n)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn unwrap_tempdir(
        result: std::io::Result<tempfile::TempDir>,
        context: &str,
    ) -> tempfile::TempDir {
        match result {
            Ok(dir) => dir,
            Err(error) => panic!("{context}: {error}"),
        }
    }

    fn unwrap_probe(
        result: std::io::Result<PersistenceProbeOutcome>,
        context: &str,
    ) -> PersistenceProbeOutcome {
        match result {
            Ok(outcome) => outcome,
            Err(error) => panic!("{context}: {error}"),
        }
    }

    #[test]
    fn missing_directory_is_absent_and_not_created() {
        let parent = unwrap_tempdir(tempfile::tempdir(), "create parent tempdir");
        let missing = parent.path().join("does-not-exist");
        let outcome = unwrap_probe(probe_persistence(&missing), "probe missing dir");
        assert_eq!(outcome, PersistenceProbeOutcome::Absent);
    }

    #[test]
    fn existing_writable_directory_is_writable() {
        let dir = unwrap_tempdir(tempfile::tempdir(), "create writable tempdir");
        let outcome = unwrap_probe(probe_persistence(dir.path()), "probe writable dir");
        assert_eq!(outcome, PersistenceProbeOutcome::Writable);
    }

    #[test]
    fn regular_file_path_is_not_writable() {
        let dir = unwrap_tempdir(tempfile::tempdir(), "create tempdir");
        let file_path = dir.path().join("not-a-dir");
        if let Err(error) = std::fs::write(&file_path, b"x") {
            panic!("seed regular file: {error}");
        }
        let outcome = unwrap_probe(probe_persistence(&file_path), "probe regular file");
        assert_eq!(outcome, PersistenceProbeOutcome::NotWritable);
    }
}

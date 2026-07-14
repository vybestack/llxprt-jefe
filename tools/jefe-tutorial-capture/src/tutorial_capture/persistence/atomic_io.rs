//! Atomic file writes: temp-file + rename with fsync durability.
//!
//! @requirement REQ-TUTORIAL-CAPTURE-007

use std::fs;
use std::io::Write;
use std::path::Path;

use super::error::PersistenceError;

/// Write arbitrary text atomically via temp-file + rename.
///
/// Improvements over a naive temp+rename:
/// - **Unique temp name**: includes PID + timestamp + counter so concurrent
///   writes to the same target don't collide on the temp file.
/// - **Exclusive temp creation**: the unpredictable temp name is opened with
///   `create_new`, refusing any pre-existing filesystem entry.
/// - **fsync both file and parent**: the file is fsync'd before rename, and
///   the parent directory is fsync'd after rename, so a crash never leaves a
///   directory entry without its data on disk.
pub fn atomic_write(target: &Path, content: &str) -> Result<(), PersistenceError> {
    let parent = target.parent().unwrap_or_else(|| Path::new("."));
    fs::create_dir_all(parent).map_err(|e| io_error_path(parent, e))?;
    let temp_name = unique_temp_name(target);
    let temp_path = parent.join(&temp_name);
    // Use create_new so if a symlink or file already exists at the temp path,
    // we get an error instead of following a symlink.
    let result = atomic_write_inner(&temp_path, target, content);
    // Cleanup only a regular temp entry; never unlink a replacement symlink.
    if result.is_err() && fs::symlink_metadata(&temp_path).is_ok_and(|metadata| metadata.is_file())
    {
        let _ = fs::remove_file(&temp_path);
    }
    result
}

/// Inner write logic, separated so the outer function can clean up the temp
/// file on failure.
fn atomic_write_inner(
    temp_path: &Path,
    target: &Path,
    content: &str,
) -> Result<(), PersistenceError> {
    let mut file = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(temp_path)
        .map_err(|e| io_error_path(temp_path, e))?;
    file.write_all(content.as_bytes())
        .map_err(|e| io_error_path(temp_path, e))?;
    file.sync_all().map_err(|e| io_error_path(temp_path, e))?;
    drop(file);
    fs::rename(temp_path, target).map_err(|e| io_error_path(target, e))?;
    // fsync the parent directory so the rename is durable. On Unix,
    // opening a directory and calling sync_all fsyncs the directory fd.
    // On filesystems that don't support this (e.g. tmpfs), we ignore
    // EINVAL silently.
    let parent = target.parent().unwrap_or_else(|| Path::new("."));
    fsync_dir(parent)
}

/// Generate a unique temporary file name for atomic writes.
///
/// Includes the PID, a timestamp, and a per-process counter so that concurrent
/// writes from the same process don't collide.
fn unique_temp_name(target: &Path) -> String {
    use std::sync::atomic::{AtomicU64, Ordering};
    static COUNTER: AtomicU64 = AtomicU64::new(0);
    let seq = COUNTER.fetch_add(1, Ordering::Relaxed);
    let pid = std::process::id();
    let time = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    let base = target
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("manifest");
    format!(".{base}.{pid}.{time}.{seq}.tmp")
}

/// fsync a directory file descriptor for durability of rename operations.
#[cfg(unix)]
fn fsync_dir(path: &Path) -> Result<(), PersistenceError> {
    let dir = fs::File::open(path).map_err(|err| io_error_path(path, err))?;
    match dir.sync_all() {
        Ok(()) => Ok(()),
        Err(err) if is_unsupported_directory_sync(&err) => Ok(()),
        Err(err) => Err(io_error_path(path, err)),
    }
}

#[cfg(unix)]
fn is_unsupported_directory_sync(err: &std::io::Error) -> bool {
    matches!(
        err.kind(),
        std::io::ErrorKind::InvalidInput | std::io::ErrorKind::Unsupported
    )
}

/// On non-Unix, directory sync is unavailable.
#[cfg(not(unix))]
fn fsync_dir(_path: &Path) -> Result<(), PersistenceError> {
    Ok(())
}

/// Build a `PersistenceError::Io` from a path and an `io::Error`.
pub fn io_error_path(path: &Path, e: std::io::Error) -> PersistenceError {
    PersistenceError::Io {
        path: path.to_string_lossy().into_owned(),
        reason: e.to_string(),
    }
}

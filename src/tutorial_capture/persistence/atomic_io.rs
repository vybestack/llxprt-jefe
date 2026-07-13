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
/// - **No symlink follow**: the temp file is created with `OpenOptions` that
///   reject existing symlinks (`create_new`).
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
    let mut file = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&temp_path)
        .map_err(|e| io_error_path(&temp_path, e))?;
    file.write_all(content.as_bytes())
        .map_err(|e| io_error_path(&temp_path, e))?;
    file.sync_all().map_err(|e| io_error_path(&temp_path, e))?;
    drop(file);
    fs::rename(&temp_path, target).map_err(|e| io_error_path(target, e))?;
    // fsync the parent directory so the rename is durable. On Unix,
    // opening a directory and calling sync_all fsyncs the directory fd.
    // On filesystems that don't support this (e.g. tmpfs), we ignore
    // EINVAL silently.
    fsync_dir(parent);
    Ok(())
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
/// On Unix, `File::sync_all` on a directory fd calls fsync. Errors are
/// silently ignored because some filesystems (e.g. tmpfs) don't support it,
/// and the data is still in page cache.
#[cfg(unix)]
fn fsync_dir(path: &Path) {
    // Opening a directory with File::open is valid on Unix and sync_all
    // maps to fsync(2) on the fd. Errors are intentionally ignored: a
    // failed directory fsync should not abort the manifest write.
    if let Ok(dir) = fs::File::open(path) {
        let _ = dir.sync_all();
    }
}

/// On non-Unix, fsync_dir is a no-op.
#[cfg(not(unix))]
fn fsync_dir(_path: &Path) {}

/// Build a `PersistenceError::Io` from a path and an `io::Error`.
pub fn io_error_path(path: &Path, e: std::io::Error) -> PersistenceError {
    PersistenceError::Io {
        path: path.to_string_lossy().into_owned(),
        reason: e.to_string(),
    }
}

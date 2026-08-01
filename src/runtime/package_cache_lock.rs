//! Cross-process mutual exclusion and atomic install swap for the managed
//! package cache (issue #556).
//!
//! On Unix the managed npm install must serialize across jefe processes and
//! appear atomically at its final path. This module owns two dependency-free,
//! std-only primitives:
//!
//! - [`InstallLock`]: a per-digest lock acquired by atomically hard-linking a
//!   pre-stamped temp file into the lock path. `hard_link` is exclusive (it
//!   fails if the target exists) and content-preserving, so a lock file either
//!   does not exist or already carries the holder's identity — there is no
//!   empty-lock window from a crashed create-before-write. The stamp records
//!   the holder's [`ProcessIdentity`] (pid + start discriminator) so a lock
//!   left behind by a dead holder is recovered immediately from the project's
//!   canonical liveness classifier (no fixed timeout, and a recycled pid whose
//!   start discriminator differs is reclaimed too).
//! - [`atomic_swap_into_place`]: build a complete tree in a sibling temp dir,
//!   then rename it into the final path so a reader never observes a partial
//!   tree.
//!
//! Windows locking behavior is intentionally unchanged here (separate Windows
//! sub-issue); this module is exercised on Unix.

#![cfg(unix)]

use std::io::Write;
use std::path::{Path, PathBuf};

use tempfile::NamedTempFile;

use crate::domain::ProcessIdentity;

use super::package_runtime::PackageRuntimeError;

/// Maximum characters retained from a filesystem diagnostic so lock and rename
/// errors stay bounded (issue #556 A5).
const DIAGNOSTIC_BOUND: usize = 256;

/// Two lines: the holder pid, then the holder's process start discriminator
/// (the platform `started_at` from [`ProcessIdentity`]). Recording the start
/// discriminator — not a wall-clock epoch — lets recovery reject a recycled pid
/// whose `started_at` no longer matches (issue #556 A4).
fn holder_identity_line(identity: &ProcessIdentity) -> String {
    match identity.started_at {
        Some(started) => format!("{}\n{started}\n", identity.pid),
        None => format!("{}\n\n", identity.pid),
    }
}

/// Capture the current process identity for stamping into a freshly held lock.
/// A probe that cannot read the start discriminator still records the pid; the
/// holder is then classified only by liveness (fail-open, never a false stale
/// claim against a live install).
fn current_identity() -> ProcessIdentity {
    let pid = std::process::id();
    super::process::capture_process_identity(pid).unwrap_or(ProcessIdentity {
        pid,
        started_at: None,
    })
}

/// Parsed holder identity (pid + start discriminator) from a lock file, when
/// both lines are present and the pid is numeric.
fn read_holder_identity(lock_path: &Path) -> Option<ProcessIdentity> {
    let content = std::fs::read_to_string(lock_path).ok()?;
    let mut lines = content.lines();
    let pid = lines.next()?.parse::<u32>().ok()?;
    let started_at = lines
        .next()
        .filter(|line| !line.is_empty())
        .and_then(|line| line.parse::<u64>().ok());
    Some(ProcessIdentity { pid, started_at })
}

/// Whether the recorded holder is gone, using the project's canonical process
/// liveness classifier. A dead holder OR a recycled pid (different start
/// discriminator) is "gone" and may be reclaimed; inaccessible and probe-failure
/// outcomes fail open so a live install is never falsely declared stale
/// (issue #556 A4).
fn holder_is_gone(identity: ProcessIdentity) -> bool {
    let liveness = super::process::process_liveness(Some(identity));
    !super::process::process_liveness_indicates_alive(liveness)
}

fn bounded(detail: &str) -> String {
    detail.chars().take(DIAGNOSTIC_BOUND).collect()
}

/// A held per-digest install lock. Dropping it releases the lock.
pub struct InstallLock {
    path: PathBuf,
    pid: u32,
}

/// The outcome of one attempt to claim a lock.
pub enum AcquireOutcome {
    /// This caller now holds the lock.
    Acquired(InstallLock),
    /// A live process holds the lock; the caller should wait and retry.
    Contended,
}

/// Try once to acquire `lock_path`. A stale lock whose recorded holder is gone
/// is reclaimed inline before a single retry. Returns [`AcquireOutcome::Contended`]
/// when a live process holds the lock or the holder identity is unreadable.
pub fn try_acquire(lock_path: &Path) -> Result<AcquireOutcome, PackageRuntimeError> {
    match link_exclusive(lock_path)? {
        Some(lock) => Ok(AcquireOutcome::Acquired(lock)),
        None => contend_or_recover(lock_path),
    }
}

/// Atomically claim an exclusive lock by hard-linking a pre-stamped temp file
/// into the lock path. `hard_link` is exclusive (it fails if the target exists)
/// and content-preserving, so the lock file either does not exist or already
/// carries the holder's identity — there is never an empty-lock window from a
/// crashed create-before-write sequence.
fn link_exclusive(lock_path: &Path) -> Result<Option<InstallLock>, PackageRuntimeError> {
    let pid = std::process::id();
    let Some(parent) = lock_path.parent() else {
        return Err(PackageRuntimeError::CacheLock(bounded(
            "lock path has no parent directory",
        )));
    };
    let mut temp = NamedTempFile::new_in(parent)
        .map_err(|error| PackageRuntimeError::CacheLock(bounded(&error.to_string())))?;
    let identity = current_identity();
    temp.write_all(holder_identity_line(&identity).as_bytes())
        .map_err(|error| PackageRuntimeError::CacheLock(bounded(&error.to_string())))?;
    match std::fs::hard_link(temp.path(), lock_path) {
        // The lock now shares the temp inode; dropping `temp` removes only the
        // temp name, leaving the lock in place.
        Ok(()) => Ok(Some(InstallLock {
            path: lock_path.to_path_buf(),
            pid,
        })),
        Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => Ok(None),
        Err(error) => Err(PackageRuntimeError::CacheLock(bounded(&error.to_string()))),
    }
}

fn contend_or_recover(lock_path: &Path) -> Result<AcquireOutcome, PackageRuntimeError> {
    let Some(identity) = read_holder_identity(lock_path) else {
        return Ok(AcquireOutcome::Contended);
    };
    if !holder_is_gone(identity) {
        return Ok(AcquireOutcome::Contended);
    }
    recover_stale(lock_path)?;
    match link_exclusive(lock_path)? {
        Some(lock) => Ok(AcquireOutcome::Acquired(lock)),
        None => Ok(AcquireOutcome::Contended),
    }
}

/// Remove a known-stale lock file (dead holder). A concurrent remover is fine.
fn recover_stale(lock_path: &Path) -> Result<(), PackageRuntimeError> {
    match std::fs::remove_file(lock_path) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(PackageRuntimeError::CacheLock(bounded(&error.to_string()))),
    }
}

impl Drop for InstallLock {
    fn drop(&mut self) {
        // Conditional release: only unlink the lock if it still names this
        // holder, so a successor that already reclaimed it is not clobbered.
        if read_holder_identity(&self.path).is_some_and(|identity| identity.pid == self.pid) {
            let _ = std::fs::remove_file(&self.path);
        }
    }
}

/// Rename a completed `built` tree into `final_dir`.
///
/// Uses a move-aside swap: any existing final tree is renamed to a backup
/// first, then the built tree is renamed into place. If the second rename fails
/// the backup is restored, so a previously valid install is never lost and a
/// reader never observes a missing tree that cannot be rebuilt (issue #556 A2).
/// POSIX `rename` cannot replace a non-empty directory in one step, so the old
/// tree must be moved aside rather than renamed over directly.
pub fn atomic_swap_into_place(built: &Path, final_dir: &Path) -> Result<(), PackageRuntimeError> {
    let backup = swap_backup_path(final_dir);
    // Recover a half-finished previous swap before moving the current tree: a
    // crashed swap may have parked the last valid tree in the backup with no
    // final tree to show for it. Restore it so a reader never loses a good
    // cache to a crash (issue #556 A2). The caller holds the install lock, so
    // the backup is not contended.
    recover_crashed_swap(final_dir, &backup)?;
    let had_existing = match std::fs::rename(final_dir, &backup) {
        Ok(()) => true,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => false,
        Err(error) => {
            return Err(PackageRuntimeError::InstallRename(bounded(
                &error.to_string(),
            )));
        }
    };
    if let Err(error) = std::fs::rename(built, final_dir) {
        if had_existing {
            let _ = std::fs::rename(&backup, final_dir);
        }
        return Err(PackageRuntimeError::InstallRename(bounded(
            &error.to_string(),
        )));
    }
    if had_existing {
        let _ = std::fs::remove_dir_all(&backup);
    }
    Ok(())
}

/// Reconcile a leftover swap backup: restore it if the previous swap crashed
/// after parking the valid tree (final missing), or discard it if the final
/// tree is already present (stale leftover from a completed swap).
fn recover_crashed_swap(final_dir: &Path, backup: &Path) -> Result<(), PackageRuntimeError> {
    if !backup.exists() {
        return Ok(());
    }
    if final_dir.exists() {
        let _ = std::fs::remove_dir_all(backup);
        return Ok(());
    }
    std::fs::rename(backup, final_dir)
        .map_err(|error| PackageRuntimeError::InstallRename(bounded(&error.to_string())))
}

/// Sibling path used to park the previous final tree during a swap.
fn swap_backup_path(final_dir: &Path) -> PathBuf {
    let Some(name) = final_dir.file_name() else {
        return final_dir
            .parent()
            .unwrap_or_else(|| Path::new("."))
            .join("install.jefe-swap-old");
    };
    let mut backup = name.to_os_string();
    backup.push(".jefe-swap-old");
    final_dir.with_file_name(backup)
}

#[cfg(test)]
mod tests {
    include!("package_cache_lock_tests.rs");
}

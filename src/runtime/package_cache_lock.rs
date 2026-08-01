//! Cross-process mutual exclusion and atomic install swap for the managed
//! package cache (issue #556).
//!
//! On Unix the managed npm install must serialize across jefe processes and
//! appear atomically at its final path. This module owns two dependency-free,
//! std-only primitives:
//!
//! - [`InstallLock`]: a per-digest lock acquired via `OpenOptions::create_new`
//!   (`O_EXCL`), which is atomic, exclusive, and effective both across
//!   processes and across threads. A lock left behind by a dead holder is
//!   recovered immediately from the recorded pid (no fixed timeout).
//! - [`atomic_swap_into_place`]: build a complete tree in a sibling temp dir,
//!   then rename it into the final path so a reader never observes a partial
//!   tree.
//!
//! Windows locking behavior is intentionally unchanged here (separate Windows
//! sub-issue); this module is exercised on Unix.

#![cfg(unix)]

use std::fs::{File, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::time::{SystemTime, UNIX_EPOCH};

use super::package_runtime::PackageRuntimeError;

/// Maximum characters retained from a filesystem diagnostic so lock and rename
/// errors stay bounded (issue #556 A5).
const DIAGNOSTIC_BOUND: usize = 256;

/// Two lines: the holder pid, then the install-start epoch (seconds).
fn holder_line(pid: u32) -> String {
    let epoch = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |duration| duration.as_secs());
    format!("{pid}\n{epoch}\n")
}

/// First line of a lock file is the holder pid, if it is readable and numeric.
fn read_holder_pid(lock_path: &Path) -> Option<u32> {
    let content = std::fs::read_to_string(lock_path).ok()?;
    content.lines().next()?.parse::<u32>().ok()
}

/// Whether a process is currently alive, via the portable `kill -0` built-in.
///
/// `kill -0 <pid>` exits successfully exactly when the pid exists and is
/// signalable by this user. All jefe processes run as the same user, so a
/// same-user live install is never misreported as dead. No external crate is
/// required.
fn pid_alive(pid: u32) -> bool {
    let status = std::process::Command::new("kill")
        .arg("-0")
        .arg(pid.to_string())
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status();
    status.is_ok_and(|outcome| outcome.success())
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

/// Try once to acquire `lock_path`. A stale lock whose recorded pid is dead is
/// reclaimed inline before a single retry. Returns [`AcquireOutcome::Contended`]
/// when a live process holds the lock or the holder cannot be read.
pub fn try_acquire(lock_path: &Path) -> Result<AcquireOutcome, PackageRuntimeError> {
    if let Some(file) = open_exclusive(lock_path) {
        return claim(lock_path, file);
    }
    contend_or_recover(lock_path)
}

fn open_exclusive(lock_path: &Path) -> Option<File> {
    OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(lock_path)
        .ok()
}

/// Stamp the freshly created lock with this holder's pid. A lock that cannot be
/// stamped is unusable — no pid means it can never be recovered — so the empty
/// file is removed and the failure surfaced (issue #556 A4 stale recovery).
fn claim(lock_path: &Path, mut file: File) -> Result<AcquireOutcome, PackageRuntimeError> {
    let pid = std::process::id();
    if let Err(error) = file.write_all(holder_line(pid).as_bytes()) {
        let _ = std::fs::remove_file(lock_path);
        return Err(PackageRuntimeError::CacheLock(bounded(&error.to_string())));
    }
    Ok(AcquireOutcome::Acquired(InstallLock {
        path: lock_path.to_path_buf(),
        pid,
    }))
}

fn contend_or_recover(lock_path: &Path) -> Result<AcquireOutcome, PackageRuntimeError> {
    let Some(pid) = read_holder_pid(lock_path) else {
        return Ok(AcquireOutcome::Contended);
    };
    if pid_alive(pid) {
        return Ok(AcquireOutcome::Contended);
    }
    recover_stale(lock_path)?;
    match open_exclusive(lock_path) {
        Some(file) => claim(lock_path, file),
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
        if read_holder_pid(&self.path) == Some(self.pid) {
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
    // A leftover backup from a crashed swap is safe to clear (the caller holds
    // the install lock, so no other installer is mid-swap).
    let _ = std::fs::remove_dir_all(&backup);
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

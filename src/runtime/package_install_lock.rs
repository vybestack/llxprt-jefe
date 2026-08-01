//! Cross-process advisory lock for the managed package install cache.
//!
//! The managed install directory `<cache_root>/<selector-digest>/` is a
//! **per-machine** resource, so the per-digest mutex in
//! [`super::package_runtime`] cannot serialize it. Without this lock, two jefe
//! processes that miss the same digest run `npm install` concurrently into one
//! directory — issue #425 Problem B reproduced inside jefe's own cache.
//!
//! # Protocol
//!
//! The lock is a kernel advisory lock (`flock` on Unix, `LockFileEx` on
//! Windows) taken through [`std::fs::File::try_lock`] on
//! `<cache_root>/<digest>.lock`, a **sibling** of the install directory rather
//! than a file inside it: the install directory is replaced wholesale by
//! `rename`, which would carry away a lock held inside it.
//!
//! Because the lock lives in the kernel and is attached to the open file
//! description, the operating system releases it when the holder exits for any
//! reason, including `SIGKILL` and a power loss. There is consequently no
//! stale-lock state to detect and no lock ownership recorded on disk:
//!
//! - a legitimate install is never declared stale no matter how long it runs,
//!   which is precisely the mistake npm's five-second stale threshold made;
//! - a crashed holder needs no recovery, so there is no window in which two
//!   processes could each decide to recover the same lock;
//! - the lock file is never unlinked, so no process can remove a lock that
//!   another process is holding.
//!
//! Waiting is bounded by the install timeout plus a grace margin, after which
//! acquisition fails closed with a typed error rather than proceeding
//! unlocked. The lock file itself is a zero-byte sentinel; it is created if
//! absent and never truncated, because another process may already hold it.
//!
//! # Scope
//!
//! This serializes processes on one machine against one local cache directory,
//! which is what `dirs::cache_dir()` provides. Advisory locking over a network
//! filesystem is emulated at best; jefe does not support a shared network
//! package cache.

use std::fs::{File, OpenOptions, TryLockError};
use std::path::Path;
use std::time::{Duration, Instant};

use crate::domain::agent_definition::limits::PACKAGE_MATERIALIZATION_TIMEOUT_MS;

use super::package_runtime::PackageRuntimeError;

/// Margin added to the install timeout to derive the wait ceiling.
const LOCK_WAIT_GRACE: Duration = Duration::from_secs(30);

/// Upper bound on any diagnostic detail carried by a lock failure.
pub(super) const MAX_DETAIL_CHARS: usize = 256;

/// Leading digest characters included in diagnostics for correlation.
const DIGEST_DIAGNOSTIC_CHARS: usize = 12;

/// Timing envelope for one lock acquisition.
///
/// [`Self::production`] is the only configuration used by jefe; the fields
/// exist so tests can exercise the waiting path without sleeping for the
/// production ceiling.
#[derive(Debug, Clone, Copy)]
pub(super) struct LockPolicy {
    /// Longest total time spent waiting before failing closed.
    pub(super) ceiling: Duration,
    /// Delay between acquisition attempts.
    pub(super) poll_interval: Duration,
}

impl LockPolicy {
    /// Production timings, with the ceiling derived from the install timeout.
    pub(super) const fn production() -> Self {
        Self {
            ceiling: Duration::from_millis(PACKAGE_MATERIALIZATION_TIMEOUT_MS)
                .saturating_add(LOCK_WAIT_GRACE),
            poll_interval: Duration::from_millis(50),
        }
    }
}

/// Held managed-install lock.
///
/// The kernel releases the lock when the file is closed, which happens when
/// this guard is dropped and equally when the process dies.
#[derive(Debug)]
pub(super) struct InstallLockGuard {
    file: File,
}

impl Drop for InstallLockGuard {
    fn drop(&mut self) {
        if let Err(error) = self.file.unlock() {
            // Closing the file releases the lock regardless, so this is
            // reported rather than escalated.
            tracing::warn!(
                kind = ?error.kind(),
                "could not release the managed package install lock"
            );
        }
    }
}

/// Acquire the cross-process managed-install lock for one selector digest.
///
/// Blocks while another process holds the lock and fails closed once
/// `policy.ceiling` elapses.
///
/// # Errors
/// [`PackageRuntimeError::InstallLockUnavailable`] when the lock file cannot
/// be opened, when the platform reports a locking error, or when the lock is
/// still held at the ceiling.
pub(super) fn acquire(
    lock_path: &Path,
    digest: &str,
    policy: LockPolicy,
) -> Result<InstallLockGuard, PackageRuntimeError> {
    // Never truncate: another process may be holding this exact file.
    let file = OpenOptions::new()
        .create(true)
        .write(true)
        .truncate(false)
        .open(lock_path)
        .map_err(|error| {
            unavailable(
                digest,
                &format!("cannot open lock file ({:?})", error.kind()),
            )
        })?;
    let started = Instant::now();
    loop {
        match file.try_lock() {
            Ok(()) => return Ok(InstallLockGuard { file }),
            Err(TryLockError::WouldBlock) => {}
            Err(TryLockError::Error(error)) => {
                return Err(unavailable(
                    digest,
                    &format!("platform lock failed ({:?})", error.kind()),
                ));
            }
        }
        if started.elapsed() >= policy.ceiling {
            return Err(unavailable(
                digest,
                &format!(
                    "still held by another process after {}s",
                    policy.ceiling.as_secs()
                ),
            ));
        }
        std::thread::sleep(policy.poll_interval);
    }
}

fn unavailable(digest: &str, message: &str) -> PackageRuntimeError {
    PackageRuntimeError::InstallLockUnavailable(bounded_detail(digest, message))
}

/// Bounded, redacted diagnostic detail.
///
/// Carries the leading digest characters for correlation and never an absolute
/// cache path, which would embed the user's home directory and account name.
pub(super) fn bounded_detail(digest: &str, message: &str) -> String {
    let short: String = digest.chars().take(DIGEST_DIAGNOSTIC_CHARS).collect();
    format!("digest={short}: {message}")
        .chars()
        .take(MAX_DETAIL_CHARS)
        .collect()
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;

    include!("package_install_lock_tests.rs");
}

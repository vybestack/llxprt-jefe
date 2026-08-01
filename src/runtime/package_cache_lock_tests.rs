use std::path::PathBuf;
use std::process::Command;
use std::time::Duration;

use tempfile::TempDir;

use super::{AcquireOutcome, atomic_swap_into_place, try_acquire};

fn lock_path(dir: &TempDir) -> PathBuf {
    dir.path().join("digest.lock")
}

/// Spawn a child that stays alive until killed, returning its pid and a handle
/// that reaps it on drop.
struct LiveChild(Option<std::process::Child>);

impl LiveChild {
    fn sleeping() -> Self {
        let child = Command::new("sleep")
            .arg("30")
            .spawn()
            .unwrap_or_else(|error| panic!("spawn sleep: {error}"));
        Self(Some(child))
    }

    fn pid(&self) -> u32 {
        self.0
            .as_ref()
            .map_or_else(|| panic!("live child missing"), std::process::Child::id)
    }
}

impl Drop for LiveChild {
    fn drop(&mut self) {
        if let Some(child) = self.0.as_mut() {
            let _ = child.kill();
            let _ = child.wait();
        }
    }
}

/// A pid guaranteed to be dead: spawn a child, let it exit, and reap it so the
/// pid is no longer present.
fn dead_pid() -> u32 {
    let mut child = Command::new("true")
        .spawn()
        .unwrap_or_else(|error| panic!("spawn true: {error}"));
    let pid = child.id();
    child
        .wait()
        .unwrap_or_else(|error| panic!("wait true: {error}"));
    pid
}

fn plant_lock(dir: &TempDir, pid: u32) {
    // Stamp the holder's real identity (pid + start discriminator) so the
    // liveness classifier sees a live holder as alive and a dead/reused one as
    // gone — matching how a real held lock is stamped by `link_exclusive`.
    let path = lock_path(dir);
    let identity = crate::runtime::process::capture_process_identity(pid)
        .unwrap_or(crate::domain::ProcessIdentity { pid, started_at: None });
    let started_line =
        identity.started_at.map_or_else(String::new, |started| started.to_string());
    let content = format!("{}\n{started_line}\n", identity.pid);
    std::fs::write(&path, content)
        .unwrap_or_else(|error| panic!("plant lock: {error}"));
}

fn is_acquired(outcome: &AcquireOutcome) -> bool {
    matches!(outcome, AcquireOutcome::Acquired(_))
}

/// A4: a stale lock whose recorded pid is gone is recovered immediately, with
/// no timeout wait.
#[test]
fn stale_lock_with_dead_pid_is_recovered() {
    let dir = tempfile::tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
    plant_lock(&dir, dead_pid());

    let started = std::time::Instant::now();
    let outcome = try_acquire(&lock_path(&dir))
        .unwrap_or_else(|error| panic!("try_acquire: {error}"));
    let elapsed = started.elapsed();

    assert!(
        is_acquired(&outcome),
        "a dead-holder lock must be reclaimed"
    );
    assert!(
        elapsed < Duration::from_secs(2),
        "recovery is immediate, not timer-bound: {elapsed:?}"
    );
}

/// A4: a lock held by a live process is never declared stale.
#[test]
fn live_lock_is_never_declared_stale() {
    let dir = tempfile::tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
    let live = LiveChild::sleeping();
    plant_lock(&dir, live.pid());

    let outcome = try_acquire(&lock_path(&dir))
        .unwrap_or_else(|error| panic!("try_acquire: {error}"));

    assert!(
        matches!(outcome, AcquireOutcome::Contended),
        "a live-holder lock must not be reclaimed"
    );
    // The live child is reaped by Drop.
    drop(live);
}

/// Within one process a second claim without releasing the first is contended,
/// proving the `create_new` lock is exclusive (cross-thread as well as
/// cross-process).
#[test]
fn acquire_is_exclusive_until_released() {
    let dir = tempfile::tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
    let path = lock_path(&dir);

    let first = try_acquire(&path).unwrap_or_else(|error| panic!("first: {error}"));
    assert!(is_acquired(&first), "first acquire succeeds");
    let second = try_acquire(&path).unwrap_or_else(|error| panic!("second: {error}"));
    assert!(
        matches!(second, AcquireOutcome::Contended),
        "a held lock is contended"
    );

    drop(first);
    assert!(
        !path.exists(),
        "release removes the lock file so a successor can claim it"
    );
    let third = try_acquire(&path).unwrap_or_else(|error| panic!("third: {error}"));
    assert!(is_acquired(&third), "lock is reclaimable after release");
}

/// A5: lock and rename diagnostics are distinct, typed, and bounded.
#[test]
fn lock_and_rename_failures_are_typed_and_bounded() {
    use crate::runtime::package_runtime::PackageRuntimeError;

    // A rename onto a final path whose parent does not exist fails with a
    // typed, bounded InstallRename error.
    let dir = tempfile::tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
    let built = dir.path().join("built");
    std::fs::create_dir_all(&built).unwrap_or_else(|error| panic!("mkdir built: {error}"));
    let unreachable_final = dir.path().join("missing-parent").join("final");
    let rename_error = atomic_swap_into_place(&built, &unreachable_final)
        .err()
        .unwrap_or_else(|| panic!("expected an InstallRename error"));
    assert!(
        matches!(rename_error, PackageRuntimeError::InstallRename(_)),
        "rename failure is a distinct InstallRename variant: {rename_error:?}"
    );

    // Removing a directory as a file fails (not NotFound) with a typed,
    // bounded CacheLock error.
    let lock_is_a_dir = dir.path().join("i-am-a-directory.lock");
    std::fs::create_dir_all(&lock_is_a_dir)
        .unwrap_or_else(|error| panic!("mkdir lock dir: {error}"));
    let lock_error = super::recover_stale(&lock_is_a_dir)
        .err()
        .unwrap_or_else(|| panic!("expected a CacheLock error"));
    assert!(
        matches!(lock_error, PackageRuntimeError::CacheLock(_)),
        "lock failure is a distinct CacheLock variant: {lock_error:?}"
    );
    assert!(
        lock_error.to_string().chars().count() < 2 * super::DIAGNOSTIC_BOUND,
        "lock diagnostic stays bounded: {lock_error}"
    );

    // The bounding helper itself truncates unbounded input (redaction).
    let long = super::bounded(&"x".repeat(2_000));
    assert_eq!(
        long.chars().count(),
        super::DIAGNOSTIC_BOUND,
        "unbounded detail is truncated to the diagnostic bound"
    );
}

/// A2: swapping a built tree into an empty final path installs it complete.
#[test]
fn atomic_swap_installs_a_built_tree_into_place() {
    let dir = tempfile::tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
    let built = dir.path().join("built");
    let final_dir = dir.path().join("final");
    std::fs::create_dir_all(&built).unwrap_or_else(|error| panic!("mkdir built: {error}"));
    std::fs::write(built.join(".jefe-installed"), "complete\n")
        .unwrap_or_else(|error| panic!("marker: {error}"));

    atomic_swap_into_place(&built, &final_dir)
        .unwrap_or_else(|error| panic!("swap: {error}"));

    assert!(final_dir.join(".jefe-installed").exists(), "final tree is complete");
    assert!(!built.exists(), "the built temp dir is gone after rename");
}

/// A2: swapping replaces a stale final directory entirely — no stale content
/// survives, and the new tree is complete.
#[test]
fn atomic_swap_replaces_a_stale_final_directory() {
    let dir = tempfile::tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
    let built = dir.path().join("built");
    let final_dir = dir.path().join("final");
    std::fs::create_dir_all(&built).unwrap_or_else(|error| panic!("mkdir built: {error}"));
    std::fs::write(built.join(".jefe-installed"), "complete\n")
        .unwrap_or_else(|error| panic!("marker: {error}"));
    std::fs::create_dir_all(&final_dir).unwrap_or_else(|error| panic!("mkdir final: {error}"));
    std::fs::write(final_dir.join("stale-artifact"), "old\n")
        .unwrap_or_else(|error| panic!("stale: {error}"));

    atomic_swap_into_place(&built, &final_dir)
        .unwrap_or_else(|error| panic!("swap: {error}"));

    assert!(final_dir.join(".jefe-installed").exists(), "new tree is complete");
    assert!(
        !final_dir.join("stale-artifact").exists(),
        "stale final content is fully replaced"
    );
}

/// A2: a leftover swap backup from a crashed previous swap, with no final tree,
/// is restored before the next swap so a good cache is never lost to a crash.
#[test]
fn crashed_swap_restores_the_previous_tree() {
    let dir = tempfile::tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
    let final_dir = dir.path().join("final");
    let backup = super::swap_backup_path(&final_dir);
    // Simulate a crashed previous swap: the last valid tree is parked in the
    // backup and the final tree is gone.
    std::fs::create_dir_all(&backup).unwrap_or_else(|error| panic!("mkdir backup: {error}"));
    std::fs::write(backup.join(".jefe-installed"), "previous
")
        .unwrap_or_else(|error| panic!("previous marker: {error}"));

    super::recover_crashed_swap(&final_dir, &backup)
        .unwrap_or_else(|error| panic!("recover crashed swap: {error}"));

    assert!(
        final_dir.join(".jefe-installed").exists(),
        "the previous valid tree is restored to the final path"
    );
    assert!(!backup.exists(), "the backup is consumed by the restore");
}

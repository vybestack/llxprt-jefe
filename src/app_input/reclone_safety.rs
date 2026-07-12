//! Safety guards for the destructive force-reclone path (issue #190).
//!
//! When the user confirms an origin mismatch, Jefe removes the existing
//! working copy and re-clones from the configured identity. That removal is
//! the most destructive operation in the issue-send flow, so it is wrapped in
//! two layers of defense:
//!
//! 1. A compile-time proof token (`ConfirmedReclone` in `issue_git_prep`)
//!    guaranteeing the user explicitly confirmed via the modal.
//! 2. The runtime target validation here: [`validate_reclone_target`] rejects
//!    catastrophic targets (empty, filesystem root, top-level directory, or
//!    any symlink on the path) so a misconfigured `work_dir` can never reach
//!    `remove_dir_all` even with confirmation.
//!
//! This module is pure (only `std::fs::symlink_metadata` for the symlink
//! check) so it is exhaustively unit-testable without spawning git.

use std::path::{Path, PathBuf};

/// Validate that `work_dir` is a safe target for a destructive force-reclone.
///
/// Rejects targets that would cause catastrophic data loss if removed:
/// - empty/whitespace-only paths,
/// - the filesystem root (`/` on Unix, `` on Windows),
/// - a bare drive root (`C:`) on Windows,
/// - a path with fewer than two named components (e.g. `/home`, `/tmp`),
///   which is too broad to remove in an automated reclone.
/// - a symlink as the **final component** of `work_dir`: `remove_dir_all`
///   follows the link, so a `work_dir` that is itself a symlink could delete
///   the symlink's target — e.g. a symlink to `/` or `/home` would pass the
///   component-count check yet destroy the target tree. (An ancestor symlink
///   such as macOS `/var` → `/private/var` is harmless: the path resolves to a
///   real directory and only the leaf subtree is removed.)
///
/// This is a defense-in-depth guard: the `ConfirmedReclone` token already
/// proves the user confirmed, but a misconfigured `work_dir` (root, empty,
/// a top-level directory, or a symlink) must never reach `rm -rf` even with
/// confirmation. The check is locale- and platform-independent.
pub fn validate_reclone_target(work_dir: &Path) -> Result<(), String> {
    let lossy = work_dir.to_string_lossy();
    let trimmed = lossy.trim();
    if trimmed.is_empty() {
        return Err("Refusing to force-reclone: work_dir is empty.".to_owned());
    }
    // Reject a symlink as the final component: `remove_dir_all` follows the
    // link, so a symlinked work_dir could delete the link's target tree. We
    // use `symlink_metadata` (which does NOT follow the link) so a symlink is
    // detected rather than resolved. An ancestor symlink (e.g. macOS
    // `/var` → `/private/var`) is harmless and intentionally allowed.
    if let Some(offending) = first_symlink_in_path(work_dir) {
        return Err(format!(
            "Refusing to force-reclone {}: the path crosses a symlink ({}), and remove_dir_all \
             would follow it and delete the link's target. The work_dir must be a real directory \
             with no symlinks on its path.",
            work_dir.display(),
            offending.display()
        ));
    }
    // Count named components (Normal). Root/prefix/curdir/parent don't count.
    // A safe work_dir has at least two named components (e.g. /home/user,
    // /tmp/agent1), so removing it cannot nuke a top-level OS directory like
    // /home or /tmp.
    let named_count = work_dir
        .components()
        .filter(|c| matches!(c, std::path::Component::Normal(_)))
        .count();
    if named_count < 2 {
        return Err(format!(
            "Refusing to force-reclone {}: it resolves to a filesystem root or a top-level \
             directory, which would delete too much. The work_dir must have at least two path \
             components.",
            work_dir.display()
        ));
    }
    Ok(())
}

/// Return `Some(work_dir)` when `work_dir` itself is a symlink, else `None`.
///
/// The catastrophic case for `remove_dir_all` is when the **final component**
/// is a symlink: `remove_dir_all` follows the link and deletes its target
/// tree. An ancestor symlink (e.g. macOS `/var` → `/private/var`) is harmless:
/// the path resolves to a real directory and only the leaf subtree is removed.
///
/// Uses [`std::fs::symlink_metadata`] (which does NOT follow the link) so a
/// symlink is detected rather than resolved. Returns `None` when the path does
/// not exist or is not a symlink (both are safe for a force-reclone target).
fn first_symlink_in_path(work_dir: &Path) -> Option<PathBuf> {
    match std::fs::symlink_metadata(work_dir) {
        Ok(meta) if meta.file_type().is_symlink() => Some(work_dir.to_path_buf()),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn validate_reclone_target_rejects_empty() {
        assert!(validate_reclone_target(Path::new("")).is_err());
        assert!(validate_reclone_target(Path::new("   ")).is_err());
    }

    #[test]
    fn validate_reclone_target_rejects_filesystem_root() {
        assert!(
            validate_reclone_target(Path::new("/")).is_err(),
            "root must be rejected"
        );
    }

    #[test]
    fn validate_reclone_target_rejects_top_level_entry() {
        // A single component directly under root is too destructive for an
        // automated reclone.
        assert!(validate_reclone_target(Path::new("/home")).is_err());
        assert!(validate_reclone_target(Path::new("/tmp")).is_err());
    }

    #[test]
    fn validate_reclone_target_accepts_nested_path() {
        assert!(
            validate_reclone_target(Path::new("/home/user/work/agent1")).is_ok(),
            "a normal nested work_dir must be accepted"
        );
        assert!(validate_reclone_target(Path::new("/srv/repos/jefe/agents/a1")).is_ok(),);
    }

    #[test]
    fn validate_reclone_target_rejects_symlink_workdir() {
        // A symlinked work_dir would be followed by remove_dir_all and could
        // delete the link's target tree. Create a symlink pointing at a real
        // nested dir and confirm it is rejected.
        #[cfg(unix)]
        {
            use std::os::unix::fs::symlink;
            let tmp = std::env::temp_dir().join(format!(
                "jefe-symlink-reject-{}-{}",
                std::process::id(),
                rand_label()
            ));
            let real = tmp.join("real/agent1");
            std::fs::create_dir_all(&real).value_or_panic("create real dir");
            let link = tmp.join("link/agent1");
            std::fs::create_dir_all(link.parent().value_or_panic("link parent"))
                .value_or_panic("create link parent");
            symlink(&real, &link).value_or_panic("create symlink");
            let result = validate_reclone_target(&link);
            let err = result.error_or_panic("symlink must error");
            let _ = std::fs::remove_dir_all(&tmp);
            assert!(err.contains("symlink"), "error must mention symlink: {err}");
        }
        // On non-Unix there are no symlinks to create; skip.
    }

    fn rand_label() -> String {
        use std::sync::atomic::{AtomicU64, Ordering};
        use std::time::{SystemTime, UNIX_EPOCH};
        static COUNTER: AtomicU64 = AtomicU64::new(0);
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_or(0, |d| d.as_nanos());
        let seq = COUNTER.fetch_add(1, Ordering::Relaxed);
        format!("{nanos}-{seq}")
    }

    trait TestResultStringExt {
        fn error_or_panic(self, context: &str) -> String;
    }

    impl TestResultStringExt for Result<(), String> {
        fn error_or_panic(self, context: &str) -> String {
            match self {
                Ok(()) => panic!("{context}: expected error"),
                Err(s) => s,
            }
        }
    }

    trait TestOptionExt<T> {
        fn value_or_panic(self, context: &str) -> T;
    }

    impl<T> TestOptionExt<T> for Option<T> {
        fn value_or_panic(self, context: &str) -> T {
            match self {
                Some(value) => value,
                None => panic!("{context}: expected Some, got None"),
            }
        }
    }

    trait TestIoResultExt<T> {
        fn value_or_panic(self, context: &str) -> T;
    }

    impl<T> TestIoResultExt<T> for std::io::Result<T> {
        fn value_or_panic(self, context: &str) -> T {
            match self {
                Ok(value) => value,
                Err(error) => panic!("{context}: {error}"),
            }
        }
    }
}

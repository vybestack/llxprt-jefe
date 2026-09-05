//! Git watch-path seam for the build script (issue #753).
//!
//! One source of truth for when cargo must re-run the build script. It is
//! included by `build.rs` via `#[path = "build_support/git_watch.rs"]` and by
//! the identity integration tests via `#[path = "../build_support/git_watch.rs"]`,
//! so both compile the same resolution logic. Paths come from git plumbing
//! (`rev-parse --git-dir`, `rev-parse --git-common-dir`, `symbolic-ref -q
//! HEAD`), which already understands ordinary repositories, gitfile worktrees,
//! packed refs, and detached HEADs.

use std::path::{Path, PathBuf};
use std::process::Command;

/// The files the build script watches, per the repository's HEAD state.
#[derive(Debug, PartialEq, Eq)]
pub enum WatchState {
    /// Attached HEAD: watch the resolved HEAD file plus the resolved loose
    /// branch-ref file. The branch file may be absent (packed refs); its
    /// absent-to-present transition is exactly the next ref update.
    Attached { head: PathBuf, branch_ref: PathBuf },
    /// Detached HEAD: every movement rewrites the HEAD file itself.
    Detached { head: PathBuf },
    /// Git missing or not a repository: the floor watch (`.git/HEAD`).
    Fallback,
}

impl WatchState {
    /// Paths for `cargo:rerun-if-changed` directives: package-relative when a
    /// resolved path sits under the package root, absolute otherwise (linked
    /// worktrees place both files outside the package).
    #[must_use]
    pub fn rerun_paths(&self, package_root: &Path) -> Vec<String> {
        match self {
            Self::Attached { head, branch_ref } => vec![
                watched_line(head, package_root),
                watched_line(branch_ref, package_root),
            ],
            Self::Detached { head } => vec![watched_line(head, package_root)],
            Self::Fallback => vec![".git/HEAD".to_owned()],
        }
    }
}

/// Resolve the watch state for the repository containing `package_root`.
#[must_use]
pub fn resolve(package_root: &Path) -> WatchState {
    let Some(git_dir) = plumbing_output(package_root, &["rev-parse", "--git-dir"]) else {
        return WatchState::Fallback;
    };
    let head = resolve_under(&git_dir, package_root).join("HEAD");
    let Some(branch_ref) = plumbing_output(package_root, &["symbolic-ref", "-q", "HEAD"]) else {
        return WatchState::Detached { head };
    };
    // Branch refs live in the common dir even when `git-dir` points at a
    // per-worktree gitdir; ordinary repositories print the same directory.
    let common_dir = plumbing_output(package_root, &["rev-parse", "--git-common-dir"]).map_or_else(
        || resolve_under(&git_dir, package_root),
        |dir| resolve_under(&dir, package_root),
    );
    WatchState::Attached {
        head,
        branch_ref: common_dir.join(branch_ref),
    }
}

/// Git plumbing output, trimmed. `None` covers every failure shape (git
/// missing, not a repository, the detached-HEAD exit of `symbolic-ref -q`) so
/// identity cosmetics never fail the build.
fn plumbing_output(directory: &Path, args: &[&str]) -> Option<String> {
    let output = Command::new("git")
        .arg("-C")
        .arg(directory)
        .args(args)
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let text = String::from_utf8(output.stdout).ok()?;
    let trimmed = text.trim();
    if trimmed.is_empty() {
        None
    } else {
        Some(trimmed.to_owned())
    }
}

/// Git prints worktree paths absolute and ordinary paths relative to the
/// directory the command ran in; resolve the relative forms against the
/// package root.
fn resolve_under(resolved: &str, package_root: &Path) -> PathBuf {
    let path = Path::new(resolved);
    if path.is_absolute() {
        path.to_path_buf()
    } else {
        package_root.join(path)
    }
}

/// Package-relative display when the watched file is under the package root,
/// so an ordinary checkout keeps today's relative watch shape.
fn watched_line(path: &Path, package_root: &Path) -> String {
    match path.strip_prefix(package_root) {
        Ok(relative) => relative.to_string_lossy().into_owned(),
        Err(_) => path.to_string_lossy().into_owned(),
    }
}

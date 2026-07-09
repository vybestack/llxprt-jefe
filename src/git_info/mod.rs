//! Git repository info for display: origin shortform and current branch.
//!
//! Provides cached, side-effect-free lookups of the git branch and origin
//! shortform for an agent's work directory. The branch is dynamic (an agent may
//! `git checkout` during its session), so it is probed live but cached with a
//! time-based TTL to avoid spawning a git process on every render frame.
//!
//! For remote repositories (SSH-backed), branch probing is skipped because it
//! would require an SSH round-trip — only the origin shortform (from the
//! configured `github_repo` field) is returned.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::Mutex;
use std::time::{Duration, Instant};

/// Cache entry: branch string + the instant it was probed.
struct CachedBranch {
    branch: Option<String>,
    probed_at: Instant,
}

/// Thread-safe cache mapping work_dir → cached branch result.
///
/// The cache is process-global so all render passes share the same TTL window.
/// Entries expire after [`GIT_CACHE_TTL`] and are re-probed lazily on the next
/// lookup.
static BRANCH_CACHE: Mutex<Option<HashMap<PathBuf, CachedBranch>>> = Mutex::new(None);

/// How long a cached branch result remains fresh before re-probing.
///
/// Agents don't switch branches frequently, but they can (`git checkout`),
/// so this is short enough to feel live without spawning git every frame.
const GIT_CACHE_TTL: Duration = Duration::from_secs(5);

/// Resolved git display info for an agent's work directory.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct GitRepoInfo {
    /// Origin shortform in `owner/repo` form (e.g. `vybestack/llxprt-jefe`).
    /// `None` when no GitHub origin could be determined.
    pub origin_shortform: Option<String>,
    /// Current git branch name (e.g. `main`). `None` for non-git dirs,
    /// detached HEAD, or remote repos where probing was skipped.
    pub branch: Option<String>,
}

impl GitRepoInfo {
    /// Build a `GitRepoInfo` for the given work directory.
    ///
    /// - `github_repo`: the configured `Repository.github_repo` field
    ///   (`owner/repo`). When non-empty, this is used directly as the origin
    ///   shortform (zero-cost, no git probing).
    /// - `is_remote`: when `true`, branch probing is skipped (would require an
    ///   SSH round-trip). Only the origin shortform is populated.
    /// - `work_dir`: the agent's working directory, probed for the branch and
    ///   for the origin shortform fallback.
    #[must_use]
    pub fn resolve(github_repo: &str, is_remote: bool, work_dir: &Path) -> Self {
        let origin_shortform = if github_repo.trim().is_empty() {
            detect_origin_shortform(work_dir)
        } else {
            Some(github_repo.trim().to_owned())
        };

        let branch = if is_remote {
            None
        } else {
            cached_branch(work_dir)
        };

        Self {
            origin_shortform,
            branch,
        }
    }

    /// Format the info as a compact suffix for the agent list row.
    ///
    /// Returns a string like `"vybestack/llxprt-jefe @ main"` when both parts
    /// are present, or a partial form when only one is available. Returns an
    /// empty string when neither is known.
    #[must_use]
    pub fn list_suffix(&self) -> String {
        match (&self.origin_shortform, &self.branch) {
            (Some(origin), Some(branch)) => format!("{origin} @ {branch}"),
            (Some(origin), None) => origin.clone(),
            (None, Some(branch)) => format!("@ {branch}"),
            (None, None) => String::new(),
        }
    }
}

/// Get the cached branch for a work directory, re-probing if stale.
///
/// Uses a global process cache with [`GIT_CACHE_TTL`]. If the git command
/// fails (non-git dir, git not installed), the result is cached as `None`
/// to avoid repeated failed probes.
fn cached_branch(work_dir: &Path) -> Option<String> {
    let now = Instant::now();

    // Fast path: check the cache under the lock.
    if let Ok(mut guard) = BRANCH_CACHE.lock() {
        let cache = guard.get_or_insert_with(HashMap::new);
        if let Some(entry) = cache.get(work_dir)
            && now.duration_since(entry.probed_at) < GIT_CACHE_TTL
        {
            return entry.branch.clone();
        }
    }

    // Slow path: probe git (outside the lock to avoid blocking other threads).
    let branch = probe_branch(work_dir);

    // Store the result back in the cache.
    if let Ok(mut guard) = BRANCH_CACHE.lock() {
        let cache = guard.get_or_insert_with(HashMap::new);
        cache.insert(
            work_dir.to_path_buf(),
            CachedBranch {
                branch: branch.clone(),
                probed_at: now,
            },
        );
    }

    branch
}

/// Probe the current git branch for a work directory.
///
/// Returns `None` for non-git directories, detached HEAD states, or when git
/// is not installed. Uses `git rev-parse --abbrev-ref HEAD` which returns the
/// branch name or `HEAD` for detached HEAD (filtered out).
fn probe_branch(work_dir: &Path) -> Option<String> {
    let output = Command::new("git")
        .arg("-C")
        .arg(work_dir)
        .args(["rev-parse", "--abbrev-ref", "HEAD"])
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let branch = String::from_utf8_lossy(&output.stdout).trim().to_owned();
    if branch.is_empty() || branch == "HEAD" {
        // Detached HEAD — fall back to the short commit hash for display.
        return probe_short_commit(work_dir);
    }
    Some(branch)
}

/// Fall back to the short commit hash for detached HEAD states.
fn probe_short_commit(work_dir: &Path) -> Option<String> {
    let output = Command::new("git")
        .arg("-C")
        .arg(work_dir)
        .args(["rev-parse", "--short", "HEAD"])
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let hash = String::from_utf8_lossy(&output.stdout).trim().to_owned();
    if hash.is_empty() {
        None
    } else {
        Some(format!("({hash})"))
    }
}

/// Detect the `owner/repo` shortform from the git remote `origin` URL.
///
/// Handles SSH (`git@github.com:owner/repo.git`), HTTPS
/// (`https://github.com/owner/repo.git`), and bare (`owner/repo`) forms.
/// Returns `None` when the origin remote is missing or the URL doesn't match
/// a known pattern.
fn detect_origin_shortform(work_dir: &Path) -> Option<String> {
    let output = Command::new("git")
        .arg("-C")
        .arg(work_dir)
        .args(["remote", "get-url", "origin"])
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let url = String::from_utf8_lossy(&output.stdout).trim().to_owned();
    parse_origin_url(&url)
}

/// Parse a git remote URL into an `owner/repo` shortform.
///
/// Handles:
/// - SSH: `git@github.com:owner/repo.git`
/// - HTTPS: `https://github.com/owner/repo.git`
/// - SSH with scheme: `ssh://git@github.com/owner/repo.git`
/// - Bare: `owner/repo`
pub(crate) fn parse_origin_url(url: &str) -> Option<String> {
    let url = url.trim();
    if url.is_empty() {
        return None;
    }

    // Try SSH form: everything after the last ':' if the left side has no '/'
    // (i.e., it's `user@host:path`).
    if !url.contains("://") {
        if let Some(colon_pos) = url.rfind(':') {
            let host_part = &url[..colon_pos];
            let path_part = &url[colon_pos + 1..];
            // SSH form: host_part is `user@host` (no '/'), path is `owner/repo.git`.
            if !host_part.contains('/') {
                return extract_owner_repo(path_part);
            }
        }
        // Bare form: `owner/repo` (no colon, no scheme).
        return extract_owner_repo(url);
    }

    // HTTPS / SSH-with-scheme form: strip the scheme and host, keep the path.
    // `https://github.com/owner/repo.git` → `owner/repo`
    // `ssh://git@github.com/owner/repo.git` → `owner/repo`
    let after_scheme = url.split("://").nth(1)?;
    // Skip the host: find the first '/' after the host.
    let path_start = after_scheme.find('/')?;
    let path = &after_scheme[path_start + 1..];
    extract_owner_repo(path)
}

/// Extract `owner/repo` from a path like `owner/repo.git` or `owner/repo`.
///
/// Strips a trailing `.git` suffix and validates that the result has exactly
/// one `/` separating two non-empty parts.
fn extract_owner_repo(path: &str) -> Option<String> {
    let path = path.trim_end_matches('/').trim_end_matches(".git");
    let (owner, repo) = path.split_once('/')?;
    let owner = owner.trim();
    let repo = repo.trim();
    if owner.is_empty() || repo.is_empty() {
        return None;
    }
    // Ensure no extra path segments (e.g., `owner/repo/sub` is invalid).
    if repo.contains('/') {
        return None;
    }
    Some(format!("{owner}/{repo}"))
}

#[cfg(test)]
mod tests;

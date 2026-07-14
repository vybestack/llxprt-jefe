//! Git repository info for display: origin shortform and current branch.
//!
//! Provides cached lookups of the git branch and origin shortform for an
//! agent's work directory. Both are probed live but cached with a time-based
//! TTL to avoid spawning git processes on every render frame.
//!
//! For remote repositories (SSH-backed), branch probing is skipped because it
//! would require an SSH round-trip — only the origin shortform (from the
//! configured `github_repo` field) is returned.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::{Duration, Instant};

/// Cached git probe result for a single work directory.
///
/// Each field has its own timestamp so that branch (short TTL) and origin
/// (long TTL) probes are refreshed independently.
struct CacheEntry {
    branch: Option<String>,
    origin: Option<String>,
    branch_probed_at: Instant,
    origin_probed_at: Instant,
}

/// Thread-safe cache mapping work_dir → cached probe results.
///
/// The cache is process-global so all render passes share the same TTL window.
/// Entries expire after their respective TTL and are re-probed lazily on the
/// next lookup. Stale entries are swept opportunistically on insertion to
/// prevent unbounded growth.
static GIT_CACHE: Mutex<Option<HashMap<PathBuf, CacheEntry>>> = Mutex::new(None);

/// How long a cached branch result remains fresh before re-probing.
///
/// Agents don't switch branches frequently, but they can (`git checkout`),
/// so this is short enough to feel live without spawning git every frame.
const BRANCH_TTL: Duration = Duration::from_secs(5);

/// How long a cached origin shortform remains fresh before re-probing.
///
/// The origin URL changes very rarely (essentially never during a session),
/// so this is much longer than the branch TTL.
const ORIGIN_TTL_SECONDS: u64 = 5 * 60;
const ORIGIN_TTL: Duration = Duration::from_secs(ORIGIN_TTL_SECONDS);

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
            cached_origin(work_dir)
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
/// Uses the global process cache with [`BRANCH_TTL`]. If the git command
/// fails (non-git dir, git not installed), the result is cached as `None`
/// to avoid repeated failed probes.
fn cached_branch(work_dir: &Path) -> Option<String> {
    let now = Instant::now();

    // Fast path: check the cache under the lock.
    if let Ok(mut guard) = GIT_CACHE.lock() {
        let cache = guard.get_or_insert_with(HashMap::new);
        if let Some(entry) = cache.get(work_dir)
            && now.duration_since(entry.branch_probed_at) < BRANCH_TTL
        {
            return entry.branch.clone();
        }
    }

    // Slow path: probe git (outside the lock to avoid blocking other threads).
    let branch = probe_branch(work_dir);

    // Store the result back in the cache, preserving the existing origin if
    // it is still fresh.
    if let Ok(mut guard) = GIT_CACHE.lock() {
        let cache = guard.get_or_insert_with(HashMap::new);
        let entry = cache.entry(work_dir.to_path_buf()).or_insert(CacheEntry {
            branch: None,
            origin: None,
            branch_probed_at: now,
            origin_probed_at: now,
        });
        entry.branch.clone_from(&branch);
        entry.branch_probed_at = now;
        sweep_stale(cache, now);
    }

    branch
}

/// Get the cached origin shortform for a work directory, re-probing if stale.
///
/// Uses the global process cache with [`ORIGIN_TTL`]. Since the origin URL
/// rarely changes, the TTL is much longer than the branch TTL.
fn cached_origin(work_dir: &Path) -> Option<String> {
    let now = Instant::now();

    // Fast path: check the cache under the lock. Cache both Some and None
    // results so non-git dirs don't spawn git on every frame.
    if let Ok(mut guard) = GIT_CACHE.lock() {
        let cache = guard.get_or_insert_with(HashMap::new);
        if let Some(entry) = cache.get(work_dir)
            && now.duration_since(entry.origin_probed_at) < ORIGIN_TTL
        {
            return entry.origin.clone();
        }
    }

    // Slow path: probe git (outside the lock to avoid blocking other threads).
    let origin = detect_origin_shortform(work_dir);

    // Store the result back in the cache, preserving the existing branch.
    if let Ok(mut guard) = GIT_CACHE.lock() {
        let cache = guard.get_or_insert_with(HashMap::new);
        let entry = cache.entry(work_dir.to_path_buf()).or_insert(CacheEntry {
            branch: None,
            origin: None,
            branch_probed_at: now,
            origin_probed_at: now,
        });
        entry.origin.clone_from(&origin);
        entry.origin_probed_at = now;
        sweep_stale(cache, now);
    }

    origin
}

/// Remove entries where both branch and origin are stale beyond a generous
/// threshold. Called opportunistically on cache writes to prevent unbounded
/// growth in long-running sessions.
fn sweep_stale(cache: &mut HashMap<PathBuf, CacheEntry>, now: Instant) {
    // Only sweep when the cache grows beyond a threshold.
    if cache.len() < 32 {
        return;
    }
    let branch_max = BRANCH_TTL * 2;
    let origin_max = ORIGIN_TTL * 2;
    cache.retain(|_, entry| {
        now.duration_since(entry.branch_probed_at) < branch_max
            || now.duration_since(entry.origin_probed_at) < origin_max
    });
}

/// Probe the current git branch for a work directory.
///
/// Returns `None` for non-git directories, detached HEAD states, or when git
/// is not installed. Uses `git rev-parse --abbrev-ref HEAD` which returns the
/// branch name or `HEAD` for detached HEAD (filtered out).
fn probe_branch(work_dir: &Path) -> Option<String> {
    let mut command = match crate::local_command::command(crate::local_command::LocalTool::Git) {
        Ok(command) => command,
        Err(error) => {
            tracing::debug!(%error, "could not resolve Git while probing branch");
            return None;
        }
    };
    let output = command
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
    let output = crate::local_command::command(crate::local_command::LocalTool::Git)
        .ok()?
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
    let output = crate::local_command::command(crate::local_command::LocalTool::Git)
        .ok()?
        .arg("-C")
        .arg(work_dir)
        .args(["remote", "get-url", "origin"])
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let url = String::from_utf8_lossy(&output.stdout).trim().to_owned();
    origin_display_shortform(&url)
}

/// A parsed repository origin with host identity preserved.
///
/// Unlike [`parse_origin_url`] (which strips the host and returns only an
/// `owner/repo` shortform for display), this type retains the **lowercased
/// host** so callers can perform host-aware security comparisons.
///
/// This is the foundation of the origin-mismatch safety check (issue #190):
/// a working copy whose `origin` points at `gitlab.com/owner/repo` or
/// `git@attacker.example:owner/repo` must NOT match a configured GitHub
/// repository even if the `owner/repo` path is identical.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParsedRepositoryOrigin {
    /// Lowercased host (e.g. `"github.com"`). Empty string for the bare
    /// `owner/repo` form (no host present in the URL).
    pub host: String,
    /// The `owner/repo` path component (e.g. `"acme/widgets"`).
    pub owner_repo: String,
}

/// Parse a git remote URL into a [`ParsedRepositoryOrigin`] that preserves
/// the host for security-aware comparison.
///
/// Handles:
/// - SSH: `git@github.com:owner/repo.git` → host=`github.com`
/// - HTTPS: `https://github.com/owner/repo.git` → host=`github.com`
/// - SSH with scheme: `ssh://git@github.com/owner/repo.git` → host=`github.com`
/// - Bare: `owner/repo` → host=`""` (empty — no host present in the URL)
///
/// For scheme URLs and scp-style SSH, the host is extracted and **lowercased**.
/// For the bare `owner/repo` form (no host), `host` is an empty string.
/// Trailing `.git` is stripped from the path.
///
/// **Scheme allowlist (security):** only `https`, `http`, `ssh`, and `git`
/// scheme URLs are accepted. Other schemes (e.g. `file://`, `ftp://`, or a
/// custom Git remote helper scheme like `myhelper://github.com/...`) are
/// rejected with `None`, even if the authority syntactically reads
/// `github.com`. Git supports pluggable remote helpers for arbitrary schemes,
/// so a non-allowlisted scheme cannot be trusted to target GitHub even when
/// the host matches.
///
/// Returns `None` for empty/malformed input, unallowed schemes, missing
/// owner/repo, or extra path segments.
#[must_use]
pub fn parse_repository_origin(url: &str) -> Option<ParsedRepositoryOrigin> {
    let url = url.trim();
    if url.is_empty() {
        return None;
    }

    if !url.contains("://") {
        // Try scp-style SSH: `user@host:owner/repo.git`
        if let Some(colon_pos) = url.rfind(':') {
            let host_part = &url[..colon_pos];
            let path_part = &url[colon_pos + 1..];
            // SSH form: host_part is `user@host` (no '/'), path is `owner/repo.git`.
            if !host_part.contains('/') {
                let owner_repo = extract_owner_repo(path_part)?;
                let host = extract_host_from_scp(host_part);
                return Some(ParsedRepositoryOrigin { host, owner_repo });
            }
        }
        // Bare form: `owner/repo` (no colon, no scheme). No host.
        let owner_repo = extract_owner_repo(url)?;
        return Some(ParsedRepositoryOrigin {
            host: String::new(),
            owner_repo,
        });
    }

    // Scheme URL form: `scheme://authority/path`. Validate the scheme against
    // an allowlist before trusting the host. A non-allowlisted scheme (e.g.
    // file://, ftp://, or a custom remote-helper scheme) is rejected even if
    // the authority reads github.com — Git remote helpers can resolve
    // arbitrary schemes to anywhere.
    let (scheme, after_scheme) = url.split_once("://")?;
    let scheme = scheme.to_lowercase();
    if !ALLOWED_REMOTE_SCHEMES.contains(&scheme.as_str()) {
        return None;
    }
    // The host is everything between the scheme and the first `/`.
    // For `ssh://git@github.com/owner/repo.git`, host_part = `git@github.com`.
    let path_start = after_scheme.find('/')?;
    let host_part = &after_scheme[..path_start];
    let path = &after_scheme[path_start + 1..];
    let owner_repo = extract_owner_repo(path)?;
    let host = extract_host_from_scheme(host_part);
    Some(ParsedRepositoryOrigin { host, owner_repo })
}

/// Remote-URL schemes trusted to target the host named in their authority.
///
/// Only these schemes map the host segment to the actual Git host. Any other
/// scheme (e.g. `file://`, `ftp://`, or a custom remote-helper scheme) may
/// resolve elsewhere, so `parse_repository_origin` rejects them regardless of
/// the authority string.
const ALLOWED_REMOTE_SCHEMES: [&str; 4] = ["https", "http", "ssh", "git"];

/// Extract the lowercased host from a scp-style `user@host` component.
///
/// For `git@github.com` → `github.com`. For `github.com` → `github.com`.
fn extract_host_from_scp(host_part: &str) -> String {
    match host_part.rfind('@') {
        Some(pos) => host_part[pos + 1..].to_lowercase(),
        None => host_part.to_lowercase(),
    }
}

/// Extract the lowercased host from a scheme-style authority component.
///
/// For `git@github.com` → `github.com`. For `github.com` → `github.com`.
/// Strips any port suffix (e.g., `github.com:22` → `github.com`).
///
/// Bracketed IPv6 literals are handled correctly: for `[::1]:22` → `[::1]`
/// and for `[::1]` → `[::1]` (the port is only stripped when it follows the
/// closing `]`; a bare `rfind(':')` would otherwise split inside the address).
fn extract_host_from_scheme(authority: &str) -> String {
    let after_user = match authority.rfind('@') {
        Some(pos) => &authority[pos + 1..],
        None => authority,
    };
    // Bracketed IPv6 literal: the host is between '[' and ']'; a port, if
    // present, follows the closing bracket (e.g. `[::1]:22`). Do NOT use a
    // bare rfind(':') — it would split inside the IPv6 address.
    if let Some(close) = after_user.rfind(']') {
        return after_user[..=close].to_lowercase();
    }
    // Plain host: strip a port suffix after the last ':' if one is present.
    let host = match after_user.rfind(':') {
        Some(pos) => &after_user[..pos],
        None => after_user,
    };
    host.to_lowercase()
}

/// Parse a git remote URL into an `owner/repo` shortform.
///
/// Handles:
/// - SSH: `git@github.com:owner/repo.git`
/// - HTTPS: `https://github.com/owner/repo.git`
/// - SSH with scheme: `ssh://git@github.com/owner/repo.git`
/// - Bare: `owner/repo`
///
/// Reused by the issue-send origin-mismatch check (issue #190) to normalize
/// a working copy's `origin` URL for comparison against the configured
/// `Repository.github_repo`. Note: this function **strips the host** and is
/// kept for backwards-compatible display use; for host-aware security
/// comparison use [`parse_repository_origin`].
///
/// # Deprecated
///
/// This function strips the host and is unsafe for origin-mismatch security
/// checks: a URL like `git@evil.example.com:owner/repo.git` would normalize to
/// the same `owner/repo` as the configured GitHub repo and wrongly "match".
/// New code MUST use [`parse_repository_origin`] (which retains the host) for
/// any comparison, and [`origin_display_shortform`] for display. This wrapper
/// is retained only for backwards compatibility with external callers.
#[deprecated(
    since = "0.4.0",
    note = "use parse_repository_origin for security comparisons (this strips the host); use origin_display_shortform for display"
)]
#[must_use]
pub fn parse_origin_url(url: &str) -> Option<String> {
    parse_repository_origin(url).map(|parsed| parsed.owner_repo)
}

/// Normalize an origin URL to an `owner/repo` shortform for **display only**.
///
/// This is the non-deprecated replacement for [`parse_origin_url`] when the
/// result is shown to the user (e.g. in an error message). It deliberately
/// strips the host and therefore MUST NOT be used for origin-mismatch
/// security comparisons — for those, use [`parse_repository_origin`], which
/// retains the host so a same-`owner/repo` URL on a different host is
/// correctly rejected.
#[must_use]
pub fn origin_display_shortform(url: &str) -> Option<String> {
    parse_repository_origin(url).map(|parsed| parsed.owner_repo)
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

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
use std::process::Command;
use std::sync::Mutex;
use std::time::{Duration, Instant};

/// Cached git probe result for a single work directory.
///
/// Each field has its own timestamp so that branch (short TTL) and origin
/// (long TTL) probes are refreshed independently. Dirty status shares the
/// branch TTL since it is probed alongside the branch.
struct CacheEntry {
    branch: Option<String>,
    origin: Option<String>,
    dirty: Option<bool>,
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

/// Maximum wall-clock time a single git subprocess probe may take before it
/// is killed and treated as unknown. Prevents a stuck/slow worktree (e.g.
/// NFS hang, giant repo) from blocking the chooser open.
const GIT_PROBE_TIMEOUT: Duration = Duration::from_secs(3);

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
    /// Dirty working-tree status. `Some(true)` when there are uncommitted or
    /// untracked changes (excluding jefe/llxprt-owned paths). `Some(false)`
    /// when the tree is clean. `None` when dirty status is unknown (remote
    /// repos, non-git dirs, or probe failure).
    pub dirty: Option<bool>,
}

impl GitRepoInfo {
    /// Build a `GitRepoInfo` for the given work directory.
    ///
    /// - `github_repo`: the configured `Repository.github_repo` field
    ///   (`owner/repo`). When non-empty, this is used directly as the origin
    ///   shortform (zero-cost, no git probing).
    /// - `is_remote`: when `true`, branch and dirty probing are skipped (would
    ///   require an SSH round-trip). Only the origin shortform is populated.
    ///   Dirty status is `None` (unknown) for remote repos.
    /// - `work_dir`: the agent's working directory, probed for the branch,
    ///   dirty status, and the origin shortform fallback.
    #[must_use]
    pub fn resolve(github_repo: &str, is_remote: bool, work_dir: &Path) -> Self {
        let origin_shortform = if github_repo.trim().is_empty() {
            cached_origin(work_dir)
        } else {
            Some(github_repo.trim().to_owned())
        };

        let (branch, dirty) = if is_remote {
            (None, None)
        } else {
            cached_branch_and_dirty(work_dir)
        };

        Self {
            origin_shortform,
            branch,
            dirty,
        }
    }

    /// Format the info as a compact suffix for the agent list row.
    ///
    /// Returns a string like `"vybestack/llxprt-jefe @ main *"` when both parts
    /// are present and the working tree is dirty. The dirty marker (` *`) is
    /// shown only when a branch is present and dirty is `Some(true)`. Returns
    /// an empty string when neither origin nor branch is known.
    #[must_use]
    pub fn list_suffix(&self) -> String {
        let dirty_marker = match (&self.branch, self.dirty) {
            (Some(_), Some(true)) => " *",
            _ => "",
        };
        match (&self.origin_shortform, &self.branch) {
            (Some(origin), Some(branch)) => {
                format!("{origin} @ {branch}{dirty_marker}")
            }
            (Some(origin), None) => origin.clone(),
            (None, Some(branch)) => format!("@ {branch}{dirty_marker}"),
            (None, None) => String::new(),
        }
    }
}

/// An `Instant` far enough in the past to be beyond any TTL, used as the
/// "never probed" sentinel for cache entries created by a different probe
/// type. This ensures `cached_branch_and_dirty` doesn't see a fresh
/// `branch_probed_at` set by `cached_origin` (and vice versa).
fn far_past() -> Instant {
    // `Instant` subtraction panics if the result would be before the epoch.
    // Use a conservative subtraction that is safely older than any TTL.
    Instant::now()
        .checked_sub(ORIGIN_TTL * 10)
        .unwrap_or_else(Instant::now)
}

/// Get the cached branch and dirty status for a work directory, re-probing
/// if stale.
///
/// Both are probed in a single pass and share the short [`BRANCH_TTL`] since
/// dirty status changes alongside working-tree state. Uses the global process
/// cache. If the git command fails (non-git dir, git not installed), the
/// results are cached as `(None, None)` to avoid repeated failed probes.
fn cached_branch_and_dirty(work_dir: &Path) -> (Option<String>, Option<bool>) {
    let now = Instant::now();

    // Fast path: check the cache under the lock.
    if let Ok(mut guard) = GIT_CACHE.lock() {
        let cache = guard.get_or_insert_with(HashMap::new);
        if let Some(entry) = cache.get(work_dir)
            && now.duration_since(entry.branch_probed_at) < BRANCH_TTL
        {
            return (entry.branch.clone(), entry.dirty);
        }
    }

    // Slow path: probe git (outside the lock to avoid blocking other threads).
    let (branch, dirty) = probe_branch_and_dirty(work_dir);

    // Store the result back in the cache, preserving the existing origin if
    // it is still fresh.
    if let Ok(mut guard) = GIT_CACHE.lock() {
        let cache = guard.get_or_insert_with(HashMap::new);
        let entry = cache
            .entry(work_dir.to_path_buf())
            .or_insert_with(|| CacheEntry {
                branch: None,
                origin: None,
                dirty: None,
                branch_probed_at: now,
                origin_probed_at: far_past(),
            });
        entry.branch.clone_from(&branch);
        entry.dirty = dirty;
        entry.branch_probed_at = now;
        sweep_stale(cache, now);
    }

    (branch, dirty)
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
        let entry = cache.entry(work_dir.to_path_buf()).or_insert_with(|| {
            // New entry: branch has never been probed, so set
            // branch_probed_at far in the past so the next branch lookup
            // will probe rather than returning the default None.
            CacheEntry {
                branch: None,
                origin: None,
                dirty: None,
                branch_probed_at: far_past(),
                origin_probed_at: now,
            }
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

/// Resolve the git command, returning `None` if the Git executable cannot be
/// found.
fn git_command() -> Option<std::process::Command> {
    match crate::local_command::command(crate::local_command::LocalTool::Git) {
        Ok(command) => Some(command),
        Err(error) => {
            tracing::debug!(%error, "could not resolve Git while probing repository metadata");
            None
        }
    }
}

/// Run a prepared git [`Command`] with a wall-clock timeout.
///
/// Spawns the child, polls for completion up to [`GIT_PROBE_TIMEOUT`], then
/// kills and reaps the child if it has not exited. On timeout, logs a warn
/// message and returns `None` so the caller treats the probe as unknown.
///
/// Cross-platform (no `unsafe`, FFI, or external dependencies): uses
/// `Child::try_wait()` polling and `Child::kill()` for termination.
fn run_git_with_timeout(command: &mut Command, work_dir: &Path) -> Option<std::process::Output> {
    // Pipe stdout/stderr so we can read them after polling. Without this,
    // spawn() inherits the parent's stdout/stderr and child.stdout is None.
    command
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped());
    let child = command.spawn().ok()?;
    run_child_with_timeout(child, work_dir, git_probe_label(command))
}

/// Poll a spawned [`std::process::Child`] for completion up to
/// [`GIT_PROBE_TIMEOUT`], reading stdout/stderr into an [`Output`] on
/// success. Kills and reaps the child on timeout, returning `None`.
///
/// Extracted from [`run_git_with_timeout`] so tests can exercise the timeout
/// with an arbitrary child (e.g. `sleep`). The `work_dir` and `probe_label`
/// are used only for the timeout warning message so diagnostics identify
/// which git probe on which directory was slow.
fn run_child_with_timeout(
    mut child: std::process::Child,
    work_dir: &Path,
    probe_label: &str,
) -> Option<std::process::Output> {
    let deadline = Instant::now() + GIT_PROBE_TIMEOUT;
    let mut status = None;

    // Poll every 50ms until the child exits or the deadline passes.
    loop {
        match child.try_wait() {
            Ok(Some(exit_status)) => {
                status = Some(exit_status);
                break;
            }
            Ok(None) => {
                if Instant::now() >= deadline {
                    break;
                }
                std::thread::sleep(Duration::from_millis(50));
            }
            Err(error) => {
                tracing::debug!(%error, "git probe try_wait failed");
                let _ = child.kill();
                let _ = child.wait();
                return None;
            }
        }
    }

    if status.is_none() {
        // Timed out — kill and reap.
        tracing::warn!(
            timeout = ?GIT_PROBE_TIMEOUT,
            work_dir = %work_dir.display(),
            probe = %probe_label,
            "git probe timed out, killing child and returning unknown metadata"
        );
        let _ = child.kill();
        let _ = child.wait();
        return None;
    }

    // Read stdout/stderr (pipes are still open since we didn't use
    // wait_with_output).
    let mut stdout = Vec::new();
    let mut stderr = Vec::new();
    if let Some(mut out) = child.stdout.take() {
        use std::io::Read;
        let _ = out.read_to_end(&mut stdout);
    }
    if let Some(mut err) = child.stderr.take() {
        use std::io::Read;
        let _ = err.read_to_end(&mut stderr);
    }

    // status is guaranteed Some here because the None case returns early
    // above (timeout path).
    let exit_status = status?;

    Some(std::process::Output {
        status: exit_status,
        stdout,
        stderr,
    })
}

/// Probe the current git branch and dirty status for a work directory.
///
/// Returns `(None, None)` for non-git directories or when git is not
/// installed. Dirty status is `Some(bool)` only when the branch probe
/// succeeds (the worktree is a valid git repo).
fn probe_branch_and_dirty(work_dir: &Path) -> (Option<String>, Option<bool>) {
    let Some(branch) = probe_branch(work_dir) else {
        return (None, None);
    };
    let dirty = probe_dirty(work_dir);
    (Some(branch), dirty)
}

/// Derive a concise, stable label for the git subcommand being run, for use
/// in timeout warning diagnostics. Inspects the command's arguments to
/// identify the probe type (e.g. `"status"`, `"rev-parse"`, `"remote"`).
fn git_probe_label(command: &Command) -> &str {
    command
        .get_args()
        .find_map(|arg| {
            arg.to_str().filter(|s| {
                matches!(
                    *s,
                    "status" | "rev-parse" | "remote" | "branch" | "symbolic-ref"
                )
            })
        })
        .unwrap_or("git")
}

/// Probe whether the working tree at `work_dir` has real (non-ignored)
/// changes.
///
/// Returns `Some(true)` when dirty, `Some(false)` when clean, and `None` when
/// the git command fails. Uses `git status --porcelain=v1 -z` (NUL-delimited
/// output) so paths containing newlines or ` -> ` are handled correctly. The
/// shared [`porcelain_is_dirty`] parser auto-detects the NUL-delimited format.
fn probe_dirty(work_dir: &Path) -> Option<bool> {
    let output = run_git_with_timeout(
        git_command()?
            .arg("-C")
            .arg(work_dir)
            .args(["status", "--porcelain=v1", "-z"]),
        work_dir,
    )?;
    if !output.status.success() {
        return None;
    }
    // NUL is valid UTF-8 (U+0000), so from_utf8_lossy preserves embedded NULs.
    let porcelain = String::from_utf8_lossy(&output.stdout);
    Some(porcelain_is_dirty(&porcelain))
}

/// Probe the current git branch for a work directory.
///
/// Returns `None` for non-git directories, detached HEAD states, or when git
/// is not installed. Uses `git rev-parse --abbrev-ref HEAD` which returns the
/// branch name or `HEAD` for detached HEAD (filtered out).
fn probe_branch(work_dir: &Path) -> Option<String> {
    let mut command = git_command()?;
    let output = run_git_with_timeout(
        command
            .arg("-C")
            .arg(work_dir)
            .args(["rev-parse", "--abbrev-ref", "HEAD"]),
        work_dir,
    )?;
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
    let output = run_git_with_timeout(
        git_command()?
            .arg("-C")
            .arg(work_dir)
            .args(["rev-parse", "--short", "HEAD"]),
        work_dir,
    )?;
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

// ── Porcelain dirty-status parser (issue #230) ──────────────────────────────
//
// This is the single source of truth for parsing `git status --porcelain=v1 -z`
// output to determine whether a working tree is dirty. Both the cached
// display probe ([`probe_dirty`]) and the issue-prep orchestration
// (`issue_git_prep::is_workdir_dirty`) consume this parser so the
// jefe/llxprt ignore semantics never drift between the two paths.
//
// Production always runs `git status --porcelain=v1 -z` (NUL-delimited).
// The parser also accepts legacy newline-delimited output for backward
// compatibility with synthetic tests that pass ` -> `-style rename strings.

/// Paths that jefe/llxprt own and that must never count as "dirty" working
/// copy state. Matched as path *prefixes* against the porcelain path column.
const IGNORED_PREFIXES: [&str; 2] = [".jefe/", ".llxprt/"];

/// NUL byte used by `git status --porcelain=v1 -z` to delimit records and
/// rename/copy path pairs.
const NUL: char = '\u{0000}';

/// Pure helper: given raw `git status --porcelain=v1` output, return `true`
/// when there is at least one non-ignored (i.e. real) change.
///
/// This is the shared parser consumed by both the cached display probe and
/// the issue-prep orchestration. Jefe/llxprt-owned paths (`.jefe/`,
/// `.llxprt/`) are excluded so jefe's own metadata does not mark the tree as
/// dirty. For rename/copy records, both old and new paths are considered: a
/// real→owned or owned→real rename is dirty; only when ALL affected paths are
/// under ignored prefixes is the record ignored.
///
/// **Format detection:** Production runs `--porcelain=v1 -z`, which emits
/// NUL-delimited records with rename/copy path order **destination THEN
/// source** (e.g. `R  new.txt\0old.txt\0`). Legacy newline-delimited output
/// uses `old -> new` order. This parser auto-detects the format by scanning
/// for embedded NUL bytes.
///
/// **Fail-safe:** Malformed or truncated records (e.g. a rename status whose
/// second path is missing) are treated as dirty, so unknown real changes are
/// never silently reported as clean.
#[must_use]
pub fn porcelain_is_dirty(porcelain: &str) -> bool {
    if porcelain.contains(NUL) {
        porcelain_is_dirty_z(porcelain)
    } else {
        porcelain_is_dirty_newline(porcelain)
    }
}

/// Parse NUL-delimited (`-z`) porcelain v1 output.
///
/// Each record begins with a 2-char status field and a space. Ordinary
/// records consume exactly one path (terminated by NUL). Rename/copy records
/// (status begins with `R` or `C`) consume TWO NUL-terminated paths: the
/// **destination** first, then the **source**. A record is ignored only if
/// ALL affected paths are under `.jefe/`/`.llxprt/`. Malformed/truncated
/// records fail safe as dirty.
fn porcelain_is_dirty_z(porcelain: &str) -> bool {
    let bytes: &[u8] = porcelain.as_bytes();
    let mut cursor = 0;
    let len = bytes.len();
    while cursor < len {
        // A leading NUL (e.g. trailing terminator) just advances.
        if bytes[cursor] == 0 {
            cursor += 1;
            continue;
        }
        // Need at least 3 bytes for the status field + space.
        if cursor + 3 > len || bytes[cursor + 2] != b' ' {
            // Malformed record — fail safe as dirty.
            return true;
        }
        let status_x = bytes[cursor];
        let status_y = bytes[cursor + 1];
        cursor += 3; // skip "XY "
        // Rename/copy can appear in EITHER column: X (staged) or Y (worktree).
        let is_rename =
            status_x == b'R' || status_x == b'C' || status_y == b'R' || status_y == b'C';

        // First path (ordinary: the path; rename/copy: the destination).
        let Some(path1) = next_nul_field(bytes, &mut cursor) else {
            return true; // truncated record — fail safe dirty
        };
        let path1_str = unquote(path1);

        if is_rename {
            // Second path (the source). Required for R/C records.
            let Some(path2) = next_nul_field(bytes, &mut cursor) else {
                return true; // truncated rename — fail safe dirty
            };
            let path2_str = unquote(path2);
            if path1_str.is_empty() || path2_str.is_empty() {
                return true; // malformed — fail safe dirty
            }
            if !is_all_ignored(&[path1_str, path2_str]) {
                return true;
            }
        } else if !path1_str.is_empty() {
            if !is_all_ignored(&[path1_str]) {
                return true;
            }
        } else {
            // Empty path in an ordinary record is malformed.
            return true;
        }
    }
    false
}

/// Return the bytes of the next NUL-terminated field, advancing `cursor`
/// past the NUL. Returns `None` if no NUL follows (truncated stream).
fn next_nul_field<'a>(bytes: &'a [u8], cursor: &mut usize) -> Option<&'a [u8]> {
    let start = *cursor;
    match bytes[*cursor..].iter().position(|&b| b == 0) {
        Some(offset) => {
            let field = &bytes[start..start + offset];
            *cursor = start + offset + 1; // skip past the NUL
            Some(field)
        }
        None => None,
    }
}

/// Strip surrounding double-quotes from a porcelain path (newline format
/// quotes paths with special chars; -z never quotes, but tolerate it).
fn unquote(field: &[u8]) -> &str {
    // Porcelain paths are UTF-8 (git stores paths as bytes but our transport
    // is a Rust String, so they were already validated as UTF-8 upstream).
    let s = std::str::from_utf8(field).unwrap_or("");
    s.strip_prefix('"')
        .and_then(|inner| inner.strip_suffix('"'))
        .unwrap_or(s)
}

/// Parse legacy newline-delimited porcelain v1 output.
///
/// Rename/copy records use the `old -> new` form. A record is ignored only if
/// ALL affected paths are under `.jefe/`/`.llxprt/`. Only split on ` -> ` for
/// records whose status begins with `R` or `C`, so that `?? ".jefe/foo -> bar"`
/// is treated as a single owned path (not a rename).
fn porcelain_is_dirty_newline(porcelain: &str) -> bool {
    for line in porcelain.lines() {
        let Some(paths) = newline_affected_paths(line) else {
            // Skip blank/garbage lines (not fail-safe dirty — a truly empty
            // line is normal between records in newline format).
            continue;
        };
        if !is_all_ignored(&paths) {
            return true;
        }
    }
    false
}

/// Extract all affected paths from a newline-delimited porcelain v1 line.
///
/// For a non-rename record (`XY <path>`), returns a single-element vec.
/// For a rename/copy record (status begins `R`/`C`), splits on ` -> ` and
/// returns BOTH old and new paths. Returns `None` for malformed/garbage
/// lines. Only R/C status records are split on ` -> `, so an untracked file
/// named `foo -> bar` is treated as a single path.
fn newline_affected_paths(line: &str) -> Option<Vec<&str>> {
    let bytes = line.as_bytes();
    // Porcelain v1 format: 2-char status + 1 space + path.
    if bytes.len() < 3 || bytes[2] != b' ' {
        return None;
    }
    let trimmed = line.trim_end();
    let rest = trimmed.get(3..)?;
    let is_rename = bytes[0] == b'R' || bytes[0] == b'C' || bytes[1] == b'R' || bytes[1] == b'C';
    if is_rename && let Some((old, new)) = rest.split_once(" -> ") {
        let old_unquoted = old.trim_matches('"');
        let new_unquoted = new.trim_matches('"');
        if old_unquoted.is_empty() || new_unquoted.is_empty() {
            return None;
        }
        return Some(vec![old_unquoted, new_unquoted]);
    }
    // Non-rename (or R/C without a ` -> `): single path.
    let unquoted = rest.trim_matches('"');
    (!unquoted.is_empty()).then(|| vec![unquoted])
}

/// True when ALL given paths are under an ignored (`.jefe/`/`.llxprt/`) prefix.
fn is_all_ignored(paths: &[&str]) -> bool {
    paths.iter().all(|path| {
        IGNORED_PREFIXES
            .iter()
            .any(|prefix| path.starts_with(prefix))
    })
}

///
/// Handles SSH (`git@github.com:owner/repo.git`), HTTPS
/// (`https://github.com/owner/repo.git`), and bare (`owner/repo`) forms.
/// Returns `None` when the origin remote is missing or the URL doesn't match
/// a known pattern.
fn detect_origin_shortform(work_dir: &Path) -> Option<String> {
    let output = run_git_with_timeout(
        git_command()?
            .arg("-C")
            .arg(work_dir)
            .args(["remote", "get-url", "origin"]),
        work_dir,
    )?;
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

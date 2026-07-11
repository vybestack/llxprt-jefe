//! Local-git working-copy primitives for issue-driven agent launches.
//!
//! This module holds the **local** git porcelain helpers used by the
//! target-aware orchestration in [`super::issue_prep`]:
//!
//! - resolving the repo's default branch (never hardcoding `main`/`master`),
//! - checking out and pulling that branch,
//! - detecting a dirty working copy (ignoring jefe/llxprt-owned metadata),
//! - and discarding working-copy changes when the user explicitly opts in.
//!
//! Clone identity validation lives in [`super::clone_identity`]; the
//! target-aware orchestration (local + remote) lives in
//! [`super::issue_prep`]. The porcelain-parsing and path-filtering logic is
//! split out as pure functions so it can be unit-tested without spawning
//! `git`.

use std::path::Path;
use std::process::{Command, Stdio};

/// Paths that jefe/llxprt own and that must never count as "dirty" working
/// copy state, nor be swept by cleanup. Matched as path *prefixes* against
/// the porcelain path column.
const IGNORED_PREFIXES: [&str; 2] = [".jefe/", ".llxprt/"];

/// Check whether `work_dir` exists and is a git working copy.
///
/// Uses `git -C <work_dir> rev-parse --is-inside-work-tree` instead of
/// checking for a `.git` entry. This correctly handles **linked worktrees**
/// where `.git` is a file (pointing to the parent's `.git/worktrees/...`),
/// not a directory.
pub(super) fn is_git_workdir(work_dir: &Path) -> bool {
    if !work_dir.is_dir() {
        return false;
    }
    let output = git_capture_optional(work_dir, ["rev-parse", "--is-inside-work-tree"]);
    match output {
        Some(out) => out.status.success() && String::from_utf8_lossy(&out.stdout).trim() == "true",
        None => false,
    }
}

/// Check whether `work_dir` exists at all (directory or otherwise).
pub(super) fn path_exists(work_dir: &Path) -> bool {
    work_dir.exists()
}

/// Ensure the agent working copy exists by cloning the repository if needed.
///
/// Returns `Ok(())` if the workdir is already a git repo (no clone needed) or
/// if the clone succeeds. Returns `Err` with a human-readable message if the
/// path exists but is not a git worktree, the clone fails, or no clone URL is
/// provided for a missing workdir.
///
/// Existing git working copies are left untouched (the caller's dirty-check
/// and prep flow handle them). Only a **missing** workdir triggers a clone;
/// an existing non-git directory fails safely.
pub(super) fn ensure_workdir_cloned(work_dir: &Path, clone_url: Option<&str>) -> PrepResult {
    if is_git_workdir(work_dir) {
        return Ok(());
    }
    if path_exists(work_dir) {
        return Err(format!(
            "Working copy {} exists but is not a git worktree.",
            work_dir.display()
        ));
    }
    let Some(url) = clone_url else {
        return Err(format!(
            "Working copy {} does not exist and no valid github_repo (owner/repo) is \
             configured to clone from.",
            work_dir.display()
        ));
    };
    clone_repository(work_dir, url)
}

/// Clone the repository into `work_dir` using the given clone URL.
///
/// Creates parent directories first (`create_dir_all` on the parent), then
/// runs `git clone <url> <work_dir>`. The clone lands on the remote's default
/// branch so the subsequent `prepare_issue_workdir` checkout+pull is a sync
/// rather than a branch switch.
fn clone_repository(work_dir: &Path, clone_url: &str) -> PrepResult {
    if let Some(parent) = work_dir.parent()
        && !parent.as_os_str().is_empty()
    {
        std::fs::create_dir_all(parent)
            .map_err(|e| format!("Failed to create parent directory for clone: {e}"))?;
    }
    // Normalize the clone working directory: when work_dir is a bare relative
    // path like "repo", parent() yields Some("") (not None). An empty CWD is
    // platform-dependent, so explicitly fall back to "." so git_capture always
    // receives a concrete directory.
    let clone_cwd = match work_dir.parent() {
        Some(p) if !p.as_os_str().is_empty() => p,
        _ => Path::new("."),
    };
    let output = git_capture(clone_cwd, ["clone", clone_url, &work_dir.to_string_lossy()])?;
    require_success(&output, &format!("git clone {clone_url}"))?;
    Ok(())
}

/// Outcome of preparing the working copy for an issue-driven launch.
///
/// `Ok(())` means the working copy is now on the default branch and clean
/// (modulo ignored jefe/llxprt paths) and the launch may proceed.
/// `Err(_)` carries a human-readable error for the user.
pub(super) type PrepResult = Result<(), String>;

/// Resolve the repository's default branch for the working copy at `work_dir`.
///
/// Uses `git symbolic-ref refs/remotes/origin/HEAD` (without `--short`), which
/// reflects whatever the remote advertises as its default branch (works for
/// `main`, `master`, `develop`, etc.) and prints the full ref
/// (`refs/remotes/origin/main`). The short branch name is extracted by
/// [`strip_remote_prefix`].
pub(super) fn resolve_default_branch(work_dir: &Path) -> Result<String, String> {
    let output = git_capture(work_dir, ["symbolic-ref", "refs/remotes/origin/HEAD"])?;
    let branch = String::from_utf8_lossy(&output.stdout).trim().to_owned();
    if branch.is_empty() {
        return Err(
            "Could not resolve the default branch (origin/HEAD is unset). \
             Run `git remote set-head origin -a` in the working copy."
                .to_owned(),
        );
    }
    let short = strip_remote_prefix(&branch).to_owned();
    // Reject anything that doesn't look like a valid git branch name.
    // This guards against option injection if a malicious remote sets
    // origin/HEAD to a value starting with `-`.
    if !is_valid_branch_name(&short) {
        return Err(format!(
            "Resolved default branch name {short:?} contains invalid characters"
        ));
    }
    Ok(short)
}

/// Validate a branch name against a conservative safe-character set.
///
/// Rejects names starting with `-` (option injection), containing spaces,
/// control characters, or path traversal sequences. This is intentionally
/// stricter than git's actual ref naming rules.
fn is_valid_branch_name(name: &str) -> bool {
    !name.is_empty()
        && !name.starts_with('-')
        && !name.contains("..")
        && name
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '/' | '_' | '-'))
}

/// Strip a `refs/remotes/origin/` prefix, returning the short branch name.
///
/// Pure helper so the transformation is unit-testable independently of git.
fn strip_remote_prefix(refname: &str) -> &str {
    const PREFIX: &str = "refs/remotes/origin/";
    refname.strip_prefix(PREFIX).unwrap_or(refname)
}

/// Return `true` when the working copy has uncommitted/untracked changes,
/// ignoring jefe/ and llxprt-owned paths.
pub(super) fn is_workdir_dirty(work_dir: &Path) -> Result<bool, String> {
    let output = git_capture(work_dir, ["status", "--porcelain=v1"])?;
    let porcelain = String::from_utf8_lossy(&output.stdout);
    Ok(porcelain_is_dirty(&porcelain))
}

/// Pure helper: given raw `git status --porcelain=v1` output, return `true`
/// when there is at least one non-ignored (i.e. real) change.
///
/// Exposed so the remote target path (which captures porcelain over SSH) can
/// reuse the exact same dirty-detection logic as the local path.
#[must_use]
pub(super) fn porcelain_is_dirty(porcelain: &str) -> bool {
    relevant_dirty_lines(porcelain).next().is_some()
}

/// Iterate the porcelain lines that represent real (non-ignored) changes.
///
/// Pure helper: given raw `git status --porcelain` output, yields only the
/// lines that parse to at least one valid path AND where not ALL affected
/// paths are under a jefe/llxprt-owned prefix. Blank/garbage lines are
/// skipped.
///
/// For rename/copy records (`R`/`C`), **both** old and new paths are
/// considered affected: a real→owned or owned→real rename is dirty. The
/// record is ignored only if ALL affected paths are under `.jefe/`/`.llxprt/`.
fn relevant_dirty_lines(porcelain: &str) -> impl Iterator<Item = &str> {
    porcelain
        .lines()
        .filter(|line| porcelain_affected_paths(line).is_some())
        .filter(|line| !is_ignored_porcelain_line(line))
}

/// Decide whether a single porcelain line refers only to ignored paths.
///
/// For rename/copy records, ALL affected paths (old and new) must be under
/// ignored prefixes. For non-rename records, the single path is checked.
fn is_ignored_porcelain_line(line: &str) -> bool {
    porcelain_affected_paths(line).is_some_and(|paths| {
        paths.iter().all(|path| {
            IGNORED_PREFIXES
                .iter()
                .any(|prefix| path.starts_with(prefix))
        })
    })
}

/// Extract all affected paths from a porcelain v1 line.
///
/// For a non-rename record (`XY <path>`), returns a single-element vec.
/// For a rename/copy record (`R  old -> new` or `C  old -> new`), returns
/// BOTH the old and new paths.
///
/// Returns `None` for malformed/garbage lines (status column missing,
/// path empty). All paths are unquoted.
fn porcelain_affected_paths(line: &str) -> Option<Vec<&str>> {
    let bytes = line.as_bytes();
    // Porcelain v1 format: 2-char status + 1 space + path.
    if bytes.len() < 3 || bytes[2] != b' ' {
        return None;
    }
    let trimmed = line.trim_end();
    let rest = trimmed.get(3..)?;
    if let Some((old, new)) = rest.split_once(" -> ") {
        // Rename/copy: both old and new paths are affected.
        let old_unquoted = old.trim_matches('"');
        let new_unquoted = new.trim_matches('"');
        if old_unquoted.is_empty() || new_unquoted.is_empty() {
            return None;
        }
        Some(vec![old_unquoted, new_unquoted])
    } else {
        // Non-rename: single path.
        let unquoted = rest.trim_matches('"');
        (!unquoted.is_empty()).then(|| vec![unquoted])
    }
}

/// Check out `branch` in the working copy at the latest remote state.
///
/// First `git fetch origin <branch>` to update the remote-tracking ref, then
/// `git checkout -B <branch> origin/<branch>` to force-reset the local branch
/// to the remote-tracking ref. This avoids `git pull`, which can trigger an
/// interactive merge or leave conflict markers if the remote advances between
/// fetch and merge.
///
/// # Precondition
///
/// `branch` must be validated by [`is_valid_branch_name`] (called from
/// [`resolve_default_branch`]) before being passed here, to prevent option
/// injection.
fn checkout_and_pull(work_dir: &Path, branch: &str) -> Result<(), String> {
    debug_assert!(
        is_valid_branch_name(branch),
        "branch must be validated by resolve_default_branch before calling checkout_and_pull"
    );
    // Fetch first so origin/<branch> is up to date, then checkout -B resets
    // the local branch to the fetched remote-tracking ref. No `git pull` —
    // it can trigger a merge or conflict markers in an automated flow.
    git_require_success(work_dir, ["fetch", "origin", branch])?;
    let remote_ref = format!("origin/{branch}");
    // The `--` disambiguates the following args (none here) from pathspecs.
    let checkout_result = git_capture(work_dir, ["checkout", "-B", branch, &remote_ref, "--"]);
    match checkout_result {
        Ok(output) if output.status.success() => Ok(()),
        Ok(output) => {
            // checkout failed — could be a linked worktree where the branch
            // is already checked out in the primary worktree. Only reset
            // --hard if the worktree is CURRENTLY on the desired default
            // branch; otherwise resetting would move the wrong branch ref
            // and risk discarding commits on an unrelated branch.
            let stderr = String::from_utf8_lossy(&output.stderr);
            // Locale-independent: git_capture sets LC_ALL=C, so this English
            // fragment is reliable regardless of the user's locale.
            if stderr.contains("already used by worktree") {
                let current = current_branch_name(work_dir)?;
                if current == branch {
                    git_require_success(work_dir, ["reset", "--hard", &remote_ref])
                } else {
                    Err(format!(
                        "Cannot reset to {remote_ref}: worktree is on branch '{current}', \
                         not the default '{branch}' (branch is locked to another worktree)."
                    ))
                }
            } else {
                Err(format!(
                    "git checkout -B {branch} {remote_ref} -- failed: {stderr}"
                ))
            }
        }
        Err(e) => Err(e),
    }
}

/// Resolve the current branch name of the working copy at `work_dir`.
///
/// Used by [`checkout_and_pull`] to guard the linked-worktree fallback so
/// `reset --hard` only runs when the worktree is already on the desired
/// default branch.
fn current_branch_name(work_dir: &Path) -> Result<String, String> {
    let output = git_capture(work_dir, ["rev-parse", "--abbrev-ref", "HEAD"])?;
    let name = String::from_utf8_lossy(&output.stdout).trim().to_owned();
    if name.is_empty() || name == "HEAD" {
        return Err(format!(
            "Working copy {} is in a detached HEAD state; cannot determine current branch",
            work_dir.display()
        ));
    }
    Ok(name)
}

/// Prepare the working copy for a fresh issue-driven launch: resolve the
/// default branch, check it out, and pull. Does **not** touch uncommitted
/// changes — callers must gate on [`is_workdir_dirty`] first.
pub(super) fn prepare_issue_workdir(work_dir: &Path) -> PrepResult {
    let branch = resolve_default_branch(work_dir)?;
    checkout_and_pull(work_dir, &branch)
}

/// Discard uncommitted/untracked changes in the working copy, preserving
/// jefe/ and llxprt-owned paths and all `.gitignore`-ed files.
///
/// Runs `git clean -fd` first (remove untracked files, respecting
/// `.gitignore`) then `git reset --hard` (discard tracked changes). Running
/// `clean` first means if it fails, the user's tracked modifications are
/// still intact — only untracked files would have been affected. Exclusions
/// for `.jefe/` and `.llxprt/` are derived from [`IGNORED_PREFIXES`].
///
/// Does **not** use `-x`, so gitignored files like `.env`, `node_modules/`,
/// and build artifacts are preserved.
///
/// # Non-atomicity
///
/// These two operations are not atomic. If `reset --hard` fails after
/// `clean -fd` has already removed untracked files, those untracked files
/// are gone. The user has explicitly confirmed this destructive operation
/// via the `ConfirmIssueDirtyCopy` modal.
///
/// # Limitation: tracked `.jefe/` files
///
/// While `git clean -e` prevents removal of *untracked* `.jefe/` files,
/// `git reset --hard` will revert any *tracked* `.jefe/` files (e.g.,
/// committed metadata that was locally modified) to their committed state.
/// In practice this is not an issue because the issue prompt file is
/// written fresh after cleanup and is never tracked by git.
pub(super) fn discard_workdir_changes(work_dir: &Path) -> Result<(), String> {
    // Build clean exclusion args from IGNORED_PREFIXES so the porcelain
    // dirty-check and the cleanup step can never drift. Each prefix is
    // added twice: as the directory itself (`.jefe/`) and as a glob for
    // nested contents (`.jefe/**`).
    let mut clean_args: Vec<String> = vec!["clean".into(), "-fd".into()];
    for prefix in IGNORED_PREFIXES {
        clean_args.push("-e".into());
        clean_args.push(prefix.into());
        let glob = format!("{prefix}**");
        clean_args.push("-e".into());
        clean_args.push(glob);
    }
    let output = git_capture(work_dir, &clean_args)?;
    // Check exit status before logging so we don't report a partial/failed
    // clean as if it succeeded.
    require_success(&output, "clean -fd")?;
    // Log what was removed so the destructive operation is auditable.
    let removed = String::from_utf8_lossy(&output.stdout);
    if !removed.trim().is_empty() {
        tracing::info!(
            work_dir = %work_dir.display(),
            removed = %removed.trim(),
            "discard_workdir_changes: git clean removed paths"
        );
    }
    // Now discard tracked modifications. Running after clean so if clean
    // fails, tracked changes are still intact.
    git_require_success(work_dir, ["reset", "--hard"])?;
    Ok(())
}

/// Run `git` with the given args in `work_dir`, capturing output. Returns an
/// error string on spawn failure (does not inspect exit status).
fn git_capture<I, S>(work_dir: &Path, args: I) -> Result<std::process::Output, String>
where
    I: IntoIterator<Item = S>,
    S: AsRef<std::ffi::OsStr>,
{
    Command::new("git")
        .current_dir(work_dir)
        // Fail fast instead of hanging on an interactive credential prompt
        // for private repos over HTTPS (stdout/stderr are piped, not a TTY).
        .env("GIT_TERMINAL_PROMPT", "0")
        // Force C locale so git emits English messages — stderr is parsed for
        // the linked-worktree fallback in checkout_and_pull, and localized
        // messages would break the contains() check.
        .env("LC_ALL", "C")
        .args(args)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .map_err(|error| format!("failed to run git: {error}"))
}

/// Run `git` with the given args in `work_dir`, returning `None` on any
/// failure (spawn failure or non-zero exit). Used by predicates like
/// [`is_git_workdir`] that must not propagate errors.
fn git_capture_optional<I, S>(work_dir: &Path, args: I) -> Option<std::process::Output>
where
    I: IntoIterator<Item = S>,
    S: AsRef<std::ffi::OsStr>,
{
    git_capture(work_dir, args).ok()
}

/// Run `git` and require a successful exit status, surfacing stderr on failure.
///
/// The context string for error messages is auto-derived from the args so it
/// can never drift from the actual command.
fn git_require_success<I, S>(work_dir: &Path, args: I) -> Result<(), String>
where
    I: IntoIterator<Item = S>,
    S: AsRef<std::ffi::OsStr>,
{
    let args_vec: Vec<S> = args.into_iter().collect();
    let context = args_vec
        .iter()
        .map(|a| a.as_ref().to_string_lossy())
        .collect::<Vec<_>>()
        .join(" ");
    let output = git_capture(work_dir, &args_vec)?;
    require_success(&output, &format!("git {context}"))
}

/// Inspect a captured `git` output and return `Err` with stderr/stdout detail
/// when the exit status was non-zero.
fn require_success(output: &std::process::Output, context: &str) -> Result<(), String> {
    if output.status.success() {
        Ok(())
    } else {
        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_owned();
        let stdout = String::from_utf8_lossy(&output.stdout).trim().to_owned();
        let detail = if !stderr.is_empty() {
            stderr
        } else if !stdout.is_empty() {
            stdout
        } else {
            format!("exit status {}", output.status)
        };
        Err(format!("git {context} failed: {detail}"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn strips_remote_prefix_from_symbolic_ref_output() {
        assert_eq!(strip_remote_prefix("refs/remotes/origin/main"), "main");
        assert_eq!(strip_remote_prefix("refs/remotes/origin/master"), "master");
        // Already-short names pass through unchanged.
        assert_eq!(strip_remote_prefix("trunk"), "trunk");
    }

    #[test]
    fn clean_porcelain_is_not_dirty() {
        assert!(!is_dirty_from(""));
        assert!(!is_dirty_from("\n\n  \n"));
    }

    #[test]
    fn untracked_source_file_is_dirty() {
        assert!(is_dirty_from("?? src/lib.rs\n"));
    }

    #[test]
    fn modified_tracked_file_is_dirty() {
        assert!(is_dirty_from(" M Cargo.toml\n"));
    }

    #[test]
    fn only_jefe_paths_are_not_dirty() {
        assert!(!is_dirty_from("?? .jefe/issue-prompt.md\n"));
        assert!(!is_dirty_from(" M .jefe/something\n"));
    }

    #[test]
    fn only_llxprt_paths_are_not_dirty() {
        assert!(!is_dirty_from("?? .llxprt/LLXPRT.md\n"));
        assert!(!is_dirty_from(" M .llxprt/session.json\n"));
    }

    #[test]
    fn jefe_plus_real_change_is_dirty() {
        let porcelain = "?? .jefe/issue-prompt.md\n M src/main.rs\n";
        assert!(is_dirty_from(porcelain));
    }

    #[test]
    fn rename_real_to_ignored_is_dirty() {
        // A real→owned rename is dirty: old path (old.txt) is NOT under
        // .jefe/.llxprt, so the rename affects a real file.
        assert!(is_dirty_from("R  old.txt -> .jefe/x.md\n"));
    }

    #[test]
    fn rename_ignored_to_real_is_dirty() {
        // An owned→real rename is dirty: new path (src/new.txt) is real.
        assert!(is_dirty_from("R  .jefe/old.md -> src/new.txt\n"));
    }

    #[test]
    fn rename_ignored_to_ignored_is_not_dirty() {
        // Only when BOTH paths are under ignored prefixes is it not dirty.
        assert!(!is_dirty_from("R  .jefe/old.md -> .jefe/new.md\n"));
        assert!(!is_dirty_from("R  .llxprt/a -> .jefe/b\n"));
    }

    #[test]
    fn rename_real_to_real_is_dirty() {
        assert!(is_dirty_from("R  src/old.txt -> src/new.txt\n"));
    }

    #[test]
    fn rename_to_real_path_is_dirty() {
        assert!(is_dirty_from("R  old.txt -> src/new.txt\n"));
    }

    #[test]
    fn copy_real_to_ignored_is_dirty() {
        // Copy records (C) follow the same rule as renames.
        assert!(is_dirty_from("C  old.txt -> .jefe/x.md\n"));
    }

    #[test]
    fn copy_ignored_to_real_is_dirty() {
        // Owned→real copy affects a real path → dirty.
        assert!(is_dirty_from("C  .jefe/old.md -> src/new.txt\n"));
    }

    #[test]
    fn copy_real_to_real_is_dirty() {
        assert!(is_dirty_from("C  src/a.txt -> src/b.txt\n"));
    }

    #[test]
    fn copy_ignored_to_ignored_is_not_dirty() {
        assert!(!is_dirty_from("C  .jefe/a -> .jefe/b\n"));
    }

    #[test]
    fn rename_with_quoted_paths_both_directions() {
        // Quoted paths in both directions.
        assert!(is_dirty_from("R  \"src/old.txt\" -> \".jefe/new.md\"\n"));
        assert!(is_dirty_from("R  \".jefe/old.md\" -> \"src/new.txt\"\n"));
        assert!(!is_dirty_from("R  \".jefe/old.md\" -> \".jefe/new.md\"\n"));
    }

    #[test]
    fn quoted_paths_are_handled() {
        assert!(is_dirty_from("?? \"src/weird name.rs\"\n"));
        assert!(!is_dirty_from("?? \".jefe/weird name.md\"\n"));
    }

    #[test]
    fn copy_with_quoted_paths_both_directions() {
        // Quoted paths for copy records in both directions.
        assert!(is_dirty_from("C  \"src/old.txt\" -> \".jefe/new.md\"\n"));
        assert!(is_dirty_from("C  \".jefe/old.md\" -> \"src/new.txt\"\n"));
        assert!(!is_dirty_from("C  \".jefe/old.md\" -> \".jefe/new.md\"\n"));
    }

    #[test]
    fn rename_with_status_xy_prefixes_both_dirty() {
        // Rename with various XY status prefixes (RM, RA, etc.).
        assert!(is_dirty_from("RM src/old.txt -> src/new.txt\n"));
        assert!(is_dirty_from("RA src/old.txt -> src/new.txt\n"));
    }

    /// Helper: evaluate dirtiness from raw porcelain text via the exported
    /// `porcelain_is_dirty` wrapper so tests exercise the same public API
    /// production code uses.
    fn is_dirty_from(porcelain: &str) -> bool {
        porcelain_is_dirty(porcelain)
    }
}

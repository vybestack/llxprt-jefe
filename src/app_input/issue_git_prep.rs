//! Git working-copy preparation for issue-driven agent launches.
//!
//! Before sending a GitHub issue to an agent, Jefe must ensure the agent's
//! working copy starts from a clean, up-to-date checkout of the repository's
//! default branch. This module encapsulates that preparation:
//!
//! - resolving the repo's default branch (never hardcoding `main`/`master`),
//! - checking out and pulling that branch,
//! - detecting a dirty working copy (ignoring jefe/llxprt-owned metadata),
//! - and discarding working-copy changes when the user explicitly opts in.
//!
//! The porcelain-parsing and path-filtering logic is split out as pure
//! functions so it can be unit-tested without spawning `git`.

use std::path::Path;
use std::process::{Command, Stdio};

/// Paths that jefe/llxprt own and that must never count as "dirty" working
/// copy state, nor be swept by cleanup. Matched as path *prefixes* against
/// the porcelain path column.
const IGNORED_PREFIXES: [&str; 2] = [".jefe/", ".llxprt/"];

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
    Ok(relevant_dirty_lines(&porcelain).next().is_some())
}

/// Iterate the porcelain lines that represent real (non-ignored) changes.
///
/// Pure helper: given raw `git status --porcelain` output, yields only the
/// lines that parse to a valid path AND whose path is not under a
/// jefe/llxprt-owned prefix. Blank/garbage lines are skipped.
fn relevant_dirty_lines(porcelain: &str) -> impl Iterator<Item = &str> {
    porcelain
        .lines()
        .filter(|line| porcelain_path(line).is_some())
        .filter(|line| !is_ignored_porcelain_line(line))
}

/// Decide whether a single porcelain line refers only to ignored paths.
fn is_ignored_porcelain_line(line: &str) -> bool {
    porcelain_path(line).is_some_and(|path| {
        IGNORED_PREFIXES
            .iter()
            .any(|prefix| path.starts_with(prefix))
    })
}

/// Extract the affected path from a porcelain v1 line.
///
/// Format is `XY <path>` (optionally quoted/renamed). We split on the fixed
/// two-column status prefix and take the remainder, trimming a trailing
/// rename (`->`) to the post-rename path.
fn porcelain_path(line: &str) -> Option<&str> {
    let bytes = line.as_bytes();
    // Porcelain v1 format: 2-char status + 1 space + path. Guard the
    // separator so a malformed line is skipped rather than mis-parsed.
    if bytes.len() < 3 || bytes[2] != b' ' {
        return None;
    }
    let trimmed = line.trim_end();
    // Skip the 2-char status + 1 space. Use `get` so a non-char-boundary
    // slice (malformed/multi-byte line) returns `None` instead of panicking.
    let rest = trimmed.get(3..)?;
    // For renames (`R  old -> new`) report the new path (after the first
    // ` -> `). Using split_once (not rsplit) handles filenames that
    // themselves contain ` -> `.
    let path = rest.split_once(" -> ").map_or(rest, |(_, new)| new);
    let unquoted = path.trim_matches('"');
    (!unquoted.is_empty()).then_some(unquoted)
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
    // Fetch first so origin/<branch> is up to date, then checkout -B resets
    // the local branch to the fetched remote-tracking ref. No `git pull` —
    // it can trigger a merge or conflict markers in an automated flow.
    git_require_success(
        work_dir,
        ["fetch", "origin", branch, "--"],
        &format!("fetch origin {branch}"),
    )?;
    let remote_ref = format!("origin/{branch}");
    // The `--` disambiguates the following args (none here) from pathspecs.
    git_require_success(
        work_dir,
        ["checkout", "-B", branch, &remote_ref, "--"],
        &format!("checkout -B {branch}"),
    )?;
    Ok(())
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
    let clean_refs: Vec<&str> = clean_args.iter().map(String::as_str).collect();
    let output = git_capture(work_dir, &clean_refs)?;
    // Log what was removed so the destructive operation is auditable.
    let removed = String::from_utf8_lossy(&output.stdout);
    if !removed.trim().is_empty() {
        tracing::info!(
            work_dir = %work_dir.display(),
            removed = %removed.trim(),
            "discard_workdir_changes: git clean removed paths"
        );
    }
    require_success(&output, "clean -fd")?;
    // Now discard tracked modifications. Running after clean so if clean
    // fails, tracked changes are still intact.
    git_require_success(work_dir, ["reset", "--hard"], "reset --hard")?;
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
        .args(args)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .map_err(|error| format!("failed to run git: {error}"))
}

/// Run `git` and require a successful exit status, surfacing stderr on failure.
fn git_require_success<I, S>(work_dir: &Path, args: I, context: &str) -> Result<(), String>
where
    I: IntoIterator<Item = S>,
    S: AsRef<std::ffi::OsStr>,
{
    let output = git_capture(work_dir, args)?;
    require_success(&output, context)
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
    fn rename_to_ignored_path_is_not_dirty() {
        // Porcelain rename: `R  old -> .jefe/x` should be ignored (new path).
        assert!(!is_dirty_from("R  old.txt -> .jefe/x.md\n"));
    }

    #[test]
    fn rename_to_real_path_is_dirty() {
        assert!(is_dirty_from("R  old.txt -> src/new.txt\n"));
    }

    #[test]
    fn quoted_paths_are_handled() {
        assert!(is_dirty_from("?? \"src/weird name.rs\"\n"));
        assert!(!is_dirty_from("?? \".jefe/weird name.md\"\n"));
    }

    /// Helper: evaluate dirtiness from raw porcelain text.
    fn is_dirty_from(porcelain: &str) -> bool {
        relevant_dirty_lines(porcelain).next().is_some()
    }
}

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
use std::process::Stdio;

#[path = "issue_cleanup.rs"]
mod issue_cleanup;
/// Safety guards for the destructive force-reclone path (issue #190).
///
/// Re-exported here so callers can reference
/// `super::issue_git_prep::validate_reclone_target` regardless of the internal
/// module split.
#[path = "reclone_safety.rs"]
mod reclone_safety;
pub(super) use issue_cleanup::discard_workdir_changes;
pub(super) use reclone_safety::validate_reclone_target;

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
    let assurance = ensure_workdir_with_origin(work_dir, clone_url, None)?;
    // With no expected shortform, OriginMismatch cannot occur: the inner
    // function only emits it when a shortform was provided for comparison.
    // Match exhaustively so a future caller adding a shortform here is a
    // compile error rather than a silently-discarded mismatch, and return a
    // descriptive error (never a runtime panic) if the invariant is somehow
    // violated.
    match assurance {
        WorkdirAssurance::Ready | WorkdirAssurance::JustCloned => Ok(()),
        WorkdirAssurance::OriginMismatch { .. } => Err(
            "OriginMismatch requires an expected shortform, which ensure_workdir_cloned never passes"
                .to_owned(),
        ),
    }
}

/// Zero-sized proof token that the caller has obtained explicit user
/// confirmation for a destructive workdir replacement.
///
/// Constructing this token is the single sanctioned way to authorize
/// [`remove_workdir`]: the only constructor ([`ConfirmedReclone::confirmed`])
/// is a private `pub(super)` item, so callers outside this module cannot
/// fabricate one. This makes the "user must confirm before destruction"
/// contract a compile-time guarantee rather than a doc comment that a future
/// caller could silently bypass.
#[derive(Debug, Clone, Copy)]
pub(super) struct ConfirmedReclone(());

impl ConfirmedReclone {
    /// Produce the confirmation token. `pub(super)` so only the force-reclone
    /// orchestration (which runs after the `ConfirmIssueOriginMismatch` modal)
    /// can mint one.
    #[must_use]
    pub(super) fn confirmed() -> Self {
        Self(())
    }
}

/// Remove the working copy directory entirely (for the force-reclone path).
///
/// This is destructive: it deletes the entire `work_dir` and all its
/// contents. The required [`ConfirmedReclone`] token is the compile-time
/// guarantee that the caller has already obtained explicit user confirmation
/// (via the `ConfirmIssueOriginMismatch` modal) — the token cannot be
/// constructed outside this module. The parameter is named without an
/// underscore prefix deliberately: its presence (not its value) IS the
/// safety contract, so an underscore would misleadingly suggest it is unused.
pub(super) fn remove_workdir(work_dir: &Path, confirmed: ConfirmedReclone) -> Result<(), String> {
    // The token's presence at the call site is the proof; it is not read
    // here. Binding it by name (then discarding) avoids an unused-variable
    // warning while keeping the parameter name (and thus the contract)
    // self-documenting.
    let _ = confirmed;
    std::fs::remove_dir_all(work_dir)
        .map_err(|e| format!("Failed to remove workdir {}: {e}", work_dir.display()))
}

/// Outcome of ensuring a working copy exists with an optional origin check.
///
/// Distinguishes three states so the caller can decide whether to proceed,
/// skip a fresh-clone's dirty check, or prompt the user before clobbering a
/// foreign repository.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum WorkdirAssurance {
    /// Already a git repo with a matching origin (or no expected shortform to
    /// compare against). Proceed with the normal prep flow.
    Ready,
    /// Was missing and has just been cloned. Proceed (the clone lands on the
    /// remote's default branch, so the origin already matches by
    /// construction).
    JustCloned,
    /// Exists and is a git repo, but the origin does not match the configured
    /// repository. The caller must prompt the user before any destructive
    /// action.
    OriginMismatch { actual: String, expected: String },
}

/// Ensure the agent working copy exists, optionally checking that its `origin`
/// remote matches the configured repository.
///
/// Returns:
/// - [`WorkdirAssurance::Ready`] when `work_dir` is already a git repo and
///   either no `expected_shortform` was supplied or the origin matches.
/// - [`WorkdirAssurance::JustCloned`] when `work_dir` was missing and has been
///   freshly cloned (origin matches by construction).
/// - [`WorkdirAssurance::OriginMismatch`] when `work_dir` is a git repo but its
///   origin does not match `expected_shortform` (or has no `origin` remote at
///   all while an expected shortform IS configured).
///
/// # Errors
///
/// Returns `Err` with a human-readable message if the path exists but is not a
/// git worktree, the clone fails, or no clone URL is provided for a missing
/// workdir.
pub(super) fn ensure_workdir_with_origin(
    work_dir: &Path,
    clone_url: Option<&str>,
    expected_shortform: Option<&str>,
) -> Result<WorkdirAssurance, String> {
    if is_git_workdir(work_dir) {
        if let Some(expected) = expected_shortform {
            let expected = expected.trim();
            match origin_raw_url(work_dir) {
                Some(raw_url) if origins_match(&raw_url, expected) => Ok(WorkdirAssurance::Ready),
                Some(raw_url) => {
                    // Display the normalized owner/repo when it parses, else
                    // the raw URL — so a malformed/unexpected origin (not just
                    // a missing one) surfaces a diagnosable actual value
                    // rather than an empty string indistinguishable from
                    // "no origin".
                    let actual = jefe::git_info::origin_display_shortform(&raw_url)
                        .filter(|s| !s.is_empty())
                        .unwrap_or_else(|| raw_url.clone());
                    Ok(WorkdirAssurance::OriginMismatch {
                        actual,
                        expected: expected.to_owned(),
                    })
                }
                // A git repo with no `origin` remote while an expected
                // shortform IS configured is a mismatch — it is not the
                // configured repository.
                None => Ok(WorkdirAssurance::OriginMismatch {
                    actual: String::new(),
                    expected: expected.to_owned(),
                }),
            }
        } else {
            Ok(WorkdirAssurance::Ready)
        }
    } else if path_exists(work_dir) {
        Err(format!(
            "Working copy {} exists but is not a git worktree.",
            work_dir.display()
        ))
    } else {
        let Some(url) = clone_url else {
            return Err(format!(
                "Working copy {} does not exist and no valid github_repo (owner/repo) is \
                 configured to clone from.",
                work_dir.display()
            ));
        };
        clone_repository(work_dir, url)?;
        Ok(WorkdirAssurance::JustCloned)
    }
}

/// Read the raw `origin` remote URL of the working copy at `work_dir`.
///
/// Runs `git -C <work_dir> remote get-url origin`, trims the output, and
/// returns the **raw** URL (not normalized). The host-aware comparison in
/// [`origins_match`] needs the raw URL to verify the host is GitHub.
/// Returns `None` on git failure or when the URL is empty.
pub(super) fn origin_raw_url(work_dir: &Path) -> Option<String> {
    let output = git_capture_optional(work_dir, ["remote", "get-url", "origin"])?;
    if !output.status.success() {
        return None;
    }
    let url = String::from_utf8_lossy(&output.stdout).trim().to_owned();
    if url.is_empty() {
        return None;
    }
    Some(url)
}

/// The expected GitHub host for origin-mismatch comparison. The configured
/// clone identity always uses `github.com` (see `CloneIdentity::clone_url`).
const EXPECTED_GITHUB_HOST: &str = "github.com";

/// Host-aware predicate: verify the actual origin URL is on GitHub
/// (`github.com`) and its `owner/repo` matches the expected shortform.
///
/// This is the **central safety check** for issue #190: it prevents operating
/// against a foreign repository (GitLab, attacker host) that happens to share
/// the same `owner/repo` path as the configured GitHub repository.
///
/// The comparison:
/// - Parses the raw actual URL via [`jefe::git_info::parse_repository_origin`],
///   which preserves the **lowercased host**.
/// - Requires the host to be exactly `github.com` (case-insensitive, already
///   lowercased by the parser).
/// - Requires the `owner/repo` to match `expected` **case-insensitively**,
///   because GitHub repository identity (`owner/repo`) is ASCII
///   case-insensitive — `Acme/Widgets` and `acme/widgets` refer to the same
///   repository. Comparing case-sensitively would surface false
///   mismatch prompts and could encourage users to approve unnecessary
///   destructive replacements.
///
/// # Fail-closed behavior
///
/// A bare `owner/repo` actual (no host) is a **mismatch**: a real GitHub
/// clone always has a host in its `origin` URL, so a bare form means the URL
/// was manually set or is from an unknown source. Empty/malformed actuals are
/// also mismatches.
#[must_use]
pub(super) fn origins_match(actual_raw_url: &str, expected: &str) -> bool {
    let expected = expected.trim();
    if expected.is_empty() {
        return false;
    }
    let Some(parsed) = jefe::git_info::parse_repository_origin(actual_raw_url) else {
        return false;
    };
    parsed.host == EXPECTED_GITHUB_HOST && same_owner_repo(&parsed.owner_repo, expected)
}

/// Compare two `owner/repo` shortforms case-insensitively (ASCII).
///
/// GitHub repository identity is case-insensitive, so `Acme/Widgets` matches
/// `acme/widgets`. Kept as a named helper so the comparison policy is explicit
/// and unit-testable in isolation.
fn same_owner_repo(a: &str, b: &str) -> bool {
    a.eq_ignore_ascii_case(b)
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
///
/// Uses `git status --porcelain=v1 -z` (NUL-delimited output) so paths
/// containing newlines or ` -> ` are handled correctly. Delegates to the
/// shared [`jefe::git_info::porcelain_is_dirty`] parser so the ignore
/// semantics are identical between the display cache and the issue-prep
/// orchestration.
pub(super) fn is_workdir_dirty(work_dir: &Path) -> Result<bool, String> {
    let output = git_capture(work_dir, ["status", "--porcelain=v1", "-z"])?;
    // NUL is valid UTF-8 (U+0000), so from_utf8_lossy preserves embedded NULs.
    let porcelain = String::from_utf8_lossy(&output.stdout);
    Ok(jefe::git_info::porcelain_is_dirty(&porcelain))
}

/// Re-export the shared porcelain parser so callers within `app_input` that
/// previously referenced `issue_git_prep::porcelain_is_dirty` continue to
/// resolve without source changes.
pub(super) use jefe::git_info::porcelain_is_dirty;

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

/// Run `git` with the given args in `work_dir`, capturing output. Returns an
/// error string on spawn failure (does not inspect exit status).
pub(super) fn git_capture<I, S>(work_dir: &Path, args: I) -> Result<std::process::Output, String>
where
    I: IntoIterator<Item = S>,
    S: AsRef<std::ffi::OsStr>,
{
    jefe::local_command::command(jefe::local_command::LocalTool::Git)
        .map_err(|error| error.to_string())?
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
pub(super) fn require_success(output: &std::process::Output, context: &str) -> Result<(), String> {
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

    // ── origins_match: host-aware predicate (issue #190 MUST-FIX #3) ───

    #[test]
    fn origins_match_accepts_github_ssh() {
        assert!(origins_match(
            "git@github.com:acme/widgets.git",
            "acme/widgets"
        ));
    }

    #[test]
    fn origins_match_accepts_github_https() {
        assert!(origins_match(
            "https://github.com/acme/widgets.git",
            "acme/widgets"
        ));
    }

    #[test]
    fn origins_match_rejects_gitlab_host() {
        assert!(!origins_match(
            "https://gitlab.com/acme/widgets.git",
            "acme/widgets"
        ));
    }

    #[test]
    fn origins_match_rejects_attacker_host() {
        assert!(!origins_match(
            "git@attacker.example:acme/widgets.git",
            "acme/widgets"
        ));
    }

    #[test]
    fn origins_match_rejects_different_owner() {
        assert!(!origins_match(
            "git@github.com:other/widgets.git",
            "acme/widgets"
        ));
    }

    #[test]
    fn origins_match_rejects_different_repo() {
        assert!(!origins_match(
            "git@github.com:acme/gadgets.git",
            "acme/widgets"
        ));
    }

    #[test]
    fn origins_match_rejects_bare_form_no_host() {
        // A bare owner/repo with no host is fail-closed: we cannot verify
        // it is GitHub, so it must NOT match.
        assert!(!origins_match("acme/widgets", "acme/widgets"));
    }

    #[test]
    fn origins_match_rejects_empty_actual() {
        assert!(!origins_match("", "acme/widgets"));
    }

    #[test]
    fn origins_match_rejects_file_scheme_with_github_authority() {
        // A file:// URL is a different transport — it reads the local
        // filesystem, NOT github.com. It must NOT match even though the
        // authority syntactically reads github.com.
        assert!(!origins_match(
            "file://github.com/acme/widgets.git",
            "acme/widgets"
        ));
    }

    #[test]
    fn origins_match_rejects_unknown_scheme_with_github_authority() {
        // Git supports pluggable remote helpers for arbitrary schemes. An
        // unknown scheme like ftp:// or myhelper:// cannot be trusted to
        // target GitHub even when the host matches.
        assert!(!origins_match(
            "ftp://github.com/acme/widgets.git",
            "acme/widgets"
        ));
        assert!(!origins_match(
            "myhelper://github.com/acme/widgets.git",
            "acme/widgets"
        ));
    }

    #[test]
    fn origins_match_accepts_ssh_scheme() {
        assert!(origins_match(
            "ssh://git@github.com/acme/widgets.git",
            "acme/widgets"
        ));
    }

    #[test]
    fn origins_match_accepts_git_scheme() {
        assert!(origins_match(
            "git://github.com/acme/widgets.git",
            "acme/widgets"
        ));
    }

    #[test]
    fn origins_match_case_insensitive_owner_repo() {
        // GitHub repository identity is ASCII case-insensitive: Acme/Widgets
        // and acme/widgets refer to the same repository. A mixed-case
        // configured value must NOT surface a false mismatch prompt.
        assert!(origins_match(
            "https://github.com/acme/widgets.git",
            "Acme/Widgets"
        ));
        assert!(origins_match(
            "git@github.com:Acme/Widgets.git",
            "acme/widgets"
        ));
    }

    #[test]
    fn origins_match_rejects_malformed_actual() {
        assert!(!origins_match("garbage", "acme/widgets"));
    }

    #[test]
    fn origins_match_accepts_uppercase_host() {
        assert!(origins_match(
            "https://GitHub.COM/acme/widgets.git",
            "acme/widgets"
        ));
    }
}

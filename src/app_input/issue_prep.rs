//! Target-aware working-copy preparation for issue-driven agent launches.
//!
//! All working-copy prep (git clone/checkout/reset/clean, `.jefe` creation,
//! issue-prompt writing) executes on the **same target** where the
//! `LaunchSignature` runs:
//!
//! - **Local** (`remote.enabled` false): local git + filesystem.
//! - **Remote** (`remote.enabled` true): noninteractive SSH (`ssh -T`) using
//!   `RemoteRepositorySettings.login_user`/`host`/`run_as_user`. Prompt bytes
//!   are transferred via stdin, never shell interpolation. The git boundary is
//!   the remote host, never `RuntimeManager` (which owns tmux/PTY only).
//!
//! One orchestration drives both `Stop` and `Discard` dirty policies and both
//! local/remote targets, so the issue-send and dirty-confirm paths share an
//! identical sequence.
//!
//! # Sequence
//!
//! 1. Detect a valid git worktree at `work_dir`.
//! 2. If the path is **absent**, clone using the validated HTTPS identity.
//! 3. If the path **exists but is not a git worktree**, fail safely.
//! 4. Check dirty status (ignoring `.jefe/`/`.llxprt/`).
//! 5. `Stop` policy: return `Dirty` without altering the worktree.
//! 6. `Discard` policy: clean after confirmation (reset --hard + clean -fd).
//! 7. Resolve `origin/HEAD`, fetch, checkout/reset the default branch.
//! 8. Create `.jefe/` and write the issue prompt **last**.
//!
//! No app/runtime state locks are held during prep: prep runs before the
//! launch path takes any lock.

use std::path::Path;

use jefe::domain::RemoteRepositorySettings;

use super::clone_identity::CloneIdentity;
use super::issue_git_prep::{
    WorkdirAssurance, discard_workdir_changes, ensure_workdir_cloned, ensure_workdir_with_origin,
    is_workdir_dirty, prepare_issue_workdir, remove_workdir,
};

/// Relative path of the issue prompt inside the work dir. This is the single
/// source of truth shared by the instruction-string construction in
/// `issues_send::prepare_issue_launch_signature` and the on-disk prompt write
/// in this module.
pub(super) const ISSUE_PROMPT_RELATIVE_PATH: &str = ".jefe/issue-prompt.md";

/// Policy for handling a dirty working copy during issue-send prep.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum DirtyPolicy {
    /// Initial send: return [`PrepOutcome::Dirty`] without touching the
    /// worktree so the caller can open the confirm modal.
    Stop,
    /// After user confirmation: discard uncommitted/untracked changes
    /// (preserving `.jefe/`/`.llxprt/`) then proceed.
    Discard,
}

/// Outcome of target-aware prep.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum PrepOutcome {
    /// The working copy is prepared and the prompt is written; launch may
    /// proceed.
    Ready,
    /// The working copy is dirty and the policy is `Stop`. The worktree is
    /// untouched; the caller should open the dirty-copy confirm modal.
    Dirty,
    /// The working copy is a git repo whose `origin` does not match the
    /// configured repository. The caller must open the origin-mismatch
    /// confirm modal.
    OriginMismatch { actual: String, expected: String },
}

/// Where prep operations execute.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum WorkTarget {
    /// Local git/filesystem.
    Local,
    /// Remote host via noninteractive SSH.
    Remote(RemoteRepositorySettings),
}

impl WorkTarget {
    /// Resolve the target from remote settings via the shared validated
    /// contract in [`crate::domain::target`].
    ///
    /// **Deprecated**: this method silently falls back to `Local` for an
    /// enabled-but-incomplete remote. Production code MUST use
    /// [`super::target_resolution::resolve_target`] instead, which returns
    /// an `Err`. Retained for the existing `WorkTarget` unit tests.
    #[must_use]
    #[cfg(test)]
    pub(super) fn from_remote(remote: &RemoteRepositorySettings) -> Self {
        if jefe::domain::target::is_valid_remote(remote) {
            Self::Remote(remote.clone())
        } else {
            Self::Local
        }
    }
}

/// Prepare the working copy for an issue-driven launch on the resolved target.
///
/// This is the single orchestration shared by the initial send (`Stop`) and
/// the dirty-confirm path (`Discard`), for both local and remote targets.
/// Returns `Ready` when the worktree is on the default branch and the prompt
/// is written, `Dirty` when the policy is `Stop` and uncommitted changes were
/// detected.
///
/// # Errors
///
/// Returns a human-readable error string for any failure (missing clone
/// identity, clone failure, non-git directory, git command failure, prompt
/// write failure, remote SSH failure). The caller surfaces it as
/// `SendToAgentFailed`.
pub(super) fn prepare_issue_target(
    target: &WorkTarget,
    work_dir: &Path,
    identity: Option<&CloneIdentity>,
    policy: DirtyPolicy,
    prompt: &str,
) -> Result<PrepOutcome, String> {
    match target {
        WorkTarget::Local => prepare_local(work_dir, identity, policy, prompt),
        WorkTarget::Remote(remote) => prepare_remote(remote, work_dir, identity, policy, prompt),
    }
}

/// Force-reclone a mismatched working copy, then proceed with normal
/// post-clone prep.
///
/// This is the opt-in action for the origin-mismatch confirm modal (issue
/// #190). It removes the mismatched workdir entirely and re-clones from the
/// configured identity, then runs the post-clone prep (no dirty-check — a
/// fresh clone is clean). The caller must have obtained explicit user
/// confirmation before invoking this.
///
/// **Ordering invariant (MUST-FIX #2):** the identity is a **required**
/// parameter (not `Option`), so the clone URL is resolved BEFORE the workdir
/// is removed. The removal can never happen without a valid replacement URL.
///
/// # Errors
///
/// Returns a human-readable error string if the remove, clone, or prep fails.
pub(super) fn prepare_issue_target_force_reclone(
    target: &WorkTarget,
    work_dir: &Path,
    identity: &CloneIdentity,
    prompt: &str,
) -> Result<PrepOutcome, String> {
    match target {
        WorkTarget::Local => prepare_local_force_reclone(work_dir, identity, prompt),
        WorkTarget::Remote(remote) => {
            prepare_remote_force_reclone(remote, work_dir, identity, prompt)
        }
    }
}

/// Local force-reclone: resolve clone URL → remove → clone → post-clone prep.
///
/// **Ordering invariant (MUST-FIX #2):** the clone URL is resolved from the
/// required `identity` BEFORE the workdir is removed. Since `identity` is a
/// non-optional `&CloneIdentity`, removal can never happen without a valid
/// replacement URL — the old bug (destroy then fail with "no identity") is
/// impossible by construction.
fn prepare_local_force_reclone(
    work_dir: &Path,
    identity: &CloneIdentity,
    prompt: &str,
) -> Result<PrepOutcome, String> {
    // 1. Resolve the clone URL BEFORE any destructive action.
    let clone_url = identity.clone_url();
    force_reclone_local_with_url(work_dir, &clone_url, prompt)
}

/// The destructive force-reclone sequence with an already-resolved clone URL:
/// remove the workdir → clone → post-clone prep.
///
/// Split from [`prepare_local_force_reclone`] so the sequence (remove → clone
/// → prep) is exercisable in tests against a local clone source (a bare repo
/// path), independent of the HTTPS-only `CloneIdentity::clone_url`. Production
/// always enters via [`prepare_local_force_reclone`], which resolves the URL
/// from a validated identity first — guaranteeing the URL is known before the
/// destructive removal.
pub(super) fn force_reclone_local_with_url(
    work_dir: &Path,
    clone_url: &str,
    prompt: &str,
) -> Result<PrepOutcome, String> {
    // Remove the mismatched workdir. The confirmation token is the
    // compile-time guarantee that the user confirmed via the modal; this
    // function is only reached from confirm_issue_origin_mismatch_enter.
    if work_dir.exists() {
        remove_workdir(
            work_dir,
            super::issue_git_prep::ConfirmedReclone::confirmed(),
        )?;
    }
    // Clone from the resolved URL.
    ensure_workdir_cloned(work_dir, Some(clone_url))?;
    // Post-clone prep (dirty-check is Stop, but a fresh clone is clean).
    run_local_policy_and_prep(work_dir, DirtyPolicy::Stop, prompt)
}

/// Local-target prep sequence.
fn prepare_local(
    work_dir: &Path,
    identity: Option<&CloneIdentity>,
    policy: DirtyPolicy,
    prompt: &str,
) -> Result<PrepOutcome, String> {
    let owned_url = identity.map(CloneIdentity::clone_url);
    let expected = identity.map(CloneIdentity::expected_shortform);
    match ensure_workdir_with_origin(work_dir, owned_url.as_deref(), expected)? {
        WorkdirAssurance::Ready | WorkdirAssurance::JustCloned => {}
        WorkdirAssurance::OriginMismatch { actual, expected } => {
            return Ok(PrepOutcome::OriginMismatch { actual, expected });
        }
    }
    run_local_policy_and_prep(work_dir, policy, prompt)
}

/// Shared local sequence after the worktree exists: dirty check → policy →
/// prep → prompt write.
fn run_local_policy_and_prep(
    work_dir: &Path,
    policy: DirtyPolicy,
    prompt: &str,
) -> Result<PrepOutcome, String> {
    if is_workdir_dirty(work_dir)? {
        match policy {
            DirtyPolicy::Stop => return Ok(PrepOutcome::Dirty),
            DirtyPolicy::Discard => discard_workdir_changes(work_dir)?,
        }
    }
    prepare_issue_workdir(work_dir)?;
    write_prompt_local(work_dir, prompt)?;
    Ok(PrepOutcome::Ready)
}

/// Write the issue prompt to the local filesystem.
fn write_prompt_local(work_dir: &Path, prompt: &str) -> Result<(), String> {
    let prompt_path = work_dir.join(ISSUE_PROMPT_RELATIVE_PATH);
    std::fs::create_dir_all(work_dir.join(".jefe"))
        .map_err(|e| format!("Failed to create .jefe dir: {e}"))?;
    std::fs::write(&prompt_path, prompt).map_err(|e| format!("Failed to write issue prompt: {e}"))
}

// ──────────────────────────────────────────────────────────────────────────
// Reusable safe target prompt writer (shared by issue + PR prep)
// ──────────────────────────────────────────────────────────────────────────

/// Validate that a prompt relative path is safe: it must start with `.jefe/`,
/// be relative (no leading `/`), and contain no path-traversal components
/// (`..`). This prevents absolute-path injection and directory traversal when
/// the path is joined with the work dir or interpolated into a remote shell.
fn validate_prompt_relative_path(relative_path: &str) -> Result<(), String> {
    if !relative_path.starts_with(".jefe/") {
        return Err(format!(
            "Prompt path {relative_path:?} must start with '.jefe/'"
        ));
    }
    if relative_path.starts_with('/') {
        return Err(format!(
            "Prompt path {relative_path:?} must be relative, not absolute"
        ));
    }
    if Path::new(relative_path)
        .components()
        .any(|c| matches!(c, std::path::Component::ParentDir))
    {
        return Err(format!(
            "Prompt path {relative_path:?} must not contain '..' traversal"
        ));
    }
    Ok(())
}

/// Write a prompt file to the selected [`WorkTarget`] at a given relative
/// path, transferring prompt bytes via stdin for remote targets (never shell
/// interpolation).
///
/// This is the **safe target prompt writer** extracted from the issue-prep
/// path so the PR-prep path can reuse the exact same local/remote write logic
/// without duplicating SSH plumbing.
///
/// - **Local**: creates `{work_dir}/.jefe` if needed and writes
///   `{work_dir}/{relative_path}` directly via `std::fs::write`.
/// - **Remote**: runs `ssh -T` with `mkdir -p .jefe; cat > {path}`, piping
///   prompt bytes via stdin. The relative path must start with `.jefe/`.
///
/// The `jefe_dir` (parent of the relative path) is created via `mkdir -p`.
/// This does NOT add issue-style clone/dirty/default-branch semantics — it is
/// purely a prompt-file write.
pub(super) fn write_prompt_to_target(
    target: &WorkTarget,
    work_dir: &Path,
    relative_path: &str,
    prompt: &str,
) -> Result<(), String> {
    validate_prompt_relative_path(relative_path)?;
    match target {
        WorkTarget::Local => write_prompt_local_generic(work_dir, relative_path, prompt),
        WorkTarget::Remote(remote) => {
            write_prompt_remote_generic(remote, work_dir, relative_path, prompt.as_bytes())
        }
    }
}

/// Write a prompt file to the local filesystem at a given relative path.
/// Creates the parent `.jefe` directory first.
fn write_prompt_local_generic(
    work_dir: &Path,
    relative_path: &str,
    prompt: &str,
) -> Result<(), String> {
    let prompt_path = work_dir.join(relative_path);
    let jefe_dir = work_dir.join(".jefe");
    std::fs::create_dir_all(&jefe_dir).map_err(|e| format!("Failed to create .jefe dir: {e}"))?;
    std::fs::write(&prompt_path, prompt).map_err(|e| format!("Failed to write prompt: {e}"))
}

/// Write a prompt file to a remote host via `ssh -T`, piping prompt bytes
/// through stdin (never shell interpolation).
fn write_prompt_remote_generic(
    remote: &RemoteRepositorySettings,
    work_dir: &Path,
    relative_path: &str,
    prompt_bytes: &[u8],
) -> Result<(), String> {
    let runner = remote::RemotePrepRunner::new(remote.clone());
    runner.write_prompt(work_dir, relative_path, prompt_bytes)
}

// ──────────────────────────────────────────────────────────────────────────
// Remote target prep
// ──────────────────────────────────────────────────────────────────────────

/// Prepare the working copy on a remote host via noninteractive SSH.
///
/// Uses `ssh -T` (no PTY) for all git/file operations — distinct from the
/// `-tt` tmux operations in `runtime::commands`. The prompt bytes are
/// transferred via stdin, never interpolated into the shell command.
///
/// This delegates to [`RemotePrepRunner`] for the actual SSH execution. For
/// deterministic testing, command planning is exercised via
/// [`RemotePrepPlanner`] which records the planned commands without executing
/// them.
fn prepare_remote(
    remote: &RemoteRepositorySettings,
    work_dir: &Path,
    identity: Option<&CloneIdentity>,
    policy: DirtyPolicy,
    prompt: &str,
) -> Result<PrepOutcome, String> {
    let runner = remote::RemotePrepRunner::new(remote.clone());
    runner.run(work_dir, identity, policy, prompt)
}

/// Remote force-reclone: validate identity → resolve URL → remove → clone → prep over SSH.
///
/// **Ordering invariant (MUST-FIX #2):** the identity is required
/// (non-optional), so the clone URL is resolved BEFORE the `rm -rf`.
fn prepare_remote_force_reclone(
    remote: &RemoteRepositorySettings,
    work_dir: &Path,
    identity: &CloneIdentity,
    prompt: &str,
) -> Result<PrepOutcome, String> {
    let runner = remote::RemotePrepRunner::new(remote.clone());
    runner.run_force_reclone(work_dir, identity, prompt)
}

#[path = "issue_prep_remote.rs"]
mod remote;

#[cfg(test)]
#[path = "issue_prep_tests.rs"]
mod tests;

#[cfg(test)]
#[path = "issue_prep_predicate_tests.rs"]
mod predicate_tests;

#[cfg(test)]
pub(super) use remote::{classify_origin_url_output, classify_predicate_output, wrap_predicate};

//! Target-aware working-copy preparation for issue-driven agent launches.
//!
//! All working-copy prep (git clone/checkout/reset/clean) executes on the
//! **same target** where the `LaunchSignature` runs:
//!
//! - **Local** (`remote.enabled` false): local git + filesystem.
//! - **Remote** (`remote.enabled` true): noninteractive SSH (`ssh -T`) using
//!   `RemoteRepositorySettings.login_user`/`host`/`run_as_user`. The git
//!   boundary is the remote host, never `RuntimeManager` (which owns
//!   tmux/PTY only).
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
//!
//! The issue/PR prompt content is inlined directly into the launch
//! instruction (issue #315); no `.jefe/` file is written.
//!
//! No app/runtime state locks are held during prep: prep runs before the
//! launch path takes any lock.

use std::path::Path;

use jefe::domain::RemoteRepositorySettings;

use super::clone_identity::CloneIdentity;
use super::issue_git_prep::{
    WorkdirAssurance, WorkdirPrepOutcome, ensure_workdir_cloned, ensure_workdir_with_origin,
    is_on_default_branch, is_workdir_dirty, prepare_issue_workdir, remove_workdir,
};

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
/// This is the single orchestration shared by the initial send and the
/// transient send paths, for both local and remote targets. Returns `Ready`
/// when the worktree is on the default branch. Returns `Dirty` when
/// uncommitted changes were detected (or the worktree is clean but not on the
/// default branch) — the caller must open the confirm modal in that case.
///
/// Issue #479: the confirm path no longer discards changes in-place; it now
/// force-reclones the entire workdir via [`prepare_issue_target_force_reclone`].
/// This initial prep therefore only ever needs to detect the dirty/not-on-
/// default state and surface it; there is no "discard and proceed" policy.
///
/// Issue #315: the prompt content is now inlined directly into the launch
/// instruction (`-i`), so prep no longer writes a `.jefe/issue-prompt.md`
/// file.
///
/// # Errors
///
/// Returns a human-readable error string for any failure (missing clone
/// identity, clone failure, non-git directory, git command failure, remote
/// SSH failure). The caller surfaces it as `SendToAgentFailed`.
pub(super) fn prepare_issue_target(
    target: &WorkTarget,
    work_dir: &Path,
    identity: Option<&CloneIdentity>,
) -> Result<PrepOutcome, String> {
    match target {
        WorkTarget::Local => prepare_local(work_dir, identity),
        WorkTarget::Remote(remote) => prepare_remote(remote, work_dir, identity),
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
) -> Result<PrepOutcome, String> {
    match target {
        WorkTarget::Local => prepare_local_force_reclone(work_dir, identity),
        WorkTarget::Remote(remote) => prepare_remote_force_reclone(remote, work_dir, identity),
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
) -> Result<PrepOutcome, String> {
    // 1. Resolve the clone URL BEFORE any destructive action.
    let clone_url = identity.clone_url();
    force_reclone_local_with_url(work_dir, &clone_url)
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
) -> Result<PrepOutcome, String> {
    // Defense-in-depth: refuse catastrophic targets (root, empty, top-level)
    // even though the user confirmed. This guards against a misconfigured
    // work_dir reaching `rm -rf`.
    super::issue_git_prep::validate_reclone_target(work_dir)?;
    // Remove the mismatched workdir. The confirmation token is the
    // compile-time guarantee that the user confirmed via the modal; this
    // function is only reached from confirm_issue_origin_mismatch_enter.
    if work_dir.exists() {
        remove_workdir(
            work_dir,
            super::issue_git_prep::ConfirmedReclone::confirmed(),
        )?;
    }
    // Any failure from here (clone or prep) occurs AFTER the original workdir
    // has been destroyed. Annotate the error so the user understands their
    // data is already gone and what step failed, rather than seeing a bare
    // clone/prep error that hides the destruction.
    ensure_workdir_cloned(work_dir, Some(clone_url))
        .map_err(|e| format!("After removing the mismatched work_dir, the clone failed (the original working copy at {} is already gone): {e}", work_dir.display()))?;
    run_local_prep(work_dir)
        .map_err(|e| format!("After force-recloning {} (the original working copy is already gone), post-clone prep failed: {e}", work_dir.display()))
}

/// Local-target prep sequence.
fn prepare_local(work_dir: &Path, identity: Option<&CloneIdentity>) -> Result<PrepOutcome, String> {
    jefe::services::validate_local_path(work_dir)?;
    let owned_url = identity.map(CloneIdentity::clone_url);
    let expected = identity.map(CloneIdentity::expected_shortform);
    match ensure_workdir_with_origin(work_dir, owned_url.as_deref(), expected)? {
        WorkdirAssurance::Ready | WorkdirAssurance::JustCloned => {}
        WorkdirAssurance::OriginMismatch { actual, expected } => {
            return Ok(PrepOutcome::OriginMismatch { actual, expected });
        }
    }
    run_local_prep(work_dir)
}

/// Shared local sequence after the worktree exists: dirty check → prep.
///
/// Issue #338: a clean working copy that is **not on the default branch**
/// also triggers the confirm modal — silently switching branches is
/// surprising. A dirty tree or a non-default branch both surface
/// [`PrepOutcome::Dirty`] so the caller opens the confirm modal; the confirm
/// path then force-reclones (issue #479), so there is no in-place discard
/// here.
fn run_local_prep(work_dir: &Path) -> Result<PrepOutcome, String> {
    let dirty = is_workdir_dirty(work_dir)?;
    // Only evaluate branch position when clean: a dirty tree triggers the
    // modal regardless of which branch it is on.
    let not_on_default = if dirty {
        false
    } else {
        !is_on_default_branch(work_dir)?
    };
    if dirty || not_on_default {
        return Ok(PrepOutcome::Dirty);
    }
    match prepare_issue_workdir(work_dir)? {
        WorkdirPrepOutcome::Ready => {}
        WorkdirPrepOutcome::CheckoutBlockedByLocalChanges => {
            // Local changes blocking checkout are themselves "dirty" — surface
            // Dirty so the confirm modal's force-reclone can replace the
            // workdir rather than in-place discarding tracked changes.
            return Ok(PrepOutcome::Dirty);
        }
    }
    Ok(PrepOutcome::Ready)
}

// ──────────────────────────────────────────────────────────────────────────
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
) -> Result<PrepOutcome, String> {
    let runner = remote::RemotePrepRunner::new(remote.clone());
    runner.run(work_dir, identity)
}

/// Remote force-reclone: validate identity → resolve URL → remove → clone → prep over SSH.
///
/// **Ordering invariant (MUST-FIX #2):** the identity is required
/// (non-optional), so the clone URL is resolved BEFORE the `rm -rf`.
fn prepare_remote_force_reclone(
    remote: &RemoteRepositorySettings,
    work_dir: &Path,
    identity: &CloneIdentity,
) -> Result<PrepOutcome, String> {
    let runner = remote::RemotePrepRunner::new(remote.clone());
    runner.run_force_reclone(work_dir, identity)
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

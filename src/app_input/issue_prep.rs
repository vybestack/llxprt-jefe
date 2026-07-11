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
use std::process::{Command, Stdio};

use jefe::domain::RemoteRepositorySettings;

use super::clone_identity::CloneIdentity;
use super::issue_git_prep::{
    discard_workdir_changes, ensure_workdir_cloned, is_workdir_dirty, prepare_issue_workdir,
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

/// Local-target prep sequence.
fn prepare_local(
    work_dir: &Path,
    identity: Option<&CloneIdentity>,
    policy: DirtyPolicy,
    prompt: &str,
) -> Result<PrepOutcome, String> {
    let owned_url = identity.map(CloneIdentity::clone_url);
    ensure_workdir_cloned(work_dir, owned_url.as_deref())?;
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
    let runner = RemotePrepRunner::new(remote.clone());
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
    let runner = RemotePrepRunner::new(remote.clone());
    runner.run(work_dir, identity, policy, prompt)
}

/// A pure planner that records the remote commands and prompt-transfer plan
/// **without** executing them. Exposed for deterministic tests proving all
/// operations target the remote host, use `ssh -T`, and transfer prompt bytes
/// via stdin.
#[cfg(test)]
#[derive(Debug, Clone)]
pub(super) struct RemotePrepPlanner {
    remote: RemoteRepositorySettings,
}

/// Inputs driving the pure remote-command planner. Bundled into a struct so
/// the planner signature stays under the project's argument-count limit
/// (`clippy::too_many_arguments`).
#[cfg(test)]
#[derive(Debug, Clone)]
pub(super) struct PlanInputs<'a> {
    /// Work dir the clone/checkout targets.
    pub work_dir: &'a Path,
    /// Validated clone identity (HTTPS URL), if any.
    pub identity: Option<&'a CloneIdentity>,
    /// Dirty-copy handling policy.
    pub policy: DirtyPolicy,
    /// The remote work dir already exists and is a git worktree.
    pub exists_is_git: bool,
    /// The remote work dir exists but is **not** a git worktree.
    pub exists_not_git: bool,
    /// The remote work dir is dirty (meaningful only when `exists_is_git`).
    pub is_dirty: bool,
    /// Prompt bytes to transfer via stdin.
    pub prompt: &'a str,
}

/// A single recorded remote operation (for test verification).
#[cfg(test)]
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct PlannedRemoteOp {
    /// The `ssh -T` argv (everything after the `ssh` binary).
    pub ssh_argv: Vec<String>,
    /// Prompt bytes sent via stdin, if this op transfers the prompt.
    pub stdin_prompt: Option<String>,
}

#[cfg(test)]
impl RemotePrepPlanner {
    /// Create a planner for the given remote settings.
    #[must_use]
    pub(super) fn new(remote: RemoteRepositorySettings) -> Self {
        Self { remote }
    }

    /// Plan the full remote prep sequence for the given state, returning the
    /// ordered list of `ssh -T` operations that would be executed.
    ///
    /// The sequence mirrors the local prep:
    /// 1. detect git worktree (if not, clone if identity present);
    /// 2. dirty check;
    /// 3. if dirty and Discard, reset+clean;
    /// 4. resolve default branch, fetch, checkout;
    /// 5. mkdir .jefe + write prompt via stdin.
    ///
    /// This is pure — it does not inspect the remote filesystem. Callers
    /// supply `exists_is_git`, `exists_not_git`, and `is_dirty` to drive the
    /// branching deterministically.
    #[must_use]
    pub(super) fn plan(&self, inputs: &PlanInputs<'_>) -> Vec<PlannedRemoteOp> {
        let mut ops = Vec::new();
        let escaped_work = shell_escape(&inputs.work_dir.to_string_lossy());
        let PlanInputs {
            work_dir,
            identity,
            policy,
            exists_is_git,
            exists_not_git,
            is_dirty,
            prompt,
        } = inputs;

        // 1. Clone if missing.
        if !*exists_is_git && !*exists_not_git {
            // Path absent → clone if identity present.
            if let Some(id) = identity {
                let url = id.clone_url();
                let script = format!(
                    "set -e; {mkdir_parent} git clone -- {url} {escaped_work}",
                    mkdir_parent = mkdir_parent_for(work_dir),
                    url = shell_escape(&url),
                );
                ops.push(self.wrapped_ssh_op(&script, None));
            }
        }

        // 2. Dirty check.
        if *exists_is_git || !*exists_not_git {
            // After clone the worktree is clean; only check dirty when it
            // pre-existed as a git worktree.
            if *exists_is_git && *is_dirty {
                match policy {
                    DirtyPolicy::Stop => {
                        // Stop: no further ops. The caller opens the confirm
                        // modal; no reset/clean is planned.
                        return ops;
                    }
                    DirtyPolicy::Discard => {
                        // Discard: reset --hard + clean -fd with exclusions.
                        let script = format!(
                            "set -e; cd {escaped_work}; \
                             git reset --hard; \
                             git clean -fd -e {jefe} -e {jefe_glob} -e {llx} -e {llx_glob}",
                            jefe = shell_escape(".jefe/"),
                            jefe_glob = shell_escape(".jefe/**"),
                            llx = shell_escape(".llxprt/"),
                            llx_glob = shell_escape(".llxprt/**"),
                        );
                        ops.push(self.wrapped_ssh_op(&script, None));
                    }
                }
            }

            // 4. Resolve default branch + fetch + checkout. The linked-worktree
            // fallback resets only when the worktree is on the desired branch.
            let script = format!(
                "set -e; cd {escaped_work}; \
                 branch=$(git symbolic-ref refs/remotes/origin/HEAD | sed 's@^refs/remotes/origin/@@'); \
                 git fetch origin \"$branch\"; \
                 if git checkout -B \"$branch\" \"origin/$branch\" -- 2>/dev/null; then \
                     :; \
                 elif [ \"$(git rev-parse --abbrev-ref HEAD)\" = \"$branch\" ]; then \
                     git reset --hard \"origin/$branch\"; \
                 else \
                     echo \"Cannot reset to origin/$branch: worktree is on a different branch\" >&2; \
                     exit 1; \
                 fi",
            );
            ops.push(self.wrapped_ssh_op(&script, None));

            // 5. mkdir .jefe + write prompt via stdin (cat > file).
            let prompt_path = format!("{escaped_work}/{ISSUE_PROMPT_RELATIVE_PATH}");
            let script = format!(
                "set -e; mkdir -p {jefe_dir}; cat > {prompt_path}",
                jefe_dir = shell_escape(&work_dir.join(".jefe").to_string_lossy()),
                prompt_path = prompt_path,
            );
            ops.push(self.wrapped_ssh_op(&script, Some((*prompt).to_owned())));
        }

        ops
    }

    /// Build a single planned `ssh -T` operation.
    fn ssh_op(&self, remote_command: &str, stdin_prompt: Option<String>) -> PlannedRemoteOp {
        let ssh_argv = self.ssh_argv(remote_command);
        PlannedRemoteOp {
            ssh_argv,
            stdin_prompt,
        }
    }

    /// Build a planned op from an unwrapped script, applying the effective-user
    /// wrapper first (binds the owned String so the borrow is valid).
    fn wrapped_ssh_op(&self, script: &str, stdin_prompt: Option<String>) -> PlannedRemoteOp {
        let wrapped = wrap_effective_user(&self.remote, script);
        self.ssh_op(&wrapped, stdin_prompt)
    }

    /// Build the `ssh -T` argv for a remote command.
    fn ssh_argv(&self, remote_command: &str) -> Vec<String> {
        vec![
            "-o".to_owned(),
            "BatchMode=yes".to_owned(),
            "-o".to_owned(),
            "ConnectTimeout=10".to_owned(),
            // -T: disable PTY allocation for noninteractive prep/file
            // transfer. This is distinct from the -tt used for tmux
            // operations in runtime::commands.
            "-T".to_owned(),
            // `--` ends option parsing (defense in depth; identity
            // validation is the primary guard).
            "--".to_owned(),
            format!(
                "{}@{}",
                self.remote.login_user.trim(),
                self.remote.host.trim()
            ),
            remote_command.to_owned(),
        ]
    }
}

/// Sentinel printed by a remote predicate probe when the condition is true.
const PREDICATE_TRUE: &str = "JEFE_PREDICATE_TRUE";
/// Sentinel printed by a remote predicate probe when the condition is false.
const PREDICATE_FALSE: &str = "JEFE_PREDICATE_FALSE";

/// Classify the raw output of a sentinel-based remote predicate probe.
///
/// The probe script must always print exactly one sentinel
/// (`JEFE_PREDICATE_TRUE` or `JEFE_PREDICATE_FALSE`) and exit 0. This pure
/// classifier enforces the protocol:
///
/// - Exit 0 with trimmed stdout exactly `JEFE_PREDICATE_TRUE` → `Ok(true)`.
/// - Exit 0 with trimmed stdout exactly `JEFE_PREDICATE_FALSE` → `Ok(false)`
///   (a safe, normal false predicate — NOT an error).
/// - SSH exit 255 → `Err` (transport/auth/host failure).
/// - Any other nonzero exit → `Err` (sudo/shell/auth failure).
/// - Exit 0 with prefix/suffix/both sentinels/empty/malformed output → `Err`
///   (protocol mismatch or banner injection — never cause a clone).
///
/// This is **fail-closed**: any ambiguity is an `Err`, so infrastructure
/// failures never masquerade as a safe `false` predicate.
fn classify_predicate_output(
    exit_code: Option<i32>,
    stdout: &str,
    stderr: &str,
) -> Result<bool, String> {
    match exit_code {
        Some(0) => {
            let trimmed = stdout.trim();
            if trimmed == PREDICATE_TRUE {
                Ok(true)
            } else if trimmed == PREDICATE_FALSE {
                Ok(false)
            } else {
                Err(format!(
                    "remote predicate returned unexpected output (expected exactly one sentinel): \
                     stdout={stdout:?} stderr={stderr:?}"
                ))
            }
        }
        Some(255) => Err(format!(
            "SSH transport/auth/host failure (exit 255): {}",
            stderr.trim()
        )),
        Some(code) => Err(format!(
            "remote predicate probe failed (exit {code}): {}",
            stderr.trim()
        )),
        None => Err("remote predicate probe terminated by signal".to_owned()),
    }
}

/// Wrap a condition command in the sentinel protocol so it always exits 0
/// (shell success) after printing exactly one sentinel.
///
/// The wrapped script runs `<condition>` and prints `JEFE_PREDICATE_TRUE`
/// when it succeeds or `JEFE_PREDICATE_FALSE` when it fails — in both cases
/// the script itself exits 0. This lets the caller distinguish a legitimate
/// false predicate from an infrastructure failure via [`classify_predicate_output`].
fn wrap_predicate(condition: &str) -> String {
    format!(
        "{{ {condition}; }} && printf '%s' {sentinel_true} || printf '%s' {sentinel_false}",
        sentinel_true = shell_escape(PREDICATE_TRUE),
        sentinel_false = shell_escape(PREDICATE_FALSE),
    )
}

/// The live SSH runner. Executes the planned sequence against the real remote.
struct RemotePrepRunner {
    remote: RemoteRepositorySettings,
}

impl RemotePrepRunner {
    fn new(remote: RemoteRepositorySettings) -> Self {
        Self { remote }
    }

    /// Execute the remote prep sequence.
    ///
    /// Determined at runtime by querying the remote host: detect whether the
    /// work dir is a git worktree, then branch accordingly.
    fn run(
        &self,
        work_dir: &Path,
        identity: Option<&CloneIdentity>,
        policy: DirtyPolicy,
        prompt: &str,
    ) -> Result<PrepOutcome, String> {
        let escaped_work = shell_escape(&work_dir.to_string_lossy());

        // 1. Detect git worktree on the remote. Use `git -C rev-parse
        // --is-inside-work-tree` instead of `test -d .git` to support linked
        // worktrees where `.git` is a file, not a directory.
        let exists = self.run_remote_check(&format!("test -d {escaped_work}"))?;
        if exists {
            let is_git = self.run_remote_check(&format!(
                "git -C {escaped_work} rev-parse --is-inside-work-tree 2>/dev/null | grep -qx true"
            ))?;
            if !is_git {
                return Err(format!(
                    "Remote path {} exists but is not a git worktree",
                    work_dir.display()
                ));
            }
        } else {
            // 2. Clone if missing.
            let Some(id) = identity else {
                return Err(format!(
                    "Remote working copy {} does not exist and no valid github_repo \
                     (owner/repo) is configured to clone from.",
                    work_dir.display()
                ));
            };
            let url = id.clone_url();
            let script = format!(
                "set -e; {mkdir_parent} git clone -- {url} {escaped_work}",
                mkdir_parent = mkdir_parent_for(work_dir),
                url = shell_escape(&url),
            );
            self.run_wrapped(&script)?;
        }

        // 3. Dirty check (only meaningful if the worktree pre-existed).
        let dirty_script = format!("cd {escaped_work}; git status --porcelain=v1");
        let porcelain = self.run_wrapped_capture(&dirty_script)?;
        let dirty = super::issue_git_prep::porcelain_is_dirty(&porcelain);

        if dirty {
            match policy {
                DirtyPolicy::Stop => return Ok(PrepOutcome::Dirty),
                DirtyPolicy::Discard => {
                    let script = format!(
                        "set -e; cd {escaped_work}; \
                         git reset --hard; \
                         git clean -fd -e {jefe} -e {jefe_glob} -e {llx} -e {llx_glob}",
                        jefe = shell_escape(".jefe/"),
                        jefe_glob = shell_escape(".jefe/**"),
                        llx = shell_escape(".llxprt/"),
                        llx_glob = shell_escape(".llxprt/**"),
                    );
                    self.run_wrapped(&script)?;
                }
            }
        }

        // 4. Resolve default branch + fetch + checkout. The checkout uses a
        // fallback to reset --hard ONLY when the worktree is already on the
        // desired default branch (linked worktrees cannot check out a branch
        // already used elsewhere). If the worktree is on a different branch,
        // reset --hard would move the wrong branch ref — so we fail with a
        // clear error instead.
        let script = format!(
            "set -e; cd {escaped_work}; \
             branch=$(git symbolic-ref refs/remotes/origin/HEAD | sed 's@^refs/remotes/origin/@@'); \
             git fetch origin \"$branch\"; \
             if git checkout -B \"$branch\" \"origin/$branch\" -- 2>/dev/null; then \
                 :; \
             elif [ \"$(git rev-parse --abbrev-ref HEAD)\" = \"$branch\" ]; then \
                 git reset --hard \"origin/$branch\"; \
             else \
                 echo \"Cannot reset to origin/$branch: worktree is on a different branch\" >&2; \
                 exit 1; \
             fi",
        );
        self.run_wrapped(&script)?;

        // 5. mkdir .jefe + write prompt via stdin.
        let jefe_dir = shell_escape(&work_dir.join(".jefe").to_string_lossy());
        let prompt_path =
            shell_escape(&work_dir.join(ISSUE_PROMPT_RELATIVE_PATH).to_string_lossy());
        let script = format!("set -e; mkdir -p {jefe_dir}; cat > {prompt_path}");
        self.run_wrapped_stdin(&script, prompt.as_bytes())?;

        Ok(PrepOutcome::Ready)
    }

    /// Write a prompt file to the remote host at `work_dir/{relative_path}`
    /// via `ssh -T`, piping prompt bytes through stdin.
    ///
    /// This is the reusable remote write used by [`write_prompt_to_target`]
    /// for both issue and PR prompts. It does NOT clone, check dirty, or
    /// switch branches — it only creates `.jefe/` and writes the file.
    fn write_prompt(
        &self,
        work_dir: &Path,
        relative_path: &str,
        prompt_bytes: &[u8],
    ) -> Result<(), String> {
        let jefe_dir = shell_escape(&work_dir.join(".jefe").to_string_lossy());
        let prompt_path = shell_escape(&work_dir.join(relative_path).to_string_lossy());
        let script = format!("set -e; mkdir -p {jefe_dir}; cat > {prompt_path}");
        self.run_wrapped_stdin(&script, prompt_bytes)
    }

    /// Run a wrapped (effective-user) remote command requiring success.
    fn run_wrapped(&self, script: &str) -> Result<(), String> {
        let wrapped = wrap_effective_user(&self.remote, script);
        self.run_remote(&wrapped)
    }

    /// Run a wrapped remote command and capture stdout.
    fn run_wrapped_capture(&self, script: &str) -> Result<String, String> {
        let wrapped = wrap_effective_user(&self.remote, script);
        self.run_remote_capture(&wrapped)
    }

    /// Run a wrapped remote command with stdin bytes.
    fn run_wrapped_stdin(&self, script: &str, stdin: &[u8]) -> Result<(), String> {
        let wrapped = wrap_effective_user(&self.remote, script);
        self.run_remote_stdin(&wrapped, stdin)
    }

    /// Run a remote predicate probe under the effective user and return its
    /// boolean result.
    ///
    /// The condition is wrapped in the sentinel protocol via
    /// [`wrap_predicate`]: the script always exits 0 after printing exactly
    /// `JEFE_PREDICATE_TRUE` or `JEFE_PREDICATE_FALSE`. The result is
    /// classified by [`classify_predicate_output`], which is **fail-closed**:
    ///
    /// - Exit 0 + exactly `JEFE_PREDICATE_TRUE` → `Ok(true)`.
    /// - Exit 0 + exactly `JEFE_PREDICATE_FALSE` → `Ok(false)` (safe false).
    /// - SSH exit 255 / auth / host / sudo / shell failure (any nonzero) /
    ///   malformed/extra output → `Err`.
    ///
    /// This replaces the old blanket `nonzero = false` which conflated
    /// infrastructure failures with a legitimate missing-path predicate.
    fn run_remote_check(&self, condition: &str) -> Result<bool, String> {
        let predicate = wrap_predicate(condition);
        let wrapped = wrap_effective_user(&self.remote, &predicate);
        let output = self.run_remote_capture_raw(&wrapped)?;
        let stdout = String::from_utf8_lossy(&output.stdout);
        let stderr = String::from_utf8_lossy(&output.stderr);
        classify_predicate_output(output.status.code(), &stdout, &stderr)
    }

    /// Run a remote command requiring success.
    fn run_remote(&self, remote_command: &str) -> Result<(), String> {
        let output = self.run_remote_capture_raw(remote_command)?;
        if output.status.success() {
            Ok(())
        } else {
            Err(remote_failure_message(
                &self.remote,
                remote_command,
                &output,
            ))
        }
    }

    /// Run a remote command and capture stdout as a string.
    fn run_remote_capture(&self, remote_command: &str) -> Result<String, String> {
        let output = self.run_remote_capture_raw(remote_command)?;
        if output.status.success() {
            Ok(String::from_utf8_lossy(&output.stdout).into_owned())
        } else {
            Err(remote_failure_message(
                &self.remote,
                remote_command,
                &output,
            ))
        }
    }

    /// Run a remote command with prompt bytes piped via stdin.
    fn run_remote_stdin(&self, remote_command: &str, stdin_bytes: &[u8]) -> Result<(), String> {
        use std::io::Write;
        let mut child = self
            .ssh_command(remote_command)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .map_err(|e| format!("failed to spawn ssh: {e}"))?;
        if let Some(mut stdin) = child.stdin.take() {
            stdin
                .write_all(stdin_bytes)
                .map_err(|e| format!("failed to write prompt via stdin: {e}"))?;
        }
        let output = child
            .wait_with_output()
            .map_err(|e| format!("ssh failed: {e}"))?;
        if output.status.success() {
            Ok(())
        } else {
            Err(remote_failure_message(
                &self.remote,
                remote_command,
                &output,
            ))
        }
    }

    /// Run a remote command capturing output (raw). Returns `Err` only on SSH
    /// spawn failure (a transport/infrastructure error); a non-zero remote
    /// exit status is returned as `Ok(output)` so callers can distinguish
    /// predicate results from transport failures.
    fn run_remote_capture_raw(&self, remote_command: &str) -> Result<std::process::Output, String> {
        self.ssh_command(remote_command)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .output()
            .map_err(|e| {
                format!(
                    "SSH transport failure to {}@{}: {e}",
                    self.remote.login_user.trim(),
                    self.remote.host.trim()
                )
            })
    }

    /// Build the `ssh -T` command for a remote script.
    fn ssh_command(&self, remote_command: &str) -> Command {
        let mut cmd = Command::new("ssh");
        cmd.args(["-o", "BatchMode=yes", "-o", "ConnectTimeout=10", "-T", "--"]);
        cmd.arg(format!(
            "{}@{}",
            self.remote.login_user.trim(),
            self.remote.host.trim()
        ));
        cmd.arg(remote_command);
        cmd
    }
}

/// Build the `mkdir -p $(dirname work)` prefix for a clone, or empty if no
/// parent.
fn mkdir_parent_for(work_dir: &Path) -> String {
    match work_dir.parent() {
        Some(parent) if !parent.as_os_str().is_empty() => {
            format!("mkdir -p {};", shell_escape(&parent.to_string_lossy()))
        }
        _ => String::new(),
    }
}

/// Wrap a remote command in the effective-user switch (`sudo -n su - <user>
/// -c '...'`) when `run_as_user` differs from `login_user`. Mirrors
/// `runtime::commands::remote_tmux_command` so the effective-user behavior is
/// consistent between prep and tmux operations.
fn wrap_effective_user(remote: &RemoteRepositorySettings, command: &str) -> String {
    let effective = if remote.run_as_user.trim().is_empty() {
        remote.login_user.trim()
    } else {
        remote.run_as_user.trim()
    };
    if effective == remote.login_user.trim() {
        command.to_owned()
    } else {
        format!(
            "sudo -n su - {} -c {}",
            shell_escape(effective),
            shell_escape(command),
        )
    }
}

/// Format a remote command failure message from a captured output.
fn remote_failure_message(
    remote: &RemoteRepositorySettings,
    command: &str,
    output: &std::process::Output,
) -> String {
    let stderr = String::from_utf8_lossy(&output.stderr).trim().to_owned();
    let stdout = String::from_utf8_lossy(&output.stdout).trim().to_owned();
    let detail = if !stderr.is_empty() {
        stderr
    } else if !stdout.is_empty() {
        stdout
    } else {
        format!("exit status {}", output.status)
    };
    format!(
        "remote prep on {}@{} failed ({command}): {detail}",
        remote.login_user.trim(),
        remote.host.trim(),
    )
}

/// Shell-escape a single-quoted string (mirrors
/// `runtime::commands::shell_escape_single`).
fn shell_escape(value: &str) -> String {
    format!("'{}'", value.replace('\'', r"'\''"))
}

#[cfg(test)]
#[path = "issue_prep_tests.rs"]
mod tests;

#[cfg(test)]
#[path = "issue_prep_predicate_tests.rs"]
mod predicate_tests;

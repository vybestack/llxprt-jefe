//! Remote SSH subsystem for target-aware working-copy preparation.
//!
//! Extracted from `issue_prep.rs` to keep the parent module under the
//! 1000-line source-file limit. Contains the live `RemotePrepRunner`
//! (noninteractive SSH execution), the test-only pure `RemotePrepPlanner`
//! (command planning without execution), and the shared SSH helper functions.

use std::path::Path;
use std::process::{Command, Stdio};

use jefe::domain::RemoteRepositorySettings;

use super::super::clone_identity::CloneIdentity;
use super::{DirtyPolicy, ISSUE_PROMPT_RELATIVE_PATH, PrepOutcome};

// ──────────────────────────────────────────────────────────────────────────
// Remote target prep
// ──────────────────────────────────────────────────────────────────────────

/// A pure planner that records the remote commands and prompt-transfer plan
/// **without** executing them. Exposed for deterministic tests proving all
/// operations target the remote host, use `ssh -T`, and transfer prompt bytes
/// via stdin.
#[cfg(test)]
#[derive(Debug, Clone)]
pub struct RemotePrepPlanner {
    remote: RemoteRepositorySettings,
}

/// Whether the remote work dir is absent, present but not a git worktree, or
/// a present git worktree. Replaces two boolean fields so `PlanInputs` stays
/// under the `clippy::too_many_bools` threshold.
#[cfg(test)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WorkdirPresence {
    /// The remote work dir does not exist.
    Absent,
    /// The remote work dir exists but is **not** a git worktree.
    NotGit,
    /// The remote work dir exists and is a git worktree.
    Git,
}

/// Inputs driving the pure remote-command planner. Bundled into a struct so
/// the planner signature stays under the project's argument-count limit
/// (`clippy::too_many_arguments`).
#[cfg(test)]
#[derive(Debug, Clone)]
pub struct PlanInputs<'a> {
    /// Work dir the clone/checkout targets.
    pub work_dir: &'a Path,
    /// Validated clone identity (HTTPS URL), if any.
    pub identity: Option<&'a CloneIdentity>,
    /// Dirty-copy handling policy.
    pub policy: DirtyPolicy,
    /// Whether the remote work dir is absent, non-git, or a git worktree.
    pub presence: WorkdirPresence,
    /// The remote work dir is dirty (meaningful only when a git worktree).
    pub is_dirty: bool,
    /// The remote work dir's origin does not match the configured repository.
    /// When true, the planner short-circuits (no checkout/pull/prompt op),
    /// mirroring the `Dirty`+`Stop` short-circuit.
    pub origin_mismatch: bool,
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
    /// supply `presence`, `is_dirty`, and `origin_mismatch` to drive the
    /// branching deterministically.
    ///
    /// Returns `Err` with a static reason when the planned state is a hard
    /// error in the live runner (e.g. `WorkdirPresence::NotGit`), so tests can
    /// assert that the planner and runner agree on the error path rather than
    /// the planner silently emitting an empty plan.
    pub(super) fn plan(
        &self,
        inputs: &PlanInputs<'_>,
    ) -> Result<Vec<PlannedRemoteOp>, &'static str> {
        let mut ops = Vec::new();
        let escaped_work = shell_escape(&inputs.work_dir.to_string_lossy());
        let PlanInputs {
            work_dir,
            identity,
            policy,
            presence,
            is_dirty,
            origin_mismatch,
            prompt,
        } = inputs;
        let is_git = *presence == WorkdirPresence::Git;

        // NotGit is a hard error in the live runner (exists but is not a git
        // worktree): encode it here rather than emitting an empty plan, so a
        // planner/runner divergence would be caught by tests.
        if *presence == WorkdirPresence::NotGit {
            return Err("exists but is not a git worktree");
        }

        // Origin mismatch short-circuits before any destructive op, mirroring
        // the Dirty+Stop short-circuit. The caller opens the confirm modal.
        if is_git && *origin_mismatch {
            return Ok(ops);
        }

        // 1. Clone if missing.
        if *presence == WorkdirPresence::Absent {
            // Path absent → clone if identity present. If no identity is
            // available, the live runner returns Err and no further ops run;
            // mirror that here so the planner cannot emit checkout/prompt
            // operations against a path that was never created.
            match identity {
                Some(id) => {
                    let url = id.clone_url();
                    let script = format!(
                        "set -e; {mkdir_parent} git clone -- {url} {escaped_work}",
                        mkdir_parent = mkdir_parent_for(work_dir),
                        url = shell_escape(&url),
                    );
                    ops.push(self.wrapped_ssh_op(&script, None));
                }
                None => return Ok(ops),
            }
        }

        // 2. Dirty check. NotGit was already handled above as a hard error,
        // so this branch covers Git (pre-existing) and Absent (just cloned).
        {
            // After clone the worktree is clean; only check dirty when it
            // pre-existed as a git worktree.
            if is_git && *is_dirty {
                match policy {
                    DirtyPolicy::Stop => {
                        // Stop: no further ops. The caller opens the confirm
                        // modal; no reset/clean is planned.
                        return Ok(ops);
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

            // 5. mkdir .jefe + write prompt via stdin (cat > file). Escape
            // the full joined prompt path (consistent with plan_force_reclone
            // and the live run() path) so a metacharacter in the work dir or
            // the constant can never break the shell command.
            let prompt_path =
                shell_escape(&work_dir.join(ISSUE_PROMPT_RELATIVE_PATH).to_string_lossy());
            let script = format!(
                "set -e; mkdir -p {jefe_dir}; cat > {prompt_path}",
                jefe_dir = shell_escape(&work_dir.join(".jefe").to_string_lossy()),
                prompt_path = prompt_path,
            );
            ops.push(self.wrapped_ssh_op(&script, Some((*prompt).to_owned())));
        }

        Ok(ops)
    }

    /// Plan the force-reclone sequence: resolve URL → rm → clone → checkout →
    /// prompt.
    ///
    /// Records the ordered list of `ssh -T` operations for the force-reclone
    /// path (issue #190 MUST-FIX #2). The identity is required (non-optional)
    /// so the clone URL is resolved BEFORE the `rm -rf`.
    #[must_use]
    pub(super) fn plan_force_reclone(
        &self,
        work_dir: &Path,
        identity: &CloneIdentity,
        prompt: &str,
    ) -> Vec<PlannedRemoteOp> {
        let mut ops = Vec::new();
        let escaped_work = shell_escape(&work_dir.to_string_lossy());

        // 1. Resolve the clone URL BEFORE any destructive action.
        let url = identity.clone_url();

        // 2. Remove the mismatched workdir.
        let rm_script = format!("rm -rf {escaped_work}");
        ops.push(self.wrapped_ssh_op(&rm_script, None));

        // 3. Clone from the resolved URL.
        let clone_script = format!(
            "set -e; {mkdir_parent} git clone -- {url} {escaped_work}",
            mkdir_parent = mkdir_parent_for(work_dir),
            url = shell_escape(&url),
        );
        ops.push(self.wrapped_ssh_op(&clone_script, None));

        // 4. Post-clone prep: resolve default branch + fetch + checkout.
        let checkout_script = format!(
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
        ops.push(self.wrapped_ssh_op(&checkout_script, None));
        // 5. mkdir .jefe + write prompt via stdin.
        let jefe_dir = shell_escape(&work_dir.join(".jefe").to_string_lossy());
        let prompt_path =
            shell_escape(&work_dir.join(ISSUE_PROMPT_RELATIVE_PATH).to_string_lossy());
        let prompt_script = format!("set -e; mkdir -p {jefe_dir}; cat > {prompt_path}");
        ops.push(self.wrapped_ssh_op(&prompt_script, Some(prompt.to_owned())));

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

/// Classify the raw output of a `git remote get-url origin` probe.
///
/// Distinguishes the following outcomes (issue #190 MUST-FIX #1):
///
/// - Exit 0 with non-empty stdout → `Ok(Some(raw_url))` (origin exists). The
///   raw URL is returned unvalidated; URL parsing/host-matching is the
///   caller's responsibility (`remote_origin_mismatch`).
/// - Exit 0 with empty stdout → `Ok(None)` (origin absent — the probe
///   script swallows the nonzero git exit and prints nothing).
/// - SSH exit 255 → `Err` (transport/auth/host failure).
/// - Any other nonzero exit → `Err` (sudo/shell/auth failure).
/// - Signal termination (no exit code) → `Err`.
///
/// This is **fail-closed**: genuine infrastructure failures propagate as
/// `Err`, while a legitimate missing-origin remote is `Ok(None)` so the
/// caller can return `OriginMismatch` (a git repo with no `origin` while an
/// expected shortform IS configured is a mismatch — see
/// [`super::remote_origin_mismatch`]).
#[must_use = "the origin classification distinguishes Ok(Some)/Ok(None)/Err; ignoring Err conflates an SSH/auth failure with a missing origin"]
pub fn classify_origin_url_output(
    exit_code: Option<i32>,
    stdout: &str,
    stderr: &str,
) -> Result<Option<String>, String> {
    match exit_code {
        Some(0) => {
            let trimmed = stdout.trim();
            if trimmed.is_empty() {
                Ok(None)
            } else {
                Ok(Some(trimmed.to_owned()))
            }
        }
        Some(255) => Err(format!(
            "SSH transport/auth/host failure (exit 255): {}",
            stderr.trim()
        )),
        Some(code) => Err(format!(
            "remote origin probe failed (exit {code}): {}",
            stderr.trim()
        )),
        None => Err("remote origin probe terminated by signal".to_owned()),
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
#[must_use = "the predicate classification distinguishes Ok(true)/Ok(false)/Err; ignoring Err conflates an SSH/auth failure with a safe false"]
pub fn classify_predicate_output(
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
///
/// # Escaping contract
///
/// `condition` is interpolated verbatim into the shell script. Callers MUST
/// pre-shell-escape any dynamic content (paths, remote-supplied values) via
/// [`shell_escape`] before passing it here. All current call sites pass
/// pre-escaped literals or `shell_escape`-d values. The function lives in a
/// private module and is re-exported only `pub(super)` to `app_input`, so no
/// code outside that boundary can reach it with untrusted input.
pub fn wrap_predicate(condition: &str) -> String {
    format!(
        "{{ {condition}; }} && printf '%s' {sentinel_true} || printf '%s' {sentinel_false}",
        sentinel_true = shell_escape(PREDICATE_TRUE),
        sentinel_false = shell_escape(PREDICATE_FALSE),
    )
}

/// The live SSH runner. Executes the planned sequence against the real remote.
pub(super) struct RemotePrepRunner {
    remote: RemoteRepositorySettings,
}

impl RemotePrepRunner {
    pub(super) fn new(remote: RemoteRepositorySettings) -> Self {
        Self { remote }
    }

    /// Execute the remote prep sequence.
    ///
    /// Determined at runtime by querying the remote host: detect whether the
    /// work dir is a git worktree, then branch accordingly.
    pub(super) fn run(
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
            // Origin-mismatch check: only when an expected shortform is
            // configured (identity present).
            if let Some(id) = identity
                && let Some(mismatch) = self.remote_origin_mismatch(work_dir, id)?
            {
                return Ok(mismatch);
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
        // `set -e` makes the script fail fast if `cd` fails (e.g., a TOCTOU
        // race that removed the dir between the existence check and here),
        // so `git status` can never run in the wrong directory and produce a
        // misleading porcelain result. Uses `-z` (NUL-delimited) so paths
        // containing newlines or ` -> ` are handled correctly; NUL is valid
        // UTF-8 so the String transport preserves embedded NULs.
        let dirty_script = format!("set -e; cd {escaped_work}; git status --porcelain=v1 -z");
        let porcelain = self.run_wrapped_capture(&dirty_script)?;
        let dirty = super::super::issue_git_prep::porcelain_is_dirty(&porcelain);

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

    /// Check if the remote workdir's origin matches the configured identity.
    ///
    /// Returns `Ok(Some(OriginMismatch))` when the origin does not match (or
    /// there is no origin remote while an expected shortform IS configured).
    /// Returns `Ok(None)` when the origin matches. Only called when an
    /// identity is present.
    ///
    /// Uses the **host-aware** `origins_match` from `issue_git_prep` so a
    /// foreign host (GitLab, attacker) with the same `owner/repo` is rejected.
    fn remote_origin_mismatch(
        &self,
        work_dir: &Path,
        identity: &CloneIdentity,
    ) -> Result<Option<PrepOutcome>, String> {
        let expected = identity.expected_shortform();
        let origin_url = self.read_remote_origin_url(work_dir)?;
        match origin_url.as_deref() {
            Some(raw_url) => {
                if super::super::issue_git_prep::origins_match(raw_url, expected) {
                    Ok(None)
                } else {
                    // Display the normalized owner/repo when it parses, else
                    // the raw URL — so a malformed/unexpected origin (not
                    // just a missing one) surfaces a diagnosable actual value
                    // rather than an empty string indistinguishable from
                    // "no origin".
                    let actual = jefe::git_info::origin_display_shortform(raw_url)
                        .filter(|s| !s.is_empty())
                        .unwrap_or_else(|| raw_url.to_owned());
                    Ok(Some(PrepOutcome::OriginMismatch {
                        actual,
                        expected: expected.to_owned(),
                    }))
                }
            }
            None => Ok(Some(PrepOutcome::OriginMismatch {
                actual: String::new(),
                expected: expected.to_owned(),
            })),
        }
    }

    /// Read the raw `origin` remote URL on the remote host, returning `None`
    /// when the remote has no `origin` remote.
    ///
    /// Uses `run_remote_capture_raw` + [`classify_origin_url_output`] so a
    /// missing `origin` remote (git exit nonzero, swallowed by the script)
    /// returns `Ok(None)`, while genuine SSH/sudo/shell failures (exit 255
    /// or other nonzero from the transport layer) propagate as `Err`.
    ///
    /// The probe script runs `git -C <work> remote get-url origin` and maps
    /// the exit code: git returns exit **2** specifically when the `origin`
    /// remote is absent, while config/permission/infrastructure failures use
    /// other codes (e.g. 128). The wrapper turns exit-2 into empty stdout
    /// (→ `Ok(None)`, the safe no-origin case) while letting any other
    /// nonzero exit propagate so genuine failures surface as `Err` rather
    /// than masquerading as "no origin" (which could mislead the user into
    /// authorizing a destructive reclone).
    fn read_remote_origin_url(&self, work_dir: &Path) -> Result<Option<String>, String> {
        let escaped_work = shell_escape(&work_dir.to_string_lossy());
        // Map git's exit-2 (no origin remote) to empty stdout; propagate any
        // other nonzero exit so the classifier reports it as an error. The
        // shell `$` variables are in a raw concatenation so `format!` does
        // not mistake them for interpolation; only `{escaped_work}` is
        // interpolated.
        let probe = format!(
            concat!(
                "out=$(git -C {w} remote get-url origin 2>/dev/null); code=$?; ",
                "if [ \"$code\" -eq 0 ]; then printf '%s' \"$out\"; ",
                "elif [ \"$code\" -eq 2 ]; then printf ''; ",
                "else exit \"$code\"; fi",
            ),
            w = escaped_work,
        );
        let script = wrap_effective_user(&self.remote, &probe);
        let output = self.run_remote_capture_raw(&script)?;
        let stdout = String::from_utf8_lossy(&output.stdout);
        let stderr = String::from_utf8_lossy(&output.stderr);
        classify_origin_url_output(output.status.code(), &stdout, &stderr)
    }

    /// Force-reclone a mismatched remote workdir: resolve URL → rm → clone →
    /// post-clone prep (no dirty-check — fresh clone is clean).
    ///
    /// **Ordering invariant (MUST-FIX #2):** the clone URL is resolved from
    /// the required `identity` BEFORE the `rm -rf`. Since `identity` is a
    /// non-optional `&CloneIdentity`, removal can never happen without a
    /// valid replacement URL.
    ///
    /// **Partial-failure note:** if the `git clone` fails after the `rm -rf`
    /// (network/auth/disk error), the existing workdir is already destroyed
    /// and this returns `Err`. The user explicitly confirmed the replacement
    /// via the `ConfirmIssueOriginMismatch` modal, and recovery is to retry
    /// the send (which will clone fresh from the resolved URL). The
    /// remove-then-clone ordering is defined by issue #190; a fully
    /// transactional clone-to-temp-then-atomic-swap is intentionally out of
    /// scope here.
    pub(super) fn run_force_reclone(
        &self,
        work_dir: &Path,
        identity: &CloneIdentity,
        prompt: &str,
    ) -> Result<PrepOutcome, String> {
        // Defense-in-depth: refuse catastrophic targets (root, empty,
        // top-level entry) even though the user confirmed. This runs BEFORE
        // constructing the rm -rf shell command so a misconfigured work_dir
        // can never reach the remote shell.
        super::super::issue_git_prep::validate_reclone_target(work_dir)?;

        let escaped_work = shell_escape(&work_dir.to_string_lossy());

        // 1. Resolve the clone URL BEFORE any destructive action.
        let url = identity.clone_url();

        // 2. Remove the mismatched workdir.
        let rm_script = format!("rm -rf {escaped_work}");
        self.run_wrapped(&rm_script)?;

        // Any failure from here (clone or prep) occurs AFTER the original
        // workdir has been destroyed. Annotate the error so the user knows
        // their data is already gone and which step failed.
        // 3. Clone from the resolved URL.
        let clone_script = format!(
            "set -e; {mkdir_parent} git clone -- {url} {escaped_work}",
            mkdir_parent = mkdir_parent_for(work_dir),
            url = shell_escape(&url),
        );
        self.run_wrapped(&clone_script)
            .map_err(|e| format!("After removing the mismatched remote work_dir, the clone failed (the original working copy at {} is already gone): {e}", work_dir.display()))?;

        // 4. Post-clone prep: resolve default branch + fetch + checkout.
        let checkout_script = format!(
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
        self.run_wrapped(&checkout_script)
            .map_err(|e| format!("After force-recloning {} remotely (the original working copy is already gone), post-clone prep failed: {e}", work_dir.display()))?;

        // 4. mkdir .jefe + write prompt via stdin.
        let jefe_dir = shell_escape(&work_dir.join(".jefe").to_string_lossy());
        let prompt_path =
            shell_escape(&work_dir.join(ISSUE_PROMPT_RELATIVE_PATH).to_string_lossy());
        let prompt_script = format!("set -e; mkdir -p {jefe_dir}; cat > {prompt_path}");
        self.run_wrapped_stdin(&prompt_script, prompt.as_bytes())?;

        Ok(PrepOutcome::Ready)
    }

    /// Write a prompt file to the remote host at `work_dir/{relative_path}`
    /// via `ssh -T`, piping prompt bytes through stdin.
    ///
    /// This is the reusable remote write used by [`write_prompt_to_target`]
    /// for both issue and PR prompts. It does NOT clone, check dirty, or
    /// switch branches — it only creates `.jefe/` and writes the file.
    pub(super) fn write_prompt(
        &self,
        work_dir: &Path,
        relative_path: &str,
        prompt_bytes: &[u8],
    ) -> Result<(), String> {
        // Defense-in-depth: validate the relative path even though current
        // call sites pass the safe ISSUE_PROMPT_RELATIVE_PATH constant. This
        // guards against future misuse of this pub(super) API with a
        // traversal value (e.g. ../../etc/passwd) that would escape work_dir.
        super::validate_prompt_relative_path(relative_path)?;
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
pub(super) fn shell_escape(value: &str) -> String {
    format!("'{}'", value.replace('\'', r"'\''"))
}

#[cfg(test)]
#[path = "issue_prep_remote_tests.rs"]
mod tests;

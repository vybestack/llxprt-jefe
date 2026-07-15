//! Side-effect-free remote agent-runtime availability probe.
//!
//! Before any issue git prep/cleanup/prompt side effect or PR prompt write,
//! the selected runtime is probed on the remote target to confirm the exact
//! binary (`code-puppy` or `llxprt`) is available for the **effective**
//! run-as user. The probe is:
//!
//! - **No-install / no-setup**: it never runs `setup_env_default` or any
//!   install command. It only checks `command -v <binary>`.
//! - **Side-effect-free**: a read-only `command -v` check. No file writes,
//!   no git operations, no package installation.
//! - **ssh -T**: noninteractive, no PTY allocation (distinct from the `-tt`
//!   used for tmux operations in `runtime::commands`).
//! - **Effective user**: executed as the effective `run_as_user` (falling
//!   back to `login_user` when `run_as_user` is empty), matching the user
//!   that will own the prep/launch side effects.
//! - **Exact binary**: probes `code-puppy` (not `code_puppy`) or `llxprt`
//!   — the exact executable name from [`AgentKind::binary_name`].
//!
//! ## Predicate classification (defect 3)
//!
//! The probe uses an explicit **sentinel protocol** so a normal missing-path
//! result is cleanly distinguished from an infrastructure failure:
//!
//! ```text
//! ssh -T <login_user>@<host> sudo -n su - <effective_user> -c \
//!   'command -v <binary> >/dev/null 2>&1 && printf JEFE_PROBE_OK || printf JEFE_PROBE_NO'
//! ```
//!
//! - `stdout == "JEFE_PROBE_OK"` → [`RemoteProbeResult::Available`].
//! - `stdout == "JEFE_PROBE_NO"` → [`RemoteProbeResult::NotAvailable`] (the
//!   binary is genuinely missing for the effective user — this is a normal
//!   false predicate, NOT an error).
//! - SSH exit code 255 / auth failure / host failure →
//!   [`RemoteProbeResult::Error`] (transport/auth failure).
//! - `sudo -n` failure → [`RemoteProbeResult::Error`] (effective-user
//!   failure).
//! - Malformed/empty output → [`RemoteProbeResult::Error`] (missing command
//!   infrastructure or protocol mismatch — never trigger a clone).
//!
//! Only `Available` allows prep/launch to proceed. `NotAvailable` blocks
//! with a clear "not installed for effective user" message. `Error` blocks
//! with a transport/auth/infrastructure message and **never triggers a
//! clone**.

use std::path::Path;

use jefe::domain::{AgentKind, RemoteRepositorySettings};

/// Sentinel emitted by the probe when the binary IS available.
const SENTINEL_OK: &str = "JEFE_PROBE_OK";
/// Sentinel emitted by the probe when the binary is NOT available.
const SENTINEL_NO: &str = "JEFE_PROBE_NO";

/// Outcome of a remote agent-runtime availability probe.
///
/// Distinguishes a normal "binary is missing" result (`NotAvailable`) from
/// a transport/auth/infrastructure failure (`Error`). Only `Available`
/// permits prep/launch side effects.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum RemoteProbeResult {
    /// The exact binary is installed and executable for the effective user.
    /// Prep/launch may proceed.
    Available,
    /// The binary is genuinely not installed for the effective user. This is
    /// a normal false predicate — NOT an error. Blocks launch with a clear
    /// message.
    NotAvailable,
    /// A transport, auth, host, effective-user, or infrastructure failure
    /// prevented the probe from completing. NEVER triggers a clone or any
    /// side effect — surfaced as an actionable error.
    Error(String),
}

/// The effective remote user for a probe/prep operation.
///
/// When `run_as_user` is empty, the effective user is `login_user`. This
/// mirrors the `remote_effective_user` logic in `runtime::commands` and
/// `wrap_effective_user` in `issue_prep`.
#[must_use]
pub(super) fn effective_user(remote: &RemoteRepositorySettings) -> String {
    let run_as = remote.run_as_user.trim();
    if run_as.is_empty() {
        remote.login_user.trim().to_owned()
    } else {
        run_as.to_owned()
    }
}

/// Classify a raw probe execution outcome (exit status + stdout) into a
/// [`RemoteProbeResult`].
///
/// This is the **pure classifier** (defect 3): it takes the raw ssh output
/// and determines whether the binary is available, not available, or whether
/// a transport/auth/infrastructure failure occurred. It never executes any
/// command — it only interprets results.
///
/// # Parameters
///
/// - `exit_code`: The exit code of the `ssh` process (`None` if terminated by
///   signal).
/// - `stdout`: The captured stdout of the ssh process.
/// - `stderr`: The captured stderr of the ssh process (used for error
///   messages on failure).
///
/// # Classification
///
/// - `exit_code == 0` with trimmed `stdout` exactly equal to `JEFE_PROBE_OK`
///   → `Available`.
/// - `exit_code == 0` with trimmed `stdout` exactly equal to `JEFE_PROBE_NO`
///   → `NotAvailable`.
/// - `exit_code == 255` → `Error` (SSH transport/auth/host failure).
/// - `exit_code == 0` with any prefix/suffix/both sentinels/malformed output
///   → `Error` (protocol mismatch, banner injection, or truncated output).
/// - Any other nonzero exit → `Error`.
///
/// Live execution classifies transport failures through `SshPlan::execute`
/// before calling this function. Accepting raw non-success outcomes here keeps
/// this pure boundary classifier complete and directly testable.
#[must_use]
pub(super) fn classify_probe_output(
    exit_code: Option<i32>,
    stdout: &str,
    stderr: &str,
) -> RemoteProbeResult {
    if exit_code != Some(0) {
        return RemoteProbeResult::Error(
            jefe::ssh::classify_failure(exit_code, stderr).to_string(),
        );
    }
    match stdout.trim() {
        SENTINEL_OK => RemoteProbeResult::Available,
        SENTINEL_NO => RemoteProbeResult::NotAvailable,
        _ => RemoteProbeResult::Error(
            "remote probe returned unexpected output; verify remote shell startup files".to_owned(),
        ),
    }
}

/// Plan the `ssh -T` argv for a remote agent-runtime availability probe.
///
/// This is a **pure planning function** — it builds the command argv without
/// executing anything. It returns the exact arguments that would be passed
/// to `ssh`, so tests can verify the probe:
///
/// - Uses `-T` (no PTY), not `-tt`.
/// - Targets `<login_user>@<host>`.
/// - Wraps the command in `sudo -n su - <effective_user> -c` when the
///   effective user differs from `login_user`.
/// - Probes the **exact binary name** (`code-puppy` or `llxprt`).
/// - Uses the sentinel protocol (`JEFE_PROBE_OK` / `JEFE_PROBE_NO`).
/// - Does NOT run any install or setup command.
///
/// ## LLxprt path-local resolution (mirrors launch resolver)
///
/// For `AgentKind::Llxprt`, the probe mirrors the **non-mutating** checks in
/// `runtime::commands::resolve_remote_llxprt_command`: it accepts a **global**
/// `command -v llxprt` OR an executable `<work_dir>/node_modules/.bin/llxprt`.
/// This keeps the pre-side-effect availability gate consistent with the
/// actual launch resolver. `CodePuppy` remains a global `command -v
/// code-puppy` check (the launch resolver has no path-local fallback for it).
///
/// The probe is side-effect-free: it never installs, sets up, or writes any
/// file.
///
/// The `work_dir` parameter is the agent's work directory. The global binary
/// check does NOT `cd` there — it probes the global PATH directly so a
/// globally-installed runtime is detected even when `work_dir` does not yet
/// exist (clone-if-missing flow). `work_dir` is only referenced for the
/// LLxprt path-local `[ -x <work_dir>/node_modules/.bin/llxprt ]` check,
/// which safely fails when the directory is absent.
#[must_use]
#[cfg(test)]
pub(super) fn plan_remote_probe(
    remote: &RemoteRepositorySettings,
    work_dir: &Path,
    kind: AgentKind,
) -> Vec<String> {
    ssh_arguments_as_strings(remote, &remote_probe_command(remote, work_dir, kind))
        .unwrap_or_else(|error| panic!("plan remote probe: {error}"))
}

#[must_use]
#[cfg(test)]
pub(super) fn plan_remote_code_puppy_probe(
    remote: &RemoteRepositorySettings,
    work_dir: &Path,
    version: &str,
) -> Vec<String> {
    ssh_arguments_as_strings(
        remote,
        &remote_code_puppy_probe_command(remote, work_dir, version),
    )
    .unwrap_or_else(|error| panic!("plan remote Code Puppy probe: {error}"))
}

fn remote_probe_command(
    remote: &RemoteRepositorySettings,
    work_dir: &Path,
    kind: AgentKind,
) -> String {
    let inner = probe_inner_command(kind, work_dir);
    wrap_probe_for_effective_user(remote, inner)
}

fn remote_code_puppy_probe_command(
    remote: &RemoteRepositorySettings,
    work_dir: &Path,
    version: &str,
) -> String {
    let inner = probe_inner_command_for_code_puppy(work_dir, version);
    wrap_probe_for_effective_user(remote, inner)
}

fn wrap_probe_for_effective_user(remote: &RemoteRepositorySettings, inner: String) -> String {
    let effective = effective_user(remote);
    if effective == remote.login_user.trim() {
        inner
    } else {
        format!(
            "sudo -n su - {} -c {}",
            shell_escape(&effective),
            shell_escape(&inner),
        )
    }
}

fn ssh_arguments_as_strings(
    remote: &RemoteRepositorySettings,
    remote_command: &str,
) -> Result<Vec<String>, String> {
    jefe::ssh::SshPlan::arguments(remote, remote_command, jefe::ssh::SshMode::NonInteractive)
        .map(|args| {
            args.into_iter()
                .map(|arg| arg.to_string_lossy().into_owned())
                .collect()
        })
        .map_err(|error| error.to_string())
}

/// Build the inner shell command for the sentinel-based availability probe.
///
/// The script always returns shell success (so `set -e` isn't needed) and
/// prints exactly one sentinel:
///
/// - `JEFE_PROBE_OK` when the binary is available.
/// - `JEFE_PROBE_NO` when it is not.
///
/// ## Global-first probing (no `cd` for the global check)
///
/// For both kinds the **global** `command -v <binary>` check runs WITHOUT
/// `cd`-ing to `work_dir` first. This is critical: the probe may fire before
/// the work directory exists (e.g. a clone-if-missing flow), and requiring
/// `cd` would turn a globally-installed runtime into a spurious
/// `NotAvailable`. The work directory is only referenced for the LLxprt
/// path-local `[ -x ... ]` check, which is itself safe against a missing
/// directory (the test simply fails).
///
/// For `AgentKind::Llxprt`, this mirrors the launch resolver's non-mutating
/// checks: global `command -v llxprt` OR executable
/// `<work_dir>/node_modules/.bin/llxprt`. For `CodePuppy`, it probes the
/// exact global `code-puppy` binary only (the launch resolver has no
/// path-local fallback for code-puppy).
fn probe_inner_command(kind: AgentKind, work_dir: &Path) -> String {
    match kind {
        AgentKind::CodePuppy => probe_inner_command_for_code_puppy(work_dir, ""),
        AgentKind::Llxprt => {
            let sentinel_ok = shell_escape(SENTINEL_OK);
            let sentinel_no = shell_escape(SENTINEL_NO);
            // LLxprt: mirror launch resolver non-mutating checks — global
            // command (no cd) OR executable <work_dir>/node_modules/.bin/llxprt.
            // The global check runs first without cd so a missing work
            // directory does not mask a globally-installed runtime.
            let path_local = shell_escape(&format!(
                "{}/node_modules/.bin/llxprt",
                work_dir.to_string_lossy()
            ));
            format!(
                "{{ command -v llxprt >/dev/null 2>&1 || [ -x {path_local} ]; }} \
                 && printf '%s' {sentinel_ok} \
                 || printf '%s' {sentinel_no}",
            )
        }
    }
}

fn probe_inner_command_for_code_puppy(_work_dir: &Path, version: &str) -> String {
    let binary = if version.trim().is_empty() {
        "code-puppy"
    } else {
        "uvx"
    };
    let sentinel_ok = shell_escape(SENTINEL_OK);
    let sentinel_no = shell_escape(SENTINEL_NO);
    format!(
        "command -v {binary} >/dev/null 2>&1 \
         && printf '%s' {sentinel_ok} \
         || printf '%s' {sentinel_no}",
    )
}

/// Execute a remote agent-runtime availability probe.
///
/// Runs `ssh` with the planned argv, captures the output, and classifies the
/// result via [`classify_probe_output`]. This is the **live execution seam**
/// — it performs a real SSH connection but does NOT install, setup, clone,
/// or write any files.
///
/// # Errors
///
/// Returns [`RemoteProbeResult::Error`] when:
/// - The `ssh` process cannot be spawned (local infrastructure failure).
/// - SSH returns exit 255 (transport/auth/host failure).
/// - The output is malformed (no sentinel, protocol mismatch).
///
/// Returns [`RemoteProbeResult::NotAvailable`] when the binary is genuinely
/// missing for the effective user (sentinel `JEFE_PROBE_NO`, exit 0).
///
/// Returns [`RemoteProbeResult::Available`] when the binary is found
/// (sentinel `JEFE_PROBE_OK`, exit 0).
pub(super) fn execute_remote_probe(
    remote: &RemoteRepositorySettings,
    work_dir: &Path,
    kind: AgentKind,
) -> RemoteProbeResult {
    execute_remote_probe_command(remote, &remote_probe_command(remote, work_dir, kind))
}

fn execute_remote_code_puppy_probe(
    remote: &RemoteRepositorySettings,
    work_dir: &Path,
    version: &str,
) -> RemoteProbeResult {
    execute_remote_probe_command(
        remote,
        &remote_code_puppy_probe_command(remote, work_dir, version),
    )
}

fn execute_remote_probe_command(
    remote: &RemoteRepositorySettings,
    command: &str,
) -> RemoteProbeResult {
    let plan = match jefe::ssh::SshPlan::new(remote, command, jefe::ssh::SshMode::NonInteractive) {
        Ok(plan) => plan,
        Err(error) => return RemoteProbeResult::Error(error.to_string()),
    };
    let output = match plan.execute(None, jefe::ssh::SSH_OPERATION_TIMEOUT, None) {
        Ok(output) => output,
        Err(error) => return RemoteProbeResult::Error(error.to_string()),
    };
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    if !output.status.success() {
        return RemoteProbeResult::Error(
            jefe::ssh::classify_failure(output.status.code(), &stderr).to_string(),
        );
    }
    classify_probe_output(output.status.code(), &stdout, &stderr)
}

/// Centralized pre-side-effect availability validation for issue/PR sends.
///
/// This is the single entry point called before any destructive/file side
/// effect (issue git prep, dirty-confirm, PR prompt write). It:
///
/// - **Local targets**: delegates to the `installed_agent_kinds` session
///   snapshot (no PATH I/O during input handling).
/// - **Remote targets**: runs [`execute_remote_probe`] — a no-install,
///   no-setup, side-effect-free `ssh -T` probe for the exact binary, executed
///   as the effective `run_as_user`.
///
/// Returns `Ok(())` when the runtime is available and prep may proceed.
/// Returns `Err(message)` with a user-facing explanation when the runtime is
/// not available or a transport/auth/infrastructure failure occurred.
///
/// # Parameters
///
/// - `target`: The resolved [`WorkTarget`] (local or remote).
/// - `work_dir`: The agent work directory (used for remote probe context).
/// - `kind`: The agent runtime kind to probe.
/// - `available`: The local installed-kinds snapshot (used for local targets
///   only).
///
/// # Local vs Remote
///
/// For **local** targets, this reuses [`require_local_kind_available`] so the
/// local check stays consistent with the form-submit and launch guards. For
/// **remote** targets, it probes the exact binary on the remote host as the
/// effective user.
fn require_signature_available(
    target: &WorkTarget,
    signature: &jefe::domain::LaunchSignature,
    available: &[AgentKind],
) -> Result<(), String> {
    if jefe::domain::llxprt_launch_source(signature.agent_kind, signature.llxprt_version.as_ref())
        .requires_npm()
    {
        return jefe::runtime::require_launch_package_available(signature)
            .map_err(|error| error.to_string());
    }
    if signature.agent_kind == AgentKind::CodePuppy
        && !signature.code_puppy_version.trim().is_empty()
    {
        return match target {
            WorkTarget::Local => jefe::runtime::require_launch_package_available(signature)
                .map_err(|error| error.to_string()),
            WorkTarget::Remote(remote) => match execute_remote_code_puppy_probe(
                remote,
                &signature.work_dir,
                &signature.code_puppy_version,
            ) {
                RemoteProbeResult::Available => {
                    jefe::runtime::require_launch_package_available(signature)
                        .map_err(|error| error.to_string())
                }
                RemoteProbeResult::NotAvailable => Err(format!(
                    "uvx is not installed on the remote host for user '{}'. Install uv on that target or clear the Code Puppy version.",
                    effective_user(remote)
                )),
                RemoteProbeResult::Error(error) => Err(error),
            },
        };
    }
    match target {
        WorkTarget::Local => super::availability::require_local_kind_available_for_target(
            signature.agent_kind,
            available,
        ),
        WorkTarget::Remote(remote) => {
            let result = execute_remote_probe(remote, &signature.work_dir, signature.agent_kind);
            match result {
                RemoteProbeResult::Available => Ok(()),
                RemoteProbeResult::NotAvailable => Err(format!(
                    "{} is not installed on the remote host for user '{}'. \
                     Install it or select a different agent kind.",
                    signature.agent_kind.binary_name(),
                    effective_user(remote)
                )),
                RemoteProbeResult::Error(error) => Err(error),
            }
        }
    }
}

#[cfg(test)]
pub(super) fn require_runtime_available(
    target: &WorkTarget,
    work_dir: &Path,
    kind: AgentKind,
    available: &[AgentKind],
) -> Result<(), String> {
    let signature = jefe::domain::LaunchSignature {
        work_dir: work_dir.to_path_buf(),
        profile: String::new(),
        code_puppy_model: String::new(),
        code_puppy_version: String::new(),
        code_puppy_yolo: None,
        code_puppy_quick_resume: false,
        mode_flags: Vec::new(),
        llxprt_debug: String::new(),
        pass_continue: false,
        sandbox_enabled: false,
        sandbox_engine: jefe::domain::SandboxEngine::Podman,
        sandbox_flags: String::new(),
        remote: match target {
            WorkTarget::Local => RemoteRepositorySettings::default(),
            WorkTarget::Remote(remote) => remote.clone(),
        },
        agent_kind: kind,
        llxprt_version: None,
    };
    require_signature_available(target, &signature, available)
}

/// Pre-side-effect availability guard for issue/PR send paths.
///
/// This is the centralized entry point called BEFORE any destructive/file
/// side effect (issue git prep, dirty-confirm discard, PR prompt write). It:
///
/// 1. Reads the `installed_agent_kinds` snapshot from `app_state` under a
///    short read-lock.
/// 2. Calls [`require_runtime_available`] with the resolved target, work
///    dir, kind, and snapshot.
/// 3. On `Err`, writes the error into `app_state.error_message` and returns
///    `false` so the caller aborts (no prep/prompt operation).
///
/// For **local** targets, this delegates to the session snapshot (no PATH
/// I/O). For **remote** targets, it executes [`execute_remote_probe`] — a
/// no-install/no-setup/side-effect-free `ssh -T` probe for the exact binary,
/// executed as the effective `run_as_user`.
///
/// The runtime launch resolver may still resolve again for race safety —
/// this guard is a pre-side-effect gate, not a substitute for the launch
/// resolver.
pub(super) fn pre_side_effect_runtime_available_or_error(
    app_state: &mut super::AppStateHandle,
    target: &WorkTarget,
    signature: &jefe::domain::LaunchSignature,
) -> bool {
    let available = {
        let state = app_state.read();
        state.installed_agent_kinds.clone()
    };
    match require_signature_available(target, signature, &available) {
        Ok(()) => true,
        Err(message) => {
            let mut state = app_state.write();
            state.error_message = Some(message);
            drop(state);
            false
        }
    }
}

// ──────────────────────────────────────────────────────────────────────────
// Production remote PR prompt planning seam (defect 4)
// ──────────────────────────────────────────────────────────────────────────

/// Relative path of the PR prompt inside the work dir.
///
/// Must be exactly `.jefe/pr-prompt.md` — NOT the issue path. Production
/// callers (`prs_orchestration`) hold their own module-local constant; this
/// test constant keeps the remote-probe planning-seam tests self-contained.
#[cfg(test)]
pub(super) const PR_PROMPT_RELATIVE_PATH: &str = ".jefe/pr-prompt.md";

/// A planned remote prompt-write operation (pure data for test verification).
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct PlannedPromptWrite {
    /// The full `ssh -T` argv (everything after the `ssh` binary).
    pub ssh_argv: Vec<String>,
    /// The controlled Unix command sent to the remote host.
    remote_command: String,
    /// The relative prompt path targeted (e.g. `.jefe/pr-prompt.md`).
    pub relative_path: String,
    /// Prompt bytes that would be transferred via stdin.
    pub stdin_prompt: String,
}

/// Plan a remote prompt-write operation for a production PR prompt.
///
/// This is the **exact production remote prompt planning seam** (defect 4).
/// It is parameterized by `relative_path` so it can be used for both PR
/// (`.jefe/pr-prompt.md`) and issue prompts. The plan:
///
/// - Targets `{work_dir}/{relative_path}` (e.g. `.jefe/pr-prompt.md`).
/// - Transfers prompt bytes via **stdin** (`cat > path`), never shell
///   interpolation.
/// - Uses `ssh -T` (no PTY), targeting `<login_user>@<host>`.
/// - Wraps in `sudo -n su - <effective_user> -c` when effective user differs.
/// - Does NOT include adversarial content in argv.
///
/// Returns `Err` when the relative path is invalid (not under `.jefe/`,
/// absolute, or contains `..` traversal).
pub(super) fn plan_remote_prompt_write(
    remote: &RemoteRepositorySettings,
    work_dir: &Path,
    relative_path: &str,
    prompt: &str,
) -> Result<PlannedPromptWrite, String> {
    validate_prompt_relative_path(relative_path)?;
    let escaped_jefe_dir = shell_escape(&work_dir.join(".jefe").to_string_lossy());
    let escaped_prompt_path = shell_escape(&work_dir.join(relative_path).to_string_lossy());
    let inner = format!("set -e; mkdir -p {escaped_jefe_dir}; cat > {escaped_prompt_path}");
    let effective = effective_user(remote);
    let command = if effective == remote.login_user.trim() {
        inner
    } else {
        format!(
            "sudo -n su - {} -c {}",
            shell_escape(&effective),
            shell_escape(&inner),
        )
    };
    let ssh_argv = ssh_arguments_as_strings(remote, &command)?;
    // Adversarial content safety: prompt bytes are in stdin, NOT argv.
    // Compare whole tokens to avoid false positives when a short prompt is a
    // coincidental substring of a fixed ssh option (e.g. "yes" ⊂ "BatchMode=yes").
    if !prompt.is_empty() && ssh_argv.iter().any(|arg| arg == prompt) {
        return Err("prompt content leaked into ssh argv — must be stdin only".to_owned());
    }
    Ok(PlannedPromptWrite {
        ssh_argv,
        remote_command: command,
        relative_path: relative_path.to_owned(),
        stdin_prompt: prompt.to_owned(),
    })
}

/// Validate that a prompt relative path is safe: it must start with `.jefe/`,
/// be relative (no leading `/`), and contain no path-traversal components
/// (`..`).
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

/// Shell-escape a single-quoted string (mirrors `runtime::commands`).
fn shell_escape(value: &str) -> String {
    format!("'{}'", value.replace('\'', r"'\''"))
}

/// Execute a production remote prompt write, reusing the planning seam.
///
/// This delegates to [`plan_remote_prompt_write`] to build the exact `ssh -T`
/// argv (with prompt bytes in stdin), then executes it. The plan guarantees:
/// - `.jefe/pr-prompt.md` (or the given relative path) is targeted.
/// - Prompt bytes are transferred via stdin (`cat > path`), never shell
///   interpolation.
/// - Adversarial content is absent from argv (validated by the planner).
///
/// # Errors
///
/// Returns `Err` when:
/// - The relative path is invalid (not `.jefe/`, absolute, traversal).
/// - The `ssh` process fails to spawn.
/// - The remote command exits nonzero.
pub(super) fn write_remote_prompt(
    remote: &RemoteRepositorySettings,
    work_dir: &Path,
    relative_path: &str,
    prompt: &str,
) -> Result<(), String> {
    let planned_write = plan_remote_prompt_write(remote, work_dir, relative_path, prompt)?;
    let plan = jefe::ssh::SshPlan::new(
        remote,
        &planned_write.remote_command,
        jefe::ssh::SshMode::NonInteractive,
    )
    .map_err(|error| error.to_string())?;
    let output = plan
        .execute(
            Some(planned_write.stdin_prompt.as_bytes()),
            jefe::ssh::SSH_OPERATION_TIMEOUT,
            None,
        )
        .map_err(|error| error.to_string())?;
    if output.status.success() {
        Ok(())
    } else {
        Err(jefe::ssh::classify_failure(
            output.status.code(),
            &String::from_utf8_lossy(&output.stderr),
        )
        .to_string())
    }
}

/// Re-export [`WorkTarget`] from `issue_prep` so this module's signatures
/// reference the shared target enum.
pub(super) use super::issue_prep::WorkTarget;

#[cfg(test)]
#[path = "remote_probe_tests.rs"]
mod tests;

//! npm / local-executable resolution helpers for runtime launch.
//!
//! Extracted from `commands.rs` so the npm/local-executable resolution logic
//! (remote npm probe script, remote env exports, remote CLI assembly, local
//! executable resolution, pane-command argv builders) lives in a cohesive
//! module under the per-file line limit. Remote/local runtime *orchestration*
//! (create/kill/attach sessions, SSH execution) stays in `commands.rs` which
//! calls these pure helpers.
//!
//! @plan PLAN-20260216-FIRSTVERSION-V1.P08
//! @requirement REQ-TECH-004

use std::ffi::OsString;
use std::path::Path;

use crate::domain::{AgentKind, LaunchSignature};

use super::agent_executable::{AgentExecutableResolver, ResolvedAgentExecutable};
use super::command_plan::{ExecutablePlan, LocalLaunchPlan};
use super::commands::{shell_escape_single, shell_join};
use super::errors::RuntimeError;

// ── Remote npm probe script ────────────────────────────────────────────────

/// Build the remote npm probe script (pure, for test verification).
///
/// The script checks for `npm` on the global remote PATH WITHOUT cd-ing
/// into the work directory. This is critical: the work directory may not
/// exist yet (clone-if-missing flow), and requiring `cd` would turn a
/// globally-installed npm into a spurious "npm not found" error.
///
/// The script normalizes the resolved path to an absolute path so a later
/// `cd` into the work directory cannot change which `npm` is invoked. On a
/// Unix remote, `command -v npm` may return a relative path if a relative
/// directory appears in PATH; a POSIX `case` statement prefixes such
/// relative paths with `$(pwd)` so the returned path is always absolute,
/// anchored at the filesystem root.
pub(super) fn remote_npm_probe_script() -> String {
    r#"p=$(command -v npm) || exit 1; case "$p" in /*) printf '%s\n' "$p";; *) printf '%s/%s\n' "$(pwd)" "$p";; esac"#
        .to_owned()
}

// ── Remote env exports / CLI assembly ─────────────────────────────────────

/// Build the remote environment export assignments for a launch signature.
///
/// Code Puppy agents do not receive any extra env exports. LLxprt agents
/// receive `SANDBOX_FLAGS` (and optionally `LLXPRT_SANDBOX_IMAGE`) when sandbox
/// is enabled, and `LLXPRT_DEBUG` when a debug level is set.
pub(super) fn remote_env_exports(signature: &LaunchSignature) -> Vec<String> {
    let mut env_exports = Vec::new();
    if signature.agent_kind == AgentKind::CodePuppy {
        return env_exports;
    }
    if signature.sandbox_enabled {
        env_exports.push(format!(
            "export SANDBOX_FLAGS={};",
            shell_escape_single(&signature.sandbox_flags)
        ));
        if let Some(image_ref) = std::env::var_os("LLXPRT_SANDBOX_IMAGE") {
            env_exports.push(format!(
                "export LLXPRT_SANDBOX_IMAGE={};",
                shell_escape_single(&image_ref.to_string_lossy())
            ));
        }
    }
    if !signature.llxprt_debug.is_empty() {
        env_exports.push(format!(
            "export LLXPRT_DEBUG={};",
            shell_escape_single(&signature.llxprt_debug)
        ));
    }
    env_exports
}

/// Assemble the remote CLI command string from an executable name and its
/// launch args, shell-escaping the executable path unless it is the bare
/// `llxprt` token (which needs no quoting).
pub(super) fn remote_cli_command(llxprt_command: &str, launch_args: &[String]) -> String {
    let executable = if llxprt_command == "llxprt" {
        llxprt_command.to_owned()
    } else {
        shell_escape_single(llxprt_command)
    };

    if launch_args.is_empty() {
        executable
    } else {
        format!("{} {}", executable, shell_join(launch_args))
    }
}

/// Assemble the remote CLI command from the executable plan, resolved agent
/// command, and launch args.
///
/// This is the pure CLI-assembly seam extracted from
/// `commands::build_remote_launch_command` so the remote shell-escaping of an
/// adversarial version selector is unit-testable without the SSH resolver
/// side effect.
///
/// For an [`ExecutablePlan::NpmExec`], the plan's `remote_command_prefix`
/// provides the fully shell-escaped `npm exec --yes --package=... -- llxprt`
/// tokens. Every token (including the version selector embedded in
/// `--package=`) is shell-escaped via single-quote wrapping so adversarial
/// metacharacters never reach the remote shell as syntax.
pub(super) fn assemble_remote_cli_command(
    plan: &ExecutablePlan,
    agent_command: &str,
    args: &[String],
) -> String {
    if plan.requires_npm() {
        let prefix = plan.remote_command_prefix(agent_command);
        if prefix.is_empty() {
            remote_cli_command(agent_command, args)
        } else if args.is_empty() {
            prefix
        } else {
            format!("{prefix} {}", shell_join(args))
        }
    } else {
        remote_cli_command(agent_command, args)
    }
}

// ── Local executable resolution ───────────────────────────────────────────

/// Resolve the platform-proven executable for a local launch plan.
///
/// Thin wrapper over [`resolve_local_executable_with_resolver`] that supplies
/// the live process resolver ([`AgentExecutableResolver::current`]). Extracted
/// so the cached-path-wins / no-cache-fallback contract is testable with an
/// injected resolver without touching the real process PATH.
pub(super) fn resolve_local_executable(
    plan: &LocalLaunchPlan,
    npm_executable: Option<&Path>,
) -> Result<ResolvedAgentExecutable, RuntimeError> {
    resolve_local_executable_with_resolver(
        plan,
        npm_executable,
        &AgentExecutableResolver::current(),
    )
}

/// Resolve the local executable for a launch plan using an explicit resolver.
///
/// For a [`ExecutablePlan::Direct`] plan, this resolves the agent runtime
/// binary (llxprt or code-puppy) through the supplied resolver.
///
/// For a [`ExecutablePlan::NpmExec`] plan, this resolves the `npm` binary
/// from the session-cached detection snapshot (if available) or the resolver.
/// The resolved executable carries a direct `node.exe` + `npm-cli.js`
/// invocation on Windows (see [`ResolvedAgentExecutable::npm_direct`]) so the
/// launcher never routes npm through `cmd.exe`; a non-standard npm layout
/// fails with a typed error instead of silently falling back.
///
/// Extracted as a production (non-test) function so tests prove the real
/// cached-path-wins and no-cache-fallback branches through the identical code
/// path that production uses — no duplicated test-only seam.
pub(super) fn resolve_local_executable_with_resolver(
    plan: &LocalLaunchPlan,
    npm_executable: Option<&Path>,
    resolver: &AgentExecutableResolver,
) -> Result<ResolvedAgentExecutable, RuntimeError> {
    match plan.plan {
        ExecutablePlan::Direct => resolver
            .resolve(plan.agent_kind)
            .map_err(RuntimeError::AgentExecutable),
        ExecutablePlan::NpmExec { .. } => {
            // Prefer the session-cached npm path (already normalized to an
            // absolute path) over a fresh PATH lookup so a long-lived tmux
            // server cannot resolve a different npm after detection. The
            // cached path is the authoritative source from the detection
            // snapshot; only fall back to a live resolver when no cache was
            // supplied (e.g. first launch before detection ran).
            //
            // On Windows both paths route through `from_path`/`resolve_named`
            // which derive a direct `node.exe` + `npm-cli.js` invocation for
            // `.cmd`/`.bat` npm wrappers or fail with a typed error — never
            // silently routing npm through `cmd.exe` (issue #269).
            if let Some(cached) = npm_executable {
                let classified = ResolvedAgentExecutable::from_path(cached)
                    .map_err(RuntimeError::AgentExecutable)?;
                // The cached path is authoritative: revalidate that it is
                // still present and launchable BEFORE returning it for a
                // prepared launch. This runs during non-destructive
                // `PreparedLocalLaunch::prepare`, so a stale cached npm
                // (uninstalled/moved/permission-changed since detection)
                // produces a typed `CachedNotLaunchable` error before any
                // kill — never silently falling back to a PATH lookup.
                classified
                    .validate_cached()
                    .map_err(RuntimeError::AgentExecutable)?;
                return Ok(classified);
            }
            resolver
                .resolve_named("npm")
                .map_err(RuntimeError::AgentExecutable)
        }
    }
}

/// Build the pane-command argv for a local agent session, following the
/// origin/main multiplexer contract: the executable is resolved separately
/// and passed to [`MultiplexerPlan::agent_pane_command_args`], which owns the
/// `env -u` scrub, wrapper strategy, and Windows launcher.
///
/// For a [`ExecutablePlan::Direct`] plan, the argv is just the trailing
/// launch args. For an [`ExecutablePlan::NpmExec`] plan, the argv includes
/// the `exec --yes --package=... -- llxprt` tokens from the plan.
pub(super) fn local_pane_command_argv(
    plan: &LocalLaunchPlan,
    _executable: &ResolvedAgentExecutable,
) -> Vec<OsString> {
    match plan.plan {
        ExecutablePlan::Direct => plan.args.iter().map(OsString::from).collect(),
        ExecutablePlan::NpmExec { .. } => {
            // The npm executable path is already in `executable`; the plan
            // provides the npm exec subcommand tokens and the agent args.
            let mut argv = plan.plan.local_argv_prefix(None);
            // local_argv_prefix emits "npm" as the first token; drop it since
            // the multiplexer launches the resolved executable directly.
            if argv.first().is_some_and(|first| first == "npm") {
                argv.remove(0);
            }
            argv.extend(plan.args.iter().map(OsString::from));
            argv
        }
    }
}

/// Build the complete Unix pane-command argv for regression tests.
///
/// This mirrors the historical `env -u TMUX ... <executable> <args>` argv that
/// [`MultiplexerPlan::pane_command_args`] produces on Unix, including the
/// issue #269 npm exec prefix. It is the pure, single-list form used by the
/// existing tests that assert the full pane argv.
///
/// Local production launch uses [`MultiplexerPlan::agent_pane_command_args`]
/// so native Windows never receives this Unix `env -u` prefix.
#[cfg(test)]
pub(super) fn local_pane_command_args(
    plan: &LocalLaunchPlan,
    npm_executable: Option<&Path>,
) -> Vec<std::ffi::OsString> {
    use super::commands::tmux_scrub_env_args;
    let mut args: Vec<std::ffi::OsString> =
        tmux_scrub_env_args().into_iter().map(Into::into).collect();
    for (key, value) in &plan.env {
        args.push(format!("{key}={value}").into());
    }
    // For a Direct plan, emit the binary name (llxprt or code-puppy). For an
    // NpmExec plan, the prefix already includes `npm exec ... -- llxprt`.
    match plan.plan {
        ExecutablePlan::Direct => {
            args.push(plan.agent_kind.binary_name().into());
        }
        ExecutablePlan::NpmExec { .. } => {
            args.extend(plan.plan.local_argv_prefix(npm_executable));
        }
    }
    args.extend(plan.args.iter().map(Into::into));
    args
}

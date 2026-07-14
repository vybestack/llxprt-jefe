//! Prepared-launch abstraction: resolve and validate all non-destructive
//! launch prerequisites BEFORE any kill, then execute from the prepared data.
//!
//! Architectural goal (issue #269): absolutely no destructive kill before all
//! non-destructive launch prerequisites are validated/resolved. A
//! [`PreparedLaunch`] carries the fully resolved local executable, multiplexer
//! plan, and launch plan (or the fully built remote command string) so the
//! post-kill spawn step never resolves again — eliminating drift between the
//! pre-kill validation and the post-kill execution.

use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::process::Command;

use tracing::debug;

use crate::domain::LaunchSignature;

use super::agent_executable::ResolvedAgentExecutable;
use super::command_plan::LocalLaunchPlan;
use super::errors::RuntimeError;
use super::multiplexer::{MultiplexerCapability, MultiplexerPlan};
use super::npm_launch;

// ── Prepared local launch ──────────────────────────────────────────────────

/// Fully resolved, validated local launch ready to execute after kill.
///
/// All non-destructive prerequisites — selector validation, multiplexer
/// resolution + capability preflight, local executable resolution — have
/// already succeeded. The prepared data is used after the destructive kill
/// to spawn the new session, avoiding a second resolution that could drift
/// from the pre-kill validation.
#[derive(Debug)]
pub struct PreparedLocalLaunch {
    session_name: String,
    work_dir: PathBuf,
    plan: LocalLaunchPlan,
    executable: ResolvedAgentExecutable,
    multiplexer: MultiplexerPlan,
}

/// Distinguishes runtime-resolution failures from command-execution failures
/// during the post-kill spawn attempt, mirroring the original
/// `LocalCreateFailure` so the fork-broken retry path is preserved.
#[derive(Debug)]
pub enum LocalExecuteFailure {
    /// A runtime error (resolution already done during prepare; only for
    /// command-construction edge cases).
    Runtime(RuntimeError),
    /// The tmux command exited non-zero; carries stderr for fork-broken
    /// detection.
    Command(String),
}

impl PreparedLocalLaunch {
    /// Non-destructive preparation: validate selector, resolve multiplexer +
    /// capabilities, resolve local executable. Does NOT kill or spawn.
    ///
    /// This is the single non-destructive entry point that `create_session` and
    /// `spawn_session_internal` call BEFORE any kill.
    pub fn prepare(
        session_name: &str,
        work_dir: &Path,
        signature: &LaunchSignature,
        npm_executable: Option<&Path>,
    ) -> Result<Self, RuntimeError> {
        validate_selector(signature)?;
        let multiplexer = resolve_multiplexer_with_capabilities()?;
        let plan = super::command_plan::local_launch_plan(signature);
        let executable = npm_launch::resolve_local_executable(&plan, npm_executable)?;
        Ok(Self {
            session_name: session_name.to_owned(),
            work_dir: work_dir.to_path_buf(),
            plan,
            executable,
            multiplexer,
        })
    }

    /// Build the tmux new-session `Command` from prepared data. No I/O beyond
    /// constructing the `Command` struct. All resolution was done during
    /// [`prepare`](Self::prepare).
    fn build_command(&self) -> Result<Command, RuntimeError> {
        let mut cmd = self.multiplexer.command();
        cmd.arg("new-session")
            .arg("-d")
            .arg("-s")
            .arg(&self.session_name)
            .arg("-c")
            .arg(&self.work_dir);

        let pane_args = npm_launch::local_pane_command_argv(&self.plan, &self.executable);
        let environment = self
            .plan
            .env
            .iter()
            .map(|(key, value)| (OsString::from(key), OsString::from(value)))
            .collect::<Vec<_>>();
        for arg in self
            .multiplexer
            .agent_pane_command_args(&self.executable, &pane_args, &environment)
            .map_err(RuntimeError::Multiplexer)?
        {
            cmd.arg(arg);
        }
        Ok(cmd)
    }

    /// Execute the prepared local launch (after kill). Spawns the tmux session
    /// and finalizes session options. No resolution happens here — everything
    /// was resolved during [`prepare`](Self::prepare).
    ///
    /// Returns `Ok(())` on success, or `Err(LocalExecuteFailure::Command(_))`
    /// carrying stderr for fork-broken detection, or
    /// `Err(LocalExecuteFailure::Runtime(_))` for command-construction errors.
    pub fn try_execute(&self) -> Result<(), LocalExecuteFailure> {
        let mut cmd = self.build_command().map_err(LocalExecuteFailure::Runtime)?;
        debug!(
            session_name = %self.session_name,
            "executing prepared local launch"
        );
        let output = cmd
            .output()
            .map_err(|e| LocalExecuteFailure::Command(e.to_string()))?;
        if output.status.success() {
            debug!(
                session_name = %self.session_name,
                "prepared local launch succeeded"
            );
            super::commands::finalize_local_session(&self.session_name, self.plan.warning.clone());
            Ok(())
        } else {
            Err(LocalExecuteFailure::Command(
                String::from_utf8_lossy(&output.stderr).into_owned(),
            ))
        }
    }

    /// The session name for this prepared launch.
    #[must_use]
    pub fn session_name(&self) -> &str {
        &self.session_name
    }
}

// ── Prepared remote launch ─────────────────────────────────────────────────

/// Fully resolved, validated remote launch ready to execute after kill.
///
/// The remote npm probe (SSH round-trip) runs during [`prepare`] so a missing
/// remote npm causes no destruction — the probe failure is returned before
/// any kill. The prepared `remote_command` is the fully-built, shell-escaped
/// tmux creation script, reused verbatim after kill to avoid probing twice.
#[derive(Debug)]
pub struct PreparedRemoteLaunch {
    session_name: String,
    remote: crate::domain::RemoteRepositorySettings,
    remote_command: String,
}

impl PreparedRemoteLaunch {
    /// Non-destructive preparation: validate selector, validate remote
    /// identity, probe remote npm, build the remote tmux command. Does NOT
    /// kill or spawn.
    pub fn prepare(
        session_name: &str,
        work_dir: &Path,
        signature: &LaunchSignature,
    ) -> Result<Self, RuntimeError> {
        validate_selector(signature)?;
        validate_remote_identity(signature)?;
        let remote_command =
            super::commands::build_remote_launch_command(session_name, work_dir, signature)?;
        Ok(Self {
            session_name: session_name.to_owned(),
            remote: signature.remote.clone(),
            remote_command,
        })
    }

    /// Execute the prepared remote launch (after kill). Runs the pre-built
    /// remote command via SSH. No re-probing occurs.
    pub fn execute(&self) -> Result<(), RuntimeError> {
        debug!(
            session_name = %self.session_name,
            "executing prepared remote launch"
        );
        let output = super::commands::run_remote_ssh(&self.remote, &self.remote_command)?;
        super::commands::ensure_remote_success(&self.remote, "remote tmux new-session", output)?;
        Ok(())
    }

    /// The session name for this prepared launch.
    #[must_use]
    pub fn session_name(&self) -> &str {
        &self.session_name
    }

    /// The remote settings (for kill dispatch).
    #[must_use]
    pub fn remote(&self) -> &crate::domain::RemoteRepositorySettings {
        &self.remote
    }
}

// ── Unified prepared launch ───────────────────────────────────────────────

/// A fully prepared launch — either local or remote — carrying everything
/// needed to kill and spawn without re-resolution.
#[derive(Debug)]
pub enum PreparedLaunch {
    /// A fully resolved local launch.
    Local(PreparedLocalLaunch),
    /// A fully resolved remote launch with the pre-built remote command.
    Remote(PreparedRemoteLaunch),
}

impl PreparedLaunch {
    /// Non-destructive preparation for either a local or remote launch.
    /// Validates the selector, resolves all prerequisites, and returns the
    /// prepared data. Does NOT kill or spawn.
    ///
    /// This is the single entry point that `create_session` calls BEFORE any
    /// kill, and that `restart_dispatch` calls before the restart kill.
    pub fn prepare(
        session_name: &str,
        work_dir: &Path,
        signature: &LaunchSignature,
        npm_executable: Option<&Path>,
    ) -> Result<Self, RuntimeError> {
        if super::commands::remote_is_enabled(&signature.remote) {
            let prepared = PreparedRemoteLaunch::prepare(session_name, work_dir, signature)?;
            Ok(Self::Remote(prepared))
        } else {
            let prepared =
                PreparedLocalLaunch::prepare(session_name, work_dir, signature, npm_executable)?;
            Ok(Self::Local(prepared))
        }
    }

    /// The session name for this prepared launch.
    #[must_use]
    pub fn session_name(&self) -> &str {
        match self {
            Self::Local(prepared) => prepared.session_name(),
            Self::Remote(prepared) => prepared.session_name(),
        }
    }

    /// Whether this is a remote launch.
    #[must_use]
    pub fn is_remote(&self) -> bool {
        matches!(self, Self::Remote(_))
    }

    /// The remote settings, if this is a remote launch.
    #[must_use]
    pub fn remote(&self) -> Option<&crate::domain::RemoteRepositorySettings> {
        match self {
            Self::Remote(prepared) => Some(prepared.remote()),
            Self::Local(_) => None,
        }
    }

    /// The local prepared launch, if this is a local launch.
    #[must_use]
    pub fn as_local(&self) -> Option<&PreparedLocalLaunch> {
        match self {
            Self::Local(prepared) => Some(prepared),
            Self::Remote(_) => None,
        }
    }
}

// ── Shared non-destructive validation helpers ─────────────────────────────

/// Validate the version selector (non-destructive). Rejects embedded NUL
/// bytes so a structurally unrepresentable selector causes no destruction.
fn validate_selector(signature: &LaunchSignature) -> Result<(), RuntimeError> {
    crate::domain::validate_version_selector(&signature.llxprt_version)
        .map_err(RuntimeError::InvalidVersionSelector)
}

/// Validate remote SSH identity fields (non-destructive).
fn validate_remote_identity(signature: &LaunchSignature) -> Result<(), RuntimeError> {
    crate::domain::target::validate_remote(&signature.remote)
        .map_err(RuntimeError::RemoteExecutionFailed)
}

/// Resolve the local multiplexer and preflight required capabilities
/// (non-destructive).
fn resolve_multiplexer_with_capabilities() -> Result<MultiplexerPlan, RuntimeError> {
    let plan = MultiplexerPlan::current().map_err(RuntimeError::Multiplexer)?;
    plan.preflight(&[
        MultiplexerCapability::AttachSession,
        MultiplexerCapability::PaneCapture,
    ])
    .map_err(RuntimeError::Multiplexer)?;
    Ok(plan)
}

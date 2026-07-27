//! Command-plan helpers for building and running `std::process::Command`.
//!
//! Every xtask command constructs its child process with `std::process::Command`
//! argument vectors — never a shell command string — so the same automation
//! runs natively on Windows and Unix (issue #459, A8).

use std::borrow::Cow;
use std::env;
use std::ffi::OsStr;
use std::path::PathBuf;
use std::process::{Command, Output, Stdio};

/// A captured failure while running an xtask-driven child process.
#[derive(Debug)]
pub struct CommandFailed {
    pub program: String,
    pub args: Vec<String>,
    pub status: Option<i32>,
    pub stdout: Vec<u8>,
    pub stderr: Vec<u8>,
}

impl std::fmt::Display for CommandFailed {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "command `{}` exited with status {:?}",
            shell_like(&self.program, &self.args),
            self.status
        )?;
        if !self.stdout.is_empty() {
            write!(
                f,
                "\n--- stdout ---\n{}",
                String::from_utf8_lossy(&self.stdout).trim_end()
            )?;
        }
        if !self.stderr.is_empty() {
            write!(
                f,
                "\n--- stderr ---\n{}",
                String::from_utf8_lossy(&self.stderr).trim_end()
            )?;
        }
        Ok(())
    }
}

impl std::error::Error for CommandFailed {}

/// A single planned child process plus the xtask-owned context that produced it.
///
/// `CommandPlan` is the unit the aggregate `ci` command sequences: building a
/// plan never spawns a process, which keeps the command-progression tests
/// deterministic (A1, A8).
#[derive(Debug, Clone)]
pub struct CommandPlan {
    pub program: String,
    pub args: Vec<String>,
    pub env: Vec<(String, String)>,
    pub current_dir: Option<PathBuf>,
}

impl CommandPlan {
    pub fn new(program: impl Into<String>) -> Self {
        Self {
            program: program.into(),
            args: Vec::new(),
            env: Vec::new(),
            current_dir: None,
        }
    }

    #[must_use]
    pub fn arg(mut self, arg: impl AsRef<OsStr>) -> Self {
        self.args.push(arg.as_ref().to_string_lossy().into_owned());
        self
    }

    #[must_use]
    pub fn args(mut self, args: impl IntoIterator<Item = impl AsRef<OsStr>>) -> Self {
        for arg in args {
            self.args.push(arg.as_ref().to_string_lossy().into_owned());
        }
        self
    }

    #[must_use]
    pub fn env(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.env.push((key.into(), value.into()));
        self
    }

    #[must_use]
    pub fn current_dir(mut self, dir: impl Into<PathBuf>) -> Self {
        self.current_dir = Some(dir.into());
        self
    }

    /// Build the underlying `std::process::Command` without running it.
    #[must_use]
    pub fn to_command(&self) -> Command {
        let mut cmd = Command::new(&self.program);
        cmd.args(&self.args);
        for (key, value) in &self.env {
            cmd.env(key, value);
        }
        if let Some(dir) = &self.current_dir {
            cmd.current_dir(dir);
        }
        cmd
    }

    /// Run the plan, inheriting stdio so the child's output streams to the
    /// caller (matching the old Makefile/CI shell behavior).
    ///
    /// # Errors
    /// Returns `CommandFailed` if the child cannot be spawned or exits nonzero.
    pub fn run_inherit(&self) -> Result<(), CommandFailed> {
        let status = self.to_command().status().map_err(|err| CommandFailed {
            program: self.program.clone(),
            args: self.args.clone(),
            status: None,
            stdout: Vec::new(),
            stderr: format!("failed to spawn `{}`: {err}", self.program).into_bytes(),
        })?;
        if status.success() {
            Ok(())
        } else {
            Err(CommandFailed {
                program: self.program.clone(),
                args: self.args.clone(),
                status: status.code(),
                stdout: Vec::new(),
                stderr: Vec::new(),
            })
        }
    }

    /// Run the plan, capturing stdout/stderr. Used by policy checks that need
    /// to inspect child output.
    ///
    /// # Errors
    /// Returns `CommandFailed` on spawn failure or nonzero exit.
    pub fn run_captured(&self) -> Result<Output, CommandFailed> {
        let output = self
            .to_command()
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .output()
            .map_err(|err| CommandFailed {
                program: self.program.clone(),
                args: self.args.clone(),
                status: None,
                stdout: Vec::new(),
                stderr: format!("failed to spawn `{}`: {err}", self.program).into_bytes(),
            })?;
        if output.status.success() {
            Ok(output)
        } else {
            Err(CommandFailed {
                program: self.program.clone(),
                args: self.args.clone(),
                status: output.status.code(),
                stdout: output.stdout,
                stderr: output.stderr,
            })
        }
    }

    /// Render the plan as a shell-like string for diagnostics and tests.
    #[must_use]
    pub fn render(&self) -> String {
        shell_like(&self.program, &self.args)
    }
}

/// Resolve the repository root (the directory containing the workspace
/// `Cargo.toml`). xtask is always invoked from a workspace member, so the
/// manifest directory is stable.
///
/// # Errors
/// Returns an error if `CARGO_MANIFEST_DIR` is unset (e.g. running the binary
/// outside a cargo invocation) — xtask is only supported via `cargo xtask`.
pub fn repo_root() -> Result<PathBuf, String> {
    let dir = env::var_os("CARGO_MANIFEST_DIR")
        .ok_or_else(|| "CARGO_MANIFEST_DIR is not set; run xtask via `cargo xtask`".to_string())?;
    // xtask/Cargo.toml -> repo root is the parent.
    let manifest_dir = PathBuf::from(dir);
    let parent = manifest_dir
        .parent()
        .map(PathBuf::from)
        .ok_or_else(|| "CARGO_MANIFEST_DIR has no parent; cannot locate repo root".to_string())?;
    Ok(parent)
}

/// Join a path onto the repository root.
///
/// # Errors
/// Propagates `repo_root` failures.
pub fn repo_path(relative: impl AsRef<OsStr>) -> Result<PathBuf, String> {
    Ok(repo_root()?.join(relative.as_ref()))
}

fn shell_like(program: &str, args: &[String]) -> String {
    let mut parts = Vec::with_capacity(args.len() + 1);
    parts.push(Cow::Borrowed(program));
    for arg in args {
        parts.push(Cow::Borrowed(arg.as_str()));
    }
    parts.join(" ")
}

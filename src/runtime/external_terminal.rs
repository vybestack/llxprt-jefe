//! External terminal launch boundary (issue #222).
//!
//! Provides typed, structural, cross-platform launch plans for opening a native
//! terminal emulator in the selected local agent's work directory. Fire-and-
//! forget: no process lifecycle tracking, no persistence.
//!
//! The launch plan is a pure value constructed from platform detection and the
//! work directory. The thin spawn boundary converts it into a `Command` with
//! Jefe's tmux client environment scrubbed. No shell-string interpolation is
//! used — argv stays structural.

use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use tracing::debug;

/// Environment variables that must never propagate into a terminal launched by
/// Jefe while it is itself attached to a tmux session. See
/// [`crate::runtime::commands`] for why this is mandatory (#171).
const TMUX_ENV_VARS_TO_SCRUB: &[&str] = &["TMUX", "TMUX_PANE", "TMUX_TMPDIR"];

/// Error returned by external-terminal plan construction or spawn.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ExternalTerminalError {
    /// The work directory does not exist or is not a directory.
    InvalidWorkDir(String),
    /// No supported terminal emulator was found and no `JEFE_TERMINAL` override
    /// was configured.
    NoTerminalFound,
    /// Spawning the resolved terminal program failed.
    SpawnFailed(String),
}

impl std::fmt::Display for ExternalTerminalError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::InvalidWorkDir(msg) => write!(f, "invalid work directory: {msg}"),
            Self::NoTerminalFound => {
                write!(
                    f,
                    "no terminal emulator found; set JEFE_TERMINAL to override"
                )
            }
            Self::SpawnFailed(msg) => write!(f, "failed to spawn terminal: {msg}"),
        }
    }
}

impl std::error::Error for ExternalTerminalError {}

/// A fully resolved, structural external-terminal launch plan.
///
/// Constructed via [`build_external_terminal_plan`]. The plan carries the
/// program name, argument vector, and work directory — never a shell command
/// string. The spawn boundary applies it directly to a `Command`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExternalTerminalPlan {
    /// Program to execute (terminal emulator binary name or full path).
    pub program: String,
    /// Structural argument vector (no shell interpolation).
    pub args: Vec<String>,
    /// Working directory the terminal opens in.
    pub work_dir: PathBuf,
}

impl ExternalTerminalPlan {
    /// Apply this plan to a `Command`, scrubbing tmux env vars.
    ///
    /// Extracted so the structural → `Command` mapping is unit-testable
    /// without spawning a process.
    #[must_use]
    pub fn to_command(&self) -> Command {
        let mut cmd = Command::new(&self.program);
        cmd.args(&self.args);
        cmd.current_dir(&self.work_dir);
        cmd.stdin(Stdio::null());
        cmd.stdout(Stdio::null());
        cmd.stderr(Stdio::null());
        for var in TMUX_ENV_VARS_TO_SCRUB {
            cmd.env_remove(var);
        }
        cmd
    }
}

/// Detect the current desktop platform for terminal-emulator resolution.
///
/// Centralised as a pure function so the per-platform branches are
/// unit-testable and CI can exercise the Windows path on any host.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DesktopPlatform {
    Macos,
    Linux,
    Windows,
}

impl DesktopPlatform {
    #[must_use]
    pub fn current() -> Self {
        if cfg!(target_os = "macos") {
            Self::Macos
        } else if cfg!(target_os = "windows") {
            Self::Windows
        } else {
            Self::Linux
        }
    }
}

/// Read the `JEFE_TERMINAL` environment override.
fn jefe_terminal_override() -> Option<String> {
    std::env::var("JEFE_TERMINAL")
        .ok()
        .filter(|s| !s.trim().is_empty())
}

/// Build a typed external-terminal launch plan for the given work directory
/// and platform.
///
/// Resolution order:
/// 1. `JEFE_TERMINAL` environment variable (user override).
/// 2. Auto-detect the emulator jefe is running inside (`TERM_PROGRAM` etc.).
/// 3. Platform-specific discovered/default emulator.
/// 4. `NoTerminalFound` error.
///
/// The work directory is validated to exist before the plan is built so callers
/// can surface an actionable warning without spawning.
pub fn build_external_terminal_plan(
    work_dir: &Path,
    platform: DesktopPlatform,
) -> Result<ExternalTerminalPlan, ExternalTerminalError> {
    if !work_dir.is_dir() {
        return Err(ExternalTerminalError::InvalidWorkDir(
            work_dir.to_string_lossy().into_owned(),
        ));
    }

    if let Some(override_prog) = jefe_terminal_override() {
        return Ok(plan_from_override(&override_prog, work_dir, platform));
    }

    // Auto-detect the terminal jefe is running inside, so a user in iTerm2 or
    // WezTerm gets that emulator rather than the platform default (#549).
    if let Some(detected) = detect_emulator_from_env()
        && let Some(plan) = plan_from_detected(&detected, work_dir, platform)
    {
        return Ok(plan);
    }

    match platform {
        DesktopPlatform::Macos => Ok(plan_macos(work_dir)),
        DesktopPlatform::Linux => plan_linux(work_dir),
        DesktopPlatform::Windows => Ok(plan_windows(work_dir)),
    }
}

/// Detect the terminal emulator jefe is running inside, from its own process
/// environment. Reads `TERM_PROGRAM` (set by iTerm2, Apple Terminal, WezTerm,
/// VS Code, …) and falls back to macOS' `__CFBundleIdentifier` bundle id.
fn detect_emulator_from_env() -> Option<String> {
    std::env::var("TERM_PROGRAM")
        .ok()
        .filter(|s| !s.is_empty())
        .or_else(|| {
            std::env::var("__CFBundleIdentifier")
                .ok()
                .filter(|s| !s.is_empty())
        })
}

/// Resolve a detected emulator identifier into a structural launch plan.
///
/// Returns `None` for unrecognized identifiers so the caller falls back to the
/// platform default rather than guessing. Pure: no env reads, no I/O.
fn plan_from_detected(
    detected: &str,
    work_dir: &Path,
    platform: DesktopPlatform,
) -> Option<ExternalTerminalPlan> {
    match platform {
        DesktopPlatform::Macos => {
            macos_app_for_emulator(detected).map(|app| plan_macos_open(work_dir, app))
        }
        DesktopPlatform::Linux => linux_plan_for_emulator(detected, work_dir),
        // Windows Terminal is already the `plan_windows` default, so a detected
        // identifier never overrides it.
        DesktopPlatform::Windows => None,
    }
}

/// Map a detected macOS terminal identifier (`TERM_PROGRAM` value or
/// `__CFBundleIdentifier` bundle id) to the app name `open -a` expects.
#[must_use]
fn macos_app_for_emulator(detected: &str) -> Option<&'static str> {
    match detected {
        "iTerm.app" | "com.googlecode.iterm2" => Some("iTerm"),
        "Apple_Terminal" | "com.apple.Terminal" => Some("Terminal"),
        "WezTerm" | "com.github.wez.wezterm" => Some("WezTerm"),
        _ => None,
    }
}

fn plan_from_override(
    program: &str,
    work_dir: &Path,
    platform: DesktopPlatform,
) -> ExternalTerminalPlan {
    // Overrides are arbitrary executables, so do not assume emulator-specific
    // flags. `Command::current_dir` supplies the portable working directory.
    let (resolved_program, resolved_args) = match platform {
        DesktopPlatform::Macos | DesktopPlatform::Linux | DesktopPlatform::Windows => {
            (program.to_owned(), Vec::new())
        }
    };

    ExternalTerminalPlan {
        program: resolved_program,
        args: resolved_args,
        work_dir: work_dir.to_path_buf(),
    }
}

/// macOS default: Terminal.app via `open`.
fn plan_macos(work_dir: &Path) -> ExternalTerminalPlan {
    plan_macos_open(work_dir, "Terminal")
}

/// macOS: open a named app (`open -a <app> <workdir>`) in the work directory.
/// Used both for the default and for a detected/override app name (#549).
fn plan_macos_open(work_dir: &Path, app: &str) -> ExternalTerminalPlan {
    ExternalTerminalPlan {
        program: "open".to_owned(),
        args: vec![
            "-a".to_owned(),
            app.to_owned(),
            work_dir.to_string_lossy().into_owned(),
        ],
        work_dir: work_dir.to_path_buf(),
    }
}

type TerminalArgBuilder = fn(&Path) -> Vec<String>;

/// Linux: discover a common emulator, falling back to `xterm`.
fn plan_linux(work_dir: &Path) -> Result<ExternalTerminalPlan, ExternalTerminalError> {
    let candidates: &[(&str, TerminalArgBuilder)] = &[
        ("gnome-terminal", linux_gnome_args),
        ("konsole", linux_konsole_args),
        ("xfce4-terminal", linux_xfce_args),
        ("xterm", linux_xterm_args),
    ];

    for (program, arg_fn) in candidates {
        if which(program).is_some() {
            return Ok(ExternalTerminalPlan {
                program: (*program).to_owned(),
                args: arg_fn(work_dir),
                work_dir: work_dir.to_path_buf(),
            });
        }
    }

    Err(ExternalTerminalError::NoTerminalFound)
}

/// Build a structural plan for a known Linux emulator detected from the
/// environment (`TERM_PROGRAM`). Returns `None` for unrecognized emulators so
/// the caller falls back to PATH discovery. Pure: no env reads, no I/O.
fn linux_plan_for_emulator(detected: &str, work_dir: &Path) -> Option<ExternalTerminalPlan> {
    let program = match detected {
        "WezTerm" => "wezterm",
        _ => return None,
    };
    Some(ExternalTerminalPlan {
        program: program.to_owned(),
        args: vec![
            "start".to_owned(),
            "--cwd".to_owned(),
            work_dir.to_string_lossy().into_owned(),
        ],
        work_dir: work_dir.to_path_buf(),
    })
}

fn linux_gnome_args(work_dir: &Path) -> Vec<String> {
    vec![format!("--working-directory={}", work_dir.display())]
}

fn linux_konsole_args(work_dir: &Path) -> Vec<String> {
    vec![
        "--workdir".to_owned(),
        work_dir.to_string_lossy().into_owned(),
    ]
}

fn linux_xfce_args(work_dir: &Path) -> Vec<String> {
    vec![
        "--working-directory".to_owned(),
        work_dir.to_string_lossy().into_owned(),
    ]
}

fn linux_xterm_args(_work_dir: &Path) -> Vec<String> {
    // xterm has no --working-directory; it inherits cwd from the Command.
    Vec::new()
}

/// Windows: Windows Terminal (`wt.exe`), then `start cmd` new-console fallback.
fn plan_windows(work_dir: &Path) -> ExternalTerminalPlan {
    if which("wt.exe").is_some() {
        // `wt -d <dir>` — structural argv (two separate args, not a shell string).
        return ExternalTerminalPlan {
            program: "wt.exe".to_owned(),
            args: vec!["-d".to_owned(), work_dir.to_string_lossy().into_owned()],
            work_dir: work_dir.to_path_buf(),
        };
    }

    // Fallback: `cmd /C start cmd /K` opens a new console window. The working
    // directory is set by the `Command::current_dir()` call in `to_command()`
    // — no shell-string `cd` is injected, keeping the argv fully structural.
    ExternalTerminalPlan {
        program: "cmd".to_owned(),
        args: vec![
            "/C".to_owned(),
            "start".to_owned(),
            "cmd".to_owned(),
            "/K".to_owned(),
        ],
        work_dir: work_dir.to_path_buf(),
    }
}

/// Spawn an external terminal from a launch plan. Fire-and-forget: the child is
/// detached and not tracked.
pub fn spawn_external_terminal(plan: &ExternalTerminalPlan) -> Result<(), ExternalTerminalError> {
    let mut cmd = plan.to_command();
    debug!(program = %plan.program, work_dir = %plan.work_dir.display(), "spawning external terminal");
    let mut child = cmd
        .spawn()
        .map_err(|e| ExternalTerminalError::SpawnFailed(format!("{}: {e}", plan.program)))?;
    std::thread::spawn(move || {
        if let Err(error) = child.wait() {
            tracing::warn!(error = %error, "external terminal launcher wait failed");
        }
    });
    Ok(())
}

/// Minimal `which`-style lookup for a program on PATH.
fn which(program: &str) -> Option<PathBuf> {
    let path = std::env::var_os("PATH")?;
    for dir in std::env::split_paths(&path) {
        let candidate = dir.join(program);
        let exe_candidate = if cfg!(windows) {
            let with_exe = dir.join(format!("{program}.exe"));
            if candidate.is_file() {
                candidate
            } else {
                with_exe
            }
        } else {
            candidate
        };
        if is_executable_file(&exe_candidate) {
            return Some(exe_candidate);
        }
    }
    None
}

#[cfg(unix)]
fn is_executable_file(path: &Path) -> bool {
    use std::os::unix::fs::PermissionsExt;

    std::fs::metadata(path)
        .is_ok_and(|metadata| metadata.is_file() && metadata.permissions().mode() & 0o111 != 0)
}

#[cfg(not(unix))]
fn is_executable_file(path: &Path) -> bool {
    path.is_file()
}

#[cfg(test)]
#[path = "external_terminal_tests.rs"]
mod tests;

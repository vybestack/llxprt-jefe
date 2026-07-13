//! Controlled runtime detection via run-scoped PATH shims.
//!
//! The tutorial-capture workflow must be able to control which agent runtimes
//! the launched Jefe process sees, without relying on whichever tools happen to
//! be installed on the host. This module plans the shim directory contents and
//! generates the shim script text. The orchestration layer writes the files.
//!
//! ## Boundary
//!
//! This module owns shim *planning* (what shims to create) and shim *content
//! generation* (the script text). File I/O is performed by the orchestration
//! layer so this module stays pure and testable.
//!
//! @requirement REQ-TUTORIAL-CAPTURE-003

use std::fs;
use std::path::PathBuf;
use std::process::Command;

use super::manifest::RuntimeProfile;

/// The stable marker text the deterministic shim prints, so scenarios can
/// assert on it.
pub const SHIM_MARKER: &str = "[jefe-tutorial-shim]";

/// Which agent runtime shims to install when using the `Shim` runtime profile.
///
/// This controls which agent binaries Jefe's startup detection sees, so
/// tutorial-capture can exercise llxprt-only, code-puppy-only, or both-installed
/// startup scenarios deterministically.
///
/// @requirement REQ-TUTORIAL-CAPTURE-003
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ShimAvailability {
    /// Install only the `llxprt` shim. `code-puppy` will not be detected.
    LlxprtOnly,
    /// Install only the `code-puppy` shim. `llxprt` will not be detected.
    CodePuppyOnly,
    /// Install both `llxprt` and `code-puppy` shims (default).
    #[default]
    Both,
}

impl ShimAvailability {
    /// Human-readable label for this availability selection.
    #[must_use]
    pub fn label(self) -> &'static str {
        match self {
            Self::LlxprtOnly => "llxprt-only",
            Self::CodePuppyOnly => "code-puppy-only",
            Self::Both => "both",
        }
    }

    /// Parse a string into a `ShimAvailability`.
    ///
    /// Accepts `llxprt-only`, `code-puppy-only`, `both` (case-insensitive).
    ///
    /// @requirement REQ-TUTORIAL-CAPTURE-003
    #[must_use]
    pub fn parse(value: &str) -> Option<Self> {
        match value.trim().to_ascii_lowercase().as_str() {
            "llxprt-only" | "llxprt_only" => Some(Self::LlxprtOnly),
            "code-puppy-only" | "code_puppy_only" => Some(Self::CodePuppyOnly),
            "both" => Some(Self::Both),
            _ => None,
        }
    }

    /// Whether the `llxprt` shim should be installed.
    #[must_use]
    pub const fn includes_llxprt(self) -> bool {
        matches!(self, Self::LlxprtOnly | Self::Both)
    }

    /// Whether the `code-puppy` shim should be installed.
    #[must_use]
    pub const fn includes_code_puppy(self) -> bool {
        matches!(self, Self::CodePuppyOnly | Self::Both)
    }
}

/// A planned shim executable to be written to the shim directory.
///
/// @requirement REQ-TUTORIAL-CAPTURE-003
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlannedShim {
    /// Executable name Jefe detects (e.g. `llxprt`, `code-puppy`).
    pub binary_name: String,
    /// Shell script content to write.
    pub script: String,
}

/// Error returned when shim planning fails.
///
/// @requirement REQ-TUTORIAL-CAPTURE-003
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ShimError {
    /// The requested real runtime executable was not found on PATH.
    RealRuntimeNotFound { binary: String },
}

impl std::fmt::Display for ShimError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::RealRuntimeNotFound { binary } => {
                write!(f, "real runtime '{binary}' not found on PATH")
            }
        }
    }
}

impl std::error::Error for ShimError {}

/// Plan which shims to create for a given runtime profile.
///
/// - `Shim` profile: create deterministic shims based on `availability`
///   (llxprt-only, code-puppy-only, or both).
/// - `RealLlxprt`: uses the real `llxprt` binary from PATH. Does NOT inject
///   any shim for the other runtime.
/// - `RealCodePuppy`: uses the real `code-puppy` binary from PATH. Does NOT
///   inject any shim for the other runtime.
///
/// **Finding #6**: Real runtime profiles must NOT inject opposite shims.
///
/// @requirement REQ-TUTORIAL-CAPTURE-003
#[must_use]
pub fn plan_shims(profile: RuntimeProfile, availability: ShimAvailability) -> Vec<PlannedShim> {
    match profile {
        RuntimeProfile::Shim => {
            let mut shims = Vec::new();
            if availability.includes_llxprt() {
                shims.push(deterministic_shim("llxprt"));
            }
            if availability.includes_code_puppy() {
                shims.push(deterministic_shim("code-puppy"));
            }
            shims
        }
        // Finding #6: real profiles must not inject opposite shims.
        RuntimeProfile::RealLlxprt | RuntimeProfile::RealCodePuppy => Vec::new(),
    }
}

/// Validate that a real runtime profile's requested executable is available
/// on the host PATH. Returns an error if the executable cannot be found.
///
/// **Finding #6**: This function checks that the requested executable EXISTS
/// on PATH. It does NOT claim that Jefe will detect it — Jefe detection can
/// only be verified by actually launching Jefe. The `validate-runtime`
/// subcommand performs the actual launch+detection validation.
///
/// @requirement REQ-TUTORIAL-CAPTURE-003
///
/// # Errors
///
/// Returns [`ShimError`] if the requested executable is not found.
pub fn validate_real_runtime(profile: RuntimeProfile) -> Result<(), ShimError> {
    match profile {
        RuntimeProfile::Shim => Ok(()),
        RuntimeProfile::RealLlxprt => {
            if which("llxprt").is_none() {
                return Err(ShimError::RealRuntimeNotFound {
                    binary: "llxprt".to_string(),
                });
            }
            Ok(())
        }
        RuntimeProfile::RealCodePuppy => {
            if which("code-puppy").is_none() {
                return Err(ShimError::RealRuntimeNotFound {
                    binary: "code-puppy".to_string(),
                });
            }
            Ok(())
        }
    }
}

/// Find a binary on PATH (like `which`). Returns the resolved path if found.
#[must_use]
pub fn which(binary: &str) -> Option<PathBuf> {
    let path = std::env::var("PATH").ok()?;
    for dir in path.split(':') {
        if dir.is_empty() {
            continue;
        }
        let candidate = PathBuf::from(dir).join(binary);
        if is_executable(&candidate) {
            return Some(candidate);
        }
    }
    // Fall back to `which` command if available.
    let output = Command::new("which").arg(binary).output().ok()?;
    if output.status.success() {
        let path_str = String::from_utf8_lossy(&output.stdout).trim().to_string();
        if !path_str.is_empty() {
            return Some(PathBuf::from(path_str));
        }
    }
    None
}

/// Whether a path exists and is executable (Unix only).
#[cfg(unix)]
fn is_executable(path: &std::path::Path) -> bool {
    use std::os::unix::fs::PermissionsExt;
    fs::metadata(path)
        .map(|m| m.is_file() && (m.permissions().mode() & 0o111 != 0))
        .unwrap_or(false)
}

/// Whether a path exists and is executable (non-Unix: just checks existence).
#[cfg(not(unix))]
fn is_executable(path: &std::path::Path) -> bool {
    fs::metadata(path).map(|m| m.is_file()).unwrap_or(false)
}

/// Build a deterministic interactive shim script for the given binary name.
///
/// The shim prints the stable marker on startup, echoes back any input it
/// receives, and stays alive so Jefe can interact with it in the terminal pane.
/// It exits on `Ctrl-C` or `Ctrl-D`.
///
/// @requirement REQ-TUTORIAL-CAPTURE-003
#[must_use]
pub fn deterministic_shim(binary_name: &str) -> PlannedShim {
    PlannedShim {
        binary_name: binary_name.to_string(),
        script: deterministic_shim_script(),
    }
}

/// Generate the deterministic shim script body.
///
/// The script is a POSIX-compatible shell script that:
/// 1. Prints the stable marker `[jefe-tutorial-shim]` and the binary name.
/// 2. Reads lines from stdin and echoes them back (simulating an agent that
///    acknowledges input).
/// 3. Exits cleanly on EOF (Ctrl-D) or SIGINT.
fn deterministic_shim_script() -> String {
    format!(
        r#"#!/bin/sh
# jefe-tutorial-capture deterministic runtime shim.
# Prints stable marker text and echoes input. No real runtime behavior.
echo "{SHIM_MARKER}"
echo "runtime-shim: ready"
while IFS= read -r line; do
    echo "> $line"
done
echo "runtime-shim: exited"
"#
    )
}

/// Compute the PATH string for a run: the shim directory prepended to the
/// inherited system PATH.
///
/// The shim directory takes precedence so Jefe's startup detection finds the
/// shim executables. The inherited PATH is retained so Jefe and tmux can still
/// find system tools.
///
/// @requirement REQ-TUTORIAL-CAPTURE-003
#[must_use]
pub fn controlled_path(shim_dir: &std::path::Path, inherited_path: &str) -> String {
    format!("{}:{}", shim_dir.to_string_lossy(), inherited_path)
}

/// Build a detection-only PATH that isolates the launched process from the
/// host's opposite-runtime agent binary using **curated PATH projection**.
///
/// Instead of inheriting the host PATH and filtering out directories (which
/// is fragile — a binary could be hidden in a directory that also has system
/// tools), this function creates a **curated bin directory** that is the
/// ONLY PATH the process sees. The curated bin contains:
///
/// 1. **Selected runtime shims** (for `Shim` profile) or symlinks to the real
///    runtime binary (for `RealLlxprt`/`RealCodePuppy` profiles).
/// 2. **Symlinks to required system tools** found on the inherited PATH
///    (git, tmux, sh, etc.) — but only for tools that are NOT agent runtimes.
///
/// This ensures:
/// - The selected runtime is the only agent runtime detected.
/// - Required system tools (git, tmux, sh) are available via symlinks.
/// - No inherited directory is used directly, so a stray opposite-runtime
///   binary in a system tool directory cannot leak through.
///
/// **Finding #2**: Curated PATH projection replaces the unsafe
/// directory-dropping filter.
///
/// @requirement REQ-TUTORIAL-CAPTURE-003
#[must_use]
pub fn detection_path(
    shim_dir: &std::path::Path,
    _profile: RuntimeProfile,
    _inherited_path: &str,
) -> String {
    // The curated bin directory is the only entry in PATH.
    // The orchestration layer writes shims and system-tool symlinks into it.
    // We return just the bin directory — no inherited PATH entries are
    // appended, so the process sees ONLY what the harness projected.
    shim_dir.to_string_lossy().into_owned()
}

/// Plan the system-tool symlinks to create in the curated bin directory.
///
/// Scans the inherited PATH for required system tools and returns a list of
/// (name, target) pairs for symlink creation. Agent runtime binaries are
/// excluded so only the selected runtime (via shim or real symlink) is
/// detected.
///
/// **Finding #2**: System tools are projected as symlinks rather than
/// inheriting PATH directories.
///
/// @requirement REQ-TUTORIAL-CAPTURE-003
#[must_use]
pub fn plan_system_tool_links(inherited_path: &str) -> Vec<SystemToolLink> {
    let mut links = Vec::new();
    for tool in REQUIRED_SYSTEM_TOOLS {
        if let Some(target) = resolve_system_tool(tool, inherited_path) {
            links.push(SystemToolLink {
                name: (*tool).to_string(),
                target,
            });
        }
    }
    links
}

/// A planned symlink for a system tool in the curated bin directory.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SystemToolLink {
    /// Tool name (e.g. "git", "tmux").
    pub name: String,
    /// Absolute path to the real binary on the host.
    pub target: PathBuf,
}

/// Required system tools that must be available in the curated bin for
/// Tier A (all) and Tier B (additionally gh).
///
/// **Finding #1**: `sh`, `git`, and `tmux` are required for Tier A (all
/// profiles). `gh` is required only for Tier B (GitHub fixture execution).
/// Agent runtime names are NOT included — they are handled separately by
/// `plan_shims` or real-runtime symlinks.
///
/// In addition to the original three, the curated bin must also project:
/// - `env` — Jefe's runtime uses `env -u TMUX -u TMUX_PANE -u TMUX_TMPDIR`
///   as the pane-command prefix when creating agent sessions (commands.rs).
///   Without `env` on PATH, the agent pane dies immediately and the agent
///   shows "Dead".
/// - `id` — Jefe's socket resolver (socket.rs) shells out to `id -u` to
///   build the UID-suffixed private socket filename. Without it, the socket
///   degrades to a shared name, which breaks isolation and can collide.
/// - `kill` — Jefe's liveness checker (liveness.rs) uses `kill -0` for the
///   PID-based fallback. Without it, liveness cannot distinguish dead workers
///   from live ones.
const TIER_A_REQUIRED_TOOLS: &[&str] = &["sh", "git", "tmux", "env", "id", "kill"];

/// Tier B required tools: Tier A required tools plus `gh` (GitHub fixture
/// execution). `gh` is the only additional tool Tier B requires beyond Tier A.
const TIER_B_REQUIRED_TOOLS: &[&str] = &["sh", "git", "tmux", "env", "id", "kill", "gh"];

/// All system tools projected into the curated bin: Tier A required tools
/// plus `gh` (Tier B only). The projection is unconditional — `gh` is
/// projected if present so Tier B works without re-preparing, but it is
/// not a Tier A requirement (checked separately by
/// `check_tier_a_required_tools`).
const REQUIRED_SYSTEM_TOOLS: &[&str] = &["git", "tmux", "sh", "env", "id", "kill", "gh"];

/// Resolve a system tool to its absolute path on the inherited PATH.
///
/// **Finding #1**: No directory exclusion. The curated projection must
/// project required tools by executable path even if the directory also
/// contains agent binaries. Symlinking only tmux/gh into the curated bin
/// would not expose agents because agents are projected separately (via
/// `plan_shims` or `plan_real_runtime_link`), not inherited from PATH
/// directories. The only PATH the process sees is the curated bin, so
/// the presence of an agent binary in the source directory is irrelevant
/// — it is never symlinked as a system tool.
fn resolve_system_tool(tool: &str, inherited_path: &str) -> Option<PathBuf> {
    for dir in inherited_path.split(':') {
        if dir.is_empty() {
            continue;
        }
        let candidate = PathBuf::from(dir).join(tool);
        if is_executable(&candidate) {
            return Some(candidate);
        }
    }
    None
}

/// Check that all Tier A required tools (sh, git, tmux) are available on PATH.
///
/// **Finding #1**: `prepare_run` must fail if any Tier A required tool is
/// unavailable. Returns the list of missing tools (empty if all present).
#[must_use]
pub fn check_tier_a_required_tools(inherited_path: &str) -> Vec<String> {
    check_required_tools(TIER_A_REQUIRED_TOOLS, inherited_path)
}

/// Check that all Tier B required tools (sh, git, tmux, env, id, kill, gh)
/// are available on PATH.
///
/// **Finding #1**: `gh` is required only for Tier B. Returns the list of
/// missing tools (empty if all present).
#[must_use]
pub fn check_tier_b_required_tools(inherited_path: &str) -> Vec<String> {
    check_required_tools(TIER_B_REQUIRED_TOOLS, inherited_path)
}

/// Check that a set of required tools are all available on PATH.
fn check_required_tools(tools: &[&str], inherited_path: &str) -> Vec<String> {
    tools
        .iter()
        .filter(|tool| resolve_system_tool(tool, inherited_path).is_none())
        .map(|tool| (*tool).to_string())
        .collect()
}

/// Plan the real-runtime symlink for a `RealLlxprt` or `RealCodePuppy` profile.
///
/// Returns the binary name and resolved path to symlink into the curated bin.
///
/// **Finding #2**: Real runtime is projected as a symlink into the curated
/// bin, not inherited from PATH.
///
/// @requirement REQ-TUTORIAL-CAPTURE-003
#[must_use]
pub fn plan_real_runtime_link(profile: RuntimeProfile) -> Option<SystemToolLink> {
    match profile {
        RuntimeProfile::RealLlxprt => which("llxprt").map(|target| SystemToolLink {
            name: "llxprt".to_string(),
            target,
        }),
        RuntimeProfile::RealCodePuppy => which("code-puppy").map(|target| SystemToolLink {
            name: "code-puppy".to_string(),
            target,
        }),
        RuntimeProfile::Shim => None,
    }
}

/// Whether a given binary name is a recognized agent runtime.
#[must_use]
pub fn is_agent_binary(name: &str) -> bool {
    matches!(name, "llxprt" | "code-puppy")
}

#[cfg(test)]
#[path = "path_shim_tests.rs"]
mod tests;

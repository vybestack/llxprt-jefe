//! One captured PATH snapshot plus the platform launchable-file policy used by
//! the generic candidate resolver (issue #382 CW-02 S2).
//!
//! The issue's deterministic algorithm #1 mandates: "Snapshot PATH once at
//! startup. ... Path-name candidate values ... are resolved ... from the same
//! PATH snapshot." This module owns that immutable snapshot and the pure
//! platform launchable-file predicate, reusing the audited platform policy
//! already shipped in [`crate::runtime::AgentExecutableResolver`]. It performs
//! only filesystem `stat`-style reads (canonicalize/metadata) — never process
//! spawns — because the resolver's contract is "select the first physically
//! valid candidate" without probing identity or capabilities.
//!
//! Product knowledge lives only in the shipped definition data; this module
//! knows nothing about any agent. It resolves a bare binary name (or a
//! repository-local relative path) to a launchable path under one captured
//! PATH snapshot and one platform policy.

use std::ffi::OsStr;
use std::path::{Path, PathBuf};

/// Operating-system executable-resolution policy.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AgentExecutablePlatform {
    /// Extensionless executable files with Unix execute permissions.
    Unix,
    /// Native Windows PATHEXT resolution plus explicitly supported PowerShell wrappers.
    Windows,
}

impl AgentExecutablePlatform {
    /// Return the current target's policy.
    #[must_use]
    pub const fn current() -> Self {
        if cfg!(windows) {
            Self::Windows
        } else {
            Self::Unix
        }
    }
}

/// Process strategy required by a resolved executable form.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum AgentWrapperKind {
    /// Native executable that can be started directly.
    Direct,
    /// Windows command script requiring `cmd.exe` mediation.
    CommandScript,
    /// PowerShell script requiring explicit PowerShell mediation.
    PowerShellScript,
}

/// One captured, immutable PATH snapshot for the candidate resolver.
///
/// Constructed once at the composition/startup boundary from the current
/// process PATH/PATHEXT, or from explicit inputs for deterministic tests.
/// Holding the directories and PATHEXT together makes resolution a pure read
/// over fixed inputs: the resolver never touches `std::env` and never spawns.
#[derive(Debug, Clone)]
pub struct PathSnapshot {
    platform: AgentExecutablePlatform,
    directories: Vec<PathBuf>,
    pathext: Option<std::ffi::OsString>,
}

impl PathSnapshot {
    /// Capture the current process PATH and PATHEXT under the current
    /// platform policy. Used once at startup/composition.
    #[must_use]
    pub fn current() -> Self {
        let directories = std::env::var_os("PATH")
            .map(|path| std::env::split_paths(&path).collect())
            .unwrap_or_default();
        Self::for_platform(
            AgentExecutablePlatform::current(),
            directories,
            std::env::var_os("PATHEXT"),
        )
    }

    /// Construct a deterministic snapshot for explicit platform inputs.
    #[must_use]
    pub const fn for_platform(
        platform: AgentExecutablePlatform,
        directories: Vec<PathBuf>,
        pathext: Option<std::ffi::OsString>,
    ) -> Self {
        Self {
            platform,
            directories,
            pathext,
        }
    }

    /// The platform policy this snapshot resolves under.
    #[must_use]
    pub const fn platform(&self) -> AgentExecutablePlatform {
        self.platform
    }

    /// Borrow the captured PATH directories in declaration order.
    #[must_use]
    pub fn directories(&self) -> &[PathBuf] {
        &self.directories
    }

    /// Resolve a bare binary name to the first launchable file under this
    /// snapshot, applying the platform's extension and launchable policy.
    ///
    /// Returns the resolved path and the wrapper kind the runtime must apply
    /// (direct, command-script, or PowerShell wrapper) so the later planner
    /// can launch it through the same audited policy as today. Returns `None`
    /// when no launchable candidate exists. This is a pure filesystem read;
    /// it does not spawn.
    #[must_use]
    pub fn resolve_binary(&self, name: &str) -> Option<(PathBuf, AgentWrapperKind)> {
        match self.platform {
            AgentExecutablePlatform::Unix => self.resolve_unix(name),
            AgentExecutablePlatform::Windows => self.resolve_windows(name),
        }
    }

    fn resolve_unix(&self, name: &str) -> Option<(PathBuf, AgentWrapperKind)> {
        for directory in &self.directories {
            let path = directory.join(name);
            if unix_launchable(&path) {
                return Some((path, AgentWrapperKind::Direct));
            }
        }
        None
    }

    fn resolve_windows(&self, name: &str) -> Option<(PathBuf, AgentWrapperKind)> {
        let extensions = windows_extensions(self.pathext.as_deref());
        for directory in &self.directories {
            for (extension, wrapper_kind) in &extensions {
                let path = directory.join(format!("{name}{extension}"));
                if path.is_file() {
                    return Some((path, *wrapper_kind));
                }
            }
        }
        None
    }
}

/// Resolve a repository-local relative path to a launchable file.
///
/// The candidate contract allows the typed `repository-llxprt` candidate
/// (whose value is `<repo>/.llxprt/bin/llxprt`) to be resolved directly from
/// the repository root. This is the one allowlisted product adapter in the
/// candidate contract; the resolver applies it generically to any validated
/// repository-local candidate relative path joined to the repository root.
///
/// Returns the joined path and wrapper kind when the file is launchable under
/// the snapshot's platform policy; `None` otherwise.
#[must_use]
pub fn resolve_repository_local(
    snapshot: &PathSnapshot,
    repository_root: &Path,
    relative: &Path,
) -> Option<(PathBuf, AgentWrapperKind)> {
    let path = repository_root.join(relative);
    let launchable = match snapshot.platform() {
        AgentExecutablePlatform::Unix => unix_launchable(&path),
        AgentExecutablePlatform::Windows => path.is_file(),
    };
    if launchable {
        Some((path, AgentWrapperKind::Direct))
    } else {
        None
    }
}

#[cfg(unix)]
fn unix_launchable(path: &Path) -> bool {
    use std::os::unix::fs::PermissionsExt;

    std::fs::metadata(path)
        .is_ok_and(|metadata| metadata.is_file() && metadata.permissions().mode() & 0o111 != 0)
}

#[cfg(not(unix))]
fn unix_launchable(path: &Path) -> bool {
    path.is_file()
}

fn windows_extensions(pathext: Option<&OsStr>) -> Vec<(String, AgentWrapperKind)> {
    // Mirrors the audited PATHEXT classification in `agent_executable.rs` so
    // the generic resolver resolves exactly the same set of Windows launchable
    // forms as today's product-specific resolver.
    const WINDOWS_DEFAULT_PATHEXT: &str = ".COM;.EXE;.BAT;.CMD";
    let source = pathext
        .and_then(OsStr::to_str)
        .filter(|value| !value.is_empty())
        .unwrap_or(WINDOWS_DEFAULT_PATHEXT);
    let mut extensions = source
        .split(';')
        .filter_map(classify_windows_extension)
        .collect::<Vec<_>>();
    if !extensions.iter().any(|(extension, _)| extension == ".ps1") {
        extensions.push((".ps1".to_owned(), AgentWrapperKind::PowerShellScript));
    }
    extensions
}

fn classify_windows_extension(extension: &str) -> Option<(String, AgentWrapperKind)> {
    let extension = extension.trim();
    if extension.is_empty() {
        return None;
    }
    let normalized = if extension.starts_with('.') {
        extension.to_ascii_lowercase()
    } else {
        format!(".{}", extension.to_ascii_lowercase())
    };
    let wrapper_kind = match normalized.as_str() {
        ".exe" | ".com" => AgentWrapperKind::Direct,
        ".cmd" | ".bat" => AgentWrapperKind::CommandScript,
        ".ps1" => AgentWrapperKind::PowerShellScript,
        _ => return None,
    };
    Some((normalized, wrapper_kind))
}

#[cfg(test)]
#[path = "agent_candidate_path_tests.rs"]
mod tests;

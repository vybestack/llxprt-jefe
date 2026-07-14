//! Platform-owned resolution of launchable local agent executables.

use std::ffi::{OsStr, OsString};
use std::path::{Path, PathBuf};

use crate::domain::AgentKind;

const WINDOWS_DEFAULT_PATHEXT: &str = ".COM;.EXE;.BAT;.CMD";
const WINDOWS_REMEDIATION: &str =
    "install a launchable .exe, .com, .cmd, .bat, or .ps1 wrapper and restart Jefe";
const UNIX_REMEDIATION: &str = "install an executable runtime on PATH and restart Jefe";

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
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AgentWrapperKind {
    /// Native executable that can be started directly.
    Direct,
    /// Windows command script requiring `cmd.exe` mediation.
    CommandScript,
    /// PowerShell script requiring explicit PowerShell mediation.
    PowerShellScript,
}

/// A runtime executable proven launchable under the selected platform policy.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResolvedAgentExecutable {
    runtime: AgentKind,
    path: PathBuf,
    wrapper_kind: AgentWrapperKind,
}

impl ResolvedAgentExecutable {
    /// Runtime represented by this executable.
    #[must_use]
    pub const fn runtime(&self) -> AgentKind {
        self.runtime
    }

    /// Fully resolved candidate path.
    #[must_use]
    pub fn path(&self) -> &Path {
        &self.path
    }

    /// Required launch strategy.
    #[must_use]
    pub const fn wrapper_kind(&self) -> AgentWrapperKind {
        self.wrapper_kind
    }
}

/// Pure resolver input, injectable for deterministic tests and startup detection.
#[derive(Debug, Clone)]
pub struct AgentExecutableResolver {
    platform: AgentExecutablePlatform,
    directories: Vec<PathBuf>,
    pathext: Option<OsString>,
}

impl AgentExecutableResolver {
    /// Resolve using the current process PATH and PATHEXT.
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

    /// Construct a deterministic resolver for explicit platform inputs.
    #[must_use]
    pub const fn for_platform(
        platform: AgentExecutablePlatform,
        directories: Vec<PathBuf>,
        pathext: Option<OsString>,
    ) -> Self {
        Self {
            platform,
            directories,
            pathext,
        }
    }

    /// Resolve a runtime to a supported executable and wrapper strategy.
    pub fn resolve(
        &self,
        runtime: AgentKind,
    ) -> Result<ResolvedAgentExecutable, AgentExecutableError> {
        match self.platform {
            AgentExecutablePlatform::Unix => self.resolve_unix(runtime),
            AgentExecutablePlatform::Windows => self.resolve_windows(runtime),
        }
    }

    fn resolve_unix(
        &self,
        runtime: AgentKind,
    ) -> Result<ResolvedAgentExecutable, AgentExecutableError> {
        for directory in &self.directories {
            let path = directory.join(runtime.binary_name());
            if unix_launchable(&path) {
                return Ok(resolved(runtime, path, AgentWrapperKind::Direct));
            }
        }
        Err(self.missing(runtime))
    }

    fn resolve_windows(
        &self,
        runtime: AgentKind,
    ) -> Result<ResolvedAgentExecutable, AgentExecutableError> {
        let extensions = windows_extensions(self.pathext.as_deref());
        for directory in &self.directories {
            for (extension, wrapper_kind) in &extensions {
                let path = directory.join(format!("{}{}", runtime.binary_name(), extension));
                if path.is_file() {
                    return Ok(resolved(runtime, path, *wrapper_kind));
                }
            }
        }
        Err(self.missing(runtime))
    }

    fn missing(&self, runtime: AgentKind) -> AgentExecutableError {
        AgentExecutableError::NotFound {
            runtime,
            remediation: match self.platform {
                AgentExecutablePlatform::Unix => UNIX_REMEDIATION,
                AgentExecutablePlatform::Windows => WINDOWS_REMEDIATION,
            },
        }
    }
}

/// Safe executable-resolution failure without arguments, prompts, or environment values.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AgentExecutableError {
    /// No supported launchable candidate exists on PATH.
    NotFound {
        runtime: AgentKind,
        remediation: &'static str,
    },
}

impl std::fmt::Display for AgentExecutableError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NotFound {
                runtime,
                remediation,
            } => write!(
                formatter,
                "{} runtime executable was not found on PATH; {remediation}",
                runtime.label()
            ),
        }
    }
}

impl std::error::Error for AgentExecutableError {}

fn resolved(
    runtime: AgentKind,
    path: PathBuf,
    wrapper_kind: AgentWrapperKind,
) -> ResolvedAgentExecutable {
    ResolvedAgentExecutable {
        runtime,
        path,
        wrapper_kind,
    }
}

fn windows_extensions(pathext: Option<&OsStr>) -> Vec<(String, AgentWrapperKind)> {
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

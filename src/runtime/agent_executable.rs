//! Platform-owned resolution of launchable local executables used by agent sessions.

use std::ffi::{OsStr, OsString};
use std::path::{Path, PathBuf};

use crate::domain::AgentKind;

const WINDOWS_DEFAULT_PATHEXT: &str = ".COM;.EXE;.BAT;.CMD";
const WINDOWS_REMEDIATION: &str =
    "install a launchable .exe, .com, .cmd, .bat, or .ps1 wrapper and restart Jefe";
const UNIX_REMEDIATION: &str = "install an executable runtime on PATH and restart Jefe";
const NPM_REMEDIATION: &str = "install Node.js with npm on PATH and restart Jefe";
const NPM_LAYOUT_REMEDIATION: &str = "install the official Node.js npm layout (npm.cmd/npm.bat beside node.exe and node_modules/npm/bin/npm-cli.js) or put npm.exe on PATH, then restart Jefe";

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

/// Executable required by an agent launch plan.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AgentExecutableTarget {
    /// A directly launched agent runtime.
    Agent(AgentKind),
    /// npm used for a selector-backed LLxprt launch or package probe.
    Npm,
}

impl AgentExecutableTarget {
    /// Executable basename resolved on PATH.
    #[must_use]
    pub const fn binary_name(self) -> &'static str {
        match self {
            Self::Agent(kind) => kind.binary_name(),
            Self::Npm => "npm",
        }
    }

    fn label(self) -> &'static str {
        match self {
            Self::Agent(kind) => kind.label(),
            Self::Npm => "npm",
        }
    }
}

impl From<AgentKind> for AgentExecutableTarget {
    fn from(value: AgentKind) -> Self {
        Self::Agent(value)
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

/// Direct Node.js invocation retained for an official Windows npm wrapper layout.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CanonicalNpmLaunchPlan {
    node: PathBuf,
    cli: PathBuf,
}

impl CanonicalNpmLaunchPlan {
    /// Canonical path to the Node.js executable.
    #[must_use]
    pub fn node(&self) -> &Path {
        &self.node
    }

    /// Canonical path to npm's JavaScript CLI entry point.
    #[must_use]
    pub fn cli(&self) -> &Path {
        &self.cli
    }
}

/// An executable proven launchable under the selected platform policy.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResolvedAgentExecutable {
    target: AgentExecutableTarget,
    path: PathBuf,
    wrapper_kind: AgentWrapperKind,
    npm_launch_plan: Option<CanonicalNpmLaunchPlan>,
}

impl ResolvedAgentExecutable {
    /// Executable role represented by this resolution.
    #[must_use]
    pub const fn target(&self) -> AgentExecutableTarget {
        self.target
    }

    /// Agent runtime represented by this executable, when it is a direct runtime.
    #[must_use]
    pub const fn runtime(&self) -> Option<AgentKind> {
        match self.target {
            AgentExecutableTarget::Agent(kind) => Some(kind),
            AgentExecutableTarget::Npm => None,
        }
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

    /// Validated direct Node.js launch plan for an official Windows npm script.
    #[must_use]
    pub fn npm_launch_plan(&self) -> Option<&CanonicalNpmLaunchPlan> {
        self.npm_launch_plan.as_ref()
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

    /// Resolve an agent runtime to a supported executable and wrapper strategy.
    pub fn resolve(
        &self,
        runtime: AgentKind,
    ) -> Result<ResolvedAgentExecutable, AgentExecutableError> {
        self.resolve_target(runtime.into())
    }

    /// Resolve any executable role used by the agent launch path.
    pub fn resolve_target(
        &self,
        target: AgentExecutableTarget,
    ) -> Result<ResolvedAgentExecutable, AgentExecutableError> {
        match self.platform {
            AgentExecutablePlatform::Unix => self.resolve_unix(target),
            AgentExecutablePlatform::Windows => self.resolve_windows(target),
        }
    }

    fn resolve_unix(
        &self,
        target: AgentExecutableTarget,
    ) -> Result<ResolvedAgentExecutable, AgentExecutableError> {
        for directory in &self.directories {
            let path = directory.join(target.binary_name());
            if unix_launchable(&path) {
                return Ok(resolved(target, path, AgentWrapperKind::Direct));
            }
        }
        Err(self.missing(target))
    }

    fn resolve_windows(
        &self,
        target: AgentExecutableTarget,
    ) -> Result<ResolvedAgentExecutable, AgentExecutableError> {
        let extensions = windows_extensions(self.pathext.as_deref());
        let mut rejected_npm_script = false;
        for directory in &self.directories {
            for (extension, wrapper_kind) in &extensions {
                let path = directory.join(format!("{}{extension}", target.binary_name()));
                if path.is_file() {
                    if target == AgentExecutableTarget::Npm
                        && *wrapper_kind == AgentWrapperKind::CommandScript
                    {
                        if let Some(plan) = canonical_npm_launch_plan(directory) {
                            return Ok(resolved_npm_script(path, plan));
                        }
                        rejected_npm_script = true;
                        continue;
                    }
                    return Ok(resolved(target, path, *wrapper_kind));
                }
            }
        }
        if rejected_npm_script {
            return Err(AgentExecutableError::NonCanonicalNpmWrapper {
                remediation: NPM_LAYOUT_REMEDIATION,
            });
        }
        Err(self.missing(target))
    }

    fn missing(&self, target: AgentExecutableTarget) -> AgentExecutableError {
        AgentExecutableError::NotFound {
            target,
            remediation: if target == AgentExecutableTarget::Npm {
                NPM_REMEDIATION
            } else {
                match self.platform {
                    AgentExecutablePlatform::Unix => UNIX_REMEDIATION,
                    AgentExecutablePlatform::Windows => WINDOWS_REMEDIATION,
                }
            },
        }
    }
}

/// Safe executable-resolution failure without arguments, prompts, or environment values.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AgentExecutableError {
    /// No supported launchable candidate exists on PATH.
    NotFound {
        /// Required executable role.
        target: AgentExecutableTarget,
        /// Action the user can take to resolve the failure.
        remediation: &'static str,
    },
    /// npm.cmd/npm.bat exists but cannot be launched without command-shell interpolation.
    NonCanonicalNpmWrapper {
        /// Action the user can take to install a structurally safe npm layout.
        remediation: &'static str,
    },
}

impl std::fmt::Display for AgentExecutableError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NotFound {
                target,
                remediation,
            } => write!(
                formatter,
                "{} executable was not found on PATH; {remediation}",
                target.label()
            ),
            Self::NonCanonicalNpmWrapper { remediation } => write!(
                formatter,
                "npm wrapper is not in a supported official Node.js layout; {remediation}"
            ),
        }
    }
}

impl std::error::Error for AgentExecutableError {}

fn resolved(
    target: AgentExecutableTarget,
    path: PathBuf,
    wrapper_kind: AgentWrapperKind,
) -> ResolvedAgentExecutable {
    ResolvedAgentExecutable {
        target,
        path,
        wrapper_kind,
        npm_launch_plan: None,
    }
}

fn resolved_npm_script(
    path: PathBuf,
    npm_launch_plan: CanonicalNpmLaunchPlan,
) -> ResolvedAgentExecutable {
    ResolvedAgentExecutable {
        target: AgentExecutableTarget::Npm,
        path,
        wrapper_kind: AgentWrapperKind::CommandScript,
        npm_launch_plan: Some(npm_launch_plan),
    }
}

fn canonical_npm_launch_plan(directory: &Path) -> Option<CanonicalNpmLaunchPlan> {
    let node = std::fs::canonicalize(directory.join("node.exe")).ok()?;
    let cli = std::fs::canonicalize(directory.join("node_modules/npm/bin/npm-cli.js")).ok()?;
    if !node.is_file() || !cli.is_file() {
        return None;
    }
    Some(CanonicalNpmLaunchPlan { node, cli })
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

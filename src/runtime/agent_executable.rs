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
///
/// On Windows, a `.cmd`/`.bat` npm wrapper is resolved to a **direct**
/// `node.exe` + `npm-cli.js` invocation (see [`NpmDirectInvocation`]) so the
/// selector and all npm arguments remain structural argv and never pass
/// through `cmd.exe`, eliminating cmd metacharacter reparsing.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResolvedAgentExecutable {
    runtime: AgentKind,
    path: PathBuf,
    wrapper_kind: AgentWrapperKind,
    /// When the resolved executable is a Windows `.cmd`/`.bat` npm wrapper,
    /// this carries the derived direct `node.exe` + `npm-cli.js` launch plan
    /// so the agent launcher never routes npm through `cmd.exe /C`.
    npm_direct: Option<NpmDirectInvocation>,
}

/// A direct `node.exe` + `npm-cli.js` invocation derived from a Windows
/// `.cmd`/`.bat` npm wrapper.
///
/// Standard npm installations place `npm.cmd` (or `npx.cmd`) alongside
/// `node.exe` in the same directory, and `node_modules/npm/bin/npm-cli.js`
/// relative to that directory. By resolving these paths and launching
/// `node.exe npm-cli.js <args>` directly, the selector and all npm arguments
/// remain distinct argv elements that never pass through `cmd.exe`, preventing
/// cmd metacharacter reparsing (`&`, `|`, `<`, `>`, `^`, `%`, `!`, etc.).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NpmDirectInvocation {
    node_executable: PathBuf,
    cli_script: PathBuf,
}

impl NpmDirectInvocation {
    /// The `node.exe` executable that runs the CLI script.
    #[must_use]
    pub fn node_executable(&self) -> &Path {
        &self.node_executable
    }

    /// The `npm-cli.js` (or `npx-cli.js`) script path passed as argv[1].
    #[must_use]
    pub fn cli_script(&self) -> &Path {
        &self.cli_script
    }

    /// Derive a direct `node.exe` + `npm-cli.js` invocation from a resolved
    /// `.cmd`/`.bat` npm (or npx) wrapper path.
    ///
    /// Standard npm layout:
    /// ```text
    /// <prefix>/
    ///   npm.cmd
    ///   npx.cmd
    ///   node.exe
    ///   node_modules/npm/bin/npm-cli.js
    ///   node_modules/npm/bin/npx-cli.js
    /// ```
    ///
    /// Returns `Ok` only when both `node.exe` and the expected CLI script
    /// exist on the filesystem. Returns `Err` otherwise — the caller must
    /// propagate this error and **never** fall back to launching npm through
    /// `cmd.exe`, because that would expose the version selector to cmd
    /// metacharacter reparsing (issue #269).
    ///
    /// # Errors
    ///
    /// Returns [`AgentExecutableError::NpmWrapperResolutionFailed`] when the
    /// wrapper path does not follow the standard npm layout (missing
    /// `node.exe` or CLI script) or the wrapper name is not `npm`/`npx`.
    pub fn from_wrapper(wrapper_path: &Path) -> Result<Self, AgentExecutableError> {
        let dir = wrapper_path.parent().ok_or_else(|| {
            AgentExecutableError::NpmWrapperResolutionFailed(format!(
                "npm wrapper path '{}' has no parent directory",
                wrapper_path.display()
            ))
        })?;
        let wrapper_name = wrapper_path
            .file_stem()
            .and_then(OsStr::to_str)
            .map(str::to_ascii_lowercase)
            .ok_or_else(|| {
                AgentExecutableError::NpmWrapperResolutionFailed(format!(
                    "npm wrapper path '{}' has no file stem",
                    wrapper_path.display()
                ))
            })?;

        let cli_name = match wrapper_name.as_str() {
            "npm" => "npm-cli.js",
            "npx" => "npx-cli.js",
            other => {
                return Err(AgentExecutableError::NpmWrapperResolutionFailed(format!(
                    "unsupported npm wrapper name '{other}' (expected npm or npx)"
                )));
            }
        };

        let node_executable = dir.join("node.exe");
        let cli_script = dir
            .join("node_modules")
            .join("npm")
            .join("bin")
            .join(cli_name);

        if !node_executable.is_file() {
            return Err(AgentExecutableError::NpmWrapperResolutionFailed(format!(
                "node.exe not found at '{}' for npm wrapper '{}'",
                node_executable.display(),
                wrapper_path.display()
            )));
        }
        if !cli_script.is_file() {
            return Err(AgentExecutableError::NpmWrapperResolutionFailed(format!(
                "{cli_name} not found at '{}' for npm wrapper '{}'",
                cli_script.display(),
                wrapper_path.display()
            )));
        }
        Ok(Self {
            node_executable,
            cli_script,
        })
    }
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

    /// When the resolved executable is a Windows `.cmd`/`.bat` npm wrapper,
    /// returns the derived direct `node.exe` + `npm-cli.js` launch plan.
    ///
    /// When present, the agent launcher launches `node.exe` directly with the
    /// CLI script as argv[1], keeping all npm arguments (including the version
    /// selector) as distinct structural argv elements that never pass through
    /// `cmd.exe`. When `None`, the executable is launched via its wrapper
    /// strategy (e.g. `Direct` for `.exe`, `CommandScript` for `.cmd`).
    #[must_use]
    pub const fn npm_direct(&self) -> Option<&NpmDirectInvocation> {
        self.npm_direct.as_ref()
    }

    /// Test-only constructor that builds a `ResolvedAgentExecutable` carrying
    /// an explicit [`NpmDirectInvocation`]. This lets launcher tests verify the
    /// `node.exe` + `npm-cli.js` launch-plan routing cross-platform without
    /// needing to fake Windows platform detection.
    #[cfg(test)]
    pub(crate) fn with_npm_direct_for_test(path: &Path, npm_direct: NpmDirectInvocation) -> Self {
        Self {
            runtime: AgentKind::Llxprt,
            path: path.to_path_buf(),
            wrapper_kind: AgentWrapperKind::CommandScript,
            npm_direct: Some(npm_direct),
        }
    }

    /// Construct a [`ResolvedAgentExecutable`] from an already-resolved path,
    /// inferring the wrapper strategy from the platform and file extension.
    ///
    /// This is the fallback constructor used when a session-cached executable
    /// path (e.g. npm detected at startup) is not on the current PATH but
    /// must still be launched. On Unix the wrapper is always [`Direct`];
    /// on Windows the extension determines the strategy so `npm.cmd` is
    /// correctly classified as a command script.
    ///
    /// **Safety gate (issue #269):** On Windows, when the path is a `.cmd`/
    /// `.bat` file named `npm` or `npx`, this derives a
    /// [`NpmDirectInvocation`] so the agent launcher can bypass `cmd.exe`
    /// entirely. If derivation fails (non-standard layout), this returns
    /// [`AgentExecutableError::NpmWrapperResolutionFailed`] — the launch is
    /// **never** routed through `cmd.exe` for npm, because that would let
    /// cmd metacharacters in the version selector be reparsed. Non-npm
    /// `.cmd`/`.bat` files (e.g. `yarn.cmd`) still succeed with the
    /// [`CommandScript`] wrapper strategy for unrelated callers.
    ///
    /// # Errors
    ///
    /// Returns [`AgentExecutableError::NpmWrapperResolutionFailed`] when the
    /// path is a Windows `npm`/`npx` `.cmd`/`.bat` wrapper whose standard
    /// `node.exe` + `npm-cli.js` layout cannot be validated.
    ///
    /// [`Direct`]: AgentWrapperKind::Direct
    pub fn from_path(path: &Path) -> Result<Self, AgentExecutableError> {
        let wrapper_kind = if cfg!(windows) {
            wrapper_kind_for_extension(path)
        } else {
            AgentWrapperKind::Direct
        };
        let npm_direct = if cfg!(windows) && wrapper_kind == AgentWrapperKind::CommandScript {
            require_npm_direct_from_path(path)?
        } else {
            None
        };
        Ok(Self {
            runtime: AgentKind::Llxprt,
            path: path.to_path_buf(),
            wrapper_kind,
            npm_direct,
        })
    }

    /// Prove that a session-cached executable path is still present and
    /// launchable on the current filesystem.
    ///
    /// [`ResolvedAgentExecutable::from_path`] classifies the wrapper strategy
    /// (and derives `NpmDirectInvocation` on Windows) but does not verify the
    /// file still exists or is executable. A long-lived tmux server can
    /// outlive the npm installation detected at startup — the cached path
    /// becomes stale (uninstalled, moved, permissions changed). This method
    /// provides the production revalidation gate called during
    /// [`PreparedLocalLaunch::prepare`] so a stale cached npm fails with an
    /// actionable typed error **before** any destructive kill — never silently
    /// falling back to a PATH lookup (the cached path is authoritative).
    ///
    /// # Platform behavior
    ///
    /// - **Unix:** the cached path must resolve to an existing regular file
    ///   with at least one execute-permission bit set (same policy as
    ///   [`AgentExecutableResolver`]'s `unix_launchable`).
    /// - **Windows:** the cached wrapper/executable file must still exist,
    ///   and when a [`NpmDirectInvocation`] is present, both its `node.exe`
    ///   and CLI script prerequisites must still exist (re-validating the
    ///   same layout proven during [`from_path`]).
    ///
    /// # Errors
    ///
    /// Returns [`AgentExecutableError::CachedNotLaunchable`] with the stale
    /// path and a human-readable detail when the file is missing, not a
    /// regular file, not executable (Unix), or a Windows npm-direct
    /// prerequisite disappeared.
    ///
    /// [`PreparedLocalLaunch::prepare`]: super::prepared_launch::PreparedLocalLaunch::prepare
    pub fn validate_cached(&self) -> Result<(), AgentExecutableError> {
        validate_cached_platform(&self.path, self.npm_direct.as_ref())
    }
}

/// Platform-dispatched cached-path validation.
///
/// Unix requires a regular file with execute permission; Windows requires the
/// file to exist and re-validates any `NpmDirectInvocation` prerequisites.
#[cfg(unix)]
fn validate_cached_platform(
    path: &Path,
    _npm_direct: Option<&NpmDirectInvocation>,
) -> Result<(), AgentExecutableError> {
    validate_cached_unix(path)
}

#[cfg(not(unix))]
fn validate_cached_platform(
    path: &Path,
    npm_direct: Option<&NpmDirectInvocation>,
) -> Result<(), AgentExecutableError> {
    validate_cached_windows(path, npm_direct)
}

#[cfg(unix)]
fn validate_cached_unix(path: &Path) -> Result<(), AgentExecutableError> {
    use std::os::unix::fs::PermissionsExt;

    // Follow the symlink chain to the target metadata. `symlink_metadata`
    // inspects only the link itself, rejecting the normal npm installation
    // layout where `/usr/local/bin/npm` is a symlink to
    // `../lib/node_modules/npm/bin/npm-cli.js`. `metadata` follows the link
    // and returns the target's metadata, so a valid symlink-to-executable
    // passes while a dangling link, a directory target, or a non-executable
    // target is rejected.
    let metadata =
        std::fs::metadata(path).map_err(|_| AgentExecutableError::CachedNotLaunchable {
            path: path.to_path_buf(),
            detail: "file no longer exists or symlink is dangling".to_owned(),
        })?;
    if !metadata.is_file() {
        return Err(AgentExecutableError::CachedNotLaunchable {
            path: path.to_path_buf(),
            detail: "path is not a regular file".to_owned(),
        });
    }
    if metadata.permissions().mode() & 0o111 == 0 {
        return Err(AgentExecutableError::CachedNotLaunchable {
            path: path.to_path_buf(),
            detail: "file is not executable (no execute permission)".to_owned(),
        });
    }
    Ok(())
}

#[cfg(not(unix))]
fn validate_cached_windows(
    path: &Path,
    npm_direct: Option<&NpmDirectInvocation>,
) -> Result<(), AgentExecutableError> {
    if !path.is_file() {
        return Err(AgentExecutableError::CachedNotLaunchable {
            path: path.to_path_buf(),
            detail: "cached wrapper/executable file no longer exists".to_owned(),
        });
    }
    if let Some(direct) = npm_direct {
        if !direct.node_executable().is_file() {
            return Err(AgentExecutableError::CachedNotLaunchable {
                path: direct.node_executable().to_path_buf(),
                detail: "node.exe prerequisite for cached npm wrapper disappeared".to_owned(),
            });
        }
        if !direct.cli_script().is_file() {
            return Err(AgentExecutableError::CachedNotLaunchable {
                path: direct.cli_script().to_path_buf(),
                detail: "npm-cli.js prerequisite for cached npm wrapper disappeared".to_owned(),
            });
        }
    }
    Ok(())
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

    /// Resolve an arbitrary named executable (e.g. `npm`) using the same
    /// platform policy as [`resolve`]: PATHEXT-aware lookup on Windows,
    /// execute-permission check on Unix.
    ///
    /// Unlike [`resolve`], the returned [`ResolvedAgentExecutable`] carries
    /// [`AgentKind::Llxprt`] only as a placeholder label; callers use the
    /// resolved path and [`ResolvedAgentExecutable::wrapper_kind`] directly.
    /// This exists so npm (needed for versioned LLxprt launches) goes through
    /// the platform-owned resolver instead of a duplicate Unix-only search.
    pub fn resolve_named(
        &self,
        name: &str,
    ) -> Result<ResolvedAgentExecutable, AgentExecutableError> {
        match self.platform {
            AgentExecutablePlatform::Unix => self.resolve_named_unix(name),
            AgentExecutablePlatform::Windows => self.resolve_named_windows(name),
        }
    }

    fn resolve_named_unix(
        &self,
        name: &str,
    ) -> Result<ResolvedAgentExecutable, AgentExecutableError> {
        for directory in &self.directories {
            let path = directory.join(name);
            if unix_launchable(&path) {
                return named_resolved(path, AgentWrapperKind::Direct);
            }
        }
        Err(AgentExecutableError::NamedNotFound {
            name: name.to_owned(),
            remediation: UNIX_REMEDIATION,
        })
    }

    fn resolve_named_windows(
        &self,
        name: &str,
    ) -> Result<ResolvedAgentExecutable, AgentExecutableError> {
        let extensions = windows_extensions(self.pathext.as_deref());
        for directory in &self.directories {
            for (extension, wrapper_kind) in &extensions {
                let path = directory.join(format!("{name}{extension}"));
                if path.is_file() {
                    return named_resolved(path, *wrapper_kind);
                }
            }
        }
        Err(AgentExecutableError::NamedNotFound {
            name: name.to_owned(),
            remediation: WINDOWS_REMEDIATION,
        })
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
    /// A named executable (e.g. npm) was not found using the platform policy.
    NamedNotFound {
        name: String,
        remediation: &'static str,
    },
    /// A Windows `.cmd`/`.bat` npm wrapper could not be resolved to a direct
    /// `node.exe` + `npm-cli.js` invocation (non-standard installation layout).
    NpmWrapperResolutionFailed(String),
    /// A session-cached executable path is no longer present or launchable.
    ///
    /// Returned by [`ResolvedAgentExecutable::validate_cached`] when a cached
    /// npm path (authoritative — never replaced by a PATH lookup) has gone
    /// stale: the file was removed, is not a regular file, lost execute
    /// permission (Unix), or a Windows npm-direct prerequisite
    /// (`node.exe` / `npm-cli.js`) disappeared.
    CachedNotLaunchable { path: PathBuf, detail: String },
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
            Self::NamedNotFound { name, remediation } => {
                write!(
                    formatter,
                    "executable '{name}' was not found on PATH; {remediation}"
                )
            }
            Self::NpmWrapperResolutionFailed(detail) => {
                write!(
                    formatter,
                    "could not resolve npm wrapper to a direct node invocation: {detail}"
                )
            }
            Self::CachedNotLaunchable { path, detail } => write!(
                formatter,
                "cached agent executable '{}' is no longer launchable: {detail}; \
                 re-run agent detection or reinstall the runtime",
                path.display()
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
        npm_direct: None,
    }
}

/// Build a [`ResolvedAgentExecutable`] for a named (non-runtime) binary.
///
/// The runtime label defaults to [`AgentKind::Llxprt`] so the existing
/// `ResolvedAgentExecutable` API can carry npm and other tools without a new
/// type. Callers rely on [`ResolvedAgentExecutable::path`] and
/// [`ResolvedAgentExecutable::wrapper_kind`], not the placeholder runtime.
///
/// **Safety gate (issue #269):** On Windows, when the named binary is `npm`
/// or `npx` and resolves to a `.cmd`/`.bat` wrapper, this derives a
/// [`NpmDirectInvocation`] so the agent launcher bypasses `cmd.exe` entirely.
/// If derivation fails (non-standard layout), this returns
/// [`AgentExecutableError::NpmWrapperResolutionFailed`] — npm launches are
/// **never** routed through `cmd.exe`. Non-npm `.cmd`/`.bat` named binaries
/// still succeed with the [`CommandScript`] wrapper strategy.
fn named_resolved(
    path: PathBuf,
    wrapper_kind: AgentWrapperKind,
) -> Result<ResolvedAgentExecutable, AgentExecutableError> {
    let npm_direct = if wrapper_kind == AgentWrapperKind::CommandScript {
        require_npm_direct_from_path(&path)?
    } else {
        None
    };
    Ok(ResolvedAgentExecutable {
        runtime: AgentKind::Llxprt,
        path,
        wrapper_kind,
        npm_direct,
    })
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

/// Infer the wrapper strategy from a Windows executable's file extension.
///
/// Used by [`ResolvedAgentExecutable::from_path`] so a cached npm path ending
/// in `.cmd` is correctly classified as a command script. For npm/npx
/// wrappers, [`ResolvedAgentExecutable::from_path`] additionally derives a
/// [`NpmDirectInvocation`] (or fails) so the launcher bypasses `cmd.exe`;
/// non-npm `.cmd`/`.bat` files keep the [`CommandScript`] wrapper. An
/// unknown extension defaults to [`Direct`] on the assumption that the
/// caller verified the file is launchable.
///
/// [`Direct`]: AgentWrapperKind::Direct
fn wrapper_kind_for_extension(path: &Path) -> AgentWrapperKind {
    path.extension()
        .and_then(OsStr::to_str)
        .and_then(|ext| classify_windows_extension(&format!(".{ext}")))
        .map_or(AgentWrapperKind::Direct, |(_, kind)| kind)
}

/// Derive a [`NpmDirectInvocation`] from a `.cmd`/`.bat` path, or fail safe.
///
/// When the path is not named `npm` or `npx`, returns `Ok(None)` so non-npm
/// command scripts keep the [`CommandScript`] wrapper strategy for unrelated
/// callers. When the path **is** named `npm` or `npx`, delegates to
/// [`NpmDirectInvocation::from_wrapper`] and propagates its
/// [`AgentExecutableError::NpmWrapperResolutionFailed`] when the standard
/// npm layout (sibling `node.exe` + `node_modules/npm/bin/*-cli.js`) cannot
/// be validated. This is the safety gate that prevents npm launches from
/// ever falling back to `cmd.exe`.
///
/// [`CommandScript`]: AgentWrapperKind::CommandScript
fn require_npm_direct_from_path(
    path: &Path,
) -> Result<Option<NpmDirectInvocation>, AgentExecutableError> {
    let stem = path
        .file_stem()
        .and_then(OsStr::to_str)
        .map(str::to_ascii_lowercase);
    match stem.as_deref() {
        Some("npm" | "npx") => Ok(Some(NpmDirectInvocation::from_wrapper(path)?)),
        _ => Ok(None),
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

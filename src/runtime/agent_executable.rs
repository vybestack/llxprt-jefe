//! Platform-owned resolution of launchable local executables used by agent sessions.

use std::ffi::{OsStr, OsString};
use std::io::Read;
use std::path::{Path, PathBuf};

use crate::domain::AgentKind;

const WINDOWS_DEFAULT_PATHEXT: &str = ".COM;.EXE;.BAT;.CMD";
const WINDOWS_REMEDIATION: &str =
    "install a launchable .exe, .com, .cmd, .bat, or .ps1 wrapper and restart Jefe";
const UNIX_REMEDIATION: &str = "install an executable runtime on PATH and restart Jefe";
const NPM_REMEDIATION: &str = "install Node.js with npm on PATH and restart Jefe";
const UVX_REMEDIATION: &str = "install uv with uvx on PATH and restart Jefe";
const NPM_LAYOUT_REMEDIATION: &str = "install the official Node.js npm layout (npm.cmd/npm.bat beside node.exe and node_modules/npm/bin/npm-cli.js) or put npm.exe on PATH, then restart Jefe";
const LLXPRT_OFFICIAL_LAYOUT_REMEDIATION: &str = "reinstall @vybestack/llxprt-code so its bundled bun.exe and index.ts ship beside llxprt.cmd, then restart Jefe";
const LLXPRT_NATIVE_LAUNCHER_MARKER: &str =
    "LLXPRT_NATIVE_LAUNCHER owned by @vybestack/llxprt-code";
const MAX_WRAPPER_MARKER_READ_BYTES: u64 = 8 * 1_024;
const LLXPRT_BUN_REL: &str = "node_modules/@vybestack/llxprt-code/node_modules/bun/bin/bun.exe";
const LLXPRT_ENTRYPOINT_REL: &str = "node_modules/@vybestack/llxprt-code/index.ts";
const NPM_NODE_REL: &str = "node.exe";
const NPM_CLI_REL: &str = "node_modules/npm/bin/npm-cli.js";

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
    /// uvx used for a pinned Code Puppy package launch or capability probe.
    Uvx,
}

impl AgentExecutableTarget {
    /// Executable basename resolved on PATH.
    #[must_use]
    pub const fn binary_name(self) -> &'static str {
        match self {
            Self::Agent(kind) => kind.binary_name(),
            Self::Npm => "npm",
            Self::Uvx => "uvx",
        }
    }

    fn label(self) -> &'static str {
        match self {
            Self::Agent(kind) => kind.label(),
            Self::Npm => "npm",
            Self::Uvx => "uvx",
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

/// Direct script runtime and entrypoint invocation for an official Windows
/// command-wrapper layout.
///
/// This supports npm's Node/npm-cli.js and LLxprt's bundled Bun/index.ts while
/// bypassing `cmd.exe` so the full argument vector survives intact.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CanonicalScriptLaunchPlan {
    runtime: PathBuf,
    entrypoint: PathBuf,
}

impl CanonicalScriptLaunchPlan {
    /// Canonical path to the script runtime executable (node.exe or bun.exe).
    #[must_use]
    pub fn runtime(&self) -> &Path {
        &self.runtime
    }

    /// Canonical path to the script entry point (npm-cli.js or index.ts).
    #[must_use]
    pub fn entrypoint(&self) -> &Path {
        &self.entrypoint
    }
}

/// An executable proven launchable under the selected platform policy.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResolvedAgentExecutable {
    target: AgentExecutableTarget,
    path: PathBuf,
    wrapper_kind: AgentWrapperKind,
    script_launch_plan: Option<CanonicalScriptLaunchPlan>,
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
            AgentExecutableTarget::Npm | AgentExecutableTarget::Uvx => None,
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

    /// Validated canonical script runtime + entrypoint plan for an official
    /// Windows wrapper layout, when one was recognized.
    #[must_use]
    pub fn script_launch_plan(&self) -> Option<&CanonicalScriptLaunchPlan> {
        self.script_launch_plan.as_ref()
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

    /// The platform policy this resolver applies.
    #[must_use]
    pub const fn platform(&self) -> AgentExecutablePlatform {
        self.platform
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
        let mut rejection: Option<AgentExecutableError> = None;
        for directory in &self.directories {
            for (extension, wrapper_kind) in &extensions {
                let path = directory.join(format!("{}{extension}", target.binary_name()));
                if path.is_file() {
                    match canonical_script_plan(target, *wrapper_kind, directory, &path) {
                        CanonicalScriptOutcome::Plan(plan) => {
                            return Ok(resolved_script(target, path, *wrapper_kind, plan));
                        }
                        CanonicalScriptOutcome::Unmarked => {
                            return Ok(resolved(target, path, *wrapper_kind));
                        }
                        CanonicalScriptOutcome::Reject(error) => {
                            if target == AgentExecutableTarget::Npm {
                                rejection = Some(error);
                                continue;
                            }
                            // A marked official wrapper is authoritative: surface package
                            // corruption instead of silently launching another PATH entry.
                            return Err(error);
                        }
                    }
                }
            }
        }
        Err(rejection.unwrap_or_else(|| self.missing(target)))
    }

    fn missing(&self, target: AgentExecutableTarget) -> AgentExecutableError {
        AgentExecutableError::NotFound {
            target,
            remediation: if target == AgentExecutableTarget::Npm {
                NPM_REMEDIATION
            } else if target == AgentExecutableTarget::Uvx {
                UVX_REMEDIATION
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
    /// A marked official LLxprt wrapper exists but its bundled runtime/entrypoint layout is incomplete.
    NonCanonicalOfficialLlxprtWrapper {
        /// Action the user can take to restore the official native-launcher layout.
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
            Self::NonCanonicalOfficialLlxprtWrapper { remediation } => write!(
                formatter,
                "LLxprt wrapper is marked as an official native launcher but its layout is incomplete; {remediation}"
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
        script_launch_plan: None,
    }
}

fn resolved_script(
    target: AgentExecutableTarget,
    path: PathBuf,
    wrapper_kind: AgentWrapperKind,
    plan: CanonicalScriptLaunchPlan,
) -> ResolvedAgentExecutable {
    ResolvedAgentExecutable {
        target,
        path,
        wrapper_kind,
        script_launch_plan: Some(plan),
    }
}

enum CanonicalScriptOutcome {
    Plan(CanonicalScriptLaunchPlan),
    Unmarked,
    Reject(AgentExecutableError),
}

fn canonical_script_plan(
    target: AgentExecutableTarget,
    wrapper_kind: AgentWrapperKind,
    directory: &Path,
    wrapper_path: &Path,
) -> CanonicalScriptOutcome {
    if wrapper_kind != AgentWrapperKind::CommandScript {
        return CanonicalScriptOutcome::Unmarked;
    }
    if target == AgentExecutableTarget::Npm {
        return canonical_npm_outcome(directory);
    }
    if matches!(target, AgentExecutableTarget::Agent(AgentKind::Llxprt)) {
        return official_llxprt_outcome(directory, wrapper_path);
    }
    CanonicalScriptOutcome::Unmarked
}

fn canonical_npm_outcome(directory: &Path) -> CanonicalScriptOutcome {
    match canonical_script_launch_plan(directory, NPM_NODE_REL, NPM_CLI_REL) {
        Some(plan) => CanonicalScriptOutcome::Plan(plan),
        None => CanonicalScriptOutcome::Reject(AgentExecutableError::NonCanonicalNpmWrapper {
            remediation: NPM_LAYOUT_REMEDIATION,
        }),
    }
}

fn official_llxprt_outcome(directory: &Path, wrapper_path: &Path) -> CanonicalScriptOutcome {
    if !wrapper_carries_native_launcher_marker(wrapper_path) {
        return CanonicalScriptOutcome::Unmarked;
    }
    match canonical_script_launch_plan(directory, LLXPRT_BUN_REL, LLXPRT_ENTRYPOINT_REL) {
        Some(plan) => CanonicalScriptOutcome::Plan(plan),
        None => CanonicalScriptOutcome::Reject(
            AgentExecutableError::NonCanonicalOfficialLlxprtWrapper {
                remediation: LLXPRT_OFFICIAL_LAYOUT_REMEDIATION,
            },
        ),
    }
}

fn canonical_script_launch_plan(
    directory: &Path,
    runtime_rel: &str,
    entrypoint_rel: &str,
) -> Option<CanonicalScriptLaunchPlan> {
    // `std::fs::canonicalize` resolves symlinks/links and reports the real file.
    // On Windows it returns a `\\?\`-prefixed verbatim path, which is the *stored*
    // value later passed as a structured argument to `node.exe`/`bun.exe`. Node's
    // module loader mishandles that verbatim prefix (issue #432: it degenerates
    // the path to the bare drive letter and fails with
    // `EISDIR: illegal operation on a directory, lstat 'C:'`). Strip the verbatim
    // prefix from the stored path so the structured-argument contract from #258
    // (direct `node.exe` + JS entrypoint, no `cmd.exe`) actually reaches the
    // runtime intact.
    //
    // The `is_file()` existence check runs on the *canonical* (still verbatim on
    // Windows) path, not the stripped one: Win32 file APIs require the `\\?\`
    // prefix for paths longer than MAX_PATH (260), so validating after stripping
    // would falsely reject valid long-path installs (OCR finding on PR #483).
    let runtime_canonical = std::fs::canonicalize(directory.join(runtime_rel)).ok()?;
    let entrypoint_canonical = std::fs::canonicalize(directory.join(entrypoint_rel)).ok()?;
    if !runtime_canonical.is_file() || !entrypoint_canonical.is_file() {
        return None;
    }
    Some(CanonicalScriptLaunchPlan {
        runtime: strip_verbatim_prefix(&runtime_canonical),
        entrypoint: strip_verbatim_prefix(&entrypoint_canonical),
    })
}

/// Strip the Windows `\\?\` verbatim prefix (and its `\\?\UNC\` network form)
/// from a canonicalized path so it can be passed as a structured argument to
/// `node.exe`/`bun.exe`, whose module loader otherwise mishandles the prefix
/// (issue #432).
///
/// On non-Windows targets this is a no-op (canonicalize does not add a prefix
/// there). The function only strips the well-known prefixes; it performs no
/// normalization, case folding, or path manipulation that could mask an
/// attacker-controlled component, because the input is always the output of
/// `std::fs::canonicalize` on a path Jefe itself joined.
#[cfg(windows)]
pub(super) fn strip_verbatim_prefix(path: &Path) -> PathBuf {
    use std::ffi::OsStr;
    use std::path::Component;

    let mut components = path.components();
    let Some(Component::Prefix(prefix)) = components.next() else {
        return path.to_path_buf();
    };
    match prefix.kind() {
        // `\\?\C:\...` → `C:\...`
        std::path::Prefix::VerbatimDisk(letter) => {
            let mut stripped = PathBuf::from(format!("{}:", letter as char));
            for component in components {
                stripped.push(component.as_os_str());
            }
            stripped
        }
        // `\\?\UNC\server\share\...` → `\\server\share\...`
        //
        // Build the UNC root as a single `OsString` and push the remaining
        // components normally. `OsStr` is used (not `to_string_lossy()`) so
        // non-UTF-8 bytes preserved by canonicalize survive intact —
        // `to_string_lossy` would replace them with U+FFFD and silently point
        // the runtime at the wrong network location (OCR finding on PR #483).
        // `PathBuf::push("\\")` is avoided because it would overwrite the
        // buffer (clippy::path_buf_push_overwrite).
        std::path::Prefix::VerbatimUNC(server, share) => {
            let mut root = std::ffi::OsString::from("\\\\");
            root.push(server);
            root.push(OsStr::new("\\"));
            root.push(share);
            let mut stripped = PathBuf::from(root);
            for component in components {
                stripped.push(component.as_os_str());
            }
            stripped
        }
        // Any other prefix form (already-disk, UNC without Verbatim) is not
        // produced by canonicalize and is returned unchanged.
        _ => path.to_path_buf(),
    }
}

#[cfg(not(windows))]
pub(super) fn strip_verbatim_prefix(path: &Path) -> PathBuf {
    path.to_path_buf()
}

fn wrapper_carries_native_launcher_marker(wrapper_path: &Path) -> bool {
    let Ok(file) = std::fs::File::open(wrapper_path) else {
        return false;
    };
    let mut bytes = Vec::new();
    if file
        .take(MAX_WRAPPER_MARKER_READ_BYTES + 1)
        .read_to_end(&mut bytes)
        .is_err()
        || bytes.len() as u64 > MAX_WRAPPER_MARKER_READ_BYTES
    {
        return false;
    }
    std::str::from_utf8(&bytes).is_ok_and(|text| text.contains(LLXPRT_NATIVE_LAUNCHER_MARKER))
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

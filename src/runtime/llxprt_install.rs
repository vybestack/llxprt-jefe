//! Jefe-managed install cache for selector-backed LLxprt launches (issue #425).
//!
//! Replaces the unreliable `npm exec --yes --package=@vybestack/llxprt-code@VERSION`
//! local launch with a jefe-owned install directory:
//!
//! `<cache_dir>/jefe/llxprt-versions/<version_dir_name>/node_modules/.bin/llxprt`
//!
//! The exact selector (dist-tag or explicit version, never a caret range) is
//! pinned in a hand-written `package.json`; `npm install` (no package args)
//! installs the pinned dependency without rewriting `package.json`, so a
//! jefe-managed install never drifts to a newer patch (issue #425 Problem C).
//! Installing into a jefe-owned directory (not the agent work_dir) avoids local
//! `node_modules` shadowing (Problem A), and jefe manages its own install
//! directory so there is no `_npx` cache lock contention (Problem B).

use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::sync::Mutex;
use std::time::Duration;

use crate::domain::{LLXPRT_NPM_PACKAGE, LlxprtNpmPackageSelector};

use super::agent_executable::{
    AgentExecutableError, AgentExecutableResolver, AgentExecutableTarget,
};
use super::agent_launcher::command_for_executable;
use super::command_capture::run_command_capture_with_timeout;

/// Generous wall-clock budget for a fresh install of `@vybestack/llxprt-code`
/// (a large package). A cache hit returns well inside this bound; a miss pays
/// the registry fetch + extract once per selector.
const INSTALL_TIMEOUT: Duration = Duration::from_secs(300);

/// Subdirectory of the cache dir holding all jefe-managed version installs.
const VERSIONS_SUBDIR: &str = "llxprt-versions";

/// Marker file recording the exact selector the install directory satisfies.
/// A cache hit requires this marker, a matching selector, and the resolved bin.
const INSTALL_MARKER: &str = ".jefe-installed";

/// Private package name written into each install dir's `package.json`.
const CACHE_PACKAGE_NAME: &str = "jefe-llxprt-cache";

/// Serialize all jefe-managed installs within this process. The runtime is
/// single-threaded (`&mut self` on the manager), but non-interactive rewrite
/// runs and the capture worker share the process, so an explicit guard makes
/// the invariant machine-checked.
static INSTALL_LOCK: Mutex<()> = Mutex::new(());

/// Failure to install or resolve a jefe-managed LLxprt version.
#[derive(Debug, Clone)]
pub enum LlxprtInstallError {
    /// npm is not available on the local machine.
    NpmMissing {
        /// Requested npm selector.
        selector: String,
    },
    /// The install directory could not be created or written.
    InstallDir {
        /// Requested npm selector.
        selector: String,
        /// Bounded failure detail.
        diagnostic: String,
    },
    /// `npm install` failed (nonzero exit or timeout).
    InstallFailed {
        /// Requested npm selector.
        selector: String,
        /// Bounded npm diagnostic.
        diagnostic: String,
    },
}

impl std::fmt::Display for LlxprtInstallError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NpmMissing { selector } => write!(
                formatter,
                "npm is not available on the local machine for LLxprt selector '{selector}'; install Node.js with npm or clear the LLxprt version selector"
            ),
            Self::InstallDir {
                selector,
                diagnostic,
            } => write!(
                formatter,
                "could not prepare the jefe-managed install directory for LLxprt selector '{selector}'; verify cache directory permissions. diagnostic: {diagnostic}"
            ),
            Self::InstallFailed {
                selector,
                diagnostic,
            } => write!(
                formatter,
                "npm install for {LLXPRT_NPM_PACKAGE}@{selector} failed; verify the selector and registry access or clear the LLxprt version selector. npm diagnostic: {diagnostic}"
            ),
        }
    }
}

impl std::error::Error for LlxprtInstallError {}

/// The jefe-managed version cache root: `<cache_dir>/jefe/llxprt-versions/`.
///
/// Precedence:
/// 1. `JEFE_LLXPRT_CACHE_DIR` env var (absolute directory) — highest, for tests
///    and explicit overrides.
/// 2. platform cache directory (`dirs::cache_dir()`): `~/Library/Caches` on
///    macOS, `~/.cache` on Linux, `%LOCALAPPDATA%` on Windows.
/// 3. `.jefe-cache` in the home directory when the platform cache dir is unset.
#[must_use]
pub fn cache_root() -> PathBuf {
    resolve_cache_root(std::env::var_os("JEFE_LLXPRT_CACHE_DIR"))
}

/// Pure cache-root resolver from an explicit env value (testable without env
/// mutation). Honors only absolute paths so a relative override cannot escape
/// into an unexpected cwd.
#[must_use]
fn resolve_cache_root(env_value: Option<OsString>) -> PathBuf {
    if let Some(dir) = env_value.filter(|s| !s.is_empty()) {
        let path = PathBuf::from(dir);
        if path.is_absolute() {
            return path.join(VERSIONS_SUBDIR);
        }
    }
    let base = dirs::cache_dir().unwrap_or_else(|| PathBuf::from(".jefe-cache"));
    base.join("jefe").join(VERSIONS_SUBDIR)
}

/// The install directory for a selector: `<cache_root>/<version_dir_name>/`.
#[must_use]
pub fn install_dir_for(selector: &LlxprtNpmPackageSelector) -> PathBuf {
    install_dir_in(&cache_root(), selector)
}

/// Install directory under an explicit cache root (injectable for tests).
#[must_use]
fn install_dir_in(cache_root: &Path, selector: &LlxprtNpmPackageSelector) -> PathBuf {
    cache_root.join(selector.version_dir_name())
}

/// The `node_modules/.bin` directory holding the resolved `llxprt` binary.
#[must_use]
pub fn bin_dir_for(selector: &LlxprtNpmPackageSelector) -> PathBuf {
    bin_dir_in(&cache_root(), selector)
}

/// Binary directory under an explicit cache root (injectable for tests).
#[must_use]
fn bin_dir_in(cache_root: &Path, selector: &LlxprtNpmPackageSelector) -> PathBuf {
    install_dir_in(cache_root, selector)
        .join("node_modules")
        .join(".bin")
}

/// The exact `package.json` contents pinning the selector for `npm install`.
///
/// `npm install` with no package arguments installs the dependencies listed in
/// `package.json` without rewriting the file, so the pin is the exact selector
/// (never a caret range). `private: true` prevents accidental publication.
#[must_use]
fn package_json_contents(selector: &LlxprtNpmPackageSelector) -> String {
    format!(
        "{{\n  \"name\": \"{CACHE_PACKAGE_NAME}\",\n  \"version\": \"0.0.0\",\n  \"private\": true,\n  \"dependencies\": {{\n    \"{LLXPRT_NPM_PACKAGE}\": \"{spec}\"\n  }}\n}}\n",
        spec = selector.install_spec_value()
    )
}

/// The marker file contents recording the exact selector this install satisfies.
#[must_use]
fn marker_contents(selector: &LlxprtNpmPackageSelector) -> String {
    selector.install_spec_value()
}

/// Whether an install directory already satisfies a selector (cache hit).
///
/// A hit requires the marker file to exist and match the selector's effective
/// install spec, and the resolved `llxprt` binary to be present in
/// `node_modules/.bin`. Any read/IO failure is treated as a miss (the install
/// path will rebuild the directory).
#[must_use]
fn is_cache_hit(install_dir: &Path, bin_dir: &Path, selector: &LlxprtNpmPackageSelector) -> bool {
    let marker = install_dir.join(INSTALL_MARKER);
    let Ok(stored) = std::fs::read_to_string(&marker) else {
        return false;
    };
    if stored.trim() != marker_contents(selector) {
        return false;
    }
    managed_binary_exists(bin_dir)
}

/// Whether the `node_modules/.bin` directory holds a launchable `llxprt`
/// binary, accounting for Windows' PATHEXT extensions (`.exe`, `.cmd`, ...)
/// and, on Unix, the execute permission bit. A cached binary without the
/// execute bit would produce a false cache hit and then fail at launch time
/// (see `AgentExecutableResolver::resolve_unix`), so it is not counted here.
fn managed_binary_exists(bin_dir: &Path) -> bool {
    let base = AgentExecutableTarget::Agent(crate::domain::AgentKind::Llxprt).binary_name();
    if cfg!(unix) {
        use std::os::unix::fs::PermissionsExt;
        let path = bin_dir.join(base);
        return std::fs::metadata(&path).is_ok_and(|metadata| {
            metadata.is_file() && metadata.permissions().mode() & 0o111 != 0
        });
    }
    if cfg!(windows) {
        for ext in [".exe", ".cmd", ".bat", ".ps1"] {
            if bin_dir.join(format!("{base}{ext}")).is_file() {
                return true;
            }
        }
    }
    false
}

/// Ensure the jefe-managed install for `selector` exists and return the
/// `node_modules/.bin` directory holding the resolved `llxprt` binary.
///
/// Idempotent: a cache hit returns immediately without reinstalling. Installs
/// are serialized within the process via [`INSTALL_LOCK`].
///
/// # Errors
///
/// Returns [`LlxprtInstallError`] when npm is missing, the install directory
/// cannot be prepared, or `npm install` fails.
pub fn ensure_installed(
    selector: &LlxprtNpmPackageSelector,
) -> Result<PathBuf, LlxprtInstallError> {
    ensure_installed_in(
        selector,
        &AgentExecutableResolver::current(),
        INSTALL_TIMEOUT,
    )
}

/// Injectable core of [`ensure_installed`] for deterministic tests.
fn ensure_installed_in(
    selector: &LlxprtNpmPackageSelector,
    resolver: &AgentExecutableResolver,
    timeout: Duration,
) -> Result<PathBuf, LlxprtInstallError> {
    // Recover from a poisoned lock (a prior install panicked) so a single
    // failure cannot wedge all future launches in this process.
    let _guard = match INSTALL_LOCK.lock() {
        Ok(guard) => guard,
        Err(poisoned) => poisoned.into_inner(),
    };
    let cache_root = cache_root();
    ensure_installed_under(&cache_root, selector, resolver, timeout)
}

/// Install path pinned to an explicit cache root (testable without env mutation).
fn ensure_installed_under(
    cache_root: &Path,
    selector: &LlxprtNpmPackageSelector,
    resolver: &AgentExecutableResolver,
    timeout: Duration,
) -> Result<PathBuf, LlxprtInstallError> {
    let install_dir = install_dir_in(cache_root, selector);
    let bin_dir = bin_dir_in(cache_root, selector);
    if is_cache_hit(&install_dir, &bin_dir, selector) {
        return Ok(bin_dir);
    }
    prepare_install_dir(&install_dir, selector)?;
    run_npm_install(&install_dir, selector, resolver, timeout)?;
    write_marker(&install_dir, selector)?;
    Ok(bin_dir)
}

fn prepare_install_dir(
    install_dir: &Path,
    selector: &LlxprtNpmPackageSelector,
) -> Result<(), LlxprtInstallError> {
    std::fs::create_dir_all(install_dir).map_err(|error| LlxprtInstallError::InstallDir {
        selector: selector.as_str().to_owned(),
        diagnostic: error.to_string(),
    })?;
    let package_json = install_dir.join("package.json");
    // Overwrite any stale pin from a prior selector that slugified to the same
    // directory name so `npm install` always resolves the current selector.
    std::fs::write(&package_json, package_json_contents(selector)).map_err(|error| {
        LlxprtInstallError::InstallDir {
            selector: selector.as_str().to_owned(),
            diagnostic: error.to_string(),
        }
    })
}

fn run_npm_install(
    install_dir: &Path,
    selector: &LlxprtNpmPackageSelector,
    resolver: &AgentExecutableResolver,
    timeout: Duration,
) -> Result<(), LlxprtInstallError> {
    let executable = resolver
        .resolve_target(AgentExecutableTarget::Npm)
        .map_err(|error| {
            let selector = selector.as_str().to_owned();
            match error {
                AgentExecutableError::NotFound { .. }
                | AgentExecutableError::NonCanonicalNpmWrapper { .. } => {
                    LlxprtInstallError::NpmMissing { selector }
                }
            }
        })?;
    let arguments = vec![OsString::from("install")];
    let mut command = command_for_executable(&executable, &arguments);
    command.current_dir(install_dir);
    command.stdin(Stdio::null());
    let output = run_command_capture_with_timeout(command, timeout, "jefe llxprt install")
        .map_err(|error| LlxprtInstallError::InstallFailed {
            selector: selector.as_str().to_owned(),
            diagnostic: error.to_string(),
        })?;
    if !output.status.success() {
        let status = output
            .status
            .code()
            .map_or_else(|| "signal".to_owned(), |code| code.to_string());
        let stderr = String::from_utf8_lossy(&output.stderr);
        let stderr_trim = stderr.trim();
        let diagnostic = if stderr_trim.is_empty() {
            format!("npm install exited with status {status}")
        } else {
            format!("npm install exited with status {status}: {stderr_trim}")
        };
        let bounded: String = diagnostic.chars().take(512).collect();
        return Err(LlxprtInstallError::InstallFailed {
            selector: selector.as_str().to_owned(),
            diagnostic: bounded,
        });
    }
    Ok(())
}

fn write_marker(
    install_dir: &Path,
    selector: &LlxprtNpmPackageSelector,
) -> Result<(), LlxprtInstallError> {
    let marker = install_dir.join(INSTALL_MARKER);
    std::fs::write(&marker, marker_contents(selector)).map_err(|error| {
        LlxprtInstallError::InstallDir {
            selector: selector.as_str().to_owned(),
            diagnostic: error.to_string(),
        }
    })
}

/// Resolve the binary directory for a local versioned launch without
/// reinstalling when the cache is already satisfied.
///
/// Convenience wrapper used by the local launch path: returns the `node_modules/.bin`
/// directory after ensuring the install. Remote launches do not use this.
pub fn local_managed_bin_dir(
    selector: &LlxprtNpmPackageSelector,
) -> Result<PathBuf, LlxprtInstallError> {
    ensure_installed(selector)
}

/// Testable core of [`local_managed_bin_dir`] pinned to an explicit cache root
/// so tests never touch the real platform cache or mutate the environment.
#[cfg(test)]
fn local_managed_bin_dir_under(
    cache_root: &Path,
    selector: &LlxprtNpmPackageSelector,
) -> Result<PathBuf, LlxprtInstallError> {
    let _guard = match INSTALL_LOCK.lock() {
        Ok(guard) => guard,
        Err(poisoned) => poisoned.into_inner(),
    };
    ensure_installed_under(
        cache_root,
        selector,
        &AgentExecutableResolver::current(),
        INSTALL_TIMEOUT,
    )
}

/// Resolve an agent executable from a jefe-managed `node_modules/.bin`
/// directory, applying the same launchability checks as PATH resolution.
///
/// Used by the local launch path (`commands::local_launch_command`) to resolve
/// the cached `llxprt` binary from the jefe-managed install dir instead of
/// searching PATH, so the work directory's `node_modules` cannot shadow the
/// pinned version.
pub fn resolve_managed_executable(
    bin_dir: &Path,
    target: AgentExecutableTarget,
) -> Result<super::agent_executable::ResolvedAgentExecutable, super::errors::RuntimeError> {
    // A scoped resolver that searches only the managed bin dir reuses the
    // platform-aware launchability checks (Unix execute bit, Windows
    // PATHEXT) so the cached binary is held to the same standard as a PATH
    // resolution.
    let scoped = AgentExecutableResolver::for_platform(
        AgentExecutableResolver::current().platform(),
        vec![bin_dir.to_path_buf()],
        std::env::var_os("PATHEXT"),
    );
    scoped.resolve_target(target).map_err(|error| {
        super::errors::RuntimeError::SpawnFailed(format!(
            "cached llxprt binary '{}' was not found in the jefe-managed install dir {}: {error}",
            target.binary_name(),
            bin_dir.display()
        ))
    })
}

#[cfg(test)]
#[path = "llxprt_install_tests.rs"]
mod tests;

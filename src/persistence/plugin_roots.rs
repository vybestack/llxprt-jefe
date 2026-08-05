//! Ordered package roots for the plugin inventory
//! (issue #389 CW-09, acceptance rows R1, R2, R7).
//!
//! Discovery order is low to high precedence:
//!
//! 1. the canonical executable directory's `../share/jefe/plugins`;
//! 2. the platform's package-manager roots — macOS
//!    `/opt/homebrew/share/jefe/plugins` then `/usr/local/share/jefe/plugins`,
//!    Linux `/usr/local/share/jefe/plugins` then `/usr/share/jefe/plugins`;
//! 3. `<config>/plugins/installed`.
//!
//! Resolution is pure: every input arrives on [`PluginRootRequest`], so `PATH`
//! and the current directory are never consulted and a test never mutates
//! process state. Skipping roots that do not exist is a separate filesystem
//! step performed by the inventory, not by this ordering.
//!
//! Only the user root is writable. A package-manager root is owned by the
//! package manager, so an install never mutates one.
//!
//! This module deliberately does **not** deduplicate lexically equal paths.
//! An executable under `/usr/local/bin` derives a root that repeats the Linux
//! system root, and physical `(device, inode)` identity is the single
//! deduplication authority for that; adding a second, lexical rule here would
//! be a competing mechanism that disagrees with it under symlinks.

use std::path::{Path, PathBuf};

use super::paths::Platform;

/// Relative location of a package root beneath an installation prefix.
const PREFIX_RELATIVE_ROOT: &str = "share/jefe/plugins";

/// Directory holding packages installed by this user.
const USER_ROOT_DIRECTORY: &str = "installed";

/// macOS package-manager prefixes, in discovery order.
const MACOS_PREFIXES: [&str; 2] = ["/opt/homebrew", "/usr/local"];

/// Linux package-manager prefixes, in discovery order.
const LINUX_PREFIXES: [&str; 2] = ["/usr/local", "/usr"];

/// Where a package root came from, which fixes whether Jefe may write to it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PluginRootKind {
    /// Derived from this executable's own installation prefix.
    Executable,
    /// A platform package-manager prefix.
    System,
    /// The user's own configuration directory.
    User,
}

/// One ordered package root.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PluginRoot {
    path: PathBuf,
    kind: PluginRootKind,
}

impl PluginRoot {
    /// Borrow the root directory.
    #[must_use]
    pub fn path(&self) -> &Path {
        &self.path
    }

    /// The provenance of this root.
    #[must_use]
    pub const fn kind(&self) -> PluginRootKind {
        self.kind
    }

    /// Whether an install may write into this root.
    ///
    /// Only the user root is writable; package-manager roots are owned by the
    /// package manager.
    #[must_use]
    pub const fn is_writable(&self) -> bool {
        matches!(self.kind, PluginRootKind::User)
    }
}

/// Every input needed to order package roots, kept explicit so resolution is
/// pure and tests never mutate process environment.
#[derive(Debug, Clone)]
pub struct PluginRootRequest {
    /// The canonical directory holding the running executable, when it could
    /// be resolved.
    pub executable_dir: Option<PathBuf>,
    /// Platform whose package-manager prefixes should be used.
    pub platform: Platform,
    /// The resolved `<config>/plugins` directory.
    pub config_plugins_dir: PathBuf,
}

/// Order every candidate package root, lowest precedence first.
///
/// The result is the complete ordered candidate list; roots that do not exist
/// on disk are skipped later by the inventory scan, which is the only step
/// permitted to touch the filesystem.
#[must_use]
pub fn candidate_roots(request: &PluginRootRequest) -> Vec<PluginRoot> {
    let mut roots = Vec::new();
    if let Some(prefix) = request.executable_dir.as_deref().and_then(Path::parent) {
        roots.push(PluginRoot {
            path: prefix.join(PREFIX_RELATIVE_ROOT),
            kind: PluginRootKind::Executable,
        });
    }
    for prefix in system_prefixes(request.platform) {
        roots.push(PluginRoot {
            path: Path::new(prefix).join(PREFIX_RELATIVE_ROOT),
            kind: PluginRootKind::System,
        });
    }
    roots.push(PluginRoot {
        path: request.config_plugins_dir.join(USER_ROOT_DIRECTORY),
        kind: PluginRootKind::User,
    });
    roots
}

/// The package-manager prefixes this platform publishes, in discovery order.
const fn system_prefixes(platform: Platform) -> &'static [&'static str] {
    match platform {
        Platform::Macos => &MACOS_PREFIXES,
        Platform::Linux => &LINUX_PREFIXES,
        // Windows has no Unix installation prefix convention, and an
        // unsupported platform must not guess one.
        Platform::Windows | Platform::Unsupported => &[],
    }
}

#[cfg(test)]
#[path = "plugin_roots_tests.rs"]
mod tests;

//! Resolution of jefe's dedicated tmux socket path.
//!
//! Jefe runs tmux on a *private* socket (`-S <path>`) so its sessions are fully
//! isolated from any unrelated user tmux sessions that may share the default
//! socket. This also means jefe never accidentally destroys unrelated sessions
//! and is not affected when the shared default server dies.
//!
//! The socket is named after the [`InstallationId`] of the jefe that owns it,
//! which is the same value Windows passes to `psmux -L`. Two worktrees launched
//! from different config/state locations therefore get different servers on
//! every platform. Naming it after the *user* instead, as this module once did
//! by shelling out to `id -u`, gave every jefe on the box one shared socket and
//! reproduced on Unix the collision that issue #547 was filed about on Windows.
//! User isolation survives the change structurally: distinct accounts resolve
//! distinct state paths, so they derive distinct identities.
//!
//! Explicit socket rendering is fail-closed: `JEFE_SOCKET_PATH` and
//! `JEFE_SOCKET_DIR` must be non-empty, absolute, and fit the Unix socket limit.
//! Without an override, the default is `dirs::runtime_dir()` when available
//! (Linux XDG_RUNTIME_DIR), then `dirs::data_local_dir()`, then `temp_dir()`.
//!
//! `dirs::runtime_dir()` returns `None` on macOS/Windows, so the fallback chain
//! always produces a usable path.

use super::namespace::InstallationId;
use std::path::PathBuf;
use std::sync::OnceLock;

/// Socket file name for an installation.
fn socket_filename(installation: &InstallationId) -> String {
    format!("{installation}.sock")
}

const SOCKET_PATH_ENV: &str = "JEFE_SOCKET_PATH";
const SOCKET_DIR_ENV: &str = "JEFE_SOCKET_DIR";

/// A validated socket rendering and the deliberate override that selected it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResolvedSocketPath {
    path: PathBuf,
    override_variable: Option<&'static str>,
}

impl ResolvedSocketPath {
    #[must_use]
    pub const fn override_variable(&self) -> Option<&'static str> {
        self.override_variable
    }

    #[must_use]
    pub fn into_path(self) -> PathBuf {
        self.path
    }
}

/// Why a deliberate socket rendering override was refused.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SocketPathError {
    Empty {
        variable: &'static str,
    },
    NonUnicode {
        variable: &'static str,
    },
    Relative {
        variable: &'static str,
        value: PathBuf,
    },
    TooLong {
        variable: &'static str,
        value: PathBuf,
    },
    DirectoryUnavailable {
        variable: Option<&'static str>,
        directory: PathBuf,
        reason: String,
    },
}

impl SocketPathError {
    #[must_use]
    pub const fn variable(&self) -> Option<&'static str> {
        match self {
            Self::Empty { variable }
            | Self::NonUnicode { variable }
            | Self::Relative { variable, .. }
            | Self::TooLong { variable, .. } => Some(variable),
            Self::DirectoryUnavailable { variable, .. } => *variable,
        }
    }
}

impl std::fmt::Display for SocketPathError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Empty { variable } => {
                write!(
                    formatter,
                    "{variable} is empty; unset it or provide an absolute path"
                )
            }
            Self::NonUnicode { variable } => {
                write!(formatter, "{variable} is not valid Unicode")
            }
            Self::Relative { variable, value } => write!(
                formatter,
                "{variable} must be absolute, got {}",
                value.display()
            ),
            Self::TooLong { variable, value } => write!(
                formatter,
                "{variable} produces a Unix socket path at or above the 100-byte safety limit: {}",
                value.display()
            ),
            Self::DirectoryUnavailable {
                directory, reason, ..
            } => write!(
                formatter,
                "cannot create Unix socket directory {}: {reason}",
                directory.display()
            ),
        }
    }
}

impl std::error::Error for SocketPathError {}

/// Prepare the selected socket directory without changing the selected path.
fn prepare_socket_path(resolution: ResolvedSocketPath) -> Result<PathBuf, SocketPathError> {
    let Some(parent) = resolution.path.parent() else {
        return Ok(resolution.path);
    };

    std::fs::create_dir_all(parent).map_err(|error| SocketPathError::DirectoryUnavailable {
        variable: resolution.override_variable,
        directory: parent.to_path_buf(),
        reason: error.to_string(),
    })?;
    if let Some(variable) = resolution.override_variable {
        tracing::warn!(
            variable,
            socket_path = %resolution.path.display(),
            "deliberate Unix socket override is in effect"
        );
    }
    Ok(resolution.path)
}

/// Whether a candidate Unix-domain-socket path fits safely under the kernel's
/// `sun_path` limit (104 bytes macOS, 108 Linux). Use 100 to stay under the
/// strictest platform limit, avoiding cryptic tmux socket-bind failures.
#[must_use]
fn socket_path_len_ok(candidate: &std::path::Path) -> bool {
    candidate.to_string_lossy().len() < 100
}

/// Resolve the default socket directory when no env var is set.
///
/// Precedence: `dirs::runtime_dir()` (Linux XDG_RUNTIME_DIR; `None` on macOS)
/// → `dirs::data_local_dir()` → `std::env::temp_dir()`.
fn default_socket_dir(filename: &str) -> PathBuf {
    if let Some(dir) = dirs::runtime_dir() {
        return dir;
    }
    if let Some(dir) = dirs::data_local_dir() {
        // Unix domain socket paths have a strict kernel limit (104 bytes
        // macOS, 108 Linux). On macOS `runtime_dir()` is `None` so the
        // fallback reaches `data_local_dir()` (`~/Library/Application
        // Support`), which with a long username + `<id>.sock` can exceed
        // 104 bytes, making tmux fail cryptically.
        let candidate = dir.join(filename);
        if socket_path_len_ok(&candidate) {
            return dir;
        }
        tracing::warn!(
            candidate = %candidate.display(),
            "default socket dir path too long for a Unix domain socket; falling back to temp_dir"
        );
    }
    std::env::temp_dir()
}

/// Resolve the jefe-private tmux socket path from explicit env values.
///
/// This is the pure validation core. A present override is either accepted
/// exactly or rejected: it can never silently select a different server.
fn resolve_from_env(
    socket_path_env: Option<&str>,
    socket_dir_env: Option<&str>,
    filename: &str,
) -> Result<ResolvedSocketPath, SocketPathError> {
    if let Some(raw) = socket_path_env {
        let value = raw.trim();
        if value.is_empty() {
            return Err(SocketPathError::Empty {
                variable: SOCKET_PATH_ENV,
            });
        }
        let path = PathBuf::from(value);
        validate_override(SOCKET_PATH_ENV, &path)?;
        return Ok(ResolvedSocketPath {
            path,
            override_variable: Some(SOCKET_PATH_ENV),
        });
    }

    if let Some(raw) = socket_dir_env {
        let value = raw.trim();
        if value.is_empty() {
            return Err(SocketPathError::Empty {
                variable: SOCKET_DIR_ENV,
            });
        }
        let directory = PathBuf::from(value);
        if !directory.is_absolute() {
            return Err(SocketPathError::Relative {
                variable: SOCKET_DIR_ENV,
                value: directory,
            });
        }
        let path = directory.join(filename);
        validate_override(SOCKET_DIR_ENV, &path)?;
        return Ok(ResolvedSocketPath {
            path,
            override_variable: Some(SOCKET_DIR_ENV),
        });
    }

    Ok(ResolvedSocketPath {
        path: default_socket_dir(filename).join(filename),
        override_variable: None,
    })
}

fn validate_override(
    variable: &'static str,
    path: &std::path::Path,
) -> Result<(), SocketPathError> {
    if !path.is_absolute() {
        return Err(SocketPathError::Relative {
            variable,
            value: path.to_path_buf(),
        });
    }
    if !socket_path_len_ok(path) {
        return Err(SocketPathError::TooLong {
            variable,
            value: path.to_path_buf(),
        });
    }
    Ok(())
}

/// Resolve the jefe-private tmux socket path, honoring env precedence.
pub fn resolve_socket_path(
    installation: &InstallationId,
) -> Result<ResolvedSocketPath, SocketPathError> {
    let socket_path = read_override(SOCKET_PATH_ENV)?;
    let socket_dir = read_override(SOCKET_DIR_ENV)?;
    resolve_from_env(
        socket_path.as_deref(),
        socket_dir.as_deref(),
        &socket_filename(installation),
    )
}

fn read_override(variable: &'static str) -> Result<Option<String>, SocketPathError> {
    std::env::var_os(variable)
        .map(|value| {
            value
                .into_string()
                .map_err(|_| SocketPathError::NonUnicode { variable })
        })
        .transpose()
}

/// Resolve the socket path for an arbitrary installation, creating its
/// directory.
pub fn socket_path_for(installation: &InstallationId) -> Result<PathBuf, SocketPathError> {
    prepare_socket_path(resolve_socket_path(installation)?)
}

/// Resolve and cache the jefe-private tmux socket path.
pub fn jefe_tmux_socket_path(
    installation: &InstallationId,
) -> Result<&'static std::path::Path, SocketPathError> {
    static SOCKET_PATH: OnceLock<PathBuf> = OnceLock::new();
    if let Some(path) = SOCKET_PATH.get() {
        return Ok(path.as_path());
    }

    let prepared = socket_path_for(installation)?;
    Ok(SOCKET_PATH.get_or_init(|| prepared).as_path())
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;
    use std::path::Path;

    fn installation() -> InstallationId {
        InstallationId::for_state_path(Path::new("/home/dev/.local/share/jefe/state.json"))
    }

    fn resolved(socket_path: Option<&str>, socket_dir: Option<&str>) -> ResolvedSocketPath {
        resolve_from_env(socket_path, socket_dir, &socket_filename(&installation()))
            .unwrap_or_else(|error| panic!("socket path should resolve: {error}"))
    }

    fn rejected(socket_path: Option<&str>, socket_dir: Option<&str>) -> SocketPathError {
        resolve_from_env(socket_path, socket_dir, &socket_filename(&installation()))
            .err()
            .unwrap_or_else(|| panic!("socket override should be rejected"))
    }

    #[test]
    fn socket_path_override_has_highest_precedence_and_visible_provenance() {
        let resolution = resolved(
            Some("/tmp/explicit-jefe.sock"),
            Some("/tmp/should-be-ignored"),
        );

        assert_eq!(resolution.path, PathBuf::from("/tmp/explicit-jefe.sock"));
        assert_eq!(resolution.override_variable(), Some(SOCKET_PATH_ENV));
    }

    #[test]
    fn relative_socket_path_fails_closed_instead_of_falling_through() {
        let error = rejected(Some("relative/jefe.sock"), Some("/tmp/jefe-sockets"));

        assert!(matches!(
            error,
            SocketPathError::Relative {
                variable: SOCKET_PATH_ENV,
                ..
            }
        ));
    }

    #[test]
    fn socket_directory_override_appends_the_installation_filename() {
        let resolution = resolved(None, Some("/tmp/jefe-sockets"));

        assert_eq!(
            resolution.path,
            PathBuf::from("/tmp/jefe-sockets").join(socket_filename(&installation()))
        );
        assert_eq!(resolution.override_variable(), Some(SOCKET_DIR_ENV));
    }

    #[test]
    fn empty_socket_path_override_fails_closed() {
        let error = rejected(Some("   "), None);

        assert_eq!(
            error,
            SocketPathError::Empty {
                variable: SOCKET_PATH_ENV
            }
        );
    }

    #[test]
    fn empty_socket_directory_override_fails_closed() {
        let error = rejected(None, Some(""));

        assert_eq!(
            error,
            SocketPathError::Empty {
                variable: SOCKET_DIR_ENV
            }
        );
    }

    #[test]
    fn overlong_socket_directory_override_fails_closed() {
        let long_dir = "/tmp/".to_owned() + &"a".repeat(95);
        let error = rejected(None, Some(&long_dir));

        assert!(matches!(
            error,
            SocketPathError::TooLong {
                variable: SOCKET_DIR_ENV,
                ..
            }
        ));
    }

    #[test]
    fn relative_socket_directory_override_fails_closed() {
        let error = rejected(None, Some("relative/sockets"));

        assert!(matches!(
            error,
            SocketPathError::Relative {
                variable: SOCKET_DIR_ENV,
                ..
            }
        ));
    }

    #[test]
    fn overlong_socket_path_override_fails_closed() {
        let overlong = "/tmp/".to_owned() + &"z".repeat(100) + ".sock";
        let error = rejected(Some(&overlong), None);

        assert!(matches!(
            error,
            SocketPathError::TooLong {
                variable: SOCKET_PATH_ENV,
                ..
            }
        ));
    }

    #[test]
    fn no_override_keeps_installation_identity_in_the_default_filename() {
        let resolution = resolved(None, None);
        let filename = resolution
            .path
            .file_name()
            .and_then(std::ffi::OsStr::to_str)
            .unwrap_or_else(|| panic!("default socket needs a filename: {resolution:?}"));

        assert_eq!(filename, socket_filename(&installation()));
        assert_eq!(resolution.override_variable(), None);
    }

    #[test]
    fn distinct_installations_get_distinct_sockets() {
        let first = InstallationId::for_state_path(Path::new("/work/tree-one/.jefe/state.json"));
        let second = InstallationId::for_state_path(Path::new("/work/tree-two/.jefe/state.json"));

        let first_path = resolve_from_env(None, Some("/tmp/jefe"), &socket_filename(&first))
            .unwrap_or_else(|error| panic!("first socket should resolve: {error}"));
        let second_path = resolve_from_env(None, Some("/tmp/jefe"), &socket_filename(&second))
            .unwrap_or_else(|error| panic!("second socket should resolve: {error}"));

        assert_ne!(first_path.path, second_path.path);
    }

    #[test]
    fn directory_creation_failure_keeps_the_selected_path_and_reports_the_boundary_error() {
        let root = std::env::temp_dir().join(format!(
            "jefe-socket-directory-failure-{}",
            std::process::id()
        ));
        std::fs::write(&root, b"not a directory")
            .unwrap_or_else(|error| panic!("test fixture should be writable: {error}"));
        let socket = root.join("jefe.sock");
        let resolution = ResolvedSocketPath {
            path: socket,
            override_variable: Some(SOCKET_PATH_ENV),
        };

        let error = prepare_socket_path(resolution)
            .err()
            .unwrap_or_else(|| panic!("a file cannot be used as a socket directory"));
        assert!(matches!(
            error,
            SocketPathError::DirectoryUnavailable {
                variable: Some(SOCKET_PATH_ENV),
                directory,
                ..
            } if directory == root
        ));

        std::fs::remove_file(&root)
            .unwrap_or_else(|error| panic!("test fixture should be removable: {error}"));
    }
}

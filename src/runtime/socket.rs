//! Resolution of jefe's dedicated tmux socket path.
//!
//! Jefe runs tmux on a *private* socket (`-S <path>`) so its sessions are fully
//! isolated from any unrelated user tmux sessions that may share the default
//! socket. This also means jefe never accidentally destroys unrelated sessions
//! and is not affected when the shared default server dies.
//!
//! The resolution mirrors the persistence layer's precedence pattern:
//! 1. `JEFE_SOCKET_PATH` env var (absolute socket file path) — highest precedence
//! 2. `JEFE_SOCKET_DIR` env var (directory; socket file = `<dir>/jefe.sock`)
//! 3. default: `dirs::runtime_dir()` if available (Linux XDG_RUNTIME_DIR),
//!    else `dirs::data_local_dir()`, else `std::env::temp_dir()`.
//!
//! `dirs::runtime_dir()` returns `None` on macOS/Windows, so the fallback chain
//! always produces a usable path.

use std::path::PathBuf;
use std::sync::OnceLock;

/// Resolve and cache the real UID via `id -u`.
///
/// Shells out once and caches the result in a process-global [`OnceLock`] so
/// [`socket_filename`] (and transitively [`resolve_socket_path`]) is
/// pure-after-first-call and avoids repeated subprocess spawns.
///
/// SAFETY note: this is not `unsafe` code — `std::os::unix::process` would
/// be, but `libc::getuid` is forbidden by the `unsafe_code = "forbid"` lint.
/// We shell out to `id -u` to stay within the no-unsafe, no-libc constraint.
fn cached_uid() -> Option<u32> {
    static UID: OnceLock<Option<u32>> = OnceLock::new();
    *UID.get_or_init(|| {
        std::process::Command::new("id")
            .arg("-u")
            .output()
            .ok()
            .filter(|output| output.status.success())
            .and_then(|output| {
                String::from_utf8_lossy(&output.stdout)
                    .trim()
                    .parse::<u32>()
                    .ok()
            })
    })
}

/// Stable socket filename (suffixed with the real UID on Unix so concurrent
/// users on the same host never collide).
fn socket_filename() -> String {
    if let Some(uid) = cached_uid() {
        format!("jefe-{uid}.sock")
    } else {
        tracing::warn!(
            "could not determine UID; falling back to shared jefe.sock — multi-user isolation may be compromised"
        );
        "jefe.sock".to_owned()
    }
}

/// Ensure the containing directory for `socket_file` exists, creating it (and
/// parents) as needed. On failure, fall back to `temp_dir()` so the caller
/// always gets a writable path rather than panicking.
fn ensure_dir_or_fallback(socket_file: PathBuf) -> PathBuf {
    let Some(parent) = socket_file.parent() else {
        return socket_file;
    };

    if std::fs::create_dir_all(parent).is_ok() {
        return socket_file;
    }

    // Fall back to temp_dir + socket filename on creation failure. temp_dir is
    // world-writable, which weakens isolation versus a user-owned runtime/data
    // dir, but it is still a UID-suffixed private socket (never the shared
    // default tmux socket). Warn so the silent fallback is diagnosable.
    tracing::warn!(
        requested_dir = %parent.display(),
        "could not create jefe tmux socket directory; falling back to temp dir",
    );
    let fallback_name = socket_file.file_name().map_or_else(
        || std::ffi::OsString::from(socket_filename()),
        std::ffi::OsStr::to_owned,
    );
    std::env::temp_dir().join(fallback_name)
}

/// Resolve the default socket directory when no env var is set.
///
/// Precedence: `dirs::runtime_dir()` (Linux XDG_RUNTIME_DIR; `None` on macOS)
/// → `dirs::data_local_dir()` → `std::env::temp_dir()`.
fn default_socket_dir() -> PathBuf {
    if let Some(dir) = dirs::runtime_dir() {
        return dir;
    }
    if let Some(dir) = dirs::data_local_dir() {
        // Unix domain socket paths have a strict kernel limit (104 bytes
        // macOS, 108 Linux). On macOS `runtime_dir()` is `None` so the
        // fallback reaches `data_local_dir()` (`~/Library/Application
        // Support`), which with a long username + `jefe-<uid>.sock` can
        // exceed 104 bytes, making tmux fail cryptically. Use 100 to stay
        // safely under macOS's 104-byte limit.
        let candidate = dir.join(socket_filename());
        if candidate.to_string_lossy().len() < 100 {
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
/// This is the pure core of [`resolve_socket_path`], factored out so the
/// precedence logic is unit-testable without mutating process env vars (which
/// is `unsafe` under edition 2024 and forbidden by the `unsafe_code = "forbid"`
/// lint).
///
/// Precedence:
/// 1. `socket_path_env` (`JEFE_SOCKET_PATH`) — absolute socket file path
/// 2. `socket_dir_env` (`JEFE_SOCKET_DIR`) — directory; socket file = `<dir>/jefe-<uid>.sock`
/// 3. platform default (`dirs::runtime_dir()` → `dirs::data_local_dir()` → tempdir)
#[must_use]
fn resolve_from_env(socket_path_env: Option<&str>, socket_dir_env: Option<&str>) -> PathBuf {
    // 1. JEFE_SOCKET_PATH — absolute socket file path. A relative path
    //    resolves against tmux's CWD (not jefe's), causing subtle bugs, so
    //    only honor it when absolute.
    if let Some(path) = socket_path_env.map(str::trim).filter(|s| !s.is_empty()) {
        let path_buf = PathBuf::from(path);
        if path_buf.is_absolute() {
            return path_buf;
        }
        tracing::warn!(
            requested_path = %path_buf.display(),
            "JEFE_SOCKET_PATH is not absolute; ignoring and falling through to JEFE_SOCKET_DIR / default"
        );
    }

    // 2. JEFE_SOCKET_DIR — directory; socket file = `<dir>/jefe-<uid>.sock`.
    if let Some(dir) = socket_dir_env.map(str::trim).filter(|s| !s.is_empty()) {
        return PathBuf::from(dir).join(socket_filename());
    }

    // 3. Platform default.
    default_socket_dir().join(socket_filename())
}

/// Resolve the jefe-private tmux socket path, honoring env precedence.
///
/// This is the pure resolver (no side effects beyond optional `create_dir_all`
/// in the public [`jefe_tmux_socket_path`]). Useful for deterministic tests.
#[must_use]
pub fn resolve_socket_path() -> PathBuf {
    resolve_from_env(
        std::env::var("JEFE_SOCKET_PATH").ok().as_deref(),
        std::env::var("JEFE_SOCKET_DIR").ok().as_deref(),
    )
}

/// Resolve and cache the jefe-private tmux socket path.
///
/// Honors, in order:
/// - `JEFE_SOCKET_PATH` (absolute socket file path)
/// - `JEFE_SOCKET_DIR` (directory; socket file = `<dir>/jefe-<uid>.sock`)
/// - default (`dirs::runtime_dir()` → `dirs::data_local_dir()` → tempdir)
///
/// Ensures the containing directory exists (creating it on first use). If
/// directory creation fails, falls back to `temp_dir()` rather than panicking.
///
/// The result is cached in a `OnceLock` because it is read on every tmux
/// invocation; re-resolving (and re-shelling-out to `id -u`) each time would be
/// wasteful and could race with concurrent env changes.
#[must_use]
pub fn jefe_tmux_socket_path() -> &'static std::path::Path {
    static SOCKET_PATH: OnceLock<PathBuf> = OnceLock::new();
    SOCKET_PATH.get_or_init(|| ensure_dir_or_fallback(resolve_socket_path()))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Assert a socket filename is either `jefe-<uid>.sock` (numeric uid) or
    /// the shared `jefe.sock` fallback. When the numeric-uid form is present,
    /// cross-check the suffix against the actual `id -u` if available.
    fn assert_valid_jefe_socket_filename(filename: &str) {
        let suffix = filename.strip_prefix("jefe-").unwrap_or(filename);
        if suffix.is_empty() {
            // The shared `jefe.sock` (no-uid) fallback form.
            assert_eq!(
                filename, "jefe.sock",
                "empty suffix means shared fallback, expected jefe.sock, got {filename}"
            );
            return;
        }
        let digits = suffix.strip_suffix(".sock").unwrap_or(suffix);
        assert!(
            !digits.is_empty() && digits.chars().all(|c| c.is_ascii_digit()),
            "expected jefe-<uid>.sock with numeric uid, got {filename}"
        );
        // Cross-check against the real uid when available.
        if let Some(real_uid) = cached_uid()
            && let Ok(parsed) = digits.parse::<u32>()
        {
            assert_eq!(parsed, real_uid, "socket uid suffix should match `id -u`");
        }
    }

    #[test]
    fn resolve_honors_socket_path_highest_precedence() {
        // JEFE_SOCKET_PATH wins even when JEFE_SOCKET_DIR is also set.
        let path = resolve_from_env(
            Some("/tmp/explicit-jefe.sock"),
            Some("/tmp/should-be-ignored"),
        );
        assert_eq!(path, PathBuf::from("/tmp/explicit-jefe.sock"));
    }

    #[test]
    fn resolve_ignores_relative_socket_path() {
        // A relative JEFE_SOCKET_PATH must be ignored (it would resolve
        // against tmux's CWD), falling through to JEFE_SOCKET_DIR.
        let path = resolve_from_env(Some("relative/jefe.sock"), Some("/tmp/jefe-sockets"));
        assert!(path.starts_with("/tmp/jefe-sockets"));
        let filename = path
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or_else(|| panic!("socket must have a filename: {path:?}"));
        assert_valid_jefe_socket_filename(filename);
    }

    #[test]
    fn resolve_honors_socket_dir_with_filename_when_path_absent() {
        let path = resolve_from_env(None, Some("/tmp/jefe-sockets"));
        assert!(path.starts_with("/tmp/jefe-sockets"));
        let filename = path
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or_else(|| panic!("socket must have a filename: {path:?}"));
        assert_valid_jefe_socket_filename(filename);
    }

    #[test]
    fn resolve_ignores_blank_env_values() {
        // Empty/whitespace values are treated as unset.
        let path = resolve_from_env(Some("   "), Some(""));
        let filename = path
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or_else(|| panic!("default must have a filename: {path:?}"));
        assert_valid_jefe_socket_filename(filename);
    }

    #[test]
    fn resolve_falls_back_to_platform_default_when_no_env() {
        let path = resolve_from_env(None, None);
        let filename = path
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or_else(|| panic!("default must have a filename: {path:?}"));
        assert_valid_jefe_socket_filename(filename);
    }
}

//! Session-cached detection of installed agent runtimes.

use std::sync::OnceLock;

use crate::domain::AgentKind;
use crate::runtime::{AgentExecutablePlatform, AgentExecutableResolver};

static INSTALLED_AGENT_KINDS: OnceLock<Vec<AgentKind>> = OnceLock::new();
static NPM_PATH: OnceLock<Option<std::path::PathBuf>> = OnceLock::new();

/// Agent kinds whose executable is present on PATH, detected once per session.
#[must_use]
pub fn installed_agent_kinds() -> &'static [AgentKind] {
    INSTALLED_AGENT_KINDS.get_or_init(detect_installed_agent_kinds)
}

/// Resolved executable path for `npm`, detected once per session.
///
/// Production runtime construction receives this exact path so a long-lived
/// tmux server cannot resolve a different `npm` from stale environment state.
#[must_use]
pub fn npm_path() -> Option<&'static std::path::Path> {
    NPM_PATH.get_or_init(detect_npm_path).as_deref()
}

fn detect_npm_path() -> Option<std::path::PathBuf> {
    let agent_resolver = AgentExecutableResolver::current();
    let resolved = agent_resolver.resolve_named("npm").ok()?;
    let path = resolved.path().to_path_buf();
    // Normalize relative PATH entries to absolute paths based on the
    // detection-time cwd. This prevents a stale cwd from resolving a
    // different npm after the detection snapshot was taken.
    normalize_npm_path(path, std::env::current_dir().ok().as_deref())
}

/// Normalize an npm candidate path to an absolute path anchored at the
/// detection-time cwd.
///
/// If the path is already absolute, it is returned unchanged. If it is
/// relative and a cwd is available, the cwd is joined to produce an absolute
/// path. If the cwd is unavailable (rare: the process working directory was
/// removed), the relative candidate is rejected because it cannot be safely
/// anchored for later reuse.
///
/// Extracted as a pure function so the normalization is deterministically
/// testable without touching the real filesystem.
#[must_use]
fn normalize_npm_path(
    path: std::path::PathBuf,
    cwd: Option<&std::path::Path>,
) -> Option<std::path::PathBuf> {
    if path.is_absolute() {
        Some(path)
    } else {
        cwd.map(|base| base.join(path))
    }
}

fn detect_installed_agent_kinds() -> Vec<AgentKind> {
    detect_with_resolver(&AgentExecutableResolver::current())
}

/// Pure detection of which agent runtimes are installed, given an explicit
/// slice of PATH directories.
///
/// Returns the kinds whose executable is present and executable (on Unix) or
/// present as a file (on non-Unix) in any of the supplied directories. The
/// detection order follows the canonical kind order in the candidate list.
///
/// Extracted as a pure function so the detection logic is deterministically
/// testable without touching the real filesystem or `PATH` environment
/// variable.
#[must_use]
pub fn detect_agent_kinds(dirs: &[std::path::PathBuf]) -> Vec<AgentKind> {
    let resolver = AgentExecutableResolver::for_platform(
        AgentExecutablePlatform::current(),
        dirs.to_vec(),
        std::env::var_os("PATHEXT"),
    );
    detect_with_resolver(&resolver)
}

fn detect_with_resolver(resolver: &AgentExecutableResolver) -> Vec<AgentKind> {
    [AgentKind::Llxprt, AgentKind::CodePuppy]
        .into_iter()
        .filter(|kind| resolver.resolve(*kind).is_ok())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;
    use std::sync::atomic::{AtomicU64, Ordering};

    static TEMP_DIR_SEQUENCE: AtomicU64 = AtomicU64::new(0);

    // ── Pure detect_agent_kinds tests (issue #184) ─────────────────────────
    //
    // Deterministic tests that exercise `detect_agent_kinds` with temp
    // directories, covering: neither installed, each installed, both
    // installed (order), and executable permission requirements.

    fn make_executable(path: &std::path::Path) {
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let _ = std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o755));
        }
        #[cfg(not(unix))]
        {
            let _ = path;
        }
    }

    fn temp_dir_with_binaries(binaries: &[&str]) -> PathBuf {
        let sequence = TEMP_DIR_SEQUENCE.fetch_add(1, Ordering::Relaxed);
        let dir = std::env::temp_dir().join(format!(
            "jefe-agent-detect-{}-{sequence}",
            std::process::id()
        ));
        std::fs::create_dir_all(&dir).unwrap_or_else(|error| panic!("create temp dir: {error}"));
        for binary in binaries {
            let filename = if cfg!(windows) {
                format!("{binary}.exe")
            } else {
                (*binary).to_owned()
            };
            let path = dir.join(filename);
            std::fs::write(&path, b"#!/bin/sh\n")
                .unwrap_or_else(|error| panic!("write binary: {error}"));
            make_executable(&path);
        }
        dir
    }

    #[test]
    fn detect_neither_installed() {
        let dir = temp_dir_with_binaries(&[]);
        let detected = detect_agent_kinds(std::slice::from_ref(&dir));
        assert!(detected.is_empty());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn detect_only_llxprt() {
        let dir = temp_dir_with_binaries(&["llxprt"]);
        let detected = detect_agent_kinds(std::slice::from_ref(&dir));
        assert_eq!(detected, vec![AgentKind::Llxprt]);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn detect_only_code_puppy() {
        let dir = temp_dir_with_binaries(&["code-puppy"]);
        let detected = detect_agent_kinds(std::slice::from_ref(&dir));
        assert_eq!(detected, vec![AgentKind::CodePuppy]);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn detect_both_in_canonical_order() {
        // Put code-puppy in an earlier dir than llxprt; the result must still
        // follow the canonical candidate order (Llxprt, CodePuppy).
        let dir_cp = temp_dir_with_binaries(&["code-puppy"]);
        let dir_ll = temp_dir_with_binaries(&["llxprt"]);
        let detected = detect_agent_kinds(&[dir_cp.clone(), dir_ll.clone()]);
        assert_eq!(detected, vec![AgentKind::Llxprt, AgentKind::CodePuppy]);
        let _ = std::fs::remove_dir_all(&dir_cp);
        let _ = std::fs::remove_dir_all(&dir_ll);
    }

    #[test]
    fn detect_both_in_same_dir() {
        let dir = temp_dir_with_binaries(&["llxprt", "code-puppy"]);
        let detected = detect_agent_kinds(std::slice::from_ref(&dir));
        assert_eq!(detected, vec![AgentKind::Llxprt, AgentKind::CodePuppy]);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[cfg(unix)]
    #[test]
    fn detect_requires_executable_permission() {
        use std::os::unix::fs::PermissionsExt;
        let dir = std::env::temp_dir().join(format!(
            "jefe-agent-detect-noexec-{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map_or(0, |d| d.as_nanos())
        ));
        std::fs::create_dir_all(&dir).unwrap_or_else(|error| panic!("create temp dir: {error}"));
        let path = dir.join("llxprt");
        std::fs::write(&path, b"#!/bin/sh\n")
            .unwrap_or_else(|error| panic!("write binary: {error}"));
        // Explicitly remove all execute bits.
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o644))
            .unwrap_or_else(|error| panic!("set non-executable permissions: {error}"));

        let detected = detect_agent_kinds(std::slice::from_ref(&dir));
        assert!(
            detected.is_empty(),
            "non-executable file must not be detected"
        );

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn detect_ignores_nonexistent_dirs() {
        let fake_dir = PathBuf::from("/this/path/does/not/exist/jefe-test");
        let detected = detect_agent_kinds(&[fake_dir]);
        assert!(detected.is_empty());
    }

    // ── normalize_npm_path tests (issue #269) ───────────────────────────────
    //
    // A relative or empty PATH entry can cause `command -v npm` to resolve a
    // relative path. The detection snapshot must normalize that to an absolute
    // path so a later cwd change cannot resolve a different npm.

    #[test]
    fn normalize_npm_path_absolute_returned_unchanged() {
        let path = PathBuf::from("/usr/local/bin/npm");
        let normalized = normalize_npm_path(path.clone(), Some(std::path::Path::new("/cwd")));
        assert_eq!(normalized.as_deref(), Some(path.as_path()));
    }

    #[test]
    fn normalize_npm_path_relative_joined_with_cwd() {
        let path = PathBuf::from("bin/npm");
        let normalized = normalize_npm_path(path, Some(std::path::Path::new("/home/user")));
        assert_eq!(
            normalized.as_deref(),
            Some(std::path::Path::new("/home/user/bin/npm"))
        );
    }

    #[test]
    fn normalize_npm_path_relative_with_no_cwd_returns_none() {
        let path = PathBuf::from("bin/npm");
        let normalized = normalize_npm_path(path, None);
        assert!(
            normalized.is_none(),
            "relative path without cwd must return None so the caller can reject it"
        );
    }

    #[test]
    fn normalize_npm_path_bare_command_joined_with_cwd() {
        // A bare "npm" from `command -v` is relative; it must be joined with
        // the cwd so the resolved path is absolute.
        let path = PathBuf::from("npm");
        let normalized = normalize_npm_path(path, Some(std::path::Path::new("/opt/node/bin")));
        assert_eq!(
            normalized.as_deref(),
            Some(std::path::Path::new("/opt/node/bin/npm"))
        );
    }
}

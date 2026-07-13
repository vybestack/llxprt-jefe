//! Session-cached detection of installed agent runtimes.

use std::sync::OnceLock;

use crate::domain::AgentKind;

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
    let path = std::env::var_os("PATH");
    let dirs: Vec<std::path::PathBuf> = path
        .map(|p| std::env::split_paths(&p).collect())
        .unwrap_or_default();
    executable_in_dirs("npm", &dirs)
}

fn detect_installed_agent_kinds() -> Vec<AgentKind> {
    let path = std::env::var_os("PATH");
    let dirs: Vec<std::path::PathBuf> = path
        .map(|p| std::env::split_paths(&p).collect())
        .unwrap_or_default();
    detect_agent_kinds(&dirs)
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
    [AgentKind::Llxprt, AgentKind::CodePuppy]
        .into_iter()
        .filter(|kind| binary_in_dirs(kind.binary_name(), dirs))
        .collect()
}

fn binary_in_dirs(binary: &str, dirs: &[std::path::PathBuf]) -> bool {
    executable_in_dirs(binary, dirs).is_some()
}

fn executable_in_dirs(binary: &str, dirs: &[std::path::PathBuf]) -> Option<std::path::PathBuf> {
    dirs.iter()
        .map(|directory| directory.join(binary))
        .find(|candidate| is_executable(candidate))
}

#[cfg(unix)]
fn is_executable(path: &std::path::Path) -> bool {
    use std::os::unix::fs::PermissionsExt;

    std::fs::metadata(path)
        .is_ok_and(|metadata| metadata.is_file() && metadata.permissions().mode() & 0o111 != 0)
}

#[cfg(not(unix))]
fn is_executable(path: &std::path::Path) -> bool {
    path.is_file()
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
            let path = dir.join(binary);
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
}

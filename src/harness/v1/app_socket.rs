//! Reaping the multiplexer server the application under test owns (issue #586).
//!
//! The harness kills the launched process group at teardown, which is enough
//! for anything that stays in it. A tmux server does not: it daemonizes, so it
//! detaches from the group and survives with no parent. A scenario that points
//! the application at a socket therefore leaks one server per run, and the
//! agents inside it, until something kills them by hand.
//!
//! The harness cannot reach these through its own cleanup, because
//! `TmuxDriver::kill_harness_server` and the signal guard both target the
//! harness's private `-L jefe-harness-<pid>` socket, and the application's
//! server is somewhere else entirely.
//!
//! # Why containment decides this, not presence
//!
//! `JEFE_SOCKET_PATH` is a scenario-supplied value, and a scenario could name
//! any path on the machine — including the developer's real
//! `jefe-<uid>.sock`. Reaping whatever it names would let a test kill live
//! agents doing real work. So the socket is reaped **only** when it lies inside
//! the run's own workspace, which is created and destroyed by this run and
//! cannot belong to anyone else. Anything outside is left strictly alone.
//!
//! That rule also pushes scenarios toward per-run sockets, which is what fixes
//! the second half of the defect: a fixed shared path collides between
//! concurrent runs and leaves stale sockets behind.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

/// Environment variable naming the socket the application will serve on.
pub const APP_SOCKET_ENV: &str = "JEFE_SOCKET_PATH";

/// Decide which multiplexer socket, if any, this run is responsible for
/// reaping.
///
/// Returns the socket only when the resolved environment names one **and** it
/// is contained by `workspace_root`. A socket outside the workspace belongs to
/// someone else and is never touched, however the scenario got it there.
#[must_use]
pub fn socket_to_reap(
    environment: &BTreeMap<String, String>,
    workspace_root: &Path,
) -> Option<PathBuf> {
    let raw = environment.get(APP_SOCKET_ENV)?;
    if raw.is_empty() {
        return None;
    }
    let path = PathBuf::from(raw);
    // A relative path cannot be shown to be contained, so it is not reaped.
    // Jefe itself ignores a relative `JEFE_SOCKET_PATH` for the same reason.
    if !path.is_absolute() {
        return None;
    }
    contained(&path, workspace_root).then_some(path)
}

/// Whether `path` lies within `root`, comparing canonicalized ancestors so a
/// symlinked or `..`-laden path cannot claim containment it does not have.
fn contained(path: &Path, root: &Path) -> bool {
    let canonical_root = root.canonicalize().unwrap_or_else(|_| root.to_path_buf());
    // The socket file itself may not exist yet, so canonicalize its parent.
    let Some(parent) = path.parent() else {
        return false;
    };
    let canonical_parent = parent
        .canonicalize()
        .unwrap_or_else(|_| parent.to_path_buf());
    canonical_parent.starts_with(&canonical_root)
}

#[cfg(test)]
#[path = "app_socket_tests.rs"]
mod tests;

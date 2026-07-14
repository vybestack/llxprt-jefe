//! Prepared-launch execution helpers extracted from `manager.rs` to keep that
//! file under the source-file size hard limit.
//!
//! These free functions execute a [`PreparedLaunch`] — the non-destructive
//! resolution has already happened during `PreparedLaunch::prepare`. The
//! functions handle only the destructive kill + post-kill spawn, reusing the
//! prepared data without re-resolution (issue #269).
//!
//! The sequencing functions ([`run_prepared_transaction`] and
//! [`orchestrate_prepared`]) enforce the exact kill → delay → spawn ordering
//! and are shared between the force-fresh spawn path and the manager's
//! prepared replacement transaction. Tests observe the call log through
//! closures to prove ordering invariants without mock theater.

use std::path::Path;
use std::time::Duration;

use tracing::debug;

use crate::domain::LaunchSignature;

use super::commands;
use super::errors::RuntimeError;
use super::prepared_launch::{LocalExecuteFailure, PreparedLaunch};

/// Teardown delay between the single kill and the post-kill spawn in a
/// prepared replacement transaction (issue #269). Gives the old tmux
/// session time to release the pane before the new session is created.
pub(super) const PREPARED_KILL_TEARDOWN_DELAY: Duration = Duration::from_millis(1500);

/// Kill the existing session for a prepared launch, dispatching to the
/// remote or local kill command based on the prepared launch type.
pub(super) fn kill_session_for_prepared(
    prepared: &PreparedLaunch,
    session_name: &str,
) -> Result<(), RuntimeError> {
    match prepared {
        PreparedLaunch::Remote(remote_prepared) => {
            commands::kill_remote_session(remote_prepared.remote(), session_name)
        }
        PreparedLaunch::Local(_) => commands::kill_session(session_name),
    }
}

/// Execute a prepared launch (local or remote) with the fork-broken retry
/// path for local launches. All data comes from the prepared launch — no
/// re-resolution occurs. This is the single post-kill execution path for the
/// force-fresh `spawn_session_internal` branch, ensuring the pre-kill
/// validation and post-kill spawn use the exact same prepared data.
pub(super) fn execute_prepared_launch(prepared: &PreparedLaunch) -> Result<(), RuntimeError> {
    match prepared {
        PreparedLaunch::Remote(remote_prepared) => remote_prepared.execute(),
        PreparedLaunch::Local(local_prepared) => match local_prepared.try_execute() {
            Ok(()) => Ok(()),
            Err(LocalExecuteFailure::Runtime(error)) => Err(error),
            Err(LocalExecuteFailure::Command(stderr))
                if commands::is_tmux_fork_broken_pub(&stderr) =>
            {
                debug!(
                    session_name = %local_prepared.session_name(),
                    stderr = %stderr,
                    "prepared launch retrying after multiplexer fork failure"
                );
                let _ = commands::kill_session(local_prepared.session_name());
                local_prepared
                    .try_execute()
                    .map_err(|failure| match failure {
                        LocalExecuteFailure::Runtime(error) => error,
                        LocalExecuteFailure::Command(stderr) => {
                            RuntimeError::SpawnFailed(format!("tmux new-session failed: {stderr}"))
                        }
                    })
            }
            Err(LocalExecuteFailure::Command(stderr)) => Err(RuntimeError::SpawnFailed(format!(
                "tmux new-session failed: {stderr}"
            ))),
        },
    }
}

/// Execute an ALREADY prepared replacement transaction in exact order:
/// kill → delay → spawn. The kill is best-effort (logged on failure, does
/// not abort the spawn). Returns the spawn result.
///
/// Generic over closures so production paths pass real tmux/sleep closures
/// and tests pass observable call-log closures — exercising the SAME
/// sequencing logic (no test-only duplicate).
pub(super) fn run_prepared_transaction<K, D, S>(
    kill: K,
    delay: D,
    spawn: S,
) -> Result<(), RuntimeError>
where
    K: FnOnce() -> Result<(), RuntimeError>,
    D: FnOnce(),
    S: FnOnce() -> Result<(), RuntimeError>,
{
    // Best-effort kill: a stale/missing session is tolerated (scoped to one
    // agent). Logged but never aborts the spawn.
    if let Err(error) = kill() {
        debug!(error = %error, "prepared transaction: kill was not clean");
    }
    delay();
    spawn()
}

/// Orchestrate prepare → kill → delay → spawn. The prepare closure runs first
/// and exclusively; a prepare `Err` yields no kill, no delay, no spawn — a
/// provably empty call log. On prepare success the prepared data is passed to
/// the kill and spawn closures.
///
/// Generic over the prepared data type `T` so it can be tested with trivial
/// stubs while production passes [`PreparedLaunch`].
///
/// Used by [`force_fresh_spawn`] so the prepare-then-kill ordering is enforced
/// by the same production function that tests observe, not a test-only
/// duplicate.
pub(super) fn orchestrate_prepared<T, P, K, D, S>(
    prepare: P,
    kill: K,
    delay: D,
    spawn: S,
) -> Result<(), RuntimeError>
where
    P: FnOnce() -> Result<T, RuntimeError>,
    K: FnOnce(&T) -> Result<(), RuntimeError>,
    D: FnOnce(),
    S: FnOnce(&T) -> Result<(), RuntimeError>,
{
    let prepared = prepare()?;
    run_prepared_transaction(|| kill(&prepared), delay, || spawn(&prepared))
}

/// Force-fresh spawn: prepare all non-destructive launch prerequisites, then
/// kill the existing session, then execute from the prepared data.
///
/// Used by `spawn_session_internal` when `allow_reattach` is false (the
/// relaunch/restart path). Resolves everything BEFORE the kill, then reuses
/// the prepared data for the post-kill spawn (issue #269).
///
/// Delegates to [`orchestrate_prepared`] so the prepare-then-kill ordering is
/// enforced by the shared production sequencing function.
pub(super) fn force_fresh_spawn(
    session_name: &str,
    work_dir: &Path,
    signature: &LaunchSignature,
    npm_executable: Option<&Path>,
) -> Result<(), RuntimeError> {
    orchestrate_prepared(
        || PreparedLaunch::prepare(session_name, work_dir, signature, npm_executable),
        |prepared| kill_session_for_prepared(prepared, session_name),
        || {}, // no teardown delay for the force-fresh path; the delay lives
        // in the manager's prepared replacement transaction.
        execute_prepared_launch,
    )
}

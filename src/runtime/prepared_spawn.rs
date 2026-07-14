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
///
/// The fork-broken retry re-attempts the spawn WITHOUT a second kill: the
/// replacement transaction owns exactly one kill (`kill_session_for_prepared`
/// called from `run_prepared_transaction`), and a fork-broken `new-session`
/// that failed to create the session does not leave a live session needing
/// another kill. A second kill here would violate the single-kill invariant.
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

/// Phase of the kill → delay → spawn replacement transaction that failed.
///
/// The caller ([`spawn_prepared_session_internal`]) uses this to apply the
/// correct runtime-map and app-state policy for each outcome:
///
/// - [`PreparedTransactionPhase::Kill`]: the kill failed; the old session may
///   still be alive and its mapping must be preserved so the caller sees the
///   agent as still running its old session.
/// - [`PreparedTransactionPhase::Spawn`]: the kill succeeded but the spawn
///   failed; the old session is gone and its stale mapping must be removed,
///   but the dead relaunch signature is preserved so the agent can be
///   relaunched from it.
/// - [`PreparedTransactionPhase::Success`]: both kill and spawn succeeded;
///   the new mapping replaces the old one.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum PreparedTransactionPhase {
    Kill,
    Spawn,
    Success,
}

impl PreparedTransactionPhase {
    /// Whether the old session mapping should be removed after this phase.
    ///
    /// `true` only when the kill has succeeded (Spawn failure or Success):
    /// the old session is gone and a stale mapping would mislead the caller.
    /// For Prepare and Kill failures the old session may still be alive, so
    /// the mapping is preserved.
    #[must_use]
    pub(super) const fn removes_old_mapping(self) -> bool {
        matches!(self, Self::Spawn | Self::Success)
    }

    /// Whether the dead relaunch signature should be preserved after this
    /// phase.
    ///
    /// `true` only for Spawn failure: the kill succeeded (the old session is
    /// gone) but the spawn could not create a new one. The dead signature
    /// must be stashed so the agent can be relaunched later from its stored
    /// launch signature — rather than losing the ability to restart.
    #[must_use]
    pub(super) const fn preserves_dead_signature(self) -> bool {
        matches!(self, Self::Spawn)
    }
}

/// Execute an ALREADY prepared replacement transaction in exact order:
/// kill → delay → spawn. The kill is propagated: if the kill returns a real
/// error, the spawn is NOT attempted and the kill error is returned to the
/// caller. This prevents a half-dead old session from racing the new spawn.
///
/// Returns `Ok(Success)` on full success, or `Err((phase, RuntimeError))`
/// identifying which phase failed so the caller can apply the correct
/// runtime-map policy. The phase is never `Success` in the `Err` variant.
///
/// Generic over closures so production paths pass real tmux/sleep closures
/// and tests pass observable call-log closures — exercising the SAME
/// sequencing logic (no test-only duplicate).
pub(super) fn run_prepared_transaction<K, D, S>(
    kill: K,
    delay: D,
    spawn: S,
) -> Result<PreparedTransactionPhase, (PreparedTransactionPhase, RuntimeError)>
where
    K: FnOnce() -> Result<(), RuntimeError>,
    D: FnOnce(),
    S: FnOnce() -> Result<(), RuntimeError>,
{
    // Propagate real kill errors: do not spawn after a failed kill, which
    // would leave the old session in a half-dead state racing the new spawn.
    if let Err(error) = kill() {
        return Err((PreparedTransactionPhase::Kill, error));
    }
    delay();
    match spawn() {
        Ok(()) => Ok(PreparedTransactionPhase::Success),
        Err(error) => Err((PreparedTransactionPhase::Spawn, error)),
    }
}

/// Orchestrate prepare → kill → delay → spawn. The prepare closure runs first
/// and exclusively; a prepare `Err` yields no kill, no delay, no spawn — a
/// provably empty call log. On prepare success the prepared data is passed to
/// the kill and spawn closures.
///
/// The kill is best-effort (logged on failure): the force-fresh spawn path
/// uses this to clean up a possible stale session, but a missing/stale session
/// must not prevent a fresh spawn. The strict replacement transaction
/// (`spawn_prepared_session_internal`) calls `run_prepared_transaction`
/// directly to propagate kill errors.
///
/// Generic over the prepared data type `T` so it can be tested with trivial
/// stubs while production passes [`PreparedLaunch`].
///
/// Used by [`force_fresh_spawn`] so the prepare-then-kill ordering is
/// enforced by the shared production sequencing function, not a test-only
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
    // Best-effort kill for the force-fresh path: a stale/missing session is
    // tolerated. The strict kill propagation lives in run_prepared_transaction
    // for the prepared restart replacement.
    if let Err(error) = kill(&prepared) {
        debug!(error = %error, "force-fresh spawn pre-kill was not clean");
    }
    delay();
    spawn(&prepared)
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

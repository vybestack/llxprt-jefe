//! Runtime manager trait and implementations.
//!
//! @plan PLAN-20260216-FIRSTVERSION-V1.P06
//! @plan PLAN-20260216-FIRSTVERSION-V1.P08
//! @requirement REQ-TECH-004
//! @requirement REQ-FUNC-007
//! @pseudocode component-002 lines 01-35

use std::collections::{HashMap, HashSet};
use std::num::NonZeroUsize;
use std::path::Path;
use std::sync::atomic::{AtomicU64, Ordering};

use lru::LruCache;
use tracing::{debug, info};

use super::attach::AttachedViewer;
use super::commands;
use super::errors::RuntimeError;
use super::liveness;
use super::session::{RuntimeSession, TerminalCell, TerminalCellStyle, TerminalSnapshot};
use crate::domain::{AgentId, LaunchSignature, RemoteRepositorySettings};

#[path = "history_cache.rs"]
mod history_cache;
use history_cache::{HistoryCache, strip_trailing_rows};

/// Maximum number of dead-session launch signatures retained for relaunch.
///
/// Repeated kill/recreate cycles of *different* agents would otherwise grow
/// `dead_signatures` without bound. Bounding it with an LRU cache caps memory
/// usage while still preserving the most-recently-killed signatures, which are
/// the ones a user is most likely to relaunch. Constructed via `NonZeroUsize`
/// so `LruCache::new` never receives a zero capacity.
const MAX_DEAD_SIGNATURES: NonZeroUsize = match NonZeroUsize::new(100) {
    Some(n) => n,
    None => NonZeroUsize::MIN,
};

/// Maximum number of scrollback history lines retained for an embedded
/// terminal session (issue #198). Matches the `terminal-scrollback.json` test
/// scenario's `history_limit` (2000), intentionally smaller than the harness
/// default (10000) to bound render/capture cost.
const HISTORY_LINE_CAP: usize = 2000;

/// Lightweight metadata for checking session liveness without holding the runtime lock.
///
/// Callers collect these under the lock, drop it, then run the (potentially slow)
/// liveness checks externally — avoiding mutex contention with input/render paths.
#[derive(Clone)]
pub struct LivenessCheck {
    pub agent_id: AgentId,
    pub session_name: String,
    pub remote: Option<RemoteRepositorySettings>,
}

/// Runtime manager trait - owns attach/reattach, input forwarding, kill/relaunch.
///
/// This trait defines the boundary between the application layer and the
/// runtime orchestration layer (tmux/PTY). Implementations handle actual
/// process management, PTY I/O, and session lifecycle.
pub trait RuntimeManager: Send {
    /// Spawn a new runtime session for an agent.
    ///
    /// @pseudocode component-002 lines 01-06
    fn spawn_session(
        &mut self,
        agent_id: &AgentId,
        work_dir: &Path,
        signature: &LaunchSignature,
    ) -> Result<(), RuntimeError>;

    /// Spawn a new runtime session and force a fresh tmux process.
    ///
    /// This bypasses reattach behavior and is used for explicit user relaunch
    /// after kill, so latest config/env values are guaranteed to apply.
    fn spawn_session_fresh(
        &mut self,
        agent_id: &AgentId,
        work_dir: &Path,
        signature: &LaunchSignature,
    ) -> Result<(), RuntimeError> {
        self.spawn_session(agent_id, work_dir, signature)
    }

    /// Attach to an existing session.
    ///
    /// @pseudocode component-002 lines 07-14
    fn attach(&mut self, agent_id: &AgentId) -> Result<(), RuntimeError>;

    /// Detach from the currently attached session.
    fn detach(&mut self) -> Result<(), RuntimeError>;

    /// Kill a running session.
    ///
    /// @pseudocode component-002 lines 21-26
    fn kill(&mut self, agent_id: &AgentId) -> Result<(), RuntimeError>;

    /// Relaunch a dead session using its stored launch signature.
    ///
    /// @pseudocode component-002 lines 27-32
    fn relaunch(&mut self, agent_id: &AgentId) -> Result<(), RuntimeError>;

    /// Check if a session is alive.
    ///
    /// @pseudocode component-002 lines 33-35
    fn is_alive(&self, agent_id: &AgentId) -> bool;

    /// Check whether a tmux session exists for the given agent.
    fn session_exists(&self, agent_id: &AgentId) -> bool;

    /// Get terminal snapshot for the currently attached session.
    fn snapshot(&self) -> Option<TerminalSnapshot>;

    /// Forward input bytes to the attached session.
    ///
    /// @pseudocode component-002 lines 15-20
    fn write_input(&mut self, bytes: &[u8]) -> Result<(), RuntimeError>;

    /// Resize the attached terminal.
    fn resize(&mut self, rows: u16, cols: u16) -> Result<(), RuntimeError>;

    /// Get the currently attached agent ID.
    fn attached_agent(&self) -> Option<&AgentId>;

    /// Whether the attached application currently has terminal mouse reporting enabled.
    fn mouse_reporting_active(&self) -> bool;

    /// Whether the attached application currently has bracketed paste enabled.
    fn bracketed_paste_active(&self) -> bool;

    /// Atomically read and clear the dirty flag on the attached viewer.
    ///
    /// Returns `true` when new PTY data has arrived since the last call,
    /// `false` otherwise. This enables event-driven rendering: the render loop
    /// only triggers a re-render when the terminal content has actually changed,
    /// avoiding wasteful ~30fps renders that block keyboard input processing.
    #[must_use]
    fn take_dirty(&self) -> bool;

    /// Non-consuming check of the dirty flag on the attached viewer (issue #198).
    ///
    /// Returns `true` when new PTY data has arrived since the last
    /// [`take_dirty`](Self::take_dirty), without clearing the flag. Used by the
    /// scrollback history cache to decide whether to re-capture without
    /// stealing the dirty flag out from under the render-decision path.
    #[must_use]
    fn is_dirty(&self) -> bool;

    /// Monotonically increasing generation counter for attached PTY output
    /// (issue #198 review fix).
    ///
    /// Increments when new output arrives on the attached viewer. The
    /// scrollback history cache stores the generation it captured at and
    /// compares it to the *current* generation to decide whether a re-capture
    /// is necessary. This decouples history-cache invalidation from the
    /// render-decision dirty flag (`take_dirty`), which is consumed during the
    /// render decision and therefore always reads `false` later in the same
    /// render frame — causing stale caches when `is_dirty()` was used.
    #[must_use]
    fn output_generation(&self) -> u64;

    /// Get a reference to a session by agent ID.
    fn get_session(&self, agent_id: &AgentId) -> Option<&RuntimeSession>;

    /// Capture pane output for a known session (used for dead-pane crash text).
    fn capture_session_output(&self, agent_id: &AgentId) -> Option<TerminalSnapshot>;

    /// Retrieve retained scrollback history lines for the currently attached
    /// session (issue #198).
    ///
    /// Returns `Option<Vec<String>>` — plain-text rows (no styles) from the
    /// tmux pane's scrollback buffer. Implementations SHOULD cache so they do
    /// not shell out on every render frame: re-capture only when `take_dirty()`
    /// returns true (new PTY data) or the attached session changes.
    ///
    /// - **`TmuxRuntimeManager`**: cached `capture-pane -S` bounded to
    ///   `HISTORY_LINE_CAP` lines.
    /// - **`StubRuntimeManager`**: always returns `None` (no PTY).
    fn capture_history(&mut self) -> Option<Vec<String>>;
}
/// Real tmux-based runtime manager.
///
/// @plan PLAN-20260216-FIRSTVERSION-V1.P08
/// @requirement REQ-TECH-004
/// @requirement REQ-FUNC-007
pub struct TmuxRuntimeManager {
    /// Active sessions by agent ID.
    sessions: HashMap<AgentId, RuntimeSession>,
    /// Currently attached viewer (single viewer model).
    viewer: Option<AttachedViewer>,
    /// Agent ID of the currently attached session.
    attached_agent_id: Option<AgentId>,
    /// Dead sessions that can be relaunched (stores signatures).
    ///
    /// Bounded by [`MAX_DEAD_SIGNATURES`]: once full, the least-recently-used
    /// dead signature is evicted to make room for newer ones.
    dead_signatures: LruCache<AgentId, LaunchSignature>,
    /// Session names for which clipboard passthrough has already been enforced.
    ///
    /// Avoids re-running the tmux option commands on every attach. Populated
    /// during local session creation and the local attach path.
    clipboard_enforced: HashSet<String>,
    /// Session names for which tmux prefix passthrough has already been
    /// enforced.
    ///
    /// Mirrors [`clipboard_enforced`](Self::clipboard_enforced): the prefix
    /// options are idempotent, but we memoize so the reattach/attach hot paths
    /// do not re-shell out to tmux for a session already remediated (#200).
    prefix_enforced: HashSet<String>,
    /// Terminal dimensions.
    rows: u16,
    cols: u16,
    /// Monotonically increasing PTY-output generation counter (issue #198).
    /// Incremented by `take_dirty()`. The history cache compares the stored
    /// generation to decide re-capture.
    output_generation: AtomicU64,
    /// Cached scrollback history (issue #198).
    history_cache: HistoryCache,
}

/// Move the current viewer (if any) out of the manager and drop it on a
/// background OS thread.
///
/// `AttachedViewer::drop` performs deterministic child teardown — killing the
/// tmux child and waiting up to 300ms for it to exit. Running that inline
/// blocks the caller (the input/render loop). Dropping on a detached thread
/// keeps the executor responsive while still guaranteeing eventual cleanup.
fn drop_viewer_in_background(viewer: &mut Option<AttachedViewer>) {
    if let Some(old_viewer) = viewer.take() {
        std::thread::spawn(move || drop(old_viewer));
    }
}

impl TmuxRuntimeManager {
    /// Create a new tmux runtime manager.
    #[must_use]
    pub fn new(rows: u16, cols: u16) -> Self {
        Self {
            sessions: HashMap::new(),
            viewer: None,
            attached_agent_id: None,
            dead_signatures: LruCache::new(MAX_DEAD_SIGNATURES),
            clipboard_enforced: HashSet::new(),
            prefix_enforced: HashSet::new(),
            rows,
            cols,
            output_generation: AtomicU64::new(0),
            history_cache: HistoryCache::default(),
        }
    }

    /// Update terminal dimensions.
    pub fn set_size(&mut self, rows: u16, cols: u16) {
        self.rows = rows;
        self.cols = cols;
    }

    /// Enforce clipboard passthrough for `session_name` if not already done.
    ///
    /// Memoized per session name so the tmux option commands run at most once
    /// per session across create + attach cycles.
    fn ensure_clipboard_passthrough(&mut self, session_name: &str) {
        if !self.clipboard_enforced.contains(session_name) {
            commands::enforce_clipboard_passthrough(session_name);
            self.clipboard_enforced.insert(session_name.to_owned());
        }
    }

    /// Test-only accessor: whether clipboard passthrough was already recorded
    /// for `session_name`.
    #[cfg(test)]
    fn clipboard_passthrough_enforced(&self, session: &str) -> bool {
        self.clipboard_enforced.contains(session)
    }

    /// Test-only setter for recording clipboard passthrough without invoking tmux.
    #[cfg(test)]
    fn record_clipboard_passthrough(&mut self, session: &str) {
        self.clipboard_enforced.insert(session.to_owned());
    }

    /// Enforce tmux prefix passthrough for `session_name` if not already done.
    ///
    /// Memoized per session name so the tmux option commands run at most once
    /// per session across create + attach cycles, mirroring
    /// [`ensure_clipboard_passthrough`](Self::ensure_clipboard_passthrough).
    ///
    /// This is the reattach-side remediation for issue #200: a session created
    /// before the prefix-disabling fix still has tmux's default `C-b` prefix,
    /// which the attach client would use to eat the `0x02` byte of application
    /// control chords. Calling this on every attach guarantees the prefix is
    /// disabled even for pre-existing sessions.
    fn ensure_prefix_passthrough(&mut self, session_name: &str) {
        if self.prefix_enforced.contains(session_name) {
            return;
        }
        // Only memoize on success, mirroring the remote path: a transient tmux
        // failure leaves the session un-remediated and un-memoized so the next
        // attach retries (#200 review).
        match commands::disable_prefix_for_passthrough(session_name) {
            Ok(()) => {
                self.prefix_enforced.insert(session_name.to_owned());
            }
            Err(error) => {
                debug!(session_name = %session_name, error = %error, "prefix passthrough failed on local attach; will retry next attach");
            }
        }
    }

    /// Enforce tmux prefix passthrough on a remote session if not already done.
    ///
    /// Remote mirror of [`ensure_prefix_passthrough`](Self::ensure_prefix_passthrough):
    /// best-effort because a transient SSH failure must not block reattach, but
    /// success is memoized so the option is applied exactly once per session.
    fn ensure_remote_prefix_passthrough(
        &mut self,
        remote: &crate::domain::RemoteRepositorySettings,
        session_name: &str,
    ) {
        if self.prefix_enforced.contains(session_name) {
            return;
        }
        let command = commands::remote_disable_prefix_command(remote, session_name);
        // run_remote_ssh returns Ok(Output) whenever SSH ran to completion — a
        // non-zero remote exit (session gone, set-option rejected, sudo denied)
        // must NOT be memoized as enforced, or future attaches skip the retry.
        match commands::run_remote_ssh(remote, &command) {
            Ok(output) if output.status.success() => {
                self.prefix_enforced.insert(session_name.to_owned());
            }
            Ok(output) => {
                debug!(
                    session_name = %session_name,
                    status = %output.status,
                    stderr = %String::from_utf8_lossy(&output.stderr),
                    "remote prefix passthrough command exited non-zero; will retry next attach"
                );
            }
            Err(error) => {
                debug!(session_name = %session_name, error = %error, "remote prefix passthrough failed on attach; will retry next attach");
            }
        }
    }

    /// Test-only accessor: whether prefix passthrough was already recorded
    /// for `session_name`.
    #[cfg(test)]
    fn prefix_passthrough_enforced(&self, session: &str) -> bool {
        self.prefix_enforced.contains(session)
    }

    /// Test-only setter for recording prefix passthrough without invoking tmux.
    #[cfg(test)]
    fn record_prefix_passthrough(&mut self, session: &str) {
        self.prefix_enforced.insert(session.to_owned());
    }

    /// Collect liveness check metadata for all tracked sessions.
    ///
    /// The caller can drop the runtime lock before performing the actual
    /// (potentially blocking) liveness checks, preventing SSH round-trips
    /// from stalling the input/render loop.
    #[must_use]
    pub fn liveness_targets(&self) -> Vec<LivenessCheck> {
        self.sessions
            .iter()
            .map(|(agent_id, session)| LivenessCheck {
                agent_id: agent_id.clone(),
                session_name: session.session_name.clone(),
                remote: if session.launch_signature.remote.enabled {
                    Some(session.launch_signature.remote.clone())
                } else {
                    None
                },
            })
            .collect()
    }

    /// Check whether a session exists using explicit launch-signature context.
    #[must_use]
    pub fn session_exists_for_signature(
        &self,
        agent_id: &AgentId,
        signature: &LaunchSignature,
    ) -> bool {
        let session_name = RuntimeSession::session_name_for(agent_id);
        if signature.remote.enabled {
            commands::remote_session_exists(&signature.remote, &session_name).unwrap_or(false)
        } else {
            liveness::check_session_alive(&session_name)
        }
    }

    pub fn mark_session_dead(&mut self, agent_id: &AgentId) -> bool {
        let Some(session) = self.sessions.remove(agent_id) else {
            return false;
        };

        if self.attached_agent_id.as_ref() == Some(agent_id) {
            self.attached_agent_id = None;
            drop_viewer_in_background(&mut self.viewer);
        }

        // Invalidate scrollback cache (fix #8).
        self.history_cache.clear(agent_id);

        // The tmux session is gone, so its memoized passthrough state is stale.
        // Clear both sets so a recreated session with the same name re-enforces
        // on the next attach, and so the sets do not grow across natural
        // session exits (#200; parity with the explicit kill() path).
        self.clipboard_enforced.remove(&session.session_name);
        self.prefix_enforced.remove(&session.session_name);

        let _ = self
            .dead_signatures
            .put(agent_id.clone(), session.launch_signature.clone());
        true
    }

    /// Return the stored worker PID (`llxprt` OS process) for an agent, if known.
    ///
    /// Bridges the runtime layer to the app/domain layer for the PID-based
    /// liveness fallback. Returns `None` for untracked agents or sessions whose
    /// PID was never captured (e.g. remote sessions, or pre-restored entries).
    #[must_use]
    pub fn worker_pid(&self, agent_id: &AgentId) -> Option<u32> {
        self.sessions.get(agent_id).and_then(|s| s.pid)
    }

    fn spawn_session_internal(
        &mut self,
        agent_id: &AgentId,
        work_dir: &Path,
        signature: &LaunchSignature,
        allow_reattach: bool,
    ) -> Result<(), RuntimeError> {
        // Check for duplicate runtime mapping in this process.
        if self.sessions.contains_key(agent_id) {
            return Err(RuntimeError::AlreadyRunning(agent_id.clone()));
        }

        // Fresh spawn (not reattach): invalidate stale cache (fix #8).
        if !allow_reattach {
            self.history_cache.clear(agent_id);
        }

        let session_name = RuntimeSession::session_name_for(agent_id);

        // Reattach-first behavior is only allowed for restore/startup paths.
        let can_reattach = allow_reattach && self.session_exists_for_signature(agent_id, signature);
        if can_reattach {
            debug!(session_name = %session_name, "reattaching to existing tmux session");
            // The session may predate the prefix-passthrough fix (#200): a
            // pre-existing session still has tmux's default `C-b` prefix, which
            // the attach client uses to eat the 0x02 byte of control chords.
            // Remediate it on reattach so local and remote sessions behave
            // identically to freshly created ones.
            if signature.remote.enabled {
                self.ensure_remote_prefix_passthrough(&signature.remote, &session_name);
            } else {
                self.ensure_prefix_passthrough(&session_name);
            }
        } else {
            if !allow_reattach {
                // Explicit relaunch-after-kill path: best-effort kill by name so a
                // stale session cannot be reused with old environment values.
                let kill_result = if signature.remote.enabled {
                    commands::kill_remote_session(&signature.remote, &session_name)
                } else {
                    commands::kill_session(&session_name)
                };
                if let Err(error) = kill_result {
                    debug!(
                        session_name = %session_name,
                        error = %error,
                        "force-fresh spawn pre-kill was not clean"
                    );
                }
            }

            debug!(session_name = %session_name, "creating new tmux session");
            commands::create_session(&session_name, work_dir, signature)?;

            // `finalize_local_session` (inside create_session) already ran
            // `enforce_clipboard_passthrough` for a freshly created local
            // session. Use ensure_clipboard_passthrough to record it — this
            // is a no-op if finalize_local_session already did the work,
            // but is robust against future refactors of that call chain.
            if !signature.remote.enabled {
                self.ensure_clipboard_passthrough(&session_name);
                // finalize_local_session also disabled the tmux prefix
                // (#200). Re-run the result-aware enforcer instead of blindly
                // memoizing: if the create-path prefix setup failed,
                // ensure_prefix_passthrough retries it now and memoizes only
                // on success. If it already succeeded this is an idempotent
                // no-op that just records the state.
                self.ensure_prefix_passthrough(&session_name);
            }
        }

        // Capture the worker PID for the PID-liveness fallback. `pane_pid`
        // only returns the worker PID when the worker runs as the pane's
        // *direct* command — jefe launches `llxprt` directly (no shell/wrapper
        // in the pane), so the pane PID *is* the worker PID. It is
        // local-only, so it is not queried for remote sessions. Captured for
        // both the reattach and create branches so creation and revival stay
        // symmetric.
        //
        // On the reattach path this is best-effort but valid: reattach only
        // occurs after `check_session_alive` confirmed a non-dead pane, which
        // means the pane's direct command (the llxprt worker) is still
        // running, so `#{pane_pid}` is the worker PID. We capture it here so
        // it persists into RuntimeBinding for the PID-liveness fallback.
        let captured_pid = if signature.remote.enabled {
            None
        } else {
            commands::pane_pid(&session_name)
        };

        // Store/refresh session binding.
        let mut session = RuntimeSession::new(agent_id.clone(), session_name, signature.clone());
        session.pid = captured_pid;
        self.sessions.insert(agent_id.clone(), session);

        // Remove from dead signatures if present.
        let _ = self.dead_signatures.pop(agent_id);

        Ok(())
    }
}

impl RuntimeManager for TmuxRuntimeManager {
    fn spawn_session(
        &mut self,
        agent_id: &AgentId,
        work_dir: &Path,
        signature: &LaunchSignature,
    ) -> Result<(), RuntimeError> {
        info!(agent_id = %agent_id.0, work_dir = %work_dir.display(), "spawning runtime session");
        self.spawn_session_internal(agent_id, work_dir, signature, true)
    }

    fn spawn_session_fresh(
        &mut self,
        agent_id: &AgentId,
        work_dir: &Path,
        signature: &LaunchSignature,
    ) -> Result<(), RuntimeError> {
        info!(
            agent_id = %agent_id.0,
            work_dir = %work_dir.display(),
            "spawning fresh runtime session"
        );
        self.spawn_session_internal(agent_id, work_dir, signature, false)
    }

    fn attach(&mut self, agent_id: &AgentId) -> Result<(), RuntimeError> {
        debug!(
            agent_id = %agent_id.0,
            current_attached = ?self.attached_agent_id.as_ref().map(|id| &id.0),
            "attaching viewer"
        );

        // Check session exists
        if !self.sessions.contains_key(agent_id) {
            return Err(RuntimeError::SessionNotFound(agent_id.0.clone()));
        }

        // Detach current viewer if different
        if self.attached_agent_id.as_ref() != Some(agent_id) {
            // Mark old session as detached
            if let Some(old_id) = self.attached_agent_id.take()
                && let Some(old_session) = self.sessions.get_mut(&old_id)
            {
                debug!(old_agent_id = %old_id.0, "detaching previous viewer");
                old_session.attached = false;
            }

            // Drop old viewer on a background thread. AttachedViewer::drop
            // performs deterministic child teardown (bounded kill/wait up to
            // 300ms), which would otherwise block the attach call.
            drop_viewer_in_background(&mut self.viewer);

            // Get session name and remote settings for spawning.
            let Some(session) = self.sessions.get(agent_id) else {
                return Err(RuntimeError::SessionNotFound(agent_id.0.clone()));
            };
            let session_name = session.session_name.clone();
            let remote_enabled = session.launch_signature.remote.enabled;
            let remote_settings = if remote_enabled {
                Some(session.launch_signature.remote.clone())
            } else {
                None
            };

            // Enforce clipboard passthrough (memoized) before spawning the
            // local viewer — the attach hot path no longer relies on
            // AttachedViewer::spawn to do this.
            if !remote_enabled {
                self.ensure_clipboard_passthrough(&session_name);
                // Same invariant for tmux prefix passthrough (#200): a
                // session reattached after an upgrade must not keep the
                // default C-b prefix that eats control-chord bytes.
                self.ensure_prefix_passthrough(&session_name);
            } else if let Some(remote) = remote_settings.as_ref() {
                self.ensure_remote_prefix_passthrough(remote, &session_name);
            }

            // Spawn new viewer
            debug!(agent_id = %agent_id.0, session_name = %session_name, "attach: spawning AttachedViewer");
            let viewer = if let Some(remote) = remote_settings {
                let ssh_command = commands::build_remote_attach_command(&remote, &session_name);
                AttachedViewer::spawn_remote(&session_name, self.rows, self.cols, &ssh_command)?
            } else {
                AttachedViewer::spawn(&session_name, self.rows, self.cols)?
            };

            if !viewer.is_alive() {
                debug!(agent_id = %agent_id.0, session_name = %session_name, "attach: viewer exited immediately");
                if let Some(session) = self.sessions.get_mut(agent_id) {
                    session.attached = false;
                }
                return Err(RuntimeError::AttachFailed(format!(
                    "session {session_name} terminated before attach completed"
                )));
            }

            debug!(agent_id = %agent_id.0, session_name = %session_name, "attach: AttachedViewer spawned");
            self.viewer = Some(viewer);
            self.attached_agent_id = Some(agent_id.clone());
        }

        // Mark new session as attached
        if let Some(session) = self.sessions.get_mut(agent_id) {
            session.attached = true;
        }
        Ok(())
    }

    fn detach(&mut self) -> Result<(), RuntimeError> {
        debug!("detaching current viewer");
        if let Some(agent_id) = self.attached_agent_id.take()
            && let Some(session) = self.sessions.get_mut(&agent_id)
        {
            session.attached = false;
        }

        // Drop the attached viewer on a background thread. AttachedViewer::drop
        // performs deterministic child teardown (bounded kill/wait up to 300ms).
        drop_viewer_in_background(&mut self.viewer);

        Ok(())
    }

    fn kill(&mut self, agent_id: &AgentId) -> Result<(), RuntimeError> {
        info!(agent_id = %agent_id.0, "killing runtime session");
        let session = self
            .sessions
            .remove(agent_id)
            .ok_or_else(|| RuntimeError::SessionNotFound(agent_id.0.clone()))?;

        // Store signature for relaunch
        let _ = self
            .dead_signatures
            .put(agent_id.clone(), session.launch_signature.clone());

        // Clear clipboard and prefix passthrough memoization for this session
        // so a recreated session with the same name re-enforces on next attach
        // (and the sets don't grow unbounded across kill/recreate cycles).
        self.clipboard_enforced.remove(&session.session_name);
        self.prefix_enforced.remove(&session.session_name);

        // Invalidate scrollback cache for this agent (fix #8).
        self.history_cache.clear(agent_id);

        // If attached, clear attachment and drop viewer.
        if self.attached_agent_id.as_ref() == Some(agent_id) {
            self.attached_agent_id = None;

            // Drop the attached viewer on a background thread. AttachedViewer::drop
            // performs deterministic child teardown (bounded kill/wait up to 300ms).
            drop_viewer_in_background(&mut self.viewer);
        }

        // Kill tmux session
        if session.launch_signature.remote.enabled {
            commands::kill_remote_session(&session.launch_signature.remote, &session.session_name)?;
        } else {
            commands::kill_session(&session.session_name)?;
        }

        Ok(())
    }

    fn relaunch(&mut self, agent_id: &AgentId) -> Result<(), RuntimeError> {
        info!(agent_id = %agent_id.0, "relaunching runtime session");
        // Check not already running
        if self.sessions.contains_key(agent_id) {
            return Err(RuntimeError::AlreadyRunning(agent_id.clone()));
        }

        // Get stored signature
        let signature = self
            .dead_signatures
            .pop(agent_id)
            .ok_or_else(|| RuntimeError::NotRunning(agent_id.clone()))?;

        // Spawn with stored signature using force-fresh semantics so runtime
        // warnings are surfaced consistently through the relaunch path.
        self.spawn_session_fresh(agent_id, &signature.work_dir.clone(), &signature)?;

        Ok(())
    }

    fn is_alive(&self, agent_id: &AgentId) -> bool {
        if let Some(session) = self.sessions.get(agent_id) {
            if session.launch_signature.remote.enabled {
                liveness::check_remote_session_alive(
                    &session.launch_signature.remote,
                    &session.session_name,
                )
            } else {
                liveness::check_session_alive(&session.session_name)
            }
        } else {
            false
        }
    }

    fn session_exists(&self, agent_id: &AgentId) -> bool {
        if let Some(session) = self.sessions.get(agent_id)
            && session.launch_signature.remote.enabled
        {
            return commands::remote_session_exists(
                &session.launch_signature.remote,
                &session.session_name,
            )
            .unwrap_or(false);
        }

        let session_name = RuntimeSession::session_name_for(agent_id);
        liveness::check_session_alive(&session_name)
    }

    fn snapshot(&self) -> Option<TerminalSnapshot> {
        self.viewer.as_ref().and_then(AttachedViewer::snapshot)
    }

    fn write_input(&mut self, bytes: &[u8]) -> Result<(), RuntimeError> {
        let viewer = self.viewer.as_ref().ok_or(RuntimeError::NoAttachedViewer)?;
        viewer.write_input(bytes)
    }

    fn resize(&mut self, rows: u16, cols: u16) -> Result<(), RuntimeError> {
        self.rows = rows;
        self.cols = cols;

        if let Some(viewer) = &self.viewer {
            viewer.resize(rows, cols)?;
        }

        Ok(())
    }

    fn attached_agent(&self) -> Option<&AgentId> {
        self.attached_agent_id.as_ref()
    }

    fn mouse_reporting_active(&self) -> bool {
        self.viewer
            .as_ref()
            .is_some_and(AttachedViewer::mouse_reporting_active)
    }

    fn bracketed_paste_active(&self) -> bool {
        self.viewer
            .as_ref()
            .is_some_and(AttachedViewer::bracketed_paste_active)
    }

    fn take_dirty(&self) -> bool {
        let dirty = self.viewer.as_ref().is_some_and(AttachedViewer::take_dirty);
        // Bump the generation whenever the render-decision path consumes new
        // PTY data. The history cache compares the stored generation to this
        // counter to decide re-capture, fully decoupled from the volatile
        // dirty flag (issue #198 review fix).
        if dirty {
            self.output_generation.fetch_add(1, Ordering::Relaxed);
        }
        dirty
    }

    fn is_dirty(&self) -> bool {
        self.viewer.as_ref().is_some_and(AttachedViewer::is_dirty)
    }

    fn output_generation(&self) -> u64 {
        self.output_generation.load(Ordering::Relaxed)
    }

    fn get_session(&self, agent_id: &AgentId) -> Option<&RuntimeSession> {
        self.sessions.get(agent_id)
    }

    fn capture_session_output(&self, agent_id: &AgentId) -> Option<TerminalSnapshot> {
        let session = self.sessions.get(agent_id)?;
        if session.launch_signature.remote.enabled {
            return None;
        }

        let lines = commands::capture_pane_lines(&session.session_name)?;

        let rows = lines.len();
        let cols = lines
            .iter()
            .map(|line| line.chars().count())
            .max()
            .unwrap_or(0);

        if rows == 0 || cols == 0 {
            return Some(TerminalSnapshot::default());
        }

        let default_style = TerminalCellStyle {
            fg: iocraft::Color::White,
            bg: iocraft::Color::Black,
            bold: false,
            dim: false,
            underline: false,
        };

        let mut snapshot = TerminalSnapshot::blank(rows, cols, default_style);
        // `capture_pane_lines` does not preserve soft-wrap metadata, so all
        // rows are treated as hard line breaks (wraps stays all-false, which
        // is the default from `blank` — issue #197).
        snapshot.wraps = vec![false; rows];
        for (r, line) in lines.iter().enumerate() {
            for (c, ch) in line.chars().enumerate() {
                snapshot.cells[r][c] = TerminalCell {
                    ch,
                    style: default_style,
                    wide_spacer: false,
                };
            }
        }

        Some(snapshot)
    }

    fn capture_history(&mut self) -> Option<Vec<String>> {
        let agent_id = self.attached_agent_id.clone()?;
        let session_name = self
            .sessions
            .get(&agent_id)
            .map(|s| s.session_name.clone())?;

        // Remote sessions do not support local capture-pane history.
        let is_remote = self
            .sessions
            .get(&agent_id)
            .is_some_and(|s| s.launch_signature.remote.enabled);
        if is_remote {
            return None;
        }

        // Cache hit: same agent + generation + not dirty → reuse (fix #2/#10).
        // The generation counter increments on take_dirty(). Also treat a
        // currently-dirty viewer as a cache miss so input-driven refresh does
        // not serve stale lines before take_dirty() bumps the generation.
        let generation = self.output_generation();
        let is_currently_dirty = self.is_dirty();
        if !is_currently_dirty && let Some(cached) = self.history_cache.get(&agent_id, generation) {
            return Some(cached.clone());
        }

        // Cache miss / dirty: re-capture. On transient failure, return prior
        // cache so a momentary tmux hiccup doesn't wipe retained history.
        let Some(raw_lines) = commands::capture_pane_history(&session_name, HISTORY_LINE_CAP)
        else {
            if let Some(prior) = self.history_cache.get_fallback(&agent_id) {
                debug!(session_name = %session_name, "capture-pane failed; retaining prior cache");
                return Some(prior.clone());
            }
            return None;
        };

        // Strip the visible pane rows (live snapshot already has them).
        let live_rows = self.snapshot().map_or(0, |s| s.rows);
        let lines = strip_trailing_rows(raw_lines, live_rows);

        // Do NOT strip trailing blank lines — they may be real blank output,
        // not tmux padding. Cache the result (including an empty capture) so
        // we don't shell out every frame. Return `Some(lines)` consistently so
        // the current frame and subsequent cache-hit frames agree (an empty
        // capture returns `Some(vec![])`, not `None` — callers normalize via
        // `map_or(0, Vec::len)`, and `None` is reserved for "no session /
        // capture not applicable").
        self.history_cache
            .store(&agent_id, generation, Some(lines.clone()));

        Some(lines)
    }
}

#[cfg(test)]
#[path = "manager_tests.rs"]
mod tests;

#[cfg(test)]
#[path = "history_tests.rs"]
mod history_tests;

//! Runtime manager trait and implementations.
//!
//! @plan PLAN-20260216-FIRSTVERSION-V1.P06
//! @plan PLAN-20260216-FIRSTVERSION-V1.P08
//! @requirement REQ-TECH-004
//! @requirement REQ-FUNC-007
use super::attach::AttachedViewer;
use super::commands;
use super::errors::RuntimeError;
use super::liveness;
use super::session::{RuntimeSession, TerminalSnapshot};
use crate::domain::agent_definition::AgentLaunchPlan;
use crate::domain::{AgentId, RemoteRepositorySettings};
use crate::jsp_host::JspLaunchCoordinator;
use crate::runtime::agent_preflight::{AuthorizedLaunchPlan, ProcessSandboxInspector};
use crate::runtime::jsp_launch;
use lru::LruCache;
use std::collections::{HashMap, HashSet};
use std::num::NonZeroUsize;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use tracing::{debug, info};
/// Inputs needed to build an `AttachedViewer` without holding the runtime lock
/// (issue #301 Phase 3).
///
/// Snapshotted under a short lock, then the viewer is built on a background
/// thread, then `apply_attach_result` installs it.
#[derive(Clone, Debug)]
pub struct AttachInputs {
    pub session_name: String,
    pub remote: Option<RemoteRepositorySettings>,
    pub rows: u16,
    pub cols: u16,
}
#[path = "manager_existing.rs"]
mod existing;
#[path = "history_cache.rs"]
pub mod history_cache;
use history_cache::HistoryCache;
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

/// Marker that a killed agent is eligible for relaunch.
///
/// Relaunch authority (the [`AuthorizedLaunchPlan`]) is supplied by the caller
/// at relaunch time through the authorized-preparation contract, so this marker
/// only records that the agent was previously running and is now dead. Its
/// bounded LRU membership is what `relaunch` checks before accepting a new
/// launch proof.
#[derive(Debug, Clone, Copy, Default)]
struct RetainedLaunch;

/// Pop a successfully relaunched agent from the dead-marker cache; leave the
/// marker in place on failure so the caller can retry.
fn complete_relaunch_attempt(
    dead_plans: &mut LruCache<AgentId, RetainedLaunch>,
    agent_id: &AgentId,
    result: Result<(), RuntimeError>,
) -> Result<(), RuntimeError> {
    if result.is_ok() {
        let _ = dead_plans.pop(agent_id);
    }
    result
}

/// Maximum scrollback history lines for an embedded terminal session (#198).
///
/// Matches the `terminal-scrollback.json` scenario's `history_limit` (2000),
/// intentionally smaller than the harness default (10000) to bound capture
/// cost.
pub const HISTORY_LINE_CAP: usize = 2000;

/// Metadata for checking session liveness without holding the runtime lock.
///
/// Callers collect these under the lock, drop it, then run the (potentially
/// slow) liveness checks externally — avoiding mutex contention with
/// input/render paths.
///
/// Issue #301 Phase 4: `binding_session_name` and `lifecycle_generation`
/// carry the identity of the binding at snapshot time so stale liveness
/// results (after rebind/restart) can be rejected.
#[derive(Clone)]
pub struct LivenessCheck {
    pub agent_id: AgentId,
    pub session_name: String,
    pub remote: Option<RemoteRepositorySettings>,
    /// The session name the runtime binding referenced at snapshot time.
    /// If the agent is rebound/restarted, this will differ from the current
    /// binding's session name, and the liveness result is stale.
    pub binding_session_name: Option<String>,
    /// Per-agent lifecycle generation at snapshot time. Incremented on
    /// spawn/relaunch/kill/rebind. A mismatch means the agent was
    /// restarted/rebound after the liveness check was dispatched.
    pub lifecycle_generation: u64,
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
        launch: &AuthorizedLaunchPlan,
        remote: Option<&RemoteRepositorySettings>,
    ) -> Result<(), RuntimeError>;

    /// Spawn a new runtime session and force a fresh tmux process.
    ///
    /// This bypasses reattach behavior and is used for explicit user relaunch
    /// after kill, so latest config/env values are guaranteed to apply.
    fn spawn_session_fresh(
        &mut self,
        agent_id: &AgentId,
        launch: &AuthorizedLaunchPlan,
        remote: Option<&RemoteRepositorySettings>,
    ) -> Result<(), RuntimeError> {
        self.spawn_session(agent_id, launch, remote)
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

    /// Relaunch a dead session using newly cleared launch authority.
    ///
    /// @pseudocode component-002 lines 27-32
    fn relaunch(
        &mut self,
        agent_id: &AgentId,
        launch: &AuthorizedLaunchPlan,
        remote: Option<&RemoteRepositorySettings>,
    ) -> Result<(), RuntimeError>;

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

    /// Manage the temporary local shell window without affecting its agent session.
    fn open_shell_window(&mut self, agent_id: &AgentId) -> Result<(), RuntimeError>;
    fn select_shell_window(&mut self, agent_id: &AgentId) -> Result<(), RuntimeError>;
    fn close_shell_window(&mut self, agent_id: &AgentId) -> Result<(), RuntimeError>;
    fn shell_window_exists(&self, agent_id: &AgentId) -> Result<bool, RuntimeError>;
    fn hide_shell_window(&mut self, agent_id: &AgentId) -> Result<(), RuntimeError>;
    fn observe_shell_window_sessions(&self) -> Result<Vec<String>, RuntimeError>;
    fn close_all_shell_windows(&mut self) -> Vec<RuntimeError>;
}
/// Real tmux-based runtime manager.
///
/// @plan PLAN-20260216-FIRSTVERSION-V1.P08
/// @requirement REQ-TECH-004
/// @requirement REQ-FUNC-007
pub struct TmuxRuntimeManager {
    /// Active sessions by agent ID.
    pub(crate) sessions: HashMap<AgentId, RuntimeSession>,
    /// Currently attached viewer (single viewer model).
    pub(crate) viewer: Option<AttachedViewer>,
    /// Agent ID of the currently attached session.
    pub(crate) attached_agent_id: Option<AgentId>,
    /// Dead sessions that can be relaunched (stores signatures).
    ///
    /// Bounded by [`MAX_DEAD_SIGNATURES`]: once full, the least-recently-used
    /// dead signature is evicted to make room for newer ones.
    dead_plans: LruCache<AgentId, RetainedLaunch>,
    /// Session names for which clipboard passthrough has already been enforced.
    ///
    /// Avoids re-running the tmux option commands on every attach. Populated
    /// during local session creation and the local attach path.
    pub(super) clipboard_enforced: HashSet<String>,
    /// Session names for which tmux prefix passthrough has already been
    /// enforced.
    ///
    /// Mirrors [`clipboard_enforced`](Self::clipboard_enforced): the prefix
    /// options are idempotent, but we memoize so the reattach/attach hot paths
    /// do not re-shell out to tmux for a session already remediated (#200).
    pub(super) prefix_enforced: HashSet<String>,
    /// Terminal dimensions.
    pub(crate) rows: u16,
    pub(crate) cols: u16,
    /// Monotonically increasing PTY-output generation counter (issue #198).
    /// Incremented by `take_dirty()`. The history cache compares the stored
    /// generation to decide re-capture.
    output_generation: AtomicU64,
    /// Cached scrollback history (issue #198).
    pub(crate) history_cache: HistoryCache,
    /// Global lifecycle generation counter. Incremented on every
    /// spawn/relaunch so each `RuntimeSession` gets a unique generation
    /// for stale-liveness rejection (issue #301 Phase 4).
    lifecycle_counter: AtomicU64,
    /// Explicit session-host root owning per-session staged Windows host
    /// images (issue #467). `None` for the legacy constructor and for every
    /// Unix/remote runtime, where staging is structurally disabled. Production
    /// supplies the resolved state-file parent joined with `session-hosts`;
    /// the manager never mutates process environment to derive it.
    session_host_root: Option<PathBuf>,
    /// Focused local JSP launch lifecycle authority, installed by production.
    jsp_launches: Option<JspLaunchCoordinator>,
}

/// Drop the current viewer (if any) on a background OS thread.
///
/// `AttachedViewer::drop` performs deterministic child teardown — killing the
/// tmux child and waiting up to 300ms for it to exit. Running that inline
/// blocks the caller. Dropping on a detached thread keeps the executor
/// responsive while still guaranteeing eventual cleanup.
fn drop_viewer_in_background(viewer: &mut Option<AttachedViewer>) {
    if let Some(old_viewer) = viewer.take() {
        std::thread::spawn(move || drop(old_viewer));
    }
}

/// Public wrapper so sibling modules (e.g. `async_attach`) can reuse the
/// same background-drop logic.
pub fn drop_viewer_in_background_pub(viewer: &mut Option<AttachedViewer>) {
    drop_viewer_in_background(viewer);
}

impl TmuxRuntimeManager {
    /// Create a new tmux runtime manager.
    #[must_use]
    pub fn new(rows: u16, cols: u16) -> Self {
        Self::build(rows, cols, None)
    }

    /// Create a tmux runtime manager that owns an explicit session-host root.
    ///
    /// Issue #467: production supplies the resolved state-file parent joined
    /// with `session-hosts` so Windows local creation can stage an immutable
    /// per-session copy of the running Jefe image below a single path
    /// authority. Unix callers and existing tests continue to use
    /// [`TmuxRuntimeManager::new`]; this constructor does not mutate process
    /// environment.
    #[must_use]
    pub fn with_session_host_root(rows: u16, cols: u16, session_host_root: PathBuf) -> Self {
        Self::build(rows, cols, Some(session_host_root))
    }

    /// Return the explicit session-host root this manager owns, if any.
    ///
    /// `None` for the legacy constructor and on platforms that never stage a
    /// host image. The kill path uses this to derive the per-session directory
    /// (AC7) and the local launch path uses it to stage (AC1).
    #[must_use]
    pub fn session_host_root(&self) -> Option<&Path> {
        self.session_host_root.as_deref()
    }

    fn build(rows: u16, cols: u16, session_host_root: Option<PathBuf>) -> Self {
        Self {
            sessions: HashMap::new(),
            viewer: None,
            attached_agent_id: None,
            dead_plans: LruCache::new(MAX_DEAD_SIGNATURES),
            clipboard_enforced: HashSet::new(),
            prefix_enforced: HashSet::new(),
            rows,
            cols,
            output_generation: AtomicU64::new(0),
            history_cache: HistoryCache::default(),
            lifecycle_counter: AtomicU64::new(0),
            session_host_root,
            jsp_launches: None,
        }
    }

    /// Install the production JSP lifecycle authority after the host is ready.
    pub fn install_jsp_launches(&mut self, coordinator: JspLaunchCoordinator) {
        self.jsp_launches = Some(coordinator);
    }

    /// Update terminal dimensions.
    pub fn set_size(&mut self, rows: u16, cols: u16) {
        self.rows = rows;
        self.cols = cols;
    }

    /// Allocate the next lifecycle generation (issue #301 Phase 4).
    ///
    /// Uses `Relaxed` ordering: all reads and writes of
    /// `lifecycle_counter` and individual session `lifecycle_generation`
    /// fields occur while holding the `TmuxRuntimeManager` `&mut self`
    /// borrow (i.e., under the `AppContext` mutex). The atomic is used
    /// only to obtain a monotonically increasing counter without a
    /// `Cell`; the mutex provides the happens-before guarantees.
    #[must_use]
    fn next_lifecycle_generation(&self) -> u64 {
        self.lifecycle_counter.fetch_add(1, Ordering::Relaxed) + 1
    }

    pub fn mark_session_dead(&mut self, agent_id: &AgentId) -> bool {
        jsp_launch::revoke(self.jsp_launches.as_ref(), agent_id);
        let Some(session) = self.sessions.remove(agent_id) else {
            return false;
        };

        // Bump lifecycle generation before removing so any in-flight liveness
        // observation for this agent is rejected as stale (issue #301 Phase 4).
        // The session is being removed, but the generation bump is recorded
        // so that if a new session is later created for the same agent, its
        // generation will be higher than any pending observation.
        let _ = self.next_lifecycle_generation();

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

        let _ = self.dead_plans.put(agent_id.clone(), RetainedLaunch);
        true
    }

    /// Bump the lifecycle generation for an agent's session (issue #301
    /// Phase 4).
    ///
    /// Called on kill/relaunch/rebind paths so stale liveness observations
    /// from the prior binding are rejected. Returns the new generation, or
    /// `None` if the agent has no tracked session.
    #[must_use]
    pub fn bump_lifecycle_generation(&mut self, agent_id: &AgentId) -> Option<u64> {
        let new_gen = self.next_lifecycle_generation();
        let session = self.sessions.get_mut(agent_id)?;
        session.lifecycle_generation = new_gen;
        Some(session.lifecycle_generation)
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

    /// Return the stable worker process identity for restart reconciliation.
    #[must_use]
    pub fn worker_process_identity(
        &self,
        agent_id: &AgentId,
    ) -> Option<crate::domain::ProcessIdentity> {
        self.sessions
            .get(agent_id)
            .and_then(|session| session.process_identity)
    }

    fn kill_before_fresh_spawn(
        allow_reattach: bool,
        remote: Option<&RemoteRepositorySettings>,
        session_name: &str,
    ) {
        if allow_reattach {
            return;
        }
        let result = if let Some(remote) = remote {
            commands::kill_remote_session(remote, session_name)
        } else {
            commands::kill_session(session_name)
        };
        if let Err(error) = result {
            debug!(
                session_name,
                error = %error,
                "force-fresh spawn pre-kill was not clean"
            );
        }
    }

    /// Probe whether a session is alive using the optional remote transport.
    fn create_or_reattach_after_probe(
        agent_id: &AgentId,
        plan: &AgentLaunchPlan,
        remote: Option<&RemoteRepositorySettings>,
        allow_reattach: bool,
        session_name: &str,
        session_host_root: Option<&Path>,
    ) -> Result<bool, RuntimeError> {
        if allow_reattach
            && Self::session_alive_for_remote(agent_id, remote) == liveness::SessionLiveness::Alive
        {
            return Ok(true);
        }
        Self::kill_before_fresh_spawn(allow_reattach, remote, session_name);
        debug!(session_name, "creating new tmux session");
        // Reattach is handled above. Only the create path receives the
        // session-host root, so reattach never stages or replaces a host.
        match commands::create_session(session_name, plan, remote, session_host_root) {
            Ok(()) => Ok(false),
            Err(_error)
                if allow_reattach
                    && Self::session_alive_for_remote(agent_id, remote)
                        == liveness::SessionLiveness::Alive =>
            {
                Ok(true)
            }
            Err(error) => Err(error),
        }
    }

    fn spawn_session_internal(
        &mut self,
        agent_id: &AgentId,
        plan: &AgentLaunchPlan,
        remote: Option<&RemoteRepositorySettings>,
        allow_reattach: bool,
        lifecycle_generation: u64,
    ) -> Result<bool, RuntimeError> {
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
        let can_reattach = allow_reattach
            && Self::session_alive_for_remote(agent_id, remote) == liveness::SessionLiveness::Alive;
        let reattached = if can_reattach {
            true
        } else {
            Self::create_or_reattach_after_probe(
                agent_id,
                plan,
                remote,
                allow_reattach,
                &session_name,
                self.session_host_root.as_deref(),
            )?
        };
        if reattached {
            debug!(session_name = %session_name, "reattaching to existing tmux session");
            if let Some(remote) = remote {
                self.ensure_remote_prefix_passthrough(remote, &session_name);
            } else {
                self.ensure_prefix_passthrough(&session_name);
            }
        } else if remote.is_none() {
            self.ensure_clipboard_passthrough(&session_name);
            self.ensure_prefix_passthrough(&session_name);
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
        let captured_pid = if remote.is_some() {
            None
        } else {
            commands::pane_pid(&session_name)
        };

        // Store/refresh session binding. Bump the lifecycle generation so
        // stale liveness results from a prior binding are rejected (issue
        // #301 Phase 4).
        let mut session = RuntimeSession::new(
            agent_id.clone(),
            session_name,
            plan.clone(),
            remote.cloned(),
        );
        session.pid = captured_pid;
        session.process_identity =
            captured_pid.and_then(|pid| super::process::capture_process_identity(pid).ok());
        // Best-effort launch-tree enumeration so a dead-launcher orphan can be
        // reaped PID-reuse-safely later (issue #332).
        session.worker_identities = super::orphan::capture_worker_identities(captured_pid);
        session.lifecycle_generation = lifecycle_generation;
        self.sessions.insert(agent_id.clone(), session);

        // Remove from dead plans if present.
        let _ = self.dead_plans.pop(agent_id);

        Ok(reattached)
    }
}

impl RuntimeManager for TmuxRuntimeManager {
    fn spawn_session(
        &mut self,
        agent_id: &AgentId,
        launch: &AuthorizedLaunchPlan,
        remote: Option<&RemoteRepositorySettings>,
    ) -> Result<(), RuntimeError> {
        if self.sessions.contains_key(agent_id) {
            return Err(RuntimeError::AlreadyRunning(agent_id.clone()));
        }
        // Preflight the unmodified plan first so an unspawnable agent fails
        // before any credential material is written. The augmented plan is
        // preflighted again below because JSP instrumentation changes it.
        launch
            .prepare_current(&ProcessSandboxInspector::new())
            .map_err(|error| RuntimeError::SpawnFailed(error.to_string()))?;
        let generation = self.next_lifecycle_generation();
        let reattaching =
            Self::session_alive_for_remote(agent_id, remote) == liveness::SessionLiveness::Alive;
        let (launch, jsp_launch) = jsp_launch::prepare(
            self.jsp_launches.as_ref(),
            agent_id,
            launch,
            remote,
            reattaching,
            generation,
        )?;
        let cleared = launch
            .prepare_current(&ProcessSandboxInspector::new())
            .map_err(|error| RuntimeError::SpawnFailed(error.to_string()))?;
        let plan = cleared.plan();
        info!(agent_id = %agent_id.0, work_dir = %plan.cwd.display(), "spawning runtime session");
        let reattached = self.spawn_session_internal(agent_id, plan, remote, true, generation)?;
        if !reattached && let Some(jsp_launch) = jsp_launch {
            jsp_launch.commit();
        }
        Ok(())
    }

    fn spawn_session_fresh(
        &mut self,
        agent_id: &AgentId,
        launch: &AuthorizedLaunchPlan,
        remote: Option<&RemoteRepositorySettings>,
    ) -> Result<(), RuntimeError> {
        if self.sessions.contains_key(agent_id) {
            return Err(RuntimeError::AlreadyRunning(agent_id.clone()));
        }
        launch
            .prepare_current(&ProcessSandboxInspector::new())
            .map_err(|error| RuntimeError::SpawnFailed(error.to_string()))?;
        let generation = self.next_lifecycle_generation();
        let (launch, jsp_launch) = jsp_launch::prepare(
            self.jsp_launches.as_ref(),
            agent_id,
            launch,
            remote,
            false,
            generation,
        )?;
        let cleared = launch
            .prepare_current(&ProcessSandboxInspector::new())
            .map_err(|error| RuntimeError::SpawnFailed(error.to_string()))?;
        let plan = cleared.plan();
        info!(
            agent_id = %agent_id.0,
            work_dir = %plan.cwd.display(),
            "spawning fresh runtime session"
        );
        let reattached = self.spawn_session_internal(agent_id, plan, remote, false, generation)?;
        if !reattached && let Some(jsp_launch) = jsp_launch {
            jsp_launch.commit();
        }
        Ok(())
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
            let remote_settings = session.remote.clone();

            // Enforce clipboard passthrough (memoized) before spawning the
            // local viewer — the attach hot path no longer relies on
            // AttachedViewer::spawn to do this.
            if remote_settings.is_none() {
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
                let ssh_plan = commands::build_remote_attach_plan(&remote, &session_name)?;
                AttachedViewer::spawn_remote(&session_name, self.rows, self.cols, &ssh_plan)?
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

        // Issue #296: nudge the child to re-advertise its DEC private mouse modes.
        self.post_attach_mode_recovery(agent_id);

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
        jsp_launch::revoke(self.jsp_launches.as_ref(), agent_id);
        let session = self
            .sessions
            .remove(agent_id)
            .ok_or_else(|| RuntimeError::SessionNotFound(agent_id.0.clone()))?;

        // Mark the killed agent as eligible for relaunch. The relaunch
        // authority itself is supplied later through the authorized-preparation
        // contract, so only a dead marker is retained here.
        let _ = self.dead_plans.put(agent_id.clone(), RetainedLaunch);

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
        if let Some(remote) = session.remote.as_ref() {
            commands::kill_remote_session(remote, &session.session_name)?;
        } else {
            commands::kill_session(&session.session_name)?;
            // AC7: a successful local kill removes only this session's host
            // directory. Cleanup failures are best-effort and retained for
            // retry; they never abort the kill because the psmux/tmux session
            // is already gone. Remote kill does not own a local host image.
            if let Some(root) = self.session_host_root.as_deref() {
                match super::session_host::cleanup_session_directory(root, &session.session_name) {
                    Ok(super::session_host::SessionCleanupOutcome::Removed) => {
                        debug!(
                            session_name = %session.session_name,
                            "removed session-host directory after local kill"
                        );
                    }
                    Ok(outcome) => debug!(
                        session_name = %session.session_name,
                        ?outcome,
                        "session-host directory cleanup skipped after local kill"
                    ),
                    Err(error) => debug!(
                        session_name = %session.session_name,
                        error = %error,
                        "session-host directory cleanup rejected after local kill; retained for retry"
                    ),
                }
            }
        }

        // Bump lifecycle generation only after a successful session removal,
        // so stale liveness observations from the killed session are rejected
        // (issue #301 Phase 4 review: make the invariant explicit).
        let _ = self.next_lifecycle_generation();

        Ok(())
    }

    fn relaunch(
        &mut self,
        agent_id: &AgentId,
        launch: &AuthorizedLaunchPlan,
        remote: Option<&RemoteRepositorySettings>,
    ) -> Result<(), RuntimeError> {
        info!(agent_id = %agent_id.0, "relaunching runtime session");
        if self.sessions.contains_key(agent_id) {
            return Err(RuntimeError::AlreadyRunning(agent_id.clone()));
        }
        if self.dead_plans.peek(agent_id).is_none() {
            return Err(RuntimeError::NotRunning(agent_id.clone()));
        }
        let result = self.spawn_session_fresh(agent_id, launch, remote);
        complete_relaunch_attempt(&mut self.dead_plans, agent_id, result)
    }

    fn is_alive(&self, agent_id: &AgentId) -> bool {
        if let Some(session) = self.sessions.get(agent_id) {
            if let Some(remote) = session.remote.as_ref() {
                liveness::check_remote_session_alive(remote, &session.session_name)
            } else {
                liveness::check_session_alive(&session.session_name)
            }
        } else {
            false
        }
    }

    fn session_exists(&self, agent_id: &AgentId) -> bool {
        if let Some(session) = self.sessions.get(agent_id)
            && let Some(remote) = session.remote.as_ref()
        {
            return commands::remote_session_exists(remote, &session.session_name).unwrap_or(false);
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
        super::capture_ops::capture_session_output(self, agent_id)
    }

    fn capture_history(&mut self) -> Option<Vec<String>> {
        super::capture_ops::capture_history(self)
    }

    fn open_shell_window(&mut self, agent_id: &AgentId) -> Result<(), RuntimeError> {
        super::shell_window::open_manager_shell_window(&self.sessions, agent_id)
    }
    fn select_shell_window(&mut self, agent_id: &AgentId) -> Result<(), RuntimeError> {
        super::shell_window::select_manager_shell_window(&self.sessions, agent_id)
    }
    fn close_shell_window(&mut self, agent_id: &AgentId) -> Result<(), RuntimeError> {
        super::shell_window::close_manager_shell_window(&self.sessions, agent_id)
    }
    fn shell_window_exists(&self, agent_id: &AgentId) -> Result<bool, RuntimeError> {
        super::shell_window::manager_shell_window_exists(&self.sessions, agent_id)
    }
    fn hide_shell_window(&mut self, agent_id: &AgentId) -> Result<(), RuntimeError> {
        super::shell_window::hide_manager_shell_window(&self.sessions, agent_id)
    }
    fn observe_shell_window_sessions(&self) -> Result<Vec<String>, RuntimeError> {
        super::shell_window::observe_shell_window_sessions()
    }
    fn close_all_shell_windows(&mut self) -> Vec<RuntimeError> {
        super::shell_window::close_all_manager_shell_windows()
    }
}
#[cfg(test)]
#[path = "history_tests.rs"]
mod history_tests;
#[cfg(test)]
#[path = "manager_tests.rs"]
mod tests;

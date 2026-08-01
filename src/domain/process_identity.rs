//! Process identities, separated by the role the process plays (issue #543).
//!
//! Three distinct operating-system processes are tracked by the runtime, and
//! before issue #543 all three were carried by the same untyped
//! [`ProcessIdentity`], so nothing stopped one being used where another was
//! meant:
//!
//! - the **pane leader**, whatever the multiplexer started as the pane's direct
//!   command,
//! - the **agent worker**, the process actually running the coding agent,
//! - the **multiplexer server** itself.
//!
//! On Unix the pane leader *is* the worker, because the pane's direct command is
//! the agent. On Windows since issue #467 it is not: the pane leader is a
//! PowerShell process which starts `jefe-session-host.exe`, which in turn spawns
//! the worker. The pane leader is then an ancestor two hops above the worker,
//! and a pane-scoped fact says nothing about the worker.
//!
//! Each role therefore gets its own type. They share a representation but are
//! not substitutable: crossing roles requires a named conversion, so every place
//! that does so is greppable and has to justify itself.

use serde::{Deserialize, Serialize};

/// Stable identity of one operating-system process instance.
///
/// A PID alone cannot identify a process, because the operating system recycles
/// PIDs. Pairing it with a creation discriminator makes the identity stable: a
/// recycled PID carries a different `started_at` and therefore does not match.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProcessIdentity {
    pub pid: u32,
    /// Platform process creation discriminator. Windows stores creation
    /// FILETIME, Linux stores `/proc` start ticks, and macOS stores UTC epoch
    /// seconds. `None` supports legacy and unavailable platform evidence.
    #[serde(default)]
    pub started_at: Option<u64>,
}

impl ProcessIdentity {
    #[must_use]
    pub const fn new(pid: u32, started_at: u64) -> Self {
        Self {
            pid,
            started_at: Some(started_at),
        }
    }
}

/// Declare one role-specific process identity.
///
/// Every role shares the same representation and accessor surface; only the type
/// differs. The macro keeps the three definitions from drifting apart while
/// still producing genuinely distinct types, which is what makes a role mix-up a
/// compile error rather than a silent runtime fault.
macro_rules! role_identity {
    ($(#[$meta:meta])* $name:ident, $role:literal) => {
        $(#[$meta])*
        ///
        /// Wraps a [`ProcessIdentity`] and is not interchangeable with the
        /// identity of any other role. Serializes transparently, so the on-disk
        /// representation is unchanged.
        #[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
        #[serde(transparent)]
        pub struct $name(ProcessIdentity);

        impl $name {
            #[doc = concat!("Record the ", $role, " from a PID and creation discriminator.")]
            #[must_use]
            pub const fn new(pid: u32, started_at: u64) -> Self {
                Self(ProcessIdentity::new(pid, started_at))
            }

            #[doc = concat!("Label an already-observed [`ProcessIdentity`] as the ", $role, ".")]
            ///
            /// This is the only way to move an identity into this role, so every
            /// role assignment is explicit at the call site.
            #[must_use]
            pub const fn from_identity(identity: ProcessIdentity) -> Self {
                Self(identity)
            }

            /// The underlying role-agnostic identity.
            ///
            /// Needed by the probing services, which compare identities without
            /// caring about roles. Deliberately named rather than a `Deref` or
            /// `From`, so it cannot be applied implicitly.
            #[must_use]
            pub const fn identity(self) -> ProcessIdentity {
                self.0
            }

            /// The operating-system PID.
            #[must_use]
            pub const fn pid(self) -> u32 {
                self.0.pid
            }

            /// The creation discriminator, when the platform supplied one.
            #[must_use]
            pub const fn started_at(self) -> Option<u64> {
                self.0.started_at
            }
        }
    };
}

role_identity!(
    /// Identity of the process the multiplexer started as the pane's direct
    /// command.
    ///
    /// This is what `#{pane_pid}` reports. It is the agent worker only on
    /// platforms whose topology says so — see [`PaneWorkerTopology`].
    PaneProcessIdentity,
    "pane leader"
);

role_identity!(
    /// Identity of the process actually running the coding agent.
    ///
    /// This is the identity that agent liveness, orphan reaping and restart
    /// reconciliation are about.
    WorkerProcessIdentity,
    "agent worker"
);

role_identity!(
    /// Identity of the multiplexer server process (`tmux`/`psmux`).
    ///
    /// Distinct from both the pane leader and the worker: the server outlives
    /// individual panes, and its replacement is what server-health detection
    /// watches for.
    ServerProcessIdentity,
    "multiplexer server"
);

/// How the pane leader relates to the agent worker on a platform.
///
/// This makes the platform difference an explicit, testable value instead of an
/// assumption buried in a comment. Before issue #543 the Unix relationship was
/// hard-coded into the capture path, which silently produced pane identities
/// labelled as worker identities on Windows.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PaneWorkerTopology {
    /// The pane's direct command *is* the agent worker, so the pane identity
    /// determines the worker identity. Unix and macOS.
    PaneIsWorker,
    /// The worker is a descendant of the pane leader, so the pane identity says
    /// nothing about the worker and the worker identity must be obtained
    /// separately. Windows, since issue #467.
    WorkerBelowPane,
}

impl PaneWorkerTopology {
    /// The topology of the platform this build targets.
    #[must_use]
    pub const fn current() -> Self {
        if cfg!(windows) {
            Self::WorkerBelowPane
        } else {
            Self::PaneIsWorker
        }
    }

    /// Whether the pane identity alone determines the worker identity.
    #[must_use]
    pub const fn pane_determines_worker(self) -> bool {
        matches!(self, Self::PaneIsWorker)
    }
}

/// Derive the worker identity from the pane identity, where the topology allows.
///
/// Returns `None` under [`PaneWorkerTopology::WorkerBelowPane`]: the pane leader
/// is merely an ancestor there, so the worker identity has to come from the
/// session host that spawned it. Returning `None` rather than the pane identity
/// is the point — it forces callers to handle "the worker is not known yet"
/// instead of silently recording an ancestor as the worker.
///
/// ```
/// use jefe::domain::{PaneProcessIdentity, PaneWorkerTopology, worker_identity_from_pane};
///
/// let pane = PaneProcessIdentity::new(42, 900);
///
/// let unix = worker_identity_from_pane(PaneWorkerTopology::PaneIsWorker, pane);
/// assert_eq!(unix.map(|worker| worker.pid()), Some(42));
///
/// let windows = worker_identity_from_pane(PaneWorkerTopology::WorkerBelowPane, pane);
/// assert!(windows.is_none());
/// ```
///
/// A pane identity cannot be passed where a worker identity is required. The
/// error code is pinned to `E0308` (mismatched types) so this stays a proof that
/// the *roles* are incompatible, rather than passing on any unrelated
/// compilation failure:
///
/// ```compile_fail,E0308
/// use jefe::domain::{PaneProcessIdentity, WorkerProcessIdentity};
///
/// fn reap(_worker: WorkerProcessIdentity) {}
///
/// reap(PaneProcessIdentity::new(42, 900));
/// ```
///
/// nor a worker identity where a server identity is required:
///
/// ```compile_fail,E0308
/// use jefe::domain::{ServerProcessIdentity, WorkerProcessIdentity};
///
/// fn server_replaced(_server: ServerProcessIdentity) {}
///
/// server_replaced(WorkerProcessIdentity::new(42, 900));
/// ```
#[must_use]
pub const fn worker_identity_from_pane(
    topology: PaneWorkerTopology,
    pane: PaneProcessIdentity,
) -> Option<WorkerProcessIdentity> {
    match topology {
        PaneWorkerTopology::PaneIsWorker => {
            Some(WorkerProcessIdentity::from_identity(pane.identity()))
        }
        PaneWorkerTopology::WorkerBelowPane => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// V7: the Unix relationship is a stated equality, not an absent
    /// distinction. The pane identity yields exactly the same PID and creation
    /// token in the worker role.
    #[test]
    fn unix_pane_identity_equals_worker_identity() {
        let pane = PaneProcessIdentity::new(4242, 900_900);

        let Some(worker) = worker_identity_from_pane(PaneWorkerTopology::PaneIsWorker, pane) else {
            panic!("the pane process is the worker on this topology");
        };

        assert_eq!(worker.pid(), pane.pid());
        assert_eq!(worker.started_at(), pane.started_at());
        assert_eq!(worker.identity(), pane.identity());
    }

    /// The Windows topology must refuse to invent a worker identity. Returning
    /// the pane identity here is precisely the issue #543 defect.
    #[test]
    fn windows_pane_identity_does_not_yield_a_worker_identity() {
        let pane = PaneProcessIdentity::new(4242, 900_900);

        assert!(
            worker_identity_from_pane(PaneWorkerTopology::WorkerBelowPane, pane).is_none(),
            "the pane leader is an ancestor of the worker, not the worker"
        );
    }

    #[test]
    fn topology_reports_whether_the_pane_determines_the_worker() {
        assert!(PaneWorkerTopology::PaneIsWorker.pane_determines_worker());
        assert!(!PaneWorkerTopology::WorkerBelowPane.pane_determines_worker());
    }

    /// The build targets the topology of the platform it runs on, so the
    /// capture path cannot pick up the wrong one.
    #[test]
    fn current_topology_matches_the_target_platform() {
        let expected = if cfg!(windows) {
            PaneWorkerTopology::WorkerBelowPane
        } else {
            PaneWorkerTopology::PaneIsWorker
        };

        assert_eq!(PaneWorkerTopology::current(), expected);
    }

    /// Roles must not change the wire format: the durable document and the
    /// legacy schema-1 backups both carry a bare `{pid, started_at}` object.
    #[test]
    fn role_identities_serialize_transparently() {
        let identity = ProcessIdentity::new(7, 11);
        let bare = serde_json::to_string(&identity)
            .unwrap_or_else(|error| panic!("identity serializes: {error}"));

        for rendered in [
            serde_json::to_string(&PaneProcessIdentity::from_identity(identity)),
            serde_json::to_string(&WorkerProcessIdentity::from_identity(identity)),
            serde_json::to_string(&ServerProcessIdentity::from_identity(identity)),
        ] {
            assert_eq!(
                rendered.unwrap_or_else(|error| panic!("role identity serializes: {error}")),
                bare,
                "a role wrapper must not change the persisted representation"
            );
        }
    }

    #[test]
    fn role_identities_round_trip_through_serde() {
        let worker = WorkerProcessIdentity::new(4242, 900_900);
        let encoded = serde_json::to_string(&worker)
            .unwrap_or_else(|error| panic!("worker serializes: {error}"));

        let decoded: WorkerProcessIdentity = serde_json::from_str(&encoded)
            .unwrap_or_else(|error| panic!("worker deserializes: {error}"));

        assert_eq!(decoded, worker);
    }

    /// Legacy evidence without a creation discriminator must still load; the
    /// PID-reuse guard treats a missing token as unverifiable rather than
    /// matching.
    #[test]
    fn role_identities_accept_evidence_without_a_creation_token() {
        let decoded: PaneProcessIdentity = serde_json::from_str(r#"{"pid":7}"#)
            .unwrap_or_else(|error| panic!("legacy identity deserializes: {error}"));

        assert_eq!(decoded.pid(), 7);
        assert_eq!(decoded.started_at(), None);
    }
}

//! Owner-lifetime anchor for the Windows session process tree (issue #542).
//!
//! Specification: `dev-docs/standards/windows-session-ownership.md`.
//!
//! The session host is the anchor holder: it owns the `KILL_ON_JOB_CLOSE` Job
//! that contains the worker, so closing that handle reaps the tree. Nothing,
//! however, contained the *host* — which is why `pwsh -> jefe-session-host.exe
//! -> bun.exe` trees survived their owning psmux process (#515).
//!
//! This module supplies the missing link. Before the worker is spawned, the
//! host captures the `ProcessIdentity` of its owner chain (pane process, then
//! psmux server), and a watchdog releases the tree the moment any captured link
//! is *confirmed* dead or replaced. Capture is capped at two levels so the Jefe
//! dashboard is never an anchor; that cap is what preserves #467's guarantee
//! that a dashboard quit or a mid-run rebuild leaves live agents alone.
//!
//! Every decision here is pure and platform-independent so it is exercised on
//! all targets. Only [`capture_owner_anchor`] touches the operating system.

use std::time::Duration;

use crate::domain::ProcessIdentity;

use super::process::ProcessLiveness;

/// Exit status used when the host releases its tree because ownership was lost.
///
/// Distinct from success and from a generic failure so a native test or a CI
/// log can tell "reaped by ownership loss" apart from "the agent exited" and
/// "the host crashed".
pub const OWNER_LOST_EXIT_CODE: i32 = 75;

/// Interval between owner-chain observations.
///
/// Bounds how long a tree can outlive its owner. Each pass is at most two
/// `OpenProcess`/`GetProcessTimes` probes, so a one-second period is
/// negligible against the lifetime of an agent session.
pub const OWNER_WATCH_INTERVAL: Duration = Duration::from_secs(1);

/// Levels of ancestry captured above the session host: the pane process and
/// the psmux server. Deliberately not three — see the specification, §4.
const OWNER_CHAIN_DEPTH: usize = 2;

/// A process in the session host's owner chain.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OwnerRole {
    /// L1: the session host's direct parent, the psmux pane process.
    PaneProcess,
    /// L2: the session host's grandparent, the psmux server that owns the pane.
    SessionServer,
}

impl OwnerRole {
    /// Human-readable role name for diagnostics.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::PaneProcess => "pane process",
            Self::SessionServer => "psmux server",
        }
    }

    const fn at_depth(depth: usize) -> Self {
        if depth == 0 {
            Self::PaneProcess
        } else {
            Self::SessionServer
        }
    }
}

impl std::fmt::Display for OwnerRole {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(self.as_str())
    }
}

/// One captured link of the owner chain: a role plus the exact process
/// identity that held it at capture time.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct OwnerLink {
    /// Which level of the chain this link occupies.
    pub role: OwnerRole,
    /// PID plus creation time. A PID alone is spoofable by reuse.
    pub identity: ProcessIdentity,
}

/// The session host's complete owner chain, captured before the worker spawn.
///
/// Non-empty by construction: an empty chain is the absence of ownership, not a
/// degraded form of it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OwnerAnchor {
    links: Vec<OwnerLink>,
}

impl OwnerAnchor {
    /// Build an anchor from a captured chain, rejecting an empty one.
    ///
    /// # Errors
    ///
    /// Returns [`OwnerAnchorError::NoAncestor`] when `links` is empty.
    pub fn from_links(links: Vec<OwnerLink>) -> Result<Self, OwnerAnchorError> {
        if links.is_empty() {
            return Err(OwnerAnchorError::NoAncestor);
        }
        Ok(Self { links })
    }

    /// The captured chain, nearest ancestor first.
    #[must_use]
    pub fn links(&self) -> &[OwnerLink] {
        &self.links
    }
}

impl std::fmt::Display for OwnerAnchor {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        for (index, link) in self.links.iter().enumerate() {
            if index > 0 {
                formatter.write_str(", ")?;
            }
            write!(formatter, "{} pid {}", link.role, link.identity.pid)?;
        }
        Ok(())
    }
}

/// Why an owner chain could not be established.
///
/// Every variant is fatal to the launch: rule 6 of the specification forbids
/// spawning a worker that nothing owns.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OwnerAnchorError {
    /// No parent process could be resolved for the session host.
    NoAncestor,
    /// A parent PID was resolved, but its identity could not be captured or
    /// could not be distinguished from a recycled PID.
    AncestorUnobservable,
    /// The session host could not observe its own identity, so the
    /// ancestor-ordering guard cannot be applied.
    SelfIdentityUnavailable,
}

impl std::fmt::Display for OwnerAnchorError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NoAncestor => formatter.write_str(
                "session host has no resolvable owning process; refusing to spawn an unowned worker",
            ),
            Self::AncestorUnobservable => formatter.write_str(
                "session host owner identity could not be captured; refusing to spawn an unowned worker",
            ),
            Self::SelfIdentityUnavailable => formatter
                .write_str("session host could not observe its own process identity"),
        }
    }
}

impl std::error::Error for OwnerAnchorError {}

/// What one observation of one link says about ownership.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OwnerStatus {
    /// The exact captured process is still running: ownership holds.
    Held,
    /// The captured process is confirmed gone or replaced: ownership is broken.
    Lost,
    /// The probe produced no usable evidence either way.
    Unverified,
}

/// The action the session host must take after observing its whole chain.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OwnerWatchDecision {
    /// Ownership still holds; change nothing.
    Hold,
    /// Ownership is broken at the named link; the host must exit so its Job
    /// handle closes and the kernel reaps the contained worker tree.
    ReleaseTree(OwnerRole),
}

/// Map a process-liveness observation onto ownership.
///
/// `Dead` and `ReusedPid` are the two ways a captured owner stops being the
/// owner — a recycled PID resolves to a live process, so PID-only checks report
/// "alive" forever. Everything else is an absence of evidence and must fail
/// open: termination is irreversible, a failed probe is not.
#[must_use]
pub const fn classify_owner_link(liveness: ProcessLiveness) -> OwnerStatus {
    match liveness {
        ProcessLiveness::Alive => OwnerStatus::Held,
        ProcessLiveness::Dead | ProcessLiveness::ReusedPid => OwnerStatus::Lost,
        ProcessLiveness::Inaccessible
        | ProcessLiveness::MalformedIdentity
        | ProcessLiveness::ProbeFailure => OwnerStatus::Unverified,
    }
}

/// Fold one pass over the owner chain into a decision.
///
/// A confirmed loss at *any* link releases the tree: every process above the
/// session host must outlive it, so a break anywhere is an ownership violation
/// whatever the termination order.
#[must_use]
pub fn decide_owner_watch(
    statuses: impl IntoIterator<Item = (OwnerRole, OwnerStatus)>,
) -> OwnerWatchDecision {
    for (role, status) in statuses {
        if status == OwnerStatus::Lost {
            return OwnerWatchDecision::ReleaseTree(role);
        }
    }
    OwnerWatchDecision::Hold
}

/// Reject a candidate ancestor that cannot have preceded its descendant.
///
/// A recycled PID sitting in the parent slot is the one case a snapshot cannot
/// detect structurally, and creation-time ordering rules it out for free. An
/// identity without a creation time can never be compared, so it can never be
/// distinguished from an impostor and is not usable as an anchor.
#[must_use]
pub const fn is_plausible_ancestor(
    descendant: ProcessIdentity,
    candidate: ProcessIdentity,
) -> bool {
    match (descendant.started_at, candidate.started_at) {
        (Some(descendant), Some(candidate)) => candidate <= descendant,
        _ => false,
    }
}

/// Observe the owner chain until ownership breaks or the caller stops ticking.
///
/// `observe` reports one link; `tick` waits for the next pass and returns
/// `false` to end the watch. In production `tick` sleeps and always returns
/// `true`, so the loop ends only by releasing the tree; the injection seam
/// exists so the policy is testable without real processes or real time.
pub fn watch_owner_anchor<Observe, Tick>(
    anchor: &OwnerAnchor,
    mut observe: Observe,
    mut tick: Tick,
) -> OwnerWatchDecision
where
    Observe: FnMut(OwnerLink) -> OwnerStatus,
    Tick: FnMut() -> bool,
{
    loop {
        let statuses: Vec<(OwnerRole, OwnerStatus)> = anchor
            .links()
            .iter()
            .map(|link| (link.role, observe(*link)))
            .collect();
        if let OwnerWatchDecision::ReleaseTree(role) = decide_owner_watch(statuses) {
            return OwnerWatchDecision::ReleaseTree(role);
        }
        if !tick() {
            return OwnerWatchDecision::Hold;
        }
    }
}

/// Observe one captured link against the live operating system.
#[must_use]
pub fn observe_owner_link(link: OwnerLink) -> OwnerStatus {
    classify_owner_link(super::process::process_liveness(Some(link.identity)))
}

/// Capture the session host's owner chain (Windows only).
///
/// Walks up to [`OWNER_CHAIN_DEPTH`] ancestors, capturing each as a full
/// `ProcessIdentity` and rejecting any candidate that cannot have preceded this
/// process. The nearest ancestor is mandatory — without it there is no owner
/// and the caller must refuse to spawn. A missing second level degrades to a
/// single-link anchor rather than failing the launch: the pane process is
/// still a true owner, and killing psmux kills the pane.
///
/// # Errors
///
/// Returns [`OwnerAnchorError`] when this process's own identity is
/// unobservable, when no parent can be resolved, or when the parent's identity
/// cannot be trusted.
#[cfg(windows)]
pub fn capture_owner_anchor() -> Result<OwnerAnchor, OwnerAnchorError> {
    capture_owner_anchor_from(std::process::id())
}

#[cfg(windows)]
fn capture_owner_anchor_from(start_pid: u32) -> Result<OwnerAnchor, OwnerAnchorError> {
    use super::process::{capture_process_identity, parent_process_id};

    let start = capture_process_identity(start_pid)
        .map_err(|_| OwnerAnchorError::SelfIdentityUnavailable)?;
    let mut links = Vec::with_capacity(OWNER_CHAIN_DEPTH);
    let mut current = start;
    for depth in 0..OWNER_CHAIN_DEPTH {
        let Some(parent_pid) = parent_process_id(current.pid).filter(|pid| *pid != 0) else {
            break;
        };
        if parent_pid == current.pid {
            break;
        }
        // A parent that exists but cannot be identified is not a usable anchor:
        // without a creation time it is indistinguishable from a recycled PID,
        // and an ordering violation means the real parent has already exited
        // and something else now occupies its PID. For the mandatory nearest
        // link that is a hard failure; beyond it the chain simply stops, which
        // still leaves a true owner in hand.
        let usable = capture_process_identity(parent_pid)
            .ok()
            .filter(|identity| is_plausible_ancestor(current, *identity));
        let Some(identity) = usable else {
            if depth == 0 {
                return Err(OwnerAnchorError::AncestorUnobservable);
            }
            break;
        };
        links.push(OwnerLink {
            role: OwnerRole::at_depth(depth),
            identity,
        });
        current = identity;
    }
    OwnerAnchor::from_links(links)
}

/// Start the owner watchdog on a background thread (Windows only).
///
/// The returned handle is intentionally dropped by the caller: the watchdog
/// must outlive every caller frame and is terminated only by process exit. On a
/// confirmed ownership loss it exits the process, which closes the host's Job
/// handle so the kernel terminates the contained worker tree.
#[cfg(windows)]
pub fn spawn_owner_watchdog(anchor: OwnerAnchor) {
    // A watchdog that cannot start is not a reason to kill a healthy tree; the
    // launch proceeds with startup reconciliation as the fallback.
    let _ = std::thread::Builder::new()
        .name("jefe-owner-watchdog".to_owned())
        .spawn(move || {
            // A panic inside a single observation must not kill the watchdog
            // thread: a dead watchdog is a silently unanchored tree, which is
            // the defect this mechanism exists to prevent. Treat it as what it
            // is -- an unusable observation -- and hold, consistent with the
            // fail-open rule applied to every other kind of uncertainty.
            let observe = |link: OwnerLink| {
                std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                    observe_owner_link(link)
                }))
                .unwrap_or_else(|_| {
                    tracing::warn!(
                        role = %link.role,
                        "owner observation panicked; holding the tree"
                    );
                    OwnerStatus::Unverified
                })
            };
            let decision = watch_owner_anchor(&anchor, observe, || {
                std::thread::sleep(OWNER_WATCH_INTERVAL);
                true
            });
            if let OwnerWatchDecision::ReleaseTree(role) = decision {
                tracing::warn!(
                    %role,
                    "owning process exited; releasing the contained worker tree"
                );
                std::process::exit(OWNER_LOST_EXIT_CODE);
            }
        });
}

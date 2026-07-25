//! Descendant-process observation and validated orphan-tree reaping.
//!
//! On native Windows with psmux, exiting/rebuilding the Jefe dashboard can kill
//! the psmux pane leader (the intermediate `jefe.exe --jefe-internal-agent-launch`
//! supervisor) while leaving the descendant LLxprt process tree alive. The
//! resulting orphan must never be treated as a reattachable session: Jefe must
//! deterministically reap the validated orphan tree and remove the stale
//! session before allowing relaunch.
//!
//! This module follows the established `process.rs` pattern: a pure, I/O-free
//! decision seam (`classify_orphan_state`) plus thin `cfg`-gated OS-level
//! probing/killing primitives (`enumerate_descendants`, `reap_orphan_tree`).
//! PID-reuse false positives are rejected before any process is considered a
//! live orphan by reusing `classify_process_observation`/`ProcessLiveness`.

use super::process::{
    ProcessLiveness, ProcessObservation, capture_process_identity, classify_process_observation,
};
use crate::domain::ProcessIdentity;

/// Outcome of classifying a session against orphan evidence.
///
/// A *dead pane* is one whose multiplexer leader has exited (`pane_dead`) or
/// whose session is entirely gone. The classifier distinguishes three states so
/// callers can choose the correct side effect: reattach, mark Dead, or
/// reap-then-Dead.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OrphanClassification {
    /// Pane is alive (or unobservable) with no orphan condition — healthy,
    /// reattachable session. No reaping required.
    NoOrphan,
    /// Pane is dead and no validated worker descendants remain. The session is
    /// simply gone; mark the agent Dead and clear the binding.
    DeadPaneNoWorker,
    /// Pane is dead but validated worker descendants are still alive. This is
    /// the orphan state: the caller must reap the tree and remove the stale
    /// session before allowing relaunch or Dead-marking.
    DeadPaneWithOrphans,
}

/// Recorded worker identity anchor plus a freshly observed liveness verdict.
///
/// The classifier consumes already-observed evidence so it stays pure and
/// unit-testable; OS probing happens at the edges (`enumerate_descendants`,
/// `reap_orphan_tree`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ObservedDescendant {
    /// Identity captured at spawn/attach time — the trusted anchor.
    pub recorded: ProcessIdentity,
    /// Fresh platform probe verdict for `recorded.pid`.
    pub liveness: ProcessLiveness,
}

impl ObservedDescendant {
    #[must_use]
    pub const fn alive(recorded: ProcessIdentity) -> Self {
        Self {
            recorded,
            liveness: ProcessLiveness::Alive,
        }
    }

    #[must_use]
    pub const fn dead(recorded: ProcessIdentity) -> Self {
        Self {
            recorded,
            liveness: ProcessLiveness::Dead,
        }
    }
}

/// Whether the multiplexer pane/session is dead from Jefe's perspective.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PaneLiveness {
    /// Pane leader alive and session exists.
    Alive,
    /// `pane_dead=1` or session entirely missing.
    Dead,
    /// Multiplexer server unavailable — cannot establish death.
    Unavailable,
}

/// Pure orphan-state classifier (no I/O).
///
/// Takes the pane's liveness, whether the session still exists, the recorded
/// worker identity anchors, and the freshly observed descendant verdicts, and
/// decides how the caller must treat the session.
///
/// A live descendant under a dead pane is only treated as an orphan when its
/// `ProcessIdentity` still matches the recorded anchor — PID reuse is rejected
/// so a recycled PID is never reaped.
#[must_use]
pub fn classify_orphan_state(
    pane: PaneLiveness,
    session_exists: bool,
    observed: &[ObservedDescendant],
) -> OrphanClassification {
    // A healthy pane is never an orphan, regardless of descendant state.
    if pane == PaneLiveness::Alive {
        return OrphanClassification::NoOrphan;
    }
    // No session at all and no descendants: nothing to reap.
    if !session_exists && observed.is_empty() {
        return OrphanClassification::DeadPaneNoWorker;
    }
    if has_validated_live_orphan(observed) {
        return OrphanClassification::DeadPaneWithOrphans;
    }
    OrphanClassification::DeadPaneNoWorker
}

/// A validated live orphan is a recorded anchor whose fresh probe is `Alive`
/// after PID-reuse rejection. Uncertain access (`Inaccessible`/`ProbeFailure`)
/// is NOT treated as a confirmed live orphan to avoid reaping unrelated
/// processes; only a confirmed `Alive` verdict with a matching identity
/// qualifies.
#[must_use]
fn has_validated_live_orphan(observed: &[ObservedDescendant]) -> bool {
    observed.iter().any(|descendant| {
        matches!(descendant.liveness, ProcessLiveness::Alive)
            && matches_recorded_anchor(descendant.recorded)
    })
}

/// Confirm a recorded anchor is internally well-formed (non-zero PID). The full
/// PID-reuse comparison against a fresh probe is performed by the OS-probe
/// callers before constructing `ObservedDescendant`; this guard rejects anchors
/// that could never have been validated.
#[must_use]
const fn matches_recorded_anchor(identity: ProcessIdentity) -> bool {
    identity.pid != 0
}

/// Re-derive a descendant's liveness against its recorded anchor using the
/// shared PID-reuse-safe comparison.
///
/// Exposed so probe helpers can build `ObservedDescendant` values from a raw
/// `ProcessObservation` without duplicating the reuse-rejection logic.
#[must_use]
pub fn descendant_liveness(
    recorded: ProcessIdentity,
    observed: ProcessObservation,
) -> ProcessLiveness {
    classify_process_observation(Some(recorded), observed)
}

#[cfg(windows)]
mod windows_probes {
    use super::{ProcessIdentity, ReapOutcome};
    use std::collections::HashMap;
    use std::process::Command;
    use winsafe::co::TH32CS;
    use winsafe::{HPROCESSLIST, PROCESSENTRY32};

    /// Recursively collect descendant PIDs whose parent chain leads back to
    /// `root`, capturing a fresh `ProcessIdentity` for each.
    ///
    /// Returns the discovered descendants in parent-to-child discovery order.
    /// Pure consumers of the classifier do not call this; only the reaping /
    /// spawn-capture edges do.
    pub(super) fn enumerate_descendants(root: u32) -> Vec<ProcessIdentity> {
        let Ok(mut snapshot) = HPROCESSLIST::CreateToolhelp32Snapshot(TH32CS::SNAPPROCESS, None)
        else {
            return Vec::new();
        };
        let mut parent_children: HashMap<u32, Vec<u32>> = HashMap::new();
        for entry in snapshot.iter_processes() {
            let entry: &PROCESSENTRY32 = match entry {
                Ok(entry) => entry,
                Err(_) => continue,
            };
            parent_children
                .entry(entry.th32ParentProcessID)
                .or_default()
                .push(entry.th32ProcessID);
        }
        let mut ordered = Vec::new();
        walk_descendants(root, &parent_children, &mut ordered);
        ordered
            .into_iter()
            .map(|pid| ProcessIdentity {
                pid,
                started_at: process_start_time(pid),
            })
            .collect()
    }

    fn walk_descendants(
        pid: u32,
        parent_children: &HashMap<u32, Vec<u32>>,
        ordered: &mut Vec<u32>,
    ) {
        if let Some(children) = parent_children.get(&pid) {
            for child in children {
                ordered.push(*child);
                walk_descendants(*child, parent_children, ordered);
            }
        }
    }

    fn process_start_time(pid: u32) -> Option<u64> {
        match super::super::process::capture_process_identity(pid) {
            Ok(identity) => identity.started_at,
            Err(_) => None,
        }
    }

    /// Terminate only the validated descendant PIDs via `taskkill /T /F`.
    ///
    /// Re-probes each PID before killing to confirm its identity still matches
    /// the recorded anchor (PID-reuse guard). Failures are best-effort and
    /// surfaced as a typed error; the caller never panics.
    pub(super) fn reap_validated(anchors: &[ProcessIdentity]) -> Result<usize, ReapOutcome> {
        let mut reaped = 0usize;
        for anchor in anchors {
            if !super::descendant_still_matches_anchor(*anchor) {
                // PID exited or was recycled: nothing to reap for this anchor.
                continue;
            }
            let status = Command::new("taskkill")
                .args(["/PID", &anchor.pid.to_string(), "/T", "/F"])
                .status();
            if status.is_ok() {
                reaped += 1;
            }
        }
        if reaped == 0 && !anchors.is_empty() {
            return Err(ReapOutcome::NothingReaped);
        }
        Ok(reaped)
    }
}

#[cfg(unix)]
mod unix_probes {
    use super::{ProcessIdentity, ProcessLiveness};
    use std::collections::HashMap;

    /// Walk `/proc/<pid>/stat` parent links to enumerate descendants of `root`.
    ///
    /// Unix orphan recovery is a non-goal (the orphan scenario is
    /// Windows/psmux-specific); this exists only so the classifier and reap
    /// logic stay cross-platform testable.
    pub(super) fn enumerate_descendants(root: u32) -> Vec<ProcessIdentity> {
        let mut parent_children: HashMap<u32, Vec<u32>> = HashMap::new();
        let Ok(entries) = std::fs::read_dir("/proc") else {
            return Vec::new();
        };
        for entry in entries.flatten() {
            let Some(name) = entry.file_name().to_str() else {
                continue;
            };
            let Ok(pid) = name.parse::<u32>() else {
                continue;
            };
            if let Some(parent) = read_parent_pid(pid) {
                parent_children.entry(parent).or_default().push(pid);
            }
        }
        let mut ordered = Vec::new();
        walk_descendants(root, &parent_children, &mut ordered);
        ordered
            .into_iter()
            .map(|pid| ProcessIdentity {
                pid,
                started_at: read_start_time(pid),
            })
            .collect()
    }

    fn walk_descendants(
        pid: u32,
        parent_children: &HashMap<u32, Vec<u32>>,
        ordered: &mut Vec<u32>,
    ) {
        if let Some(children) = parent_children.get(&pid) {
            for child in children {
                ordered.push(*child);
                walk_descendants(*child, parent_children, ordered);
            }
        }
    }

    fn read_parent_pid(pid: u32) -> Option<u32> {
        let stat = std::fs::read_to_string(format!("/proc/{pid}/stat")).ok()?;
        let command_end = stat.rfind(')')?;
        stat.get(command_end + 2..)?
            .split_whitespace()
            .nth(1)?
            .parse()
            .ok()
    }

    #[cfg(target_os = "linux")]
    fn read_start_time(pid: u32) -> Option<u64> {
        let stat = std::fs::read_to_string(format!("/proc/{pid}/stat")).ok()?;
        let command_end = stat.rfind(')')?;
        stat.get(command_end + 2..)?
            .split_whitespace()
            .nth(19)?
            .parse()
            .ok()
    }

    #[cfg(not(target_os = "linux"))]
    fn read_start_time(_pid: u32) -> Option<u64> {
        None
    }

    pub(super) fn reap_validated(anchors: &[ProcessIdentity]) -> Result<usize, ReapOutcome> {
        let mut reaped = 0usize;
        for anchor in anchors {
            if !super::descendant_still_matches_anchor(*anchor) {
                continue;
            }
            let _ = nix_like_kill(anchor.pid);
            reaped += 1;
        }
        if reaped == 0 && !anchors.is_empty() {
            return Err(ReapOutcome::NothingReaped);
        }
        Ok(reaped)
    }

    fn nix_like_kill(pid: u32) -> std::io::Result<()> {
        std::process::Command::new("kill")
            .args(["-KILL", &pid.to_string()])
            .status()?;
        Ok(())
    }
}

#[cfg(not(any(unix, windows)))]
mod other_probes {
    use super::{ProcessIdentity, ReapOutcome};

    pub(super) fn enumerate_descendants(_root: u32) -> Vec<ProcessIdentity> {
        Vec::new()
    }

    pub(super) fn reap_validated(_anchors: &[ProcessIdentity]) -> Result<usize, ReapOutcome> {
        Ok(0)
    }
}

#[cfg(not(any(unix, windows)))]
use other_probes as platform_probes;
#[cfg(unix)]
use unix_probes as platform_probes;
#[cfg(windows)]
use windows_probes as platform_probes;

/// Outcome of a best-effort reap attempt.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReapOutcome {
    /// Every recorded anchor was probed but none could be validated/reaped.
    NothingReaped,
}

/// Enumerate the descendant process tree of `root`, capturing a fresh
/// `ProcessIdentity` for each.
#[must_use]
pub fn enumerate_descendants(root: u32) -> Vec<ProcessIdentity> {
    if root == 0 {
        return Vec::new();
    }
    platform_probes::enumerate_descendants(root)
}

/// Capture the worker descendant tree for a launcher PID, returning an empty
/// vec for `None`/zero PIDs. Convenience wrapper for spawn/reattach paths
/// (issue #332).
#[must_use]
pub fn capture_worker_identities(launcher_pid: Option<u32>) -> Vec<ProcessIdentity> {
    launcher_pid
        .filter(|pid| *pid != 0)
        .map_or_else(Vec::new, enumerate_descendants)
}

/// Confirm a recorded anchor's PID still matches its original identity
/// (PID-reuse guard) using the shared probe path.
#[must_use]
pub fn descendant_still_matches_anchor(anchor: ProcessIdentity) -> bool {
    if anchor.pid == 0 {
        return false;
    }
    // `capture_process_identity` probes the PID and returns a fresh identity on
    // success. Matching `started_at` (when both are known) confirms the PID was
    // not recycled; absence of a start time fails open conservatively (treated
    // as not-a-confirmed-match) so unrelated processes are never reaped.
    match capture_process_identity(anchor.pid) {
        Ok(actual) => pid_anchors_match(anchor, actual),
        Err(_) => false,
    }
}

#[must_use]
fn pid_anchors_match(expected: ProcessIdentity, actual: ProcessIdentity) -> bool {
    if expected.pid != actual.pid {
        return false;
    }
    match (expected.started_at, actual.started_at) {
        (Some(expected), Some(actual)) => expected == actual,
        // Without a comparable start time we cannot rule out reuse, so refuse
        // to treat this as a validated orphan.
        _ => false,
    }
}

/// Reap the validated orphan process tree.
///
/// Before terminating, each candidate PID is re-probed and confirmed against
/// its recorded anchor. Only validated members are terminated. Reaping is
/// strictly agent-scoped and best-effort: failures are surfaced as a typed
/// `ReapOutcome`/`Result` and never panic.
///
/// Returns the number of anchors that were reaped.
pub fn reap_orphan_tree(anchors: &[ProcessIdentity]) -> Result<usize, ReapOutcome> {
    if anchors.is_empty() {
        return Ok(0);
    }
    platform_probes::reap_validated(anchors)
}

/// Best-effort reap of a dead-launcher orphan: terminate the validated worker
/// descendant tree, then remove the stale multiplexer session (issue #332).
///
/// Combines [`reap_orphan_tree`] with a `kill_session` so callers at the
/// startup/relaunch/delete boundaries get a single agent-scoped cleanup
/// operation. Every failure is logged as a warning and swallowed; this never
/// returns an error that a caller must propagate, because cleanup must not
/// abort startup, relaunch, or deletion.
pub fn reap_orphan_session(anchors: &[ProcessIdentity], session_name: &str) {
    if let Err(outcome) = reap_orphan_tree(anchors) {
        tracing::warn!(
            ?outcome,
            "orphan reap did not terminate any validated descendant"
        );
    }
    if let Err(error) = super::commands::kill_session(session_name) {
        tracing::warn!(
            session = session_name,
            error = %error,
            "best-effort kill of stale orphan session failed"
        );
    }
}

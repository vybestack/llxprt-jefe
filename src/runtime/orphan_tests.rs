//! Behavioral contracts for orphan-process classification and reaping (issue #332).

use super::orphan::{
    ObservedDescendant, OrphanClassification, PaneLiveness, ReapOutcome, classify_orphan_state,
    reap_orphan_tree,
};
use crate::domain::ProcessIdentity;
use crate::runtime::process::ProcessLiveness;

const ANCHOR_PID: u32 = 1234;
const ANCHOR_START: u64 = 10_000;

fn anchor(pid: u32, started_at: u64) -> ProcessIdentity {
    ProcessIdentity::new(pid, started_at)
}

#[test]
fn dead_pane_no_worker_is_not_an_orphan() {
    // AC1: pane dead, session exists, no recorded identities, no observed descendants.
    let classification = classify_orphan_state(PaneLiveness::Dead, true, &[]);
    assert_eq!(
        classification,
        OrphanClassification::DeadPaneNoWorker,
        "dead pane with no worker descendants must not be treated as an orphan"
    );
}

#[test]
fn dead_pane_with_validated_live_orphan_is_orphaned() {
    // AC2: pane dead, session exists, recorded anchor X, X observed alive & matching.
    let recorded = anchor(ANCHOR_PID, ANCHOR_START);
    let observed = [ObservedDescendant::alive(recorded)];
    let classification = classify_orphan_state(PaneLiveness::Dead, true, &observed);
    assert_eq!(
        classification,
        OrphanClassification::DeadPaneWithOrphans,
        "dead pane with a validated live descendant must be classified Orphaned"
    );
}

#[test]
fn alive_pane_is_never_an_orphan() {
    // AC3: pane alive, session exists, descendant alive — healthy/reattachable.
    let recorded = anchor(ANCHOR_PID, ANCHOR_START);
    let observed = [ObservedDescendant::alive(recorded)];
    let classification = classify_orphan_state(PaneLiveness::Alive, true, &observed);
    assert_eq!(
        classification,
        OrphanClassification::NoOrphan,
        "an alive pane is never an orphan even with live descendants"
    );
}

#[test]
fn dead_pane_reused_pid_is_not_treated_as_orphan() {
    // AC4: descendant PID alive but identity mismatches recorded anchor (PID reuse).
    let recorded = anchor(ANCHOR_PID, ANCHOR_START);
    let observed = [ObservedDescendant {
        recorded,
        // A reused PID would probe as Alive but with a different start time;
        // the caller is responsible for setting ReusedPid when the probe
        // diverges. Here we simulate the reuse verdict directly.
        liveness: ProcessLiveness::ReusedPid,
    }];
    let classification = classify_orphan_state(PaneLiveness::Dead, true, &observed);
    assert_eq!(
        classification,
        OrphanClassification::DeadPaneNoWorker,
        "a reused PID under a dead pane must not be reaped as an orphan"
    );
}

#[test]
fn dead_pane_inaccessible_descendant_is_not_confirmed_orphan() {
    // Uncertain access (Inaccessible/ProbeFailure) must NOT confirm an orphan,
    // so unrelated processes are never reaped on a probe hiccup.
    let recorded = anchor(ANCHOR_PID, ANCHOR_START);
    let observed = [ObservedDescendant {
        recorded,
        liveness: ProcessLiveness::Inaccessible,
    }];
    let classification = classify_orphan_state(PaneLiveness::Dead, true, &observed);
    assert_eq!(classification, OrphanClassification::DeadPaneNoWorker);
}

#[test]
fn dead_pane_dead_descendant_is_not_an_orphan() {
    // Recorded anchor whose fresh probe is Dead: the orphan already exited.
    let recorded = anchor(ANCHOR_PID, ANCHOR_START);
    let observed = [ObservedDescendant::dead(recorded)];
    let classification = classify_orphan_state(PaneLiveness::Dead, true, &observed);
    assert_eq!(classification, OrphanClassification::DeadPaneNoWorker);
}

#[test]
fn missing_session_no_descendants_is_dead_pane_no_worker() {
    let classification = classify_orphan_state(PaneLiveness::Dead, false, &[]);
    assert_eq!(classification, OrphanClassification::DeadPaneNoWorker);
}

#[test]
fn unavailable_pane_with_live_descendant_is_not_an_orphan() {
    // When the multiplexer server is unavailable we cannot confirm the pane is
    // dead, so we must not reap — the session may still be healthy. Fail safe.
    let recorded = anchor(ANCHOR_PID, ANCHOR_START);
    let observed = [ObservedDescendant::alive(recorded)];
    let classification = classify_orphan_state(PaneLiveness::Unavailable, true, &observed);
    assert_eq!(classification, OrphanClassification::NoOrphan);
}

#[test]
fn empty_orphan_tree_reaps_zero() {
    let result = reap_orphan_tree(&[]);
    assert_eq!(result, Ok(0));
}

#[test]
fn reap_orphan_tree_with_bogus_anchor_reaps_nothing() {
    // A PID that does not exist cannot match its anchor; reap is best-effort.
    let bogus = anchor(u32::MAX, ANCHOR_START);
    let result = reap_orphan_tree(&[bogus]);
    assert_eq!(result, Err(ReapOutcome::NothingReaped));
}

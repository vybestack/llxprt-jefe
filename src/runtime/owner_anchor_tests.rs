//! Behavioral tests for the owner-lifetime anchor (issue #542).
//!
//! See `dev-docs/standards/windows-session-ownership.md`. Every test here maps
//! to a rule in §4 of that document; the decision logic is pure so it is
//! exercised on every platform, not only the one where the bug reproduces.

use std::cell::RefCell;

use super::owner_anchor::{
    OWNER_LOST_EXIT_CODE, OwnerAnchor, OwnerAnchorError, OwnerLink, OwnerRole, OwnerStatus,
    OwnerWatchDecision, classify_owner_link, decide_owner_watch, is_plausible_ancestor,
    watch_owner_anchor,
};
use super::process::ProcessLiveness;
use crate::domain::ProcessIdentity;

fn identity(pid: u32, started_at: u64) -> ProcessIdentity {
    ProcessIdentity {
        pid,
        started_at: Some(started_at),
    }
}

fn pane(pid: u32, started_at: u64) -> OwnerLink {
    OwnerLink {
        role: OwnerRole::PaneProcess,
        identity: identity(pid, started_at),
    }
}

fn server(pid: u32, started_at: u64) -> OwnerLink {
    OwnerLink {
        role: OwnerRole::SessionServer,
        identity: identity(pid, started_at),
    }
}

fn anchor(links: Vec<OwnerLink>) -> OwnerAnchor {
    OwnerAnchor::from_links(links)
        .unwrap_or_else(|error| panic!("test anchor must have at least one link: {error:?}"))
}

// ── §4 rule 4: owner death releases the tree ────────────────────────────────

/// A living owner chain is not an event. The host must not touch the worker,
/// the Job, or any status while every captured link is alive.
#[test]
fn a_living_owner_chain_holds_the_tree() {
    let decision = decide_owner_watch([
        (OwnerRole::PaneProcess, OwnerStatus::Held),
        (OwnerRole::SessionServer, OwnerStatus::Held),
    ]);
    assert_eq!(decision, OwnerWatchDecision::Hold);
}

/// #515's exact reproduction, as a decision: the psmux grandparent dies while
/// the pane process is still alive. The old model saw a live parent and did
/// nothing, which is how `pwsh -> jefe-session-host.exe -> bun.exe` survived.
#[test]
fn a_dead_session_server_releases_the_tree_even_while_the_pane_lives() {
    let decision = decide_owner_watch([
        (OwnerRole::PaneProcess, OwnerStatus::Held),
        (OwnerRole::SessionServer, OwnerStatus::Lost),
    ]);
    assert_eq!(
        decision,
        OwnerWatchDecision::ReleaseTree(OwnerRole::SessionServer),
        "a confirmed psmux server death is an ownership violation regardless \
         of the pane process's state; this is issue #515"
    );
}

/// The mirror case: the pane dies but the server survives. Ownership is a
/// chain, so a break at any link releases the tree.
#[test]
fn a_dead_pane_process_releases_the_tree_even_while_the_server_lives() {
    let decision = decide_owner_watch([
        (OwnerRole::PaneProcess, OwnerStatus::Lost),
        (OwnerRole::SessionServer, OwnerStatus::Held),
    ]);
    assert_eq!(
        decision,
        OwnerWatchDecision::ReleaseTree(OwnerRole::PaneProcess)
    );
}

// ── §4 rule 2: PID reuse cannot spoof ownership (V5) ────────────────────────

/// V5. A recycled PID resolves to a live process, so any PID-only check reports
/// "owner alive" forever and the tree is never reaped. Identity comparison must
/// classify it as owner death.
#[test]
fn a_reused_pid_is_owner_death_not_a_live_owner() {
    assert_eq!(
        classify_owner_link(ProcessLiveness::ReusedPid),
        OwnerStatus::Lost,
        "a PID that now belongs to a different process is not the owner; \
         treating it as alive is how ownership gets silently transferred to an \
         impostor"
    );
    assert_eq!(
        decide_owner_watch([(OwnerRole::SessionServer, OwnerStatus::Lost)]),
        OwnerWatchDecision::ReleaseTree(OwnerRole::SessionServer)
    );
}

#[test]
fn a_confirmed_exit_is_owner_death() {
    assert_eq!(
        classify_owner_link(ProcessLiveness::Dead),
        OwnerStatus::Lost
    );
}

#[test]
fn a_live_matching_identity_holds_ownership() {
    assert_eq!(
        classify_owner_link(ProcessLiveness::Alive),
        OwnerStatus::Held
    );
}

// ── §4 rule 5: uncertainty must never terminate ─────────────────────────────

/// Termination is irreversible and a probe failure is not. Every non-positive
/// observation must fail open, or a transient `ACCESS_DENIED` under load
/// becomes a killed agent with unsaved work.
#[test]
fn an_unverifiable_owner_never_terminates_the_tree() {
    for liveness in [
        ProcessLiveness::Inaccessible,
        ProcessLiveness::ProbeFailure,
        ProcessLiveness::MalformedIdentity,
    ] {
        assert_eq!(
            classify_owner_link(liveness),
            OwnerStatus::Unverified,
            "{liveness:?} is an absence of evidence, not evidence of death"
        );
    }
    assert_eq!(
        decide_owner_watch([
            (OwnerRole::PaneProcess, OwnerStatus::Unverified),
            (OwnerRole::SessionServer, OwnerStatus::Unverified),
        ]),
        OwnerWatchDecision::Hold,
        "a completely unverifiable chain must hold the tree, not reap it"
    );
}

/// Evidence of death at one link is decisive even when the other link is
/// merely unverifiable: one positive observation is enough.
#[test]
fn one_confirmed_loss_outweighs_an_unverifiable_link() {
    assert_eq!(
        decide_owner_watch([
            (OwnerRole::PaneProcess, OwnerStatus::Unverified),
            (OwnerRole::SessionServer, OwnerStatus::Lost),
        ]),
        OwnerWatchDecision::ReleaseTree(OwnerRole::SessionServer)
    );
}

// ── The watch loop ──────────────────────────────────────────────────────────

/// The watchdog must keep holding across an arbitrary number of live ticks and
/// release on the first confirmed loss — not on the first tick, and not only
/// after some bounded number of retries.
#[test]
fn the_watchdog_holds_until_a_link_is_confirmed_lost() {
    let script = RefCell::new(vec![
        OwnerStatus::Held,
        OwnerStatus::Held,
        OwnerStatus::Unverified,
        OwnerStatus::Held,
        OwnerStatus::Lost,
    ]);
    let ticks = RefCell::new(0usize);
    let decision = watch_owner_anchor(
        &anchor(vec![server(4242, 900)]),
        |_| script.borrow_mut().remove(0),
        || {
            *ticks.borrow_mut() += 1;
            true
        },
    );
    assert_eq!(
        decision,
        OwnerWatchDecision::ReleaseTree(OwnerRole::SessionServer)
    );
    assert_eq!(
        *ticks.borrow(),
        4,
        "the watchdog must survive live and unverifiable observations and act \
         only on the confirmed loss"
    );
}

/// Sustained uncertainty must never accumulate into a kill. A host whose owner
/// is permanently unprobeable keeps its agent alive forever.
#[test]
fn sustained_uncertainty_never_accumulates_into_a_kill() {
    let remaining = RefCell::new(500usize);
    let decision = watch_owner_anchor(
        &anchor(vec![pane(11, 100), server(7, 50)]),
        |_| OwnerStatus::Unverified,
        || {
            let mut remaining = remaining.borrow_mut();
            *remaining -= 1;
            *remaining > 0
        },
    );
    assert_eq!(
        decision,
        OwnerWatchDecision::Hold,
        "500 consecutive failed probes is still not evidence of death"
    );
}

/// The watchdog observes every link on each pass, so a loss at the second link
/// is found in the same pass rather than one interval later.
#[test]
fn the_watchdog_observes_every_link_in_the_chain() {
    let observed = RefCell::new(Vec::new());
    let decision = watch_owner_anchor(
        &anchor(vec![pane(11, 100), server(7, 50)]),
        |link| {
            observed.borrow_mut().push(link.role);
            match link.role {
                OwnerRole::PaneProcess => OwnerStatus::Held,
                OwnerRole::SessionServer => OwnerStatus::Lost,
            }
        },
        || true,
    );
    assert_eq!(
        decision,
        OwnerWatchDecision::ReleaseTree(OwnerRole::SessionServer)
    );
    assert_eq!(
        *observed.borrow(),
        vec![OwnerRole::PaneProcess, OwnerRole::SessionServer]
    );
}

// ── §4 rule 3: an ancestor cannot be younger than its descendant ────────────

/// A recycled PID that happens to sit in our parent slot is not an ancestor.
/// Creation-time ordering is the cheap structural check that rejects it at
/// capture time, before it can ever be trusted as an anchor.
#[test]
fn an_ancestor_younger_than_its_descendant_is_rejected() {
    let host = identity(1000, 5_000);
    assert!(
        !is_plausible_ancestor(host, identity(900, 6_000)),
        "a process created after the session host cannot have spawned its \
         parent chain; this is a recycled PID"
    );
    assert!(is_plausible_ancestor(host, identity(900, 4_000)));
    assert!(
        is_plausible_ancestor(host, identity(900, 5_000)),
        "clock granularity can make a real ancestor look simultaneous; only \
         strictly-younger candidates are impostors"
    );
}

/// An identity with no creation time can never be compared, so it can never be
/// distinguished from a recycled PID. It is not usable as an anchor.
#[test]
fn an_ancestor_without_a_creation_time_is_not_an_anchor() {
    let host = identity(1000, 5_000);
    let timeless = ProcessIdentity {
        pid: 900,
        started_at: None,
    };
    assert!(!is_plausible_ancestor(host, timeless));
    assert!(!is_plausible_ancestor(
        ProcessIdentity {
            pid: 1000,
            started_at: None
        },
        identity(900, 4_000)
    ));
}

// ── §4 rule 6: no anchor, no worker ─────────────────────────────────────────

/// An empty chain is not a degraded anchor, it is the absence of one. The
/// constructor must refuse it so no caller can spawn an unowned worker.
#[test]
fn an_anchor_cannot_be_constructed_without_a_link() {
    assert_eq!(
        OwnerAnchor::from_links(Vec::new()),
        Err(OwnerAnchorError::NoAncestor)
    );
}

#[test]
fn an_anchor_preserves_its_chain_in_order() {
    let anchor = anchor(vec![pane(11, 100), server(7, 50)]);
    assert_eq!(anchor.links(), &[pane(11, 100), server(7, 50)]);
}

/// The exit code is the observable evidence in a native test and in CI logs
/// that the tree was released by ownership loss rather than by the agent
/// finishing or by an unrelated crash.
#[test]
fn owner_loss_has_a_distinguishable_exit_code() {
    assert_ne!(OWNER_LOST_EXIT_CODE, 0);
    assert_ne!(OWNER_LOST_EXIT_CODE, 1);
}

#[test]
fn owner_roles_are_named_for_diagnostics() {
    assert_eq!(OwnerRole::PaneProcess.as_str(), "pane process");
    assert_eq!(OwnerRole::SessionServer.as_str(), "psmux server");
}

/// Rule 1 and rule 6 of the ownership model: the session host must be able to
/// name a real owner from the live operating system, not merely accept one a
/// test handed it. Walking up from this process must produce a chain that is
/// non-empty, capped at the pane/server depth, ordered nearest-first, and made
/// of identities that could actually have preceded their descendants.
#[cfg(windows)]
#[test]
fn the_owner_chain_is_captured_from_the_live_process_tree() {
    let anchor = super::owner_anchor::capture_owner_anchor()
        .unwrap_or_else(|error| panic!("a test process always has a resolvable parent: {error:?}"));
    let links = anchor.links();
    assert!(
        (1..=2).contains(&links.len()),
        "the chain is capped at the pane process and the psmux server so the \
         dashboard can never become an anchor, got {} links",
        links.len()
    );
    assert_eq!(
        links[0].role,
        OwnerRole::PaneProcess,
        "the nearest ancestor is the pane process"
    );
    if let Some(second) = links.get(1) {
        assert_eq!(
            second.role,
            OwnerRole::SessionServer,
            "the second ancestor is the session server"
        );
    }
    let self_identity = super::process::capture_process_identity(std::process::id())
        .unwrap_or_else(|error| panic!("this process can observe itself: {error:?}"));
    let mut descendant = self_identity;
    for link in links {
        assert!(
            is_plausible_ancestor(descendant, link.identity),
            "captured {} pid {} cannot have preceded pid {}; a recycled PID \
             must never be accepted as an owner",
            link.role,
            link.identity.pid,
            descendant.pid
        );
        assert!(
            link.identity.started_at.is_some(),
            "an anchor identity without a creation time cannot be \
             distinguished from a recycled PID"
        );
        descendant = link.identity;
    }
}

/// A captured chain must report ownership as held while every member is
/// genuinely running. This is the guard against a watchdog that reaps healthy
/// trees: the ancestors of a live test process are, by construction, alive.
#[cfg(windows)]
#[test]
fn a_freshly_captured_chain_reports_its_own_ancestors_as_alive() {
    let anchor = super::owner_anchor::capture_owner_anchor()
        .unwrap_or_else(|error| panic!("a test process always has a resolvable parent: {error:?}"));
    for link in anchor.links() {
        assert_eq!(
            super::owner_anchor::observe_owner_link(*link),
            OwnerStatus::Held,
            "the live {} pid {} must not be read as lost",
            link.role,
            link.identity.pid
        );
    }
}

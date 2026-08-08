//! Core persistent-provider lifecycle (issue #390 CW-10, Slice C2):
//! CW10-03 ordered startup, atomic publication with rollback reap, no
//! auto-restart, explicit shutdown reap, and duplicate-plugin-id rejection.
//!
//! Drives the cross-platform `jefe-provider-fixture` through the real persistent
//! supervisor to prove the deterministic plugin-id start order, the
//! all-or-nothing publication, the rollback reap at every handshake phase, and
//! the absence of auto-restart.

use std::time::{Duration, Instant};

use jefe::runtime::provider::persistent::{
    CandidateFailure, CandidateHealth, PersistentPhase, PersistentStartup,
    PersistentStartupFailure, PersistentStartupResult, StartupFailure, run_persistent_startup,
};
use jefe::runtime::provider::protocol::Capability;
use jefe::runtime::provider::supervisor::SupervisorFailure;

use super::persistent_support::{
    EmptyEnv, Scene, assert_all_reaped, fast_bounds, process_is_gone, startup_sequence,
};

#[test]
fn cw10_03_two_candidates_in_reverse_input_order_start_in_plugin_id_order() {
    let _budget = super::persistent_support::process_budget();
    let scene = Scene::new();
    // Provided in reverse of canonical text order.
    let startup = PersistentStartup {
        candidates: vec![
            scene.candidate("vendor.zeta", "persistent-ready", vec![Capability::Actions]),
            scene.candidate(
                "vendor.alpha",
                "persistent-ready",
                vec![Capability::Actions],
            ),
        ],
    };
    let result = run_persistent_startup(&startup, &fast_bounds(), &EmptyEnv);
    let mut supervisor = match result {
        PersistentStartupResult::Started { supervisor, .. } => supervisor,
        other @ PersistentStartupResult::Failed(_) => panic!("expected started, got {other:?}"),
    };
    // The publication lists candidates in canonical plugin-id order.
    let ids: Vec<&str> = supervisor
        .publication()
        .ready()
        .iter()
        .map(|candidate| candidate.plugin_id.as_str())
        .collect();
    assert_eq!(ids, vec!["vendor.alpha", "vendor.zeta"]);
    // The fixtures recorded the order they actually received hello: sorted.
    assert_eq!(
        startup_sequence(&scene),
        vec!["vendor.alpha", "vendor.zeta"],
        "candidates start in deterministic plugin-id order, not input order"
    );
    let shutdown = supervisor.shutdown();
    assert_eq!(shutdown.len(), 2);
    assert!(shutdown.iter().all(|entry| entry.process_reaped));
}

#[test]
fn cw10_03_every_required_candidate_ready_before_one_atomic_publication() {
    let _budget = super::persistent_support::process_budget();
    let scene = Scene::new();
    let startup = PersistentStartup {
        candidates: vec![
            scene.candidate(
                "vendor.alpha",
                "persistent-ready",
                vec![Capability::Actions],
            ),
            scene.candidate("vendor.zeta", "persistent-ready", vec![Capability::Actions]),
        ],
    };
    let result = run_persistent_startup(&startup, &fast_bounds(), &EmptyEnv);
    let (mut supervisor, publication) = match result {
        PersistentStartupResult::Started {
            supervisor,
            publication,
        } => (supervisor, publication),
        other @ PersistentStartupResult::Failed(_) => panic!("expected started, got {other:?}"),
    };
    assert_eq!(
        publication.ready().len(),
        2,
        "publication is atomic: both ready"
    );
    // Health reports every candidate ready until shutdown.
    let health = supervisor.health();
    assert_eq!(health.len(), 2);
    assert!(
        health
            .iter()
            .all(|snapshot| matches!(snapshot.health, CandidateHealth::Ready { .. }))
    );
    let shutdown = supervisor.shutdown();
    assert_eq!(shutdown.len(), 2);
    assert!(shutdown.iter().all(|entry| entry.process_reaped));
}

#[test]
fn cw10_04_a_spawn_failure_returns_no_publication_and_reaps_nothing() {
    let _budget = super::persistent_support::process_budget();
    let scene = Scene::new();
    let startup = PersistentStartup {
        candidates: vec![scene.unspawnable_candidate("vendor.alpha")],
    };
    let result = run_persistent_startup(&startup, &fast_bounds(), &EmptyEnv);
    let failure = match result {
        PersistentStartupResult::Failed(failure) => failure,
        other @ PersistentStartupResult::Started { .. } => panic!("expected failed, got {other:?}"),
    };
    assert!(
        matches!(
            &failure.failure,
            StartupFailure::Candidate(CandidateFailure {
                phase: PersistentPhase::Spawn,
                failure: SupervisorFailure::Spawn(_),
                ..
            })
        ),
        "expected spawn failure, got {:?}",
        failure.failure
    );
    assert!(
        failure.rollback.is_empty(),
        "no candidate was started, so rollback is empty"
    );
}

#[test]
fn cw10_04_a_hello_ack_timeout_fails_and_is_reaped() {
    let _budget = super::persistent_support::process_budget();
    let scene = Scene::new();
    let startup = PersistentStartup {
        candidates: vec![scene.candidate(
            "vendor.alpha",
            "persistent-hello-hang",
            vec![Capability::Actions],
        )],
    };
    let result = run_persistent_startup(&startup, &fast_bounds(), &EmptyEnv);
    let failure = expect_failed(result);
    assert!(
        matches!(
            &failure.failure,
            StartupFailure::Candidate(CandidateFailure {
                phase: PersistentPhase::HelloAck,
                failure: SupervisorFailure::HandshakeTimeout,
                ..
            })
        ),
        "expected hello-ack timeout, got {:?}",
        failure.failure
    );
    assert_all_reaped(&failure.rollback);
}

#[test]
fn cw10_04_a_ready_timeout_fails_and_is_reaped() {
    let _budget = super::persistent_support::process_budget();
    let scene = Scene::new();
    let startup = PersistentStartup {
        candidates: vec![scene.candidate(
            "vendor.alpha",
            "persistent-ready-hang",
            vec![Capability::Actions],
        )],
    };
    let result = run_persistent_startup(&startup, &fast_bounds(), &EmptyEnv);
    let failure = expect_failed(result);
    assert!(
        matches!(
            &failure.failure,
            StartupFailure::Candidate(CandidateFailure {
                phase: PersistentPhase::Ready,
                failure: SupervisorFailure::HandshakeTimeout,
                ..
            })
        ),
        "expected ready timeout, got {:?}",
        failure.failure
    );
    assert_all_reaped(&failure.rollback);
}

#[test]
fn cw10_04_a_protocol_fault_at_hello_ack_fails_and_is_reaped() {
    let _budget = super::persistent_support::process_budget();
    let scene = Scene::new();
    let startup = PersistentStartup {
        candidates: vec![scene.candidate(
            "vendor.alpha",
            "persistent-protocol",
            vec![Capability::Actions],
        )],
    };
    let result = run_persistent_startup(&startup, &fast_bounds(), &EmptyEnv);
    let failure = expect_failed(result);
    assert!(
        matches!(
            &failure.failure,
            StartupFailure::Candidate(CandidateFailure {
                phase: PersistentPhase::HelloAck,
                failure: SupervisorFailure::Protocol(_),
                ..
            })
        ),
        "expected protocol fault, got {:?}",
        failure.failure
    );
    assert_all_reaped(&failure.rollback);
}

#[test]
fn cw10_04_a_crash_after_ack_fails_at_configure_or_ready_and_is_reaped() {
    let _budget = super::persistent_support::process_budget();
    let scene = Scene::new();
    let startup = PersistentStartup {
        candidates: vec![scene.candidate(
            "vendor.alpha",
            "persistent-crash-after-ack",
            vec![Capability::Actions],
        )],
    };
    let result = run_persistent_startup(&startup, &fast_bounds(), &EmptyEnv);
    let failure = expect_failed(result);
    assert!(
        matches!(
            &failure.failure,
            StartupFailure::Candidate(CandidateFailure {
                phase: PersistentPhase::Configure | PersistentPhase::Ready,
                ..
            })
        ),
        "expected configure/ready failure, got {:?}",
        failure.failure
    );
    assert_all_reaped(&failure.rollback);
}

#[test]
fn cw10_04_an_undeclared_capability_is_rejected_before_publication() {
    let _budget = super::persistent_support::process_budget();
    let scene = Scene::new();
    // The host declares only actions; the fixture reports actions + panels.
    let startup = PersistentStartup {
        candidates: vec![scene.candidate(
            "vendor.alpha",
            "persistent-undeclared-cap",
            vec![Capability::Actions],
        )],
    };
    let result = run_persistent_startup(&startup, &fast_bounds(), &EmptyEnv);
    let failure = expect_failed(result);
    assert!(
        matches!(
            &failure.failure,
            StartupFailure::Candidate(CandidateFailure {
                phase: PersistentPhase::Capability,
                failure: SupervisorFailure::Protocol(_),
                ..
            })
        ),
        "expected capability rejection, got {:?}",
        failure.failure
    );
    assert_all_reaped(&failure.rollback);
}

#[test]
fn cw10_04_a_second_candidate_failure_rolls_back_the_first() {
    let _budget = super::persistent_support::process_budget();
    let scene = Scene::new();
    // alpha starts and reaches ready; zeta hangs at hello-ack and fails.
    let startup = PersistentStartup {
        candidates: vec![
            scene.candidate(
                "vendor.alpha",
                "persistent-ready",
                vec![Capability::Actions],
            ),
            scene.candidate(
                "vendor.zeta",
                "persistent-hello-hang",
                vec![Capability::Actions],
            ),
        ],
    };
    let result = run_persistent_startup(&startup, &fast_bounds(), &EmptyEnv);
    let failure = expect_failed(result);
    // alpha was started first and rolled back; zeta failed last.
    let reaped_ids: Vec<&str> = failure
        .rollback
        .iter()
        .map(|entry| entry.plugin_id.as_str())
        .collect();
    assert_eq!(
        reaped_ids,
        vec!["vendor.alpha", "vendor.zeta"],
        "every previously started and the failing candidate are reaped in start order"
    );
    assert_all_reaped(&failure.rollback);
}

#[test]
fn cw10_04_there_is_no_auto_restart_after_a_ready_process_exits() {
    let _budget = super::persistent_support::process_budget();
    let scene = Scene::new();
    let startup = PersistentStartup {
        candidates: vec![scene.candidate(
            "vendor.alpha",
            "persistent-ready-then-exit",
            vec![Capability::Actions],
        )],
    };
    let result = run_persistent_startup(&startup, &fast_bounds(), &EmptyEnv);
    let mut supervisor = match result {
        PersistentStartupResult::Started { supervisor, .. } => supervisor,
        other @ PersistentStartupResult::Failed(_) => panic!("expected started, got {other:?}"),
    };
    assert_eq!(
        supervisor.candidate_count(),
        1,
        "one candidate published, before exit"
    );
    // The fixture exits 150ms after ready; wait past that.
    std::thread::sleep(Duration::from_millis(400));
    let health = supervisor.health();
    assert_eq!(health.len(), 1, "no candidate is respawned");
    match &health[0].health {
        CandidateHealth::Exited { .. } => {}
        other => panic!("expected exited (no restart), got {other:?}"),
    }
    // Still exactly one candidate: no auto-restart occurred.
    assert_eq!(supervisor.candidate_count(), 1);
    let shutdown = supervisor.shutdown();
    assert_eq!(shutdown.len(), 1);
}

#[test]
fn cw10_11_an_explicit_host_shutdown_reaps_every_candidate() {
    let _budget = super::persistent_support::process_budget();
    let scene = Scene::new();
    let startup = PersistentStartup {
        candidates: vec![
            scene.candidate(
                "vendor.alpha",
                "persistent-ready",
                vec![Capability::Actions],
            ),
            scene.candidate("vendor.zeta", "persistent-ready", vec![Capability::Actions]),
        ],
    };
    let result = run_persistent_startup(&startup, &fast_bounds(), &EmptyEnv);
    let mut supervisor = match result {
        PersistentStartupResult::Started { supervisor, .. } => supervisor,
        other @ PersistentStartupResult::Failed(_) => panic!("expected started, got {other:?}"),
    };
    let shutdown = supervisor.shutdown();
    assert_eq!(shutdown.len(), 2);
    assert!(
        shutdown.iter().all(|entry| entry.process_reaped),
        "every candidate is reaped on explicit host shutdown"
    );
    // A second shutdown is a no-op: the supervisor is already shut down.
    let again = supervisor.shutdown();
    assert!(again.is_empty(), "shutdown is idempotent");
}

#[test]
fn cw10_11_dropping_a_supervisor_without_shutdown_reaps_every_candidate() {
    let _budget = super::persistent_support::process_budget();
    let scene = Scene::new();
    let startup = PersistentStartup {
        candidates: vec![
            scene.candidate(
                "vendor.alpha",
                "persistent-ready",
                vec![Capability::Actions],
            ),
            scene.candidate("vendor.zeta", "persistent-ready", vec![Capability::Actions]),
        ],
    };
    let bounds = fast_bounds();
    let worst_case = bounds.shutdown_ack + bounds.stdin_close + bounds.final_drain;
    let result = run_persistent_startup(&startup, &bounds, &EmptyEnv);
    let (pids, supervisor) = match result {
        PersistentStartupResult::Started { supervisor, .. } => {
            let pids: Vec<u32> = supervisor
                .publication()
                .ready()
                .iter()
                .filter_map(|candidate| {
                    std::fs::read_to_string(
                        scene
                            .record_dir
                            .join(format!("{}.pid", candidate.plugin_id.as_str())),
                    )
                    .ok()
                    .and_then(|text| text.trim().parse::<u32>().ok())
                })
                .collect();
            (pids, supervisor)
        }
        other @ PersistentStartupResult::Failed(_) => panic!("expected started, got {other:?}"),
    };
    assert_eq!(pids.len(), 2, "each candidate recorded its pid");
    let start = Instant::now();
    drop(supervisor);
    let elapsed = start.elapsed();
    assert!(
        elapsed <= worst_case,
        "drop shutdown exceeded the aggregate bound: {elapsed:?} > {worst_case:?}"
    );
    // Every candidate process is gone after the bounded drop reap.
    for pid in pids {
        assert!(
            process_is_gone(pid),
            "candidate pid {pid} survived the drop reap (orphaned)"
        );
    }
}

#[test]
fn cw10_03_duplicate_candidate_plugin_ids_are_rejected_before_any_spawn() {
    let _budget = super::persistent_support::process_budget();
    let scene = Scene::new();
    let startup = PersistentStartup {
        candidates: vec![
            scene.candidate(
                "vendor.alpha",
                "persistent-ready",
                vec![Capability::Actions],
            ),
            scene.candidate(
                "vendor.alpha",
                "persistent-ready",
                vec![Capability::Actions],
            ),
        ],
    };
    let result = run_persistent_startup(&startup, &fast_bounds(), &EmptyEnv);
    let failure = expect_failed(result);
    match &failure.failure {
        StartupFailure::DuplicatePluginId { plugin_id } => {
            assert_eq!(plugin_id.as_str(), "vendor.alpha");
        }
        other @ StartupFailure::Candidate(_) => {
            panic!("expected duplicate plugin id, got {other:?}")
        }
    }
    assert!(
        failure.rollback.is_empty(),
        "duplicate ids are rejected before any spawn, so rollback is empty"
    );
    // No fixture recorded a startup: nothing was spawned.
    assert!(
        !scene.record_dir.join("startup-sequence.txt").exists(),
        "no candidate was spawned for a duplicate-id batch"
    );
}

/// Unwrap a failed result (panics on an unexpected Started with context).
fn expect_failed(result: PersistentStartupResult) -> PersistentStartupFailure {
    match result {
        PersistentStartupResult::Failed(failure) => failure,
        other @ PersistentStartupResult::Started { .. } => panic!("expected failed, got {other:?}"),
    }
}

//! Issue #390 CW-10 C2 remediation: redaction, cleanup evidence, strict ack,
//! fail-fast health, post-exit pipe closure, and the shutdown-frame write
//! failure. RED-first, no commit.

use std::time::{Duration, Instant};

use jefe::runtime::provider::persistent::{
    CandidateFailure, CandidateHealth, PersistentPhase, PersistentStartup, PersistentStartupResult,
    StartupFailure, run_persistent_startup,
};
use jefe::runtime::provider::protocol::Capability;
use jefe::runtime::provider::supervisor::{CleanupFailure, SupervisorFailure};

use super::support::{
    EmptyEnv, FixedEnv, SECRET, assert_all_reaped, candidate_pid, expect_supervisor, fast_bounds,
    process_is_gone, wait_until,
};

#[test]
fn cw10_14_a_persistent_startup_failure_redacts_a_secret_echoed_in_the_protocol_error() {
    let scene = super::support::Scene::new();
    // The fixture echoes the resolved Configure secret as an invalid `ready`
    // capability, so the host's parse fault carries the secret verbatim. The
    // supervisor must redact it from every operator-visible diagnostic surface.
    let startup = PersistentStartup {
        candidates: vec![scene.candidate_with_secret(
            "vendor.alpha",
            "persistent-secret-protocol",
            vec![Capability::Actions],
        )],
    };
    let result = run_persistent_startup(
        &startup,
        &fast_bounds(),
        &FixedEnv::from_pairs(&[("HOST_DEPLOY_KEY", SECRET)]),
    );
    let failure = match result {
        PersistentStartupResult::Failed(failure) => failure,
        other @ PersistentStartupResult::Started { .. } => panic!("expected failed, got {other:?}"),
    };
    assert!(
        matches!(
            &failure.failure,
            StartupFailure::Candidate(CandidateFailure {
                phase: PersistentPhase::Ready,
                failure: SupervisorFailure::Protocol(_),
                ..
            })
        ),
        "expected a ready-phase protocol fault, got {:?}",
        failure.failure
    );
    let debug = format!("{failure:?}");
    assert!(
        !debug.contains(SECRET),
        "secret leaked into the persistent startup-failure Debug: {debug}"
    );
    assert_all_reaped(&failure.rollback);
}

#[test]
#[cfg(unix)]
fn cw10_11_a_descendant_holding_pipes_surfaces_a_drain_timeout_not_a_clean_reap() {
    let scene = super::support::Scene::new();
    let startup = PersistentStartup {
        candidates: vec![scene.candidate(
            "vendor.alpha",
            "persistent-descendant-hang",
            vec![Capability::Actions],
        )],
    };
    let result = run_persistent_startup(&startup, &fast_bounds(), &EmptyEnv);
    let mut supervisor = expect_supervisor(result);
    let shutdown = supervisor.shutdown();
    assert_eq!(shutdown.len(), 1);
    let entry = &shutdown[0];
    assert!(
        !entry.process_reaped,
        "a descendant holding the pipes must not report a clean reap: {entry:?}"
    );
    match &entry.cleanup_failure {
        Some(CleanupFailure::DrainTimeout) => {}
        other => panic!("expected DrainTimeout cleanup failure, got {other:?}"),
    }
    // The leader was still killed even though the tree was not cleanly drained.
    assert!(
        process_is_gone(candidate_pid(&scene, "vendor.alpha")),
        "the leader was still killed/reaped despite the lingering descendant"
    );
}

#[test]
fn cw10_11_a_wrong_shutdown_ack_produces_a_cleanup_failure_while_still_reaping() {
    strict_ack_failure_produces_a_cleanup_failure_while_still_reaping("persistent-ack-wrong-kind");
}

#[test]
fn cw10_11_a_missing_shutdown_ack_produces_a_cleanup_failure_while_still_reaping() {
    strict_ack_failure_produces_a_cleanup_failure_while_still_reaping("persistent-ack-missing");
}

#[test]
fn cw10_11_an_eof_before_ack_produces_a_cleanup_failure_while_still_reaping() {
    strict_ack_failure_produces_a_cleanup_failure_while_still_reaping("persistent-ack-eof-before");
}

#[test]
fn cw10_11_data_after_ack_produces_a_cleanup_failure_while_still_reaping() {
    strict_ack_failure_produces_a_cleanup_failure_while_still_reaping("persistent-ack-data-after");
}

/// A healthy ready candidate that answers shutdown with a wrong/missing/malformed
/// ack (or sends data after the ack) produces a typed shutdown-ack cleanup
/// failure while still being killed and reaped.
fn strict_ack_failure_produces_a_cleanup_failure_while_still_reaping(mode: &str) {
    let scene = super::support::Scene::new();
    let startup = PersistentStartup {
        candidates: vec![scene.candidate("vendor.alpha", mode, vec![Capability::Actions])],
    };
    let result = run_persistent_startup(&startup, &fast_bounds(), &EmptyEnv);
    let mut supervisor = expect_supervisor(result);
    let pid = candidate_pid(&scene, "vendor.alpha");
    let shutdown = supervisor.shutdown();
    assert_eq!(shutdown.len(), 1);
    assert!(
        matches!(
            shutdown[0].cleanup_failure,
            Some(CleanupFailure::ShutdownAck(_))
        ),
        "expected a shutdown-ack cleanup failure for {mode:?}, got {:?}",
        shutdown[0].cleanup_failure
    );
    assert!(
        process_is_gone(pid),
        "the candidate was still killed/reaped for {mode:?} despite the ack fault"
    );
}

#[test]
fn cw10_04_a_ready_candidate_emitting_illegal_bytes_is_a_health_protocol_fault() {
    let scene = super::support::Scene::new();
    let startup = PersistentStartup {
        candidates: vec![scene.candidate(
            "vendor.alpha",
            "persistent-illegal-bytes",
            vec![Capability::Actions],
        )],
    };
    let result = run_persistent_startup(&startup, &fast_bounds(), &EmptyEnv);
    let mut supervisor = expect_supervisor(result);
    // The fixture emits an unsolicited frame after ready; the health probe must
    // mark it a protocol fault (fail-fast) rather than Ready.
    let deadline = Instant::now() + Duration::from_secs(3);
    let is_fault = wait_until(deadline, || {
        let health = supervisor.health();
        !matches!(health[0].health, CandidateHealth::Ready { .. })
    });
    assert!(is_fault, "the illegal bytes were never observed by health");
    let health = supervisor.health();
    match &health[0].health {
        CandidateHealth::ProtocolFault { .. } => {}
        other => panic!("expected a protocol fault, got {other:?}"),
    }
    // The faulted candidate is unavailable; shutdown still reaps it.
    let shutdown = supervisor.shutdown();
    assert_eq!(shutdown.len(), 1);
    assert!(process_is_gone(candidate_pid(&scene, "vendor.alpha")));
}

#[test]
fn cw10_11_an_already_exited_candidate_still_collects_pipe_closure_on_shutdown() {
    let scene = super::support::Scene::new();
    let startup = PersistentStartup {
        candidates: vec![scene.candidate(
            "vendor.alpha",
            "persistent-ready-then-exit",
            vec![Capability::Actions],
        )],
    };
    let result = run_persistent_startup(&startup, &fast_bounds(), &EmptyEnv);
    let mut supervisor = expect_supervisor(result);
    // Wait for the ready candidate to exit on its own.
    let deadline = Instant::now() + Duration::from_secs(3);
    let exited = wait_until(deadline, || {
        matches!(
            supervisor.health()[0].health,
            CandidateHealth::Exited { .. }
        )
    });
    assert!(exited, "the ready candidate never exited");
    // An already-reaped leader still collects bounded stdout/stderr closure; it
    // is never short-cut to a clean reap without observing the pipes close.
    let shutdown = supervisor.shutdown();
    assert_eq!(shutdown.len(), 1);
    assert!(
        shutdown[0].process_reaped,
        "the exited leader reaps cleanly: {:?}",
        shutdown[0]
    );
    assert!(
        shutdown[0].cleanup_failure.is_none(),
        "pipe closure was collected (no early reaped shortcut): {:?}",
        shutdown[0].cleanup_failure
    );
}

/// A healthy candidate that has exited (closing its stdin read end) before the
/// host signals shutdown must surface the failed shutdown write as typed I/O
/// cleanup evidence while still being killed and reaped. The candidate is left
/// marked alive/healthy (no `health()` probe) so the shutdown path attempts the
/// write rather than short-cutting an already-exited reap.
#[test]
fn cw10_11_a_shutdown_frame_write_failure_surfaces_a_typed_cleanup_failure_while_still_reaping() {
    let scene = super::support::Scene::new();
    let startup = PersistentStartup {
        candidates: vec![scene.candidate(
            "vendor.alpha",
            "persistent-ready-then-exit",
            vec![Capability::Actions],
        )],
    };
    let result = run_persistent_startup(&startup, &fast_bounds(), &EmptyEnv);
    let mut supervisor = expect_supervisor(result);
    // The fixture exits 150ms after ready. Let it exit on its own so its stdin
    // read end closes, deliberately WITHOUT probing health first (the candidate
    // stays alive/healthy, so shutdown attempts the write rather than the
    // already-exited reap short-cut).
    std::thread::sleep(Duration::from_millis(350));
    let pid = candidate_pid(&scene, "vendor.alpha");
    let shutdown = supervisor.shutdown();
    assert_eq!(shutdown.len(), 1);
    // The write to the closed stdin is typed I/O evidence, not silently dropped.
    match &shutdown[0].cleanup_failure {
        Some(CleanupFailure::Io(message)) => assert!(
            !message.contains(SECRET),
            "the cleanup evidence leaked the secret: {message}"
        ),
        other => panic!("expected a shutdown I/O cleanup failure, got {other:?}"),
    }
    assert!(
        process_is_gone(pid),
        "the candidate was still killed/reaped despite the write failure"
    );
}

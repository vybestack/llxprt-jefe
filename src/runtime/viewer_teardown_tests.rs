//! Behavioral coverage for the issue #664 attach/teardown serialization gate.

use super::attach::AttachedViewer;
use super::viewer_teardown::{VIEWER_TEARDOWN_WAIT, ViewerTeardown, viewer_teardown};
use std::sync::mpsc;
use std::time::{Duration, Instant};

/// Long enough to be unambiguous against scheduler jitter, short enough to keep
/// the suite fast.
const HOLD: Duration = Duration::from_millis(150);

/// A gate with nothing in flight must not delay an attach at all. The gate
/// exists to order teardown against spawn, not to add latency to the common
/// case where no viewer is being torn down.
#[test]
fn an_idle_gate_admits_a_spawn_immediately() {
    let gate = ViewerTeardown::new();

    let started = Instant::now();
    let became_idle = gate.wait_until_idle(VIEWER_TEARDOWN_WAIT);

    assert!(became_idle, "an idle gate must report itself idle");
    assert!(
        started.elapsed() < HOLD,
        "an idle gate delayed a spawn by {:?}",
        started.elapsed()
    );
}

/// The defect in issue #664: a spawn began while the previous viewer's teardown
/// was still running. The waiter must not be admitted until teardown releases.
#[test]
fn a_spawn_waits_for_an_in_flight_teardown_to_finish() {
    let gate = ViewerTeardown::new();
    let (teardown_released, observed) = mpsc::channel();

    std::thread::scope(|scope| {
        let guard = gate.begin();
        scope.spawn(move || {
            std::thread::sleep(HOLD);
            let released_at = Instant::now();
            drop(guard);
            let _ = teardown_released.send(released_at);
        });

        let became_idle = gate.wait_until_idle(VIEWER_TEARDOWN_WAIT);
        let admitted_at = Instant::now();

        assert!(became_idle, "the gate must go idle once teardown releases");
        let released_at = observed
            .recv_timeout(VIEWER_TEARDOWN_WAIT)
            .expect("the teardown thread must report when it released");
        assert!(
            admitted_at >= released_at,
            "the spawn was admitted before teardown released"
        );
    });
}

/// A teardown that never finishes must not freeze the UI. The wait is bounded,
/// and an expired wait reports failure rather than blocking forever.
#[test]
fn a_wedged_teardown_expires_the_bound_instead_of_blocking_forever() {
    let gate = ViewerTeardown::new();
    let guard = gate.begin();

    let started = Instant::now();
    let became_idle = gate.wait_until_idle(Duration::from_millis(50));
    let waited = started.elapsed();

    assert!(
        !became_idle,
        "a still-held teardown must not be reported idle"
    );
    assert!(
        waited < VIEWER_TEARDOWN_WAIT,
        "the bounded wait ran for {waited:?}, so it is not bounded"
    );
    drop(guard);
}

/// Both attach paths can be tearing viewers down at once, so the gate counts
/// teardowns rather than tracking a single one.
#[test]
fn the_gate_stays_closed_until_every_concurrent_teardown_releases() {
    let gate = ViewerTeardown::new();
    let first = gate.begin();
    let second = gate.begin();

    drop(first);
    assert!(
        !gate.wait_until_idle(Duration::from_millis(50)),
        "one of two teardowns released and the gate opened early"
    );

    drop(second);
    assert!(
        gate.wait_until_idle(VIEWER_TEARDOWN_WAIT),
        "the gate stayed closed after every teardown released"
    );
}

/// Wiring proof for the behavior issue #664 actually asks for: a viewer spawn
/// must consult the shared gate. Both attach paths funnel through
/// `spawn_command`, so proving one entry point waits proves the overlap that
/// produced the incident cannot recur.
///
/// The session name is deliberately absent; whether the attach then succeeds or
/// fails is irrelevant, because the gate is consulted before any of that.
#[test]
fn a_viewer_spawn_waits_for_a_teardown_held_on_the_shared_gate() {
    let started = Instant::now();

    // Measured inside the scope: `thread::scope` joins the holder on exit, so a
    // reading taken afterwards would always exceed HOLD and prove nothing.
    let waited = std::thread::scope(|scope| {
        let guard = viewer_teardown().begin();
        scope.spawn(move || {
            std::thread::sleep(HOLD);
            drop(guard);
        });

        drop(AttachedViewer::spawn(
            "jefe-issue664-absent-session",
            24,
            80,
        ));
        started.elapsed()
    });

    assert!(
        waited >= HOLD,
        "the spawn was admitted after {waited:?}, so it did not wait for the in-flight teardown"
    );
}

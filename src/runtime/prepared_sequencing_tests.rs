//! Observable call-log sequencing tests for the prepared-launch transaction
//! (issue #269).
//!
//! These tests exercise the SAME production sequencing functions used by the
//! force-fresh spawn path and the manager's prepared replacement transaction:
//!
//! - [`super::prepared_spawn::run_prepared_transaction`]: executes an ALREADY
//!   prepared transaction in exact order kill → delay → spawn.
//! - [`super::prepared_spawn::orchestrate_prepared`]: runs prepare first; a
//!   prepare `Err` yields no kill/delay/spawn (empty call log).
//!
//! No mock theater: the closures passed to these functions are observed via a
//! shared `Rc<RefCell<Vec<String>>>` call log, proving the exact ordering
//! invariants against the production code path. `orchestrate_prepared` is
//! generic over the prepared data type, so these tests use a trivial stub
//! (`String`) instead of a real `PreparedLaunch` (which requires live tmux).

use std::cell::RefCell;
use std::rc::Rc;

use super::errors::RuntimeError;
use super::prepared_spawn;

// ── run_prepared_transaction: kill → delay → spawn ordering ──────────────

/// Success path: the call log is exactly [kill, delay, spawn] in that order.
#[test]
fn run_prepared_transaction_success_exact_kill_delay_spawn() {
    let log = Rc::new(RefCell::new(Vec::<String>::new()));

    let result = prepared_spawn::run_prepared_transaction(
        || {
            log.borrow_mut().push("kill".to_owned());
            Ok(())
        },
        || log.borrow_mut().push("delay".to_owned()),
        || {
            log.borrow_mut().push("spawn".to_owned());
            Ok(())
        },
    );

    assert!(result.is_ok(), "success must return Ok");
    assert_eq!(
        log.borrow().as_slice(),
        ["kill", "delay", "spawn"],
        "success must invoke kill → delay → spawn exactly"
    );
}

/// Kill failure policy: a kill `Err` is logged but does NOT abort the spawn.
/// The call log is still [kill, delay, spawn] — the kill error is tolerated.
#[test]
fn run_prepared_transaction_kill_failure_still_spawns() {
    let log = Rc::new(RefCell::new(Vec::<String>::new()));

    let result = prepared_spawn::run_prepared_transaction(
        || {
            log.borrow_mut().push("kill".to_owned());
            Err(RuntimeError::KillFailed("no session".to_owned()))
        },
        || log.borrow_mut().push("delay".to_owned()),
        || {
            log.borrow_mut().push("spawn".to_owned());
            Ok(())
        },
    );

    assert!(
        result.is_ok(),
        "kill failure must not abort spawn; expected Ok, got {result:?}"
    );
    assert_eq!(
        log.borrow().as_slice(),
        ["kill", "delay", "spawn"],
        "kill failure must still reach delay + spawn"
    );
}

/// Spawn failure: kill and delay still run, then spawn returns its error. The
/// call log is exactly [kill, delay, spawn].
#[test]
fn run_prepared_transaction_spawn_failure_exact_sequence() {
    let log = Rc::new(RefCell::new(Vec::<String>::new()));

    let result = prepared_spawn::run_prepared_transaction(
        || {
            log.borrow_mut().push("kill".to_owned());
            Ok(())
        },
        || log.borrow_mut().push("delay".to_owned()),
        || {
            log.borrow_mut().push("spawn".to_owned());
            Err(RuntimeError::SpawnFailed("boom".to_owned()))
        },
    );

    assert!(
        matches!(result, Err(RuntimeError::SpawnFailed(_))),
        "spawn error must propagate, got {result:?}"
    );
    assert_eq!(
        log.borrow().as_slice(),
        ["kill", "delay", "spawn"],
        "spawn failure must still invoke kill → delay → spawn"
    );
}

// ── orchestrate_prepared: prepare Err yields empty call log ──────────────

/// A prepare `Err` yields a provably empty call log — no kill, no delay, no
/// spawn is invoked.
#[test]
fn orchestrate_prepared_prepare_failure_empty_call_log() {
    let log = Rc::new(RefCell::new(Vec::<String>::new()));
    let log_kill = Rc::clone(&log);
    let log_delay = Rc::clone(&log);
    let log_spawn = Rc::clone(&log);

    let result: Result<(), RuntimeError> = prepared_spawn::orchestrate_prepared(
        || Err(RuntimeError::SpawnFailed("preflight failure".to_owned())),
        |_prepared: &String| {
            log_kill.borrow_mut().push("kill".to_owned());
            Ok(())
        },
        || log_delay.borrow_mut().push("delay".to_owned()),
        |_prepared: &String| {
            log_spawn.borrow_mut().push("spawn".to_owned());
            Ok(())
        },
    );

    assert!(result.is_err(), "prepare failure must propagate as Err");
    assert!(
        log.borrow().is_empty(),
        "prepare failure must yield empty call log (no kill/delay/spawn), got {:?}",
        log.borrow()
    );
}

/// A successful prepare followed by kill → delay → spawn produces the exact
/// call log [prepare, kill, delay, spawn].
#[test]
fn orchestrate_prepared_success_full_sequence() {
    let log = Rc::new(RefCell::new(Vec::<String>::new()));
    let log_kill = Rc::clone(&log);
    let log_delay = Rc::clone(&log);
    let log_spawn = Rc::clone(&log);

    let result: Result<(), RuntimeError> = prepared_spawn::orchestrate_prepared(
        || {
            log.borrow_mut().push("prepare".to_owned());
            Ok("prepared-stub".to_owned())
        },
        |_prepared: &String| {
            log_kill.borrow_mut().push("kill".to_owned());
            Ok(())
        },
        || log_delay.borrow_mut().push("delay".to_owned()),
        |_prepared: &String| {
            log_spawn.borrow_mut().push("spawn".to_owned());
            Ok(())
        },
    );

    assert!(result.is_ok(), "full success must return Ok");
    assert_eq!(
        log.borrow().as_slice(),
        ["prepare", "kill", "delay", "spawn"],
        "full success must invoke prepare → kill → delay → spawn exactly"
    );
}

/// A successful prepare followed by a spawn failure still runs kill and delay.
/// The call log is [prepare, kill, delay, spawn] and the spawn error propagates.
#[test]
fn orchestrate_prepared_spawn_failure_runs_kill_and_delay() {
    let log = Rc::new(RefCell::new(Vec::<String>::new()));
    let log_kill = Rc::clone(&log);
    let log_delay = Rc::clone(&log);
    let log_spawn = Rc::clone(&log);

    let result: Result<(), RuntimeError> = prepared_spawn::orchestrate_prepared(
        || {
            log.borrow_mut().push("prepare".to_owned());
            Ok("prepared-stub".to_owned())
        },
        |_prepared: &String| {
            log_kill.borrow_mut().push("kill".to_owned());
            Ok(())
        },
        || log_delay.borrow_mut().push("delay".to_owned()),
        |_prepared: &String| {
            log_spawn.borrow_mut().push("spawn".to_owned());
            Err(RuntimeError::SpawnFailed(
                "post-kill spawn failed".to_owned(),
            ))
        },
    );

    assert!(
        matches!(result, Err(RuntimeError::SpawnFailed(_))),
        "spawn error must propagate, got {result:?}"
    );
    assert_eq!(
        log.borrow().as_slice(),
        ["prepare", "kill", "delay", "spawn"],
        "spawn failure must still run prepare → kill → delay → spawn"
    );
}

/// The kill failure policy inside orchestrate: a kill `Err` does not abort the
/// spawn, so the call log is [prepare, kill, delay, spawn].
#[test]
fn orchestrate_prepared_kill_failure_still_spawns() {
    let log = Rc::new(RefCell::new(Vec::<String>::new()));
    let log_kill = Rc::clone(&log);
    let log_delay = Rc::clone(&log);
    let log_spawn = Rc::clone(&log);

    let result: Result<(), RuntimeError> = prepared_spawn::orchestrate_prepared(
        || {
            log.borrow_mut().push("prepare".to_owned());
            Ok("prepared-stub".to_owned())
        },
        |_prepared: &String| {
            log_kill.borrow_mut().push("kill".to_owned());
            Err(RuntimeError::KillFailed("stale session".to_owned()))
        },
        || log_delay.borrow_mut().push("delay".to_owned()),
        |_prepared: &String| {
            log_spawn.borrow_mut().push("spawn".to_owned());
            Ok(())
        },
    );

    assert!(
        result.is_ok(),
        "kill failure must not abort spawn; expected Ok, got {result:?}"
    );
    assert_eq!(
        log.borrow().as_slice(),
        ["prepare", "kill", "delay", "spawn"],
        "kill failure must still reach prepare → kill → delay → spawn"
    );
}

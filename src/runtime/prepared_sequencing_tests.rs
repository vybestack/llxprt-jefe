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

    assert!(
        result.is_ok(),
        "success must return Ok(Success), got {result:?}"
    );
    assert_eq!(
        result.unwrap_or_else(|(phase, _)| panic!("expected Ok(Success), got Err({phase:?})")),
        prepared_spawn::PreparedTransactionPhase::Success,
        "success must return the Success phase"
    );
    assert_eq!(
        log.borrow().as_slice(),
        ["kill", "delay", "spawn"],
        "success must invoke kill → delay → spawn exactly"
    );
    run_prepared_transaction_kill_failure_aborts_spawn();
}

/// Kill failure policy for the STRICT replacement transaction: a kill `Err`
/// MUST abort the spawn and propagate the kill error. The call log is
/// exactly [kill] — no delay, no spawn is invoked. This is the replacement /
/// restart path where a half-dead old session must not race a new spawn
/// (issue #269).
fn run_prepared_transaction_kill_failure_aborts_spawn() {
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

    let (phase, error) = match result {
        Err((phase, error)) => (phase, error),
        Ok(phase) => panic!("kill failure must return Err, got Ok({phase:?})"),
    };
    assert!(
        matches!(error, RuntimeError::KillFailed(_)),
        "strict replacement kill failure must propagate and abort spawn, got {error:?}"
    );
    assert_eq!(
        phase,
        prepared_spawn::PreparedTransactionPhase::Kill,
        "kill failure must report the Kill phase"
    );
    assert_eq!(
        log.borrow().as_slice(),
        ["kill"],
        "strict replacement kill failure must NOT reach delay or spawn"
    );
    strict_vs_best_effort_kill_error_handling_diverge();
}

/// Distinction proof: the STRICT path (`run_prepared_transaction`) propagates
/// kill errors, while the BEST-EFFORT path (`orchestrate_prepared`) tolerates
/// them. Both observe the same kill-error scenario; the strict path returns
/// `Err` and stops, the best-effort path returns `Ok` and continues to spawn.
/// This is the single focused test that distinguishes both policies.
fn strict_vs_best_effort_kill_error_handling_diverge() {
    // ── Strict path: kill error aborts ──
    let strict_log = Rc::new(RefCell::new(Vec::<String>::new()));
    let strict_result = prepared_spawn::run_prepared_transaction(
        || {
            strict_log.borrow_mut().push("kill".to_owned());
            Err(RuntimeError::KillFailed(
                "replacement target dead".to_owned(),
            ))
        },
        || strict_log.borrow_mut().push("delay".to_owned()),
        || {
            strict_log.borrow_mut().push("spawn".to_owned());
            Ok(())
        },
    );
    let (strict_phase, strict_error) = match strict_result {
        Err((phase, error)) => (phase, error),
        Ok(phase) => panic!("strict path must propagate kill error, got Ok({phase:?})"),
    };
    assert!(
        matches!(strict_error, RuntimeError::KillFailed(_)),
        "strict path must propagate kill error"
    );
    assert_eq!(
        strict_phase,
        prepared_spawn::PreparedTransactionPhase::Kill,
        "strict path must report Kill phase"
    );
    assert_eq!(
        strict_log.borrow().as_slice(),
        &["kill"],
        "strict path must not reach delay/spawn on kill failure"
    );

    // ── Best-effort path: kill error tolerated, spawn proceeds ──
    let best_effort_log = Rc::new(RefCell::new(Vec::<String>::new()));
    let log_kill = Rc::clone(&best_effort_log);
    let log_delay = Rc::clone(&best_effort_log);
    let log_spawn = Rc::clone(&best_effort_log);
    let best_effort_result: Result<(), RuntimeError> = prepared_spawn::orchestrate_prepared(
        || {
            best_effort_log.borrow_mut().push("prepare".to_owned());
            Ok("stub".to_owned())
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
        best_effort_result.is_ok(),
        "best-effort path must tolerate kill failure, got {best_effort_result:?}"
    );
    assert_eq!(
        best_effort_log.borrow().as_slice(),
        &["prepare", "kill", "delay", "spawn"],
        "best-effort path must still reach spawn on kill failure"
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

    let (phase, error) = match result {
        Err((phase, error)) => (phase, error),
        Ok(phase) => panic!("spawn failure must return Err, got Ok({phase:?})"),
    };
    assert!(
        matches!(error, RuntimeError::SpawnFailed(_)),
        "spawn error must propagate, got {error:?}"
    );
    assert_eq!(
        phase,
        prepared_spawn::PreparedTransactionPhase::Spawn,
        "spawn failure must report the Spawn phase"
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
// ── PreparedTransactionPhase policy helpers ──────────────────────────────

/// The phase policy helpers enforce the runtime-map and dead-signature
/// invariants the manager relies on for each transaction outcome.
#[test]
fn prepared_transaction_phase_removes_old_mapping_only_after_kill() {
    use prepared_spawn::PreparedTransactionPhase as P;
    // Kill failure: old session may be alive → preserve mapping.
    assert!(
        !P::Kill.removes_old_mapping(),
        "kill failure must NOT remove the old mapping"
    );
    // Spawn failure: kill succeeded, old session gone → remove stale mapping.
    assert!(
        P::Spawn.removes_old_mapping(),
        "spawn failure must remove the stale mapping"
    );
    // Success: old session replaced → remove old mapping.
    assert!(
        P::Success.removes_old_mapping(),
        "success must remove the old mapping"
    );
}

/// The dead relaunch signature is preserved ONLY on spawn failure (kill
/// succeeded, new session could not be created → agent should be
/// relaunchable from its stored signature).
#[test]
fn prepared_transaction_phase_preserves_dead_signature_only_on_spawn_failure() {
    use prepared_spawn::PreparedTransactionPhase as P;
    assert!(
        !P::Kill.preserves_dead_signature(),
        "kill failure must NOT preserve a dead signature (old session may be alive)"
    );
    assert!(
        P::Spawn.preserves_dead_signature(),
        "spawn failure must preserve the dead relaunch signature"
    );
    assert!(
        !P::Success.preserves_dead_signature(),
        "success must NOT preserve a dead signature (new session is live)"
    );
}

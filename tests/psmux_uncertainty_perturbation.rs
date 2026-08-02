//! E4 perturbation suite against a real psmux (jefe issue #541, V8).
//!
//! Every other test in this issue reasons about uncertainty with a fabricated
//! observation. That proves the classifier, not the probe: a hand-built
//! `Unavailable` cannot show that a real psmux, perturbed in a realistic way,
//! actually produces one. These tests perturb a live server and assert the
//! invariant end to end.
//!
//! Each case is written as a contrast, because the invariant has two failure
//! directions and pinning only one of them is how "fail closed" quietly became
//! "never closed" earlier in this issue:
//!
//! * a question that was **answered** must still reach a verdict -- otherwise
//!   holding everything forever would pass a one-sided suite;
//! * a question that could **not be asked** must not reach one.
//!
//! Skips rather than fails when psmux is absent, unless `JEFE_REQUIRE_PSMUX`
//! is set, which is how the other psmux suites here behave.

#![cfg(all(windows, feature = "psmux-smoke"))]

use std::cell::RefCell;
use std::path::PathBuf;
use std::process::{Command, Output};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

use jefe::domain::WorkerProcessIdentity;
use jefe::runtime::{
    LocalPlatform, MultiplexerIsolation, MultiplexerPlan, ServerLivenessObservation,
    WorkerDisposition, observe_server_liveness, observe_worker_disposition,
};

static NAMESPACE_SEQUENCE: AtomicU64 = AtomicU64::new(0);

struct ServerCleanup {
    plan: MultiplexerPlan,
}

impl Drop for ServerCleanup {
    fn drop(&mut self) {
        let _ = self.plan.command().arg("kill-server").status();
    }
}

fn psmux_executable() -> PathBuf {
    std::env::var_os("JEFE_PSMUX_BIN")
        .filter(|value| !value.is_empty())
        .map_or_else(|| PathBuf::from("psmux"), PathBuf::from)
}

fn psmux_test_context() -> Option<(MultiplexerPlan, ServerCleanup)> {
    let executable = psmux_executable();
    if Command::new(&executable).arg("-V").output().is_err() {
        assert!(
            std::env::var_os("JEFE_REQUIRE_PSMUX").is_none(),
            "psmux is required but {} is unavailable",
            executable.display()
        );
        return None;
    }
    let plan = match MultiplexerPlan::for_platform(
        LocalPlatform::Windows,
        executable,
        MultiplexerIsolation::Namespace(unique_namespace()),
    ) {
        Ok(plan) => plan,
        Err(error) => panic!("construct psmux plan: {error}"),
    };
    let cleanup = ServerCleanup { plan: plan.clone() };
    Some((plan, cleanup))
}

/// A plan built against a genuine psmux that is then removed from underneath
/// it -- the E4 "rebuild" perturbation.
///
/// Neither shortcut works here, and both failures are informative:
/// `for_platform` rejects a path that does not exist, and it also rejects a
/// real executable that is not official psmux (the qualification gate from
/// #540). So the only way to reach a probe that cannot get an answer is to
/// pass qualification honestly and *then* take the binary away, which is
/// exactly what a rebuild or reinstall does to a running jefe.
///
/// Returns `None` when the copy cannot be made, so the suite skips rather
/// than reporting a false failure on a machine where the temporary directory
/// is not writable.
fn plan_whose_binary_disappears() -> Option<MultiplexerPlan> {
    // Qualification requires the file to be named exactly `psmux.exe`, so the
    // copy is uniquified by its directory rather than by its filename.
    let directory = std::env::temp_dir().join(unique_namespace());
    std::fs::create_dir_all(&directory).ok()?;
    let copy = directory.join("psmux.exe");
    std::fs::copy(resolved_psmux_path()?, &copy).ok()?;

    let plan = match MultiplexerPlan::for_platform(
        LocalPlatform::Windows,
        copy.clone(),
        MultiplexerIsolation::Namespace(unique_namespace()),
    ) {
        Ok(plan) => plan,
        Err(error) => panic!("a copy of the real psmux must qualify: {error}"),
    };

    // The plan is now valid and points at nothing.
    std::fs::remove_file(&copy).ok()?;
    Some(plan)
}

/// Resolve psmux to a real file, following `PATH` when the configured value is
/// a bare command name.
///
/// `psmux_executable` may legitimately return just `psmux`, which names a
/// program but not a file. Copying it would fail, and because the copy helper
/// reports failure by skipping, that silently turned this test into a no-op
/// locally while it ran for real in CI. Resolving the path first is what makes
/// the test actually execute wherever psmux exists.
fn resolved_psmux_path() -> Option<PathBuf> {
    let configured = psmux_executable();
    if configured.components().count() > 1 {
        return configured.exists().then_some(configured);
    }
    let output = Command::new("where").arg(&configured).output().ok()?;
    if !output.status.success() {
        return None;
    }
    let listing = String::from_utf8_lossy(&output.stdout);
    let first = listing.lines().next()?.trim();
    let path = PathBuf::from(first);
    path.exists().then_some(path)
}

fn unique_namespace() -> String {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |duration| duration.as_nanos());
    let sequence = NAMESPACE_SEQUENCE.fetch_add(1, Ordering::Relaxed);
    format!("jefe-issue541-{}-{nanos}-{sequence}", std::process::id())
}

fn run(plan: &MultiplexerPlan, args: &[&str]) -> Output {
    match plan.command().args(args).output() {
        Ok(output) => output,
        Err(error) => panic!("psmux {args:?} could not start: {error}"),
    }
}

fn assert_success(output: &Output, operation: &str) {
    assert!(
        output.status.success(),
        "{operation} failed: status={} stderr={}",
        output.status,
        String::from_utf8_lossy(&output.stderr).trim()
    );
}

/// Perturbation: the server jefe was talking to is destroyed and a different
/// one takes its place under the same namespace.
///
/// This is the case that produced phantom agents. The replacement answers
/// every query perfectly well -- it simply knows nothing about the sessions
/// the previous server owned, so "healthy server, no such session" is exactly
/// how a live agent gets buried.
#[test]
fn a_replaced_server_is_not_mistaken_for_the_one_we_were_watching() {
    let Some((plan, _cleanup)) = psmux_test_context() else {
        return;
    };
    assert_success(
        &run(&plan, &["new-session", "-d", "-s", "perturb-a"]),
        "create the original server",
    );

    let applied = RefCell::new(None);
    let baseline = observe_server_liveness(&plan, None, &applied);
    let ServerLivenessObservation::Healthy(Some(original)) = baseline else {
        panic!("expected a healthy first observation, got {baseline:?}");
    };

    assert_success(&run(&plan, &["kill-server"]), "destroy the original server");
    assert_success(
        &run(&plan, &["new-session", "-d", "-s", "perturb-b"]),
        "start a replacement server in the same namespace",
    );

    let after = observe_server_liveness(&plan, Some(&original), &applied);
    assert_ne!(
        after,
        ServerLivenessObservation::Healthy(Some(original.clone())),
        "a replacement server must not be reported as the server we were watching"
    );
    match after {
        ServerLivenessObservation::Healthy(Some(replacement)) => assert_ne!(
            replacement, original,
            "the replacement must be distinguishable from the original"
        ),
        ServerLivenessObservation::Gone | ServerLivenessObservation::Replaced { .. } => {}
        other => panic!("a replaced server must be observable as such, got {other:?}"),
    }
}

/// Perturbation: the multiplexer binary is removed while jefe is using it.
///
/// The result must be "I could not get an answer", never "I asked and there
/// was nothing there". This is the shape of the very first collapse in this
/// issue, where a failed scan returned an empty inventory that read as every
/// agent being gone.
#[test]
fn a_multiplexer_that_vanished_yields_uncertainty_not_an_empty_answer() {
    if psmux_test_context().is_none() {
        return;
    }
    // Once psmux is known to exist, an unbuildable plan is a fault in this
    // test rather than a reason to skip. Skipping here is how this case
    // silently became a no-op locally while it ran for real in CI.
    let plan = plan_whose_binary_disappears()
        .unwrap_or_else(|| panic!("psmux is available, so the vanishing-binary plan must build"));
    let applied = RefCell::new(None);

    let observation = observe_server_liveness(&plan, None, &applied);

    assert_ne!(
        observation,
        ServerLivenessObservation::Healthy(None),
        "a probe that could not run must not be reported as a healthy empty server"
    );
    assert!(
        !matches!(observation, ServerLivenessObservation::Healthy(Some(_))),
        "a probe that could not run must not manufacture a server identity: {observation:?}"
    );
}

/// The contrast that stops the suite being one-sided: a server that really is
/// gone must still be answered, not held.
///
/// Without this, holding unconditionally would satisfy every other assertion
/// here while leaving jefe unable to ever notice a death.
#[test]
fn a_server_that_really_is_gone_is_still_answered() {
    let Some((plan, _cleanup)) = psmux_test_context() else {
        return;
    };
    assert_success(
        &run(&plan, &["new-session", "-d", "-s", "perturb-gone"]),
        "create a server to then destroy",
    );

    let applied = RefCell::new(None);
    let baseline = observe_server_liveness(&plan, None, &applied);
    let ServerLivenessObservation::Healthy(Some(original)) = baseline else {
        panic!("expected a healthy observation before the kill, got {baseline:?}");
    };

    assert_success(&run(&plan, &["kill-server"]), "destroy the server");

    assert_eq!(
        observe_server_liveness(&plan, Some(&original), &applied),
        ServerLivenessObservation::Gone,
        "a genuinely absent server must reach a verdict, or nothing can ever be reaped"
    );
}

/// Perturbation: a worker anchor recorded without a start token, probed while
/// the process is unambiguously alive.
///
/// A token-less anchor cannot rule out PID reuse, so the honest answer is
/// "unverifiable". Reporting it gone would bury a live agent; reporting it
/// alive would be the mirror, and both were real defects in this issue.
#[test]
fn a_worker_anchor_that_cannot_be_verified_is_not_reported_gone() {
    let live_but_unverifiable = WorkerProcessIdentity::from_pid(std::process::id());

    let disposition = observe_worker_disposition(&[live_but_unverifiable]);

    assert_ne!(
        disposition,
        WorkerDisposition::GoneWithPane,
        "an anchor that cannot be verified must not be reported as a dead worker"
    );
}

/// The mirror of the previous case: an anchor naming a PID that cannot be
/// running must not be reported as a surviving worker.
#[test]
fn an_impossible_worker_anchor_is_not_reported_as_surviving() {
    // PID 0 is never a live user process on Windows.
    let impossible = WorkerProcessIdentity::from_pid(0);

    let disposition = observe_worker_disposition(&[impossible]);

    assert_ne!(
        disposition,
        WorkerDisposition::SurvivedPane,
        "an anchor that cannot name a live process must not be reported as surviving"
    );
}

#![cfg(all(windows, feature = "psmux-smoke"))]

use std::cell::RefCell;
use std::path::PathBuf;
use std::process::{Command, Output};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

use jefe::runtime::{
    LocalPlatform, MultiplexerIsolation, MultiplexerPlan, ServerLivenessObservation,
    observe_server_liveness,
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

fn psmux_test_context() -> Option<(MultiplexerPlan, ServerCleanup)> {
    let executable = std::env::var_os("JEFE_PSMUX_BIN")
        .filter(|value| !value.is_empty())
        .map_or_else(|| PathBuf::from("psmux"), PathBuf::from);
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

fn unique_namespace() -> String {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |duration| duration.as_nanos());
    let sequence = NAMESPACE_SEQUENCE.fetch_add(1, Ordering::Relaxed);
    format!("jefe-issue493-{}-{nanos}-{sequence}", std::process::id())
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

#[test]
fn native_psmux_server_loss_is_observed_without_empty_inventory_reconciliation() {
    let Some((plan, _cleanup)) = psmux_test_context() else {
        return;
    };
    assert_success(
        &run(&plan, &["new-session", "-d", "-s", "issue493-a"]),
        "create first server session",
    );

    let applied = RefCell::new(None);
    let baseline = observe_server_liveness(&plan, None, &applied);
    let ServerLivenessObservation::Healthy(Some(first)) = baseline.clone() else {
        panic!("expected a healthy first server observation, got {baseline:?}");
    };
    let option = run(&plan, &["show-options", "-s", "exit-empty"]);
    assert_success(&option, "read exit-empty after observation");
    assert!(
        String::from_utf8_lossy(&option.stdout).contains("exit-empty off"),
        "observer must disable exit-empty once for the server identity"
    );
    assert_eq!(
        observe_server_liveness(&plan, Some(&first), &applied),
        baseline
    );
    assert_eq!(*applied.borrow(), Some(first.clone()));

    assert_success(&run(&plan, &["kill-server"]), "terminate owned server");
    assert_eq!(
        observe_server_liveness(&plan, Some(&first), &applied),
        ServerLivenessObservation::Gone
    );

    assert_success(
        &run(&plan, &["new-session", "-d", "-s", "issue493-b"]),
        "create replacement server session",
    );
    let replacement = observe_server_liveness(&plan, Some(&first), &applied);
    let ServerLivenessObservation::Replaced(second) = replacement else {
        panic!("expected replacement server observation, got {replacement:?}");
    };
    assert_ne!(second.process, first.process);
    assert_eq!(*applied.borrow(), Some(second.clone()));
}

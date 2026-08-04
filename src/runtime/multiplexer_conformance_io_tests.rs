//! Behavioral contracts for the conformance runner's namespace lifetime
//! (issue #613).
//!
//! Conformance runs in a throwaway multiplexer namespace. The server that
//! namespace brings up outlives jefe, so a run that ends anywhere other than
//! its last line strands that server forever. These tests hold the runner to
//! tearing the namespace down along the unwinding path, not only the happy one.

use super::multiplexer_conformance_io::{SCRATCH_SESSION, ScratchNamespace, execute_probe};

#[cfg(unix)]
use super::multiplexer::{LocalPlatform, MultiplexerIsolation, MultiplexerPlan};

/// A stand-in multiplexer that records the arguments it was invoked with.
///
/// The teardown of a namespace is only observable as a command the runner
/// issued, so the probe path is pointed at a recorder rather than a real
/// multiplexer: that keeps the test deterministic and leaves no server behind
/// even when the assertion it makes is the one that fails.
#[cfg(unix)]
fn recording_multiplexer(directory: &std::path::Path, log: &std::path::Path) -> std::path::PathBuf {
    use std::os::unix::fs::PermissionsExt;

    let executable = directory.join("tmux");
    let script = format!("#!/bin/sh\nprintf '%s\\n' \"$*\" >> {}\n", log.display());
    std::fs::write(&executable, script)
        .unwrap_or_else(|error| panic!("recorder must be writable: {error}"));
    let permissions = std::fs::Permissions::from_mode(0o755);
    std::fs::set_permissions(&executable, permissions)
        .unwrap_or_else(|error| panic!("recorder must be executable: {error}"));
    executable
}

#[cfg(unix)]
#[test]
fn a_scratch_namespace_is_torn_down_when_the_run_unwinds() {
    let directory = tempfile::tempdir()
        .unwrap_or_else(|error| panic!("temporary directory must be available: {error}"));
    let log = directory.path().join("invocations.log");
    let executable = recording_multiplexer(directory.path(), &log);
    let plan = MultiplexerPlan::for_platform(
        LocalPlatform::Unix,
        executable,
        MultiplexerIsolation::Socket(directory.path().join("jefe.sock")),
    )
    .unwrap_or_else(|error| panic!("recorder plan must be valid: {error}"));

    let unwound = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let Some(scratch) = ScratchNamespace::reserve(&plan) else {
            panic!("the recorder plan must yield a scratch namespace");
        };
        let _ = execute_probe(
            scratch.plan(),
            &[
                "new-session".to_owned(),
                "-d".to_owned(),
                "-s".to_owned(),
                SCRATCH_SESSION.to_owned(),
            ],
        );
        panic!("a probe exploded mid-run");
    }));

    assert!(unwound.is_err(), "the panic must still reach the caller");
    let recorded = std::fs::read_to_string(&log)
        .unwrap_or_else(|error| panic!("the recorder log must be readable: {error}"));
    assert!(
        recorded.contains("new-session"),
        "the scratch namespace must have been brought up: {recorded}"
    );
    assert!(
        recorded.contains("kill-server"),
        "the unwinding run must still tear its namespace down: {recorded}"
    );
}

/// Whether this environment promises a usable psmux.
///
/// CI sets `JEFE_REQUIRE_PSMUX` on the native Windows job. Where it is set, a
/// missing binary is a failure rather than a reason to skip -- a test that
/// quietly does nothing is how a broken runner survives a green build.
#[cfg(windows)]
fn psmux_is_required() -> bool {
    std::env::var("JEFE_REQUIRE_PSMUX").is_ok_and(|value| value == "1")
}

#[cfg(windows)]
#[test]
fn a_scratch_namespace_server_does_not_outlive_an_unwinding_run() {
    use super::multiplexer::MultiplexerPlan;

    if !psmux_is_required() {
        return;
    }
    let plan = match MultiplexerPlan::current_for_test() {
        Ok(plan) => plan,
        Err(error) => panic!("JEFE_REQUIRE_PSMUX is set but no multiplexer resolved: {error}"),
    };

    let observed: std::sync::Mutex<Option<(MultiplexerPlan, Option<i32>)>> =
        std::sync::Mutex::new(None);
    let unwound = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let Some(scratch) = ScratchNamespace::reserve(&plan) else {
            panic!("the resolved plan must yield a scratch namespace");
        };
        let _ = execute_probe(
            scratch.plan(),
            &[
                "new-session".to_owned(),
                "-d".to_owned(),
                "-s".to_owned(),
                SCRATCH_SESSION.to_owned(),
            ],
        );
        let started = execute_probe(
            scratch.plan(),
            &[
                "has-session".to_owned(),
                "-t".to_owned(),
                SCRATCH_SESSION.to_owned(),
            ],
        );
        if let Ok(mut slot) = observed.lock() {
            *slot = Some((scratch.plan().clone(), started.exit_code));
        }
        panic!("a probe exploded mid-run");
    }));

    assert!(unwound.is_err(), "the panic must still reach the caller");
    let Some((scratch, started)) = observed.lock().ok().and_then(|slot| slot.clone()) else {
        panic!("the scratch namespace must have been recorded before the unwind");
    };
    assert_eq!(
        started,
        Some(0),
        "the scratch server must have been running before the unwind"
    );
    let surviving = execute_probe(
        &scratch,
        &[
            "has-session".to_owned(),
            "-t".to_owned(),
            SCRATCH_SESSION.to_owned(),
        ],
    );
    assert_ne!(
        surviving.exit_code,
        Some(0),
        "the scratch server must not have survived the unwind"
    );
}

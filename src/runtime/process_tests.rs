//! Behavioral contracts for process-instance liveness.

use std::process::{Command, Stdio};
use std::thread;
use std::time::Duration;

use super::process::{
    ProcessLiveness, ProcessObservation, capture_process_identity, classify_process_observation,
    process_liveness,
};
use crate::domain::ProcessIdentity;

#[test]
fn classification_distinguishes_every_required_state() {
    let expected = ProcessIdentity::new(41, 900);
    assert_eq!(
        classify_process_observation(Some(expected), ProcessObservation::Running(expected)),
        ProcessLiveness::Alive
    );
    assert_eq!(
        classify_process_observation(Some(expected), ProcessObservation::Exited),
        ProcessLiveness::Dead
    );
    assert_eq!(
        classify_process_observation(Some(expected), ProcessObservation::Inaccessible),
        ProcessLiveness::Inaccessible
    );
    assert_eq!(
        classify_process_observation(
            Some(expected),
            ProcessObservation::Running(ProcessIdentity::new(41, 901))
        ),
        ProcessLiveness::ReusedPid
    );
    assert_eq!(
        classify_process_observation(None, ProcessObservation::Running(expected)),
        ProcessLiveness::MalformedIdentity
    );
    assert_eq!(
        classify_process_observation(Some(expected), ProcessObservation::ProbeFailed),
        ProcessLiveness::ProbeFailure
    );
}

#[test]
fn production_probe_observes_running_and_normal_exit() {
    let mut child = spawn_sleeping_fixture(Duration::from_millis(120));
    let identity = capture_process_identity(child.id())
        .unwrap_or_else(|error| panic!("capture child process identity: {error}"));
    assert_eq!(process_liveness(Some(identity)), ProcessLiveness::Alive);
    let status = child
        .wait()
        .unwrap_or_else(|error| panic!("wait for child fixture: {error}"));
    assert!(status.success());
    assert_eq!(process_liveness(Some(identity)), ProcessLiveness::Dead);
}

#[test]
fn production_probe_observes_forced_termination() {
    let mut child = spawn_sleeping_fixture(Duration::from_secs(10));
    let identity = capture_process_identity(child.id())
        .unwrap_or_else(|error| panic!("capture child process identity: {error}"));
    child
        .kill()
        .unwrap_or_else(|error| panic!("terminate child fixture: {error}"));
    let _status = child
        .wait()
        .unwrap_or_else(|error| panic!("reap child fixture: {error}"));
    assert_eq!(process_liveness(Some(identity)), ProcessLiveness::Dead);
}

fn spawn_sleeping_fixture(duration: Duration) -> std::process::Child {
    let executable =
        std::env::current_exe().unwrap_or_else(|error| panic!("resolve test executable: {error}"));
    Command::new(executable)
        .args(["--exact", "runtime::process_tests::native_sleep_fixture"])
        .env(
            "JEFE_PROCESS_TEST_SLEEP_MS",
            duration.as_millis().to_string(),
        )
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .unwrap_or_else(|error| panic!("spawn process fixture: {error}"))
}

#[test]
fn native_sleep_fixture() {
    let Some(milliseconds) = std::env::var_os("JEFE_PROCESS_TEST_SLEEP_MS") else {
        return;
    };
    let milliseconds = milliseconds
        .to_string_lossy()
        .parse::<u64>()
        .unwrap_or_else(|error| panic!("parse fixture duration: {error}"));
    thread::sleep(Duration::from_millis(milliseconds));
}

//! Behavioral contracts for process-instance liveness.

use std::process::{Command, Stdio};
use std::thread;
use std::time::Duration;

use super::process::{
    ProcessLiveness, ProcessObservation, capture_process_identity, classify_process_observation,
    process_liveness, process_liveness_indicates_alive,
};
#[cfg(unix)]
use super::process::{UnixProbeOutcome, classify_unix_probe, unix_probe_command};
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
fn fail_open_policy_covers_every_final_liveness_state() {
    for liveness in [
        ProcessLiveness::Alive,
        ProcessLiveness::Inaccessible,
        ProcessLiveness::ProbeFailure,
    ] {
        assert!(process_liveness_indicates_alive(liveness));
    }
    for liveness in [
        ProcessLiveness::Dead,
        ProcessLiveness::ReusedPid,
        ProcessLiveness::MalformedIdentity,
    ] {
        assert!(!process_liveness_indicates_alive(liveness));
    }
}

#[test]
fn windows_access_denied_and_query_failure_remain_fail_open() {
    let expected = ProcessIdentity::new(41, 900);
    for observation in [
        ProcessObservation::Inaccessible,
        ProcessObservation::ProbeFailed,
    ] {
        let liveness = classify_process_observation(Some(expected), observation);
        assert!(process_liveness_indicates_alive(liveness));
    }

    let reused = classify_process_observation(
        Some(expected),
        ProcessObservation::Running(ProcessIdentity::new(41, 901)),
    );
    assert!(!process_liveness_indicates_alive(reused));
}

#[cfg(unix)]
#[test]
fn unix_probe_classifier_distinguishes_exit_access_and_failure() {
    assert_eq!(classify_unix_probe(true, ""), UnixProbeOutcome::Running);
    assert_eq!(
        classify_unix_probe(false, "kill: 41: Operation not permitted"),
        UnixProbeOutcome::Inaccessible
    );
    assert_eq!(
        classify_unix_probe(false, "kill: 41: Permission denied"),
        UnixProbeOutcome::Inaccessible
    );
    assert_eq!(
        classify_unix_probe(false, "kill: 41: No such process"),
        UnixProbeOutcome::Exited
    );
    assert_eq!(
        classify_unix_probe(false, "kill: unexpected diagnostic"),
        UnixProbeOutcome::ProbeFailed
    );
}

#[cfg(unix)]
#[test]
fn unix_probe_command_uses_structured_arguments_and_c_locale() {
    let command = unix_probe_command(41);
    let arguments: Vec<_> = command.get_args().collect();
    assert_eq!(command.get_program(), "kill");
    assert_eq!(arguments, ["-0", "41"]);
    assert!(
        command
            .get_envs()
            .any(|(key, value)| key == "LC_ALL" && value.is_some_and(|value| value == "C"))
    );
}

#[test]
fn platform_identity_without_persisted_creation_time_is_malformed() {
    let legacy = ProcessIdentity {
        pid: 41,
        started_at: None,
    };
    let observed = ProcessIdentity::new(41, 900);

    assert_eq!(
        classify_process_observation(Some(legacy), ProcessObservation::Running(observed)),
        ProcessLiveness::MalformedIdentity
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

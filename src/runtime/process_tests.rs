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
#[cfg(target_os = "macos")]
use super::process::{macos_start_time_command, parse_macos_process_start_time};
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
#[cfg(target_os = "macos")]
#[test]
fn macos_start_time_parser_returns_utc_epoch_and_rejects_malformed_values() {
    assert_eq!(
        parse_macos_process_start_time("Tue Jan  2 03:04:05 2024"),
        Some(1_704_164_645)
    );
    for malformed in [
        "",
        "Tue Jan 2 03:04 2024",
        "Tue Feb 30 03:04:05 2024",
        "Tue Jan 2 25:04:05 2024",
        "Nope Jan 2 03:04:05 2024",
        "Tue Jan 2 03:04:05 2024 extra",
    ] {
        assert_eq!(parse_macos_process_start_time(malformed), None);
    }
}

#[cfg(target_os = "macos")]
#[test]
fn macos_start_time_command_uses_structured_arguments_and_utc_locale() {
    let command = macos_start_time_command(41);
    let arguments: Vec<_> = command.get_args().collect();
    assert_eq!(command.get_program(), "ps");
    assert_eq!(arguments, ["-p", "41", "-o", "lstart="]);
    for (key, expected) in [("TZ", "UTC"), ("LC_ALL", "C")] {
        assert!(command.get_envs().any(|(actual, value)| {
            actual == key && value.is_some_and(|value| value == expected)
        }));
    }
}

#[cfg(target_os = "macos")]
#[test]
fn macos_identity_is_stable_across_parent_timezones() {
    let pid = std::process::id();
    let utc = capture_macos_identity_from_timezone(pid, "UTC");
    let pacific = capture_macos_identity_from_timezone(pid, "America/Los_Angeles");
    let tokyo = capture_macos_identity_from_timezone(pid, "Asia/Tokyo");
    assert_eq!(utc, pacific);
    assert_eq!(utc, tokyo);
}

#[cfg(target_os = "macos")]
fn capture_macos_identity_from_timezone(pid: u32, timezone: &str) -> u64 {
    let executable =
        std::env::current_exe().unwrap_or_else(|error| panic!("resolve test executable: {error}"));
    let result = tempfile::NamedTempFile::new()
        .unwrap_or_else(|error| panic!("create identity result file: {error}"));
    let status = Command::new(executable)
        .args([
            "--exact",
            "runtime::process_tests::macos_identity_capture_fixture",
        ])
        .env("TZ", timezone)
        .env("JEFE_PROCESS_TEST_PID", pid.to_string())
        .env("JEFE_PROCESS_TEST_RESULT", result.path())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .unwrap_or_else(|error| panic!("spawn identity fixture: {error}"));
    assert!(status.success(), "identity fixture failed for {timezone}");
    let value = std::fs::read_to_string(result.path())
        .unwrap_or_else(|error| panic!("read identity result: {error}"));
    value
        .parse()
        .unwrap_or_else(|error| panic!("parse identity result: {error}"))
}

#[cfg(target_os = "macos")]
#[test]
fn macos_identity_capture_fixture() {
    let (Some(pid), Some(result_path)) = (
        std::env::var_os("JEFE_PROCESS_TEST_PID"),
        std::env::var_os("JEFE_PROCESS_TEST_RESULT"),
    ) else {
        return;
    };
    let pid = pid
        .to_string_lossy()
        .parse::<u32>()
        .unwrap_or_else(|error| panic!("parse fixture PID: {error}"));
    let identity = capture_process_identity(pid)
        .unwrap_or_else(|error| panic!("capture fixture identity: {error}"));
    let Some(started_at) = identity.started_at else {
        panic!("macOS identity must include start time");
    };
    std::fs::write(result_path, started_at.to_string())
        .unwrap_or_else(|error| panic!("write identity result: {error}"));
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

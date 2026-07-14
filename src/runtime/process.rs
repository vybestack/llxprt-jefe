//! Typed local process-instance liveness service.

use crate::domain::ProcessIdentity;

/// Platform observation before comparison with persisted identity.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProcessObservation {
    Running(ProcessIdentity),
    Exited,
    Inaccessible,
    ProbeFailed,
}

/// Complete process-liveness classification used by restart reconciliation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProcessLiveness {
    Alive,
    Dead,
    Inaccessible,
    ReusedPid,
    MalformedIdentity,
    ProbeFailure,
}

/// Failure to capture a valid running process identity.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProcessIdentityError {
    classification: ProcessLiveness,
}

impl std::fmt::Display for ProcessIdentityError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            formatter,
            "process identity unavailable: {:?}",
            self.classification
        )
    }
}

impl std::error::Error for ProcessIdentityError {}

/// Pure comparison of persisted process identity and a fresh platform probe.
#[must_use]
pub const fn classify_process_observation(
    expected: Option<ProcessIdentity>,
    observation: ProcessObservation,
) -> ProcessLiveness {
    let Some(expected) = expected else {
        return ProcessLiveness::MalformedIdentity;
    };
    if expected.pid == 0 {
        return ProcessLiveness::MalformedIdentity;
    }
    match observation {
        ProcessObservation::Exited => ProcessLiveness::Dead,
        ProcessObservation::Inaccessible => ProcessLiveness::Inaccessible,
        ProcessObservation::ProbeFailed => ProcessLiveness::ProbeFailure,
        ProcessObservation::Running(actual) => classify_running(expected, actual),
    }
}

const fn classify_running(expected: ProcessIdentity, actual: ProcessIdentity) -> ProcessLiveness {
    if expected.pid != actual.pid {
        return ProcessLiveness::ReusedPid;
    }
    match (expected.started_at, actual.started_at) {
        (Some(expected), Some(actual)) if expected != actual => ProcessLiveness::ReusedPid,
        (Some(_), None) => ProcessLiveness::ProbeFailure,
        _ => ProcessLiveness::Alive,
    }
}

/// Capture the identity of a currently running process.
pub fn capture_process_identity(pid: u32) -> Result<ProcessIdentity, ProcessIdentityError> {
    match probe_process(pid) {
        ProcessObservation::Running(identity) => Ok(identity),
        observation => Err(ProcessIdentityError {
            classification: classify_process_observation(
                Some(ProcessIdentity {
                    pid,
                    started_at: None,
                }),
                observation,
            ),
        }),
    }
}

/// Classify one persisted process instance against the local operating system.
#[must_use]
pub fn process_liveness(identity: Option<ProcessIdentity>) -> ProcessLiveness {
    let Some(identity) = identity else {
        return ProcessLiveness::MalformedIdentity;
    };
    classify_process_observation(Some(identity), probe_process(identity.pid))
}

#[cfg(windows)]
fn probe_process(pid: u32) -> ProcessObservation {
    use winsafe::{HPROCESS, co};

    if pid == 0 {
        return ProcessObservation::ProbeFailed;
    }
    let access = co::PROCESS::QUERY_LIMITED_INFORMATION | co::PROCESS::SYNCHRONIZE;
    let process = match HPROCESS::OpenProcess(access, false, pid) {
        Ok(process) => process,
        Err(error) if error == co::ERROR::ACCESS_DENIED => {
            return ProcessObservation::Inaccessible;
        }
        Err(error) if error == co::ERROR::INVALID_PARAMETER => {
            return ProcessObservation::Exited;
        }
        Err(_) => return ProcessObservation::ProbeFailed,
    };
    match process.WaitForSingleObject(Some(0)) {
        Ok(wait) if wait == co::WAIT::OBJECT_0 => ProcessObservation::Exited,
        Ok(wait) if wait == co::WAIT::TIMEOUT => match process.GetProcessTimes() {
            Ok((creation, _, _, _)) => ProcessObservation::Running(ProcessIdentity {
                pid,
                started_at: Some(
                    (u64::from(creation.dwHighDateTime) << 32) | u64::from(creation.dwLowDateTime),
                ),
            }),
            Err(error) if error == co::ERROR::ACCESS_DENIED => ProcessObservation::Inaccessible,
            Err(_) => ProcessObservation::ProbeFailed,
        },
        Err(error) if error == co::ERROR::ACCESS_DENIED => ProcessObservation::Inaccessible,
        Ok(_) | Err(_) => ProcessObservation::ProbeFailed,
    }
}

#[cfg(unix)]
fn probe_process(pid: u32) -> ProcessObservation {
    if pid == 0 {
        return ProcessObservation::ProbeFailed;
    }
    match std::process::Command::new("kill")
        .arg("-0")
        .arg(pid.to_string())
        .status()
    {
        Ok(status) if status.success() => ProcessObservation::Running(ProcessIdentity {
            pid,
            started_at: unix_process_start_time(pid),
        }),
        Ok(_) => ProcessObservation::Exited,
        Err(_) => ProcessObservation::ProbeFailed,
    }
}

#[cfg(target_os = "linux")]
fn unix_process_start_time(pid: u32) -> Option<u64> {
    let stat = std::fs::read_to_string(format!("/proc/{pid}/stat")).ok()?;
    let command_end = stat.rfind(')')?;
    stat.get(command_end + 2..)?
        .split_whitespace()
        .nth(19)?
        .parse()
        .ok()
}

/// macOS process start token: a stable hash of the exact `ps -o lstart` output
/// for the process.
///
/// macOS does not expose `/proc`, so a process start token is obtained by
/// shelling out to `ps -o lstart= -p <pid>`, which prints the process start
/// time in a human-readable format (e.g. "Mon Jul 13 19:40:26 2026"). The
/// output is hashed into a compact `u64` start token via an FNV-1a-inspired
/// hash. While less precise than a kernel `kinfo_proc` read, this is
/// sufficient as a start token because `ps` reads the same kernel
/// `p_starttime` field — a recycled PID will have a different start time
/// and therefore a different hash.
///
/// The hash is stable across calls for the same input, and different start
/// times produce different hashes with high probability — sufficient for
/// PID-reuse detection.
#[cfg(target_os = "macos")]
fn unix_process_start_time(pid: u32) -> Option<u64> {
    let output = std::process::Command::new("ps")
        .args(["-o", "lstart=", "-p", &pid.to_string()])
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let start_str = String::from_utf8_lossy(&output.stdout);
    stable_hash(&start_str)
}

/// FNV-1a-inspired stable hash of a byte string into a `u64`.
///
/// Used to convert the human-readable `ps -o lstart` output into a compact
/// start token. The hash is stable across calls for the same input, and
/// different start times produce different hashes with high probability —
/// sufficient for PID-reuse detection.
#[cfg(target_os = "macos")]
fn stable_hash(input: &str) -> Option<u64> {
    let trimmed = input.trim();
    if trimmed.is_empty() {
        return None;
    }
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    for byte in trimmed.as_bytes() {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    }
    Some(hash)
}

#[cfg(all(unix, not(any(target_os = "linux", target_os = "macos"))))]
const fn unix_process_start_time(_pid: u32) -> Option<u64> {
    None
}

#[cfg(not(any(unix, windows)))]
fn probe_process(_pid: u32) -> ProcessObservation {
    ProcessObservation::ProbeFailed
}

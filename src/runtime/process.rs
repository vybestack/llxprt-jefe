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
        (None, Some(_)) => ProcessLiveness::MalformedIdentity,
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

#[must_use]
pub(super) const fn process_liveness_indicates_alive(liveness: ProcessLiveness) -> bool {
    matches!(
        liveness,
        ProcessLiveness::Alive | ProcessLiveness::Inaccessible | ProcessLiveness::ProbeFailure
    )
}

#[must_use]
pub(super) fn pid_liveness(pid: u32) -> ProcessLiveness {
    match probe_process(pid) {
        ProcessObservation::Running(_) => ProcessLiveness::Alive,
        ProcessObservation::Exited => ProcessLiveness::Dead,
        ProcessObservation::Inaccessible => ProcessLiveness::Inaccessible,
        ProcessObservation::ProbeFailed => ProcessLiveness::ProbeFailure,
    }
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
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum UnixProbeOutcome {
    Running,
    Exited,
    Inaccessible,
    ProbeFailed,
}

#[cfg(unix)]
#[must_use]
pub(super) fn classify_unix_probe(success: bool, stderr: &str) -> UnixProbeOutcome {
    if success {
        return UnixProbeOutcome::Running;
    }
    let diagnostic = stderr.to_ascii_lowercase();
    if diagnostic.contains("operation not permitted") || diagnostic.contains("permission denied") {
        UnixProbeOutcome::Inaccessible
    } else if diagnostic.contains("no such process") {
        UnixProbeOutcome::Exited
    } else {
        UnixProbeOutcome::ProbeFailed
    }
}

#[cfg(unix)]
pub(super) fn unix_probe_command(pid: u32) -> std::process::Command {
    let mut command = std::process::Command::new("kill");
    command.args(["-0", &pid.to_string()]).env("LC_ALL", "C");
    command
}

#[cfg(unix)]
fn probe_process(pid: u32) -> ProcessObservation {
    if pid == 0 {
        return ProcessObservation::ProbeFailed;
    }
    let Ok(output) = unix_probe_command(pid).output() else {
        return ProcessObservation::ProbeFailed;
    };
    match classify_unix_probe(
        output.status.success(),
        String::from_utf8_lossy(&output.stderr).as_ref(),
    ) {
        UnixProbeOutcome::Running => ProcessObservation::Running(ProcessIdentity {
            pid,
            started_at: unix_process_start_time(pid),
        }),
        UnixProbeOutcome::Exited => ProcessObservation::Exited,
        UnixProbeOutcome::Inaccessible => ProcessObservation::Inaccessible,
        UnixProbeOutcome::ProbeFailed => ProcessObservation::ProbeFailed,
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

#[cfg(target_os = "macos")]
pub(super) fn macos_start_time_command(pid: u32) -> std::process::Command {
    let mut command = std::process::Command::new("ps");
    command
        .args(["-p", &pid.to_string(), "-o", "lstart="])
        .env("TZ", "UTC")
        .env("LC_ALL", "C");
    command
}

#[cfg(target_os = "macos")]
fn unix_process_start_time(pid: u32) -> Option<u64> {
    let output = macos_start_time_command(pid).output().ok()?;
    if !output.status.success() {
        return None;
    }
    parse_macos_process_start_time(String::from_utf8_lossy(&output.stdout).trim())
}

#[cfg(target_os = "macos")]
#[must_use]
pub(super) fn parse_macos_process_start_time(value: &str) -> Option<u64> {
    let mut parts = value.split_whitespace();
    let weekday = parts.next()?;
    let month = parse_macos_month(parts.next()?)?;
    let day = parts.next()?.parse().ok()?;
    let (hour, minute, second) = parse_macos_clock(parts.next()?)?;
    let year = parts.next()?.parse().ok()?;
    if parts.next().is_some() || !is_macos_weekday(weekday) {
        return None;
    }
    utc_epoch_seconds(year, month, day, hour, minute, second)
}

#[cfg(target_os = "macos")]
fn is_macos_weekday(value: &str) -> bool {
    matches!(value, "Mon" | "Tue" | "Wed" | "Thu" | "Fri" | "Sat" | "Sun")
}

#[cfg(target_os = "macos")]
fn parse_macos_month(value: &str) -> Option<u32> {
    match value {
        "Jan" => Some(1),
        "Feb" => Some(2),
        "Mar" => Some(3),
        "Apr" => Some(4),
        "May" => Some(5),
        "Jun" => Some(6),
        "Jul" => Some(7),
        "Aug" => Some(8),
        "Sep" => Some(9),
        "Oct" => Some(10),
        "Nov" => Some(11),
        "Dec" => Some(12),
        _ => None,
    }
}

#[cfg(target_os = "macos")]
fn parse_macos_clock(value: &str) -> Option<(u32, u32, u32)> {
    let mut parts = value.split(':');
    let hour = parts.next()?.parse().ok()?;
    let minute = parts.next()?.parse().ok()?;
    let second = parts.next()?.parse().ok()?;
    if parts.next().is_some() || hour > 23 || minute > 59 || second > 59 {
        return None;
    }
    Some((hour, minute, second))
}

#[cfg(target_os = "macos")]
fn utc_epoch_seconds(
    year: u32,
    month: u32,
    day: u32,
    hour: u32,
    minute: u32,
    second: u32,
) -> Option<u64> {
    let days = days_since_unix_epoch(year, month, day)?;
    let seconds = days
        .checked_mul(86_400)?
        .checked_add(u64::from(hour).checked_mul(3_600)?)?
        .checked_add(u64::from(minute).checked_mul(60)?)?
        .checked_add(u64::from(second))?;
    Some(seconds)
}

#[cfg(target_os = "macos")]
fn days_since_unix_epoch(year: u32, month: u32, day: u32) -> Option<u64> {
    if year < 1970 || !(1..=12).contains(&month) || day == 0 || day > days_in_month(year, month) {
        return None;
    }
    let year = i64::from(year) - i64::from(month <= 2);
    let era = year.div_euclid(400);
    let year_of_era = year - era * 400;
    let adjusted_month = i64::from(month) + if month > 2 { -3 } else { 9 };
    let day_of_year = (153 * adjusted_month + 2) / 5 + i64::from(day) - 1;
    let day_of_era = year_of_era * 365 + year_of_era / 4 - year_of_era / 100 + day_of_year;
    u64::try_from(era * 146_097 + day_of_era - 719_468).ok()
}

#[cfg(target_os = "macos")]
const fn days_in_month(year: u32, month: u32) -> u32 {
    match month {
        2 if is_leap_year(year) => 29,
        2 => 28,
        4 | 6 | 9 | 11 => 30,
        _ => 31,
    }
}

#[cfg(target_os = "macos")]
const fn is_leap_year(year: u32) -> bool {
    year % 4 == 0 && (year % 100 != 0 || year % 400 == 0)
}

#[cfg(all(unix, not(any(target_os = "linux", target_os = "macos"))))]
const fn unix_process_start_time(_pid: u32) -> Option<u64> {
    None
}

#[cfg(not(any(unix, windows)))]
fn probe_process(_pid: u32) -> ProcessObservation {
    ProcessObservation::ProbeFailed
}

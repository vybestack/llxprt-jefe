//! Issue #467 Slice 4 fixture: a standalone pane host that owns a kill-on-close
//! Windows Job Object and a long-lived worker descendant.
//!
//! This fixture models the production `jefe.exe --jefe-internal-agent-launch`
//! pane-host entrypoint without depending on llxprt/network/package
//! availability. It is the unit under test for the native psmux lifecycle
//! regression in `tests/psmux_session_host.rs`.
//!
//! Modes:
//! - `--session-host <marker-path>`: create and own a kill-on-close Job Object,
//!   assign this process to it, spawn the worker child (inheriting the Job),
//!   record both PIDs plus a `host_owned_job` boolean to `<marker-path>`, then
//!   hold until killed. Host death closes the Job handle and the kernel reaps
//!   the contained worker tree (AC6).
//! - `--worker <marker-path>`: long-lived worker child. Records its own PID to
//!   the marker file, then blocks until killed. Detached from the host's
//!   console so it is not cascaded by console-close events; the Job is the
//!   only containment boundary under test.

#![cfg(feature = "psmux-smoke")]

use std::fs;
use std::io::Write;
use std::path::PathBuf;
use std::process::ExitCode;
use std::thread;
use std::time::Duration;

use serde::Serialize;

/// Host hold time: keeps the pane alive until the test kills it.
const HOST_HOLD: Duration = Duration::new(3600, 0);

#[derive(Serialize)]
struct HostMarker {
    host_pid: u32,
    worker_pid: u32,
    /// Whether the host established a kill-on-close Job Object owning the worker tree.
    host_owned_job: bool,
    started_at: Option<u64>,
}

#[derive(Serialize)]
struct WorkerRecord {
    worker_pid: u32,
    started_at: Option<u64>,
}

fn main() -> ExitCode {
    if let Err(error) = run() {
        let _ = writeln!(
            std::io::stderr(),
            "psmux session-host fixture failed: {error}"
        );
        return ExitCode::FAILURE;
    }
    ExitCode::SUCCESS
}

fn run() -> Result<(), Box<dyn std::error::Error>> {
    let mut args = std::env::args().skip(1);
    let mode = args.next().ok_or("missing mode argument")?;
    match mode.as_str() {
        "--session-host" => {
            let marker_path: PathBuf = args
                .next()
                .ok_or("--session-host requires a marker path")?
                .into();
            if args.next().is_some() {
                return Err("unexpected extra argument".into());
            }
            run_session_host(&marker_path)
        }
        "--worker" => {
            let marker_path: PathBuf = args.next().ok_or("--worker requires a marker path")?.into();
            if args.next().is_some() {
                return Err("unexpected extra argument".into());
            }
            run_worker(&marker_path)
        }
        other => Err(format!("unknown mode: {other}").into()),
    }
}

/// Pane-host mode: own a kill-on-close Job, spawn a contained worker, record
/// the PIDs, and hold until killed.
fn run_session_host(marker_path: &std::path::Path) -> Result<(), Box<dyn std::error::Error>> {
    // Establish Job containment exactly as the production private pane host
    // does (`run_launch_plan`). Host death closes the handle and reaps the tree.
    establish_host_job()?;

    let current_exe = std::env::current_exe()?;
    let mut child = std::process::Command::new(&current_exe);
    child.arg("--worker").arg(marker_path);
    child.stdin(std::process::Stdio::null());
    child.stdout(std::process::Stdio::null());
    child.stderr(std::process::Stdio::null());
    // CREATE_NEW_PROCESS_GROUP (0x00000200) | DETACHED_PROCESS (0x00000008):
    // detach the worker from this host's console so a console-close event does
    // not cascade-kill it. The Job is the only containment boundary under test.
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        child.creation_flags(0x0000_0208);
    }
    let mut child = child.spawn()?;
    let worker_pid = child.id();

    // Record the marker once the worker has spawned. The worker records its
    // own PID independently as a cross-check.
    let marker = HostMarker {
        host_pid: std::process::id(),
        worker_pid,
        host_owned_job: true,
        started_at: now_unix_seconds(),
    };
    fs::write(marker_path, serde_json::to_vec(&marker)?)?;

    // Hold the pane host alive until the test kills the session. Do not wait
    // on the child: we want host death (kill-session) to be the trigger, not
    // worker exit. Reap the child defensively to avoid a zombie if it exits
    // first, but keep holding regardless.
    let _ = child.try_wait();
    loop {
        thread::sleep(HOST_HOLD);
    }
}

/// Worker mode: record own PID into the marker's worker-pid slot (overwriting
/// only that field is unsafe across writers, so the worker writes its own
/// sidecar file `<marker>.worker`), then block until killed.
fn run_worker(marker_path: &std::path::Path) -> Result<(), Box<dyn std::error::Error>> {
    let worker_sidecar = marker_path.with_extension("worker.json");
    let record = WorkerRecord {
        worker_pid: std::process::id(),
        started_at: now_unix_seconds(),
    };
    fs::write(&worker_sidecar, serde_json::to_vec(&record)?)?;
    loop {
        thread::sleep(HOST_HOLD);
    }
}

#[cfg(windows)]
fn establish_host_job() -> Result<(), Box<dyn std::error::Error>> {
    // Reuse the production-safe win32job boundary by inlining the minimal
    // create/configure/assign sequence. This fixture must not link the
    // production `runtime::job_object` module (it lives behind the library's
    // private modules), and the win32job dependency is already approved and
    // present for cfg(windows).
    use win32job::Job;
    let job = Job::create()?;
    let mut info = job.query_extended_limit_info()?;
    info.limit_kill_on_job_close();
    job.set_extended_limit_info(&info)?;
    job.assign_current_process()?;
    // Leak the handle: this process owns the Job for its whole lifetime. Drop
    // would close the handle prematurely and defeat the test.
    std::mem::forget(job);
    Ok(())
}

#[cfg(not(windows))]
fn establish_host_job() -> Result<(), Box<dyn std::error::Error>> {
    // Unix is structurally out of scope for #467; the fixture is only built
    // under the psmux-smoke feature which is windows-gated.
    Ok(())
}

#[cfg(windows)]
fn now_unix_seconds() -> Option<u64> {
    use winsafe::{HPROCESS, co};
    let pid = std::process::id();
    let process = HPROCESS::OpenProcess(co::PROCESS::QUERY_LIMITED_INFORMATION, false, pid).ok()?;
    let (creation, _, _, _) = process.GetProcessTimes().ok()?;
    let raw = (u64::from(creation.dwHighDateTime) << 32) | u64::from(creation.dwLowDateTime);
    // Convert Windows FILETIME (100ns ticks since 1601) to Unix seconds.
    Some(raw.saturating_sub(116_444_736_000_000_000) / 10_000_000)
}

#[cfg(not(windows))]
fn now_unix_seconds() -> Option<u64> {
    None
}

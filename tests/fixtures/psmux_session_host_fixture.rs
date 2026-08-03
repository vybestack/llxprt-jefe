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
//! - `--owned-session-host <marker-path>`: issue #542. Everything
//!   `--session-host` does, but it first captures its owner chain with
//!   `jefe::runtime::capture_owner_anchor` and records those links in the
//!   marker, then arms `jefe::runtime::spawn_owner_watchdog`. This is the mode
//!   that exercises the owner-lifetime anchor itself.
//! - `--pane-launcher <marker-path>`: issue #542. Stands in for the pane
//!   process. It spawns `--owned-session-host` *detached* and then blocks
//!   forever as a stable, killable owner.
//!
//!   The detachment is what makes the regression meaningful. A host that shares
//!   the pane's console is torn down by ConPTY when the pane dies, whether or
//!   not the anchor works — so a console-attached fixture measures Windows, not
//!   Jefe, and stays green even with the watchdog deleted. A detached host also
//!   reproduces issue #515's actual topology, where the surviving hosts were
//!   never console-cascaded. With this mode the only thing that can reap the
//!   tree is the watchdog.

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

/// Issue #542 marker: the `HostMarker` fields plus the owner chain that was
/// captured before the worker was spawned.
#[derive(Serialize)]
struct OwnedHostMarker {
    host_pid: u32,
    worker_pid: u32,
    host_owned_job: bool,
    owner_links: Vec<OwnerLinkRecord>,
    started_at: Option<u64>,
}

#[derive(Serialize)]
struct OwnerLinkRecord {
    role: String,
    pid: u32,
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
        // Issue #542: identical to `--session-host` except that it captures its
        // owner chain before spawning the worker and then watches it, exactly
        // as `run_launch_plan` does in production.
        "--owned-session-host" => {
            let marker_path: PathBuf = args
                .next()
                .ok_or("--owned-session-host requires a marker path")?
                .into();
            if args.next().is_some() {
                return Err("unexpected extra argument".into());
            }
            run_owned_session_host(&marker_path)
        }
        // Issue #542: stand in for the pane process that owns the host.
        // Spawning the host detached is what makes the regression honest: a
        // console-attached host is reaped by ConPTY teardown whether or not
        // the owner anchor works, so such a test stays green with the
        // mechanism deleted. #515's surviving hosts were likewise not
        // console-cascaded.
        "--pane-launcher" => {
            let marker_path: PathBuf = args
                .next()
                .ok_or("--pane-launcher requires a marker path")?
                .into();
            if args.next().is_some() {
                return Err("unexpected extra argument".into());
            }
            run_pane_launcher(&marker_path)
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
    #[cfg(windows)]
    establish_host_job()?;

    let worker_pid = spawn_contained_worker(marker_path)?;

    // Record the marker once the worker has spawned. The worker records its
    // own PID independently as a cross-check.
    let marker = HostMarker {
        host_pid: std::process::id(),
        worker_pid,
        host_owned_job: cfg!(windows),
        started_at: now_unix_seconds(),
    };
    fs::write(marker_path, serde_json::to_vec(&marker)?)?;

    // Hold the pane host alive until the test kills the session. Do not wait
    // on the child: we want host death (kill-session) to be the trigger, not
    // worker exit.
    loop {
        thread::sleep(HOST_HOLD);
    }
}

/// Owner-anchored pane-host mode (issue #542).
///
/// Ordering is the behaviour under test and mirrors `run_launch_plan`:
/// capture the owner chain *before* the worker exists, so a worker can never
/// be spawned into a tree that has no anchor. Only then is Job containment
/// established and the worker spawned, and only then is the watchdog started.
#[cfg(windows)]
fn run_owned_session_host(marker_path: &std::path::Path) -> Result<(), Box<dyn std::error::Error>> {
    let anchor = jefe::runtime::capture_owner_anchor()?;
    let owner_links: Vec<OwnerLinkRecord> = anchor
        .links()
        .iter()
        .map(|link| OwnerLinkRecord {
            role: link.role.as_str().to_owned(),
            pid: link.identity.pid,
        })
        .collect();

    establish_host_job()?;
    let worker_pid = spawn_contained_worker(marker_path)?;

    let marker = OwnedHostMarker {
        host_pid: std::process::id(),
        worker_pid,
        host_owned_job: true,
        owner_links,
        started_at: now_unix_seconds(),
    };
    fs::write(marker_path, serde_json::to_vec(&marker)?)?;

    jefe::runtime::spawn_owner_watchdog(anchor);
    loop {
        thread::sleep(HOST_HOLD);
    }
}

#[cfg(not(windows))]
fn run_owned_session_host(
    _marker_path: &std::path::Path,
) -> Result<(), Box<dyn std::error::Error>> {
    Err("--owned-session-host is a Windows-only mode".into())
}
/// Pane-launcher mode (issue #542): act as the pane process that owns the
/// session host, and start that host *detached from this console*.
///
/// This is the whole point of the mode. A session host that shares the pane's
/// console is torn down by ConPTY when the pane dies, so a regression test
/// built on one passes even with the owner anchor deleted — it measures
/// Windows, not Jefe. #515's surviving hosts were not console-cascaded either,
/// so detaching is also the faithful topology: after the launcher dies the
/// host is reachable but unowned, and only the anchor can reap it.
///
/// The launcher then blocks, so it is a stable, killable owner for the test.
fn run_pane_launcher(marker_path: &std::path::Path) -> Result<(), Box<dyn std::error::Error>> {
    let current_exe = std::env::current_exe()?;
    let mut child = std::process::Command::new(&current_exe);
    child.arg("--owned-session-host").arg(marker_path);
    child.stdin(std::process::Stdio::null());
    child.stdout(std::process::Stdio::null());
    child.stderr(std::process::Stdio::null());
    // CREATE_NEW_PROCESS_GROUP (0x00000200) | DETACHED_PROCESS (0x00000008).
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        child.creation_flags(0x0000_0208);
    }
    let mut child = child.spawn()?;
    let _ = child.try_wait();
    loop {
        thread::sleep(HOST_HOLD);
    }
}

/// Spawn the long-lived worker child that the host's Job contains.
fn spawn_contained_worker(
    marker_path: &std::path::Path,
) -> Result<u32, Box<dyn std::error::Error>> {
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
    let _ = child.try_wait();
    Ok(worker_pid)
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

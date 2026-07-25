//! Deterministic fixture for the issue #332 orphan-reap regression.
//!
//! Mode `--orphan-leader`: spawns a long-lived child process (the simulated
//! LLxprt worker descendant), writes the child's PID + creation time to a
//! marker file, then sleeps until killed. This models the Windows/psmux
//! scenario where killing the pane leader leaves the descendant worker alive
//! as an orphan.
//!
//! The marker file lets the regression test capture a validated
//! `ProcessIdentity` anchor and later confirm the reap path terminates exactly
//! that descendant.

#![cfg(feature = "psmux-smoke")]

use std::fs;
use std::io::Write;
use std::path::PathBuf;
use std::process::ExitCode;
use std::thread;
use std::time::Duration;

#[derive(serde::Serialize)]
struct OrphanMarker {
    pid: u32,
    started_at: Option<u64>,
}

fn main() -> ExitCode {
    if let Err(error) = run() {
        let _ = writeln!(std::io::stderr(), "psmux orphan fixture failed: {error}");
        return ExitCode::FAILURE;
    }
    ExitCode::SUCCESS
}

fn run() -> Result<(), Box<dyn std::error::Error>> {
    let mut args = std::env::args().skip(1);
    let mode = args.next().ok_or("missing mode argument")?;
    match mode.as_str() {
        "--orphan-leader" => {
            let marker_path: PathBuf = args
                .next()
                .ok_or("--orphan-leader requires a marker path")?
                .into();
            spawn_orphan_leader(&marker_path)
        }
        "--orphan-child" => {
            // Long-lived child: block until killed. Writes nothing; the leader
            // records this PID before sleeping.
            loop {
                thread::sleep(Duration::from_secs(3600));
            }
        }
        other => Err(format!("unknown mode: {other}").into()),
    }
}

fn spawn_orphan_leader(marker_path: &std::path::Path) -> Result<(), Box<dyn std::error::Error>> {
    let current_exe = std::env::current_exe()?;
    let mut child = std::process::Command::new(&current_exe);
    child.arg("--orphan-child");
    child.stdin(std::process::Stdio::null());
    child.stdout(std::process::Stdio::null());
    child.stderr(std::process::Stdio::null());
    // Detach the child from this leader's process group / console so it
    // SURVIVES leader termination — this is exactly the orphan scenario from
    // issue #332 (dead pane leader, surviving worker descendant). Without
    // detachment, killing the leader cascades to the child and no orphan
    // exists to reap.
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        // CREATE_NEW_PROCESS_GROUP (0x00000200) | DETACHED_PROCESS (0x00000008)
        child.creation_flags(0x0000_0208);
    }
    let child = child.spawn()?;
    let pid = child.id();
    let started_at = start_time(pid);
    let marker = OrphanMarker { pid, started_at };
    fs::write(marker_path, serde_json::to_vec(&marker)?)?;
    // Keep the leader alive so the pane is considered alive until the test
    // kills it. The child outlives the leader (orphan scenario).
    loop {
        thread::sleep(Duration::from_secs(3600));
    }
}

// Minimal creation-time capture for the marker, isolated to this fixture.
#[cfg(windows)]
fn start_time(pid: u32) -> Option<u64> {
    use winsafe::{HPROCESS, co};
    let access = co::PROCESS::QUERY_LIMITED_INFORMATION;
    let process = HPROCESS::OpenProcess(access, false, pid).ok()?;
    let (creation, _, _, _) = process.GetProcessTimes().ok()?;
    Some((u64::from(creation.dwHighDateTime) << 32) | u64::from(creation.dwLowDateTime))
}

#[cfg(not(windows))]
fn start_time(_pid: u32) -> Option<u64> {
    None
}

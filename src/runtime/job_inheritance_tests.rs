//! Job containment through the path production uses (issue #542 S1a, #467).
//!
//! `job_object_tests` proves reaping via `contain_handle`: spawn a child,
//! assign *its handle* to the Job, drop the guard, watch it die. Production
//! does the opposite -- `enable_for_current_process` assigns *self*, and every
//! worker spawned afterwards is expected to **inherit** membership, so that
//! host death closes the handle and the kernel reaps the tree.
//!
//! Handle-assignment is tested. Inheritance is not, and inheritance is the half
//! production runs on. This file tests it, using the real host entrypoint:
//! `jefe.exe --jefe-internal-agent-launch <plan>` establishes the Job and
//! spawns the worker exactly as it does in a psmux pane.
//!
//! The host is killed **without** `/T`. Killing the tree would prove only that
//! `taskkill` works; killing the host alone means anything else that dies was
//! reaped by the kernel closing the Job.

use std::os::windows::process::CommandExt as _;
use std::path::PathBuf;
use std::time::{Duration, Instant};

use super::{AgentLaunchPayload, AgentWrapperKindPayload};

/// The kernel reaps on handle close, so this needs no polling grace beyond
/// process teardown.
const REAP_DEADLINE: Duration = Duration::from_secs(15);

/// No console, so console teardown cannot be mistaken for containment.
const DETACHED_PROCESS: u32 = 0x0000_0008;

const MARKER_PREFIX: &str = "jefe-job-inheritance-probe";

/// Passes: inheritance works, and this is the first test to say so.
///
/// It briefly appeared to fail, and that failure was an artefact of the probe
/// rather than the code. Counting processes with
/// `Get-CimInstance Win32_Process | Where-Object CommandLine -like '*marker*'`
/// matches the PowerShell process running the query, because the marker is in
/// its own command line. The count was therefore never below one, and a marker
/// matching nothing at all still returned one. Restricting to the worker's
/// image name fixes it, since the probe can never be `cmd.exe`.
///
/// Worth keeping despite passing: it closes the coverage gap that made the
/// wrong conclusion plausible in the first place. `job_object_tests` proves
/// reaping only through `contain_handle` -- spawn a child, assign *its handle*,
/// drop the guard -- while production assigns *self* and relies on spawned
/// workers inheriting membership. That inheritance is what every layer above
/// delegates its killing to, and until now nothing exercised it.
#[test]
fn a_worker_spawned_into_the_hosts_job_is_reaped_when_the_host_dies() {
    let Some(jefe) = jefe_binary() else {
        return;
    };

    let marker = format!("{MARKER_PREFIX}-{}-{}", std::process::id(), nanos());
    let plan_path = std::env::temp_dir().join(format!(
        "jefe-agent-launch-{}-{}-job.json",
        std::process::id(),
        nanos()
    ));
    std::fs::write(&plan_path, plan_bytes(&marker))
        .unwrap_or_else(|error| panic!("could not write launch plan: {error}"));

    let mut host = std::process::Command::new(&jefe)
        .arg("--jefe-internal-agent-launch")
        .arg(&plan_path)
        .creation_flags(DETACHED_PROCESS)
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn()
        .unwrap_or_else(|error| panic!("could not spawn the session host: {error}"));

    if !wait_for(REAP_DEADLINE, || marked_processes(&marker) > 0) {
        let _ = host.kill();
        let _ = host.wait();
        cleanup(&marker, &plan_path);
        panic!("the worker never started, so this proves nothing about containment");
    }

    // Kill the host only. Everything below it must now be reaped by the kernel
    // closing the Job the host owned.
    let _ = host.kill();
    let _ = host.wait();

    let reaped = wait_for(REAP_DEADLINE, || marked_processes(&marker) == 0);
    let survivors = marked_processes(&marker);
    cleanup(&marker, &plan_path);

    assert!(
        reaped,
        "{survivors} process(es) outlived the host that contained them. The host \
         assigns itself to a KILL_ON_JOB_CLOSE Job before spawning, so a worker \
         spawned afterwards should inherit that Job and be terminated when the \
         host dies. It was not, which means workers are not inheriting \
         containment -- the half of #467 that contain_handle never covered, and \
         the reason the owner anchor cannot reap anything (issue #542)."
    );
}

fn plan_bytes(marker: &str) -> Vec<u8> {
    let payload = AgentLaunchPayload {
        path: PathBuf::from("cmd"),
        wrapper: AgentWrapperKindPayload::Direct,
        script_launch: None,
        args: vec![
            "/D".into(),
            "/S".into(),
            "/C".into(),
            format!("ping -n 600 127.0.0.1 >nul & rem {marker}").into(),
        ],
        environment: Vec::new(),
        cwd: std::env::temp_dir(),
        worker_report: None,
    };
    serde_json::to_vec(&payload).unwrap_or_else(|error| panic!("payload should serialize: {error}"))
}

/// Count of live processes carrying `marker`.
///
/// CIM, not `wmic`: `wmic` is absent on current Windows and exits 255, so a
/// probe built on it reports nothing for every query and would declare the tree
/// reaped without ever looking.
fn marked_processes(marker: &str) -> usize {
    let script = format!(
        "@(Get-CimInstance Win32_Process | Where-Object {{ $_.Name -eq 'cmd.exe' -and $_.CommandLine -like '*{marker}*' }}).Count"
    );
    let Ok(output) = std::process::Command::new("powershell")
        .args(["-NoProfile", "-NonInteractive", "-Command", &script])
        .output()
    else {
        return 0;
    };
    String::from_utf8_lossy(&output.stdout)
        .trim()
        .parse()
        .unwrap_or(0)
}

fn cleanup(marker: &str, plan_path: &std::path::Path) {
    let script = format!(
        "Get-CimInstance Win32_Process | Where-Object {{ $_.Name -eq 'cmd.exe' -and $_.CommandLine -like '*{marker}*' }} | \
         ForEach-Object {{ Stop-Process -Id $_.ProcessId -Force -ErrorAction SilentlyContinue }}"
    );
    let _ = std::process::Command::new("powershell")
        .args(["-NoProfile", "-NonInteractive", "-Command", &script])
        .output();
    let _ = std::fs::remove_file(plan_path);
}

fn wait_for(limit: Duration, mut condition: impl FnMut() -> bool) -> bool {
    let deadline = Instant::now() + limit;
    loop {
        if condition() {
            return true;
        }
        if Instant::now() >= deadline {
            return false;
        }
        std::thread::sleep(Duration::from_millis(250));
    }
}

fn jefe_binary() -> Option<PathBuf> {
    let current = std::env::current_exe().ok()?;
    let candidate = current.parent()?.parent()?.join("jefe.exe");
    candidate.exists().then_some(candidate)
}

fn nanos() -> u128 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |value| value.as_nanos())
}

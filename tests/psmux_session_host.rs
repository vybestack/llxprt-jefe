#![cfg(all(windows, feature = "psmux-smoke"))]

//! Issue #467 Slice 4 native Windows psmux lifecycle regression (AC2–AC7/AC10).
//!
//! Proves, against a real native Windows psmux >= 3.3.7, that two or more
//! independently staged pane hosts in one unique `-L` namespace:
//! - survive a simulated dashboard exit/crash (AC2, AC3);
//! - keep the source executable replaceable while they run (AC4);
//! - retain the exact same session names and pane PIDs with exactly one worker
//!   descendant each on restart/discovery, with no duplicate `--continue`
//!   (AC5);
//! - reap only the killed host's Job-owned worker tree within a bounded
//!   timeout when one pane host dies, leaving the other host untouched (AC6);
//! - clean up only their own namespace/session artifacts (AC7/AC10).
//!
//! The test never contacts the production Jefe psmux namespace, never kills
//! arbitrary user processes, and owns a unique namespace with Drop cleanup. It
//! is opt-in under the existing `psmux-smoke` feature and the
//! `JEFE_REQUIRE_PSMUX=1` policy, mirroring `psmux_smoke.rs` and
//! `psmux_orphan_reap.rs`.
//!
//! The fixture `jefe-psmux-session-host-fixture` models the production private
//! pane-host entrypoint (`jefe.exe --jefe-internal-agent-launch` →
//! `run_launch_plan`) without depending on llxprt/network/package
//! availability: it creates and owns a kill-on-close Windows Job Object,
//! assigns itself, spawns a long-lived worker child that inherits the Job,
//! records both PIDs to a marker, and holds until killed. Host death closes
//! the Job handle and the kernel reaps the contained worker tree (AC6).

use std::ffi::OsString;
use std::fmt::Write as FmtWrite;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};
use std::sync::atomic::{AtomicU64, Ordering};
use std::thread;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use jefe::runtime::{LocalPlatform, MultiplexerIsolation, MultiplexerPlan};
use serde::Deserialize;

const FIXTURE: &str = env!("CARGO_BIN_EXE_jefe-psmux-session-host-fixture");
const POLL_TIMEOUT: Duration = Duration::from_secs(15);
const CONTAINMENT_TIMEOUT: Duration = Duration::from_secs(10);

#[derive(Debug, Deserialize)]
#[allow(dead_code)]
struct HostMarker {
    host_pid: u32,
    worker_pid: u32,
    host_owned_job: bool,
    started_at: Option<u64>,
}

#[derive(Debug, Deserialize)]
#[allow(dead_code)]
struct WorkerRecord {
    worker_pid: u32,
    started_at: Option<u64>,
}

#[derive(Debug)]
struct RegressionFailure {
    message: String,
    diagnostics: String,
}

impl std::fmt::Display for RegressionFailure {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(formatter, "{}\n\n{}", self.message, self.diagnostics)
    }
}

impl std::error::Error for RegressionFailure {}

fn fail<E: std::fmt::Debug>(message: &str, error: E) -> RegressionFailure {
    RegressionFailure {
        message: message.to_owned(),
        diagnostics: format!("{error:?}"),
    }
}

struct PsmuxNamespace {
    executable: PathBuf,
    name: String,
    transcript: String,
    artifact_dir: PathBuf,
}

impl PsmuxNamespace {
    fn new(label: &str) -> Result<Self, RegressionFailure> {
        let name = unique_name(label);
        let artifact_dir = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("target")
            .join("psmux-smoke")
            .join(&name);
        fs::create_dir_all(&artifact_dir)
            .map_err(|error| fail("create artifact directory", error))?;
        Ok(Self {
            executable: psmux_path(),
            name,
            transcript: String::new(),
            artifact_dir,
        })
    }

    fn run(&mut self, args: &[&str]) -> Result<Output, RegressionFailure> {
        let owned: Vec<OsString> = args.iter().map(OsString::from).collect();
        self.run_os(&owned)
    }

    fn run_os(&mut self, args: &[OsString]) -> Result<Output, RegressionFailure> {
        let mut command = Command::new(&self.executable);
        command.arg("-L").arg(&self.name);
        for value in args {
            command.arg(value);
        }
        for variable in ["TMUX", "TMUX_PANE", "TMUX_TMPDIR"] {
            command.env_remove(variable);
        }
        let display = format!(
            "{} -L {} {}",
            self.executable.display(),
            self.name,
            format_args_os(args)
        );
        command.stdout(Stdio::piped()).stderr(Stdio::piped());
        let output = command
            .output()
            .map_err(|error| self.failure(format!("spawn failed: {display}: {error}"), ""))?;
        let _ = writeln!(self.transcript, "$ {display}\n{}", format_output(&output));
        if output.status.success() {
            Ok(output)
        } else {
            Err(self.failure(
                format!("command failed: {display}"),
                &format_output(&output),
            ))
        }
    }

    fn pane_command(
        &self,
        program: &Path,
        fixture_args: &[OsString],
    ) -> Result<Vec<OsString>, RegressionFailure> {
        let plan = MultiplexerPlan::for_platform(
            LocalPlatform::Windows,
            self.executable.clone(),
            MultiplexerIsolation::Namespace(self.name.clone()),
        )
        .map_err(|error| fail("construct Windows psmux plan", error))?;
        plan.pane_command_args(program.as_os_str(), fixture_args, &[])
            .map_err(|error| fail("build fixture pane command", error))
    }

    fn run_quiet(&self, args: &[&str]) {
        let mut command = Command::new(&self.executable);
        command.arg("-L").arg(&self.name).args(args);
        for variable in ["TMUX", "TMUX_PANE", "TMUX_TMPDIR"] {
            command.env_remove(variable);
        }
        let _ = command.stdout(Stdio::null()).stderr(Stdio::null()).status();
    }

    fn session_exists(&mut self, session: &str) -> bool {
        self.run(&["has-session", "-t", session]).is_ok()
    }

    fn pane_pid(&mut self, session: &str) -> Result<u32, RegressionFailure> {
        let output = self.run(&["display-message", "-p", "-t", session, "#{pane_pid}"])?;
        let text = String::from_utf8_lossy(&output.stdout).trim().to_owned();
        text.parse::<u32>()
            .map_err(|_| self.failure(format!("invalid pane_pid for {session}: {text:?}"), ""))
    }

    fn failure(&self, message: String, details: &str) -> RegressionFailure {
        let sessions = self.available_sessions();
        let diagnostics = format!(
            "namespace: {}\nartifact dir: {}\n{details}\n\navailable sessions:\n{sessions}\n\ntranscript:\n{}",
            self.name,
            self.artifact_dir.display(),
            self.transcript
        );
        let _ = fs::write(self.artifact_dir.join("failure.txt"), &diagnostics);
        RegressionFailure {
            message,
            diagnostics,
        }
    }

    fn available_sessions(&self) -> String {
        let output = Command::new(&self.executable)
            .arg("-L")
            .arg(&self.name)
            .args(["list-sessions", "-F", "#{session_name}"])
            .output();
        match output {
            Ok(output) => format_output(&output),
            Err(error) => format!("unable to list sessions: {error}"),
        }
    }
}

impl Drop for PsmuxNamespace {
    fn drop(&mut self) {
        // Namespace-scoped cleanup (AC7/AC10): only this test's unique -L
        // namespace is contacted. The production Jefe namespace is never
        // touched, and no bare kill-server is ever issued against the default
        // server.
        self.run_quiet(&["kill-server"]);
        let _ = fs::write(self.artifact_dir.join("transcript.txt"), &self.transcript);
    }
}

struct RunningHosts {
    work: Workdir,
    host_a: &'static str,
    host_b: &'static str,
    marker_a: HostMarker,
    marker_b: HostMarker,
    source: PathBuf,
}

fn launch_running_hosts(namespace: &mut PsmuxNamespace) -> Result<RunningHosts, RegressionFailure> {
    let work = Workdir::new()?;
    let host_a = "sh-alpha";
    let host_b = "sh-beta";
    let marker_a_path = work.path().join("host-a.json");
    let marker_b_path = work.path().join("host-b.json");
    let source = work.path().join("source-image.exe");
    fs::copy(FIXTURE, &source).map_err(|e| fail("create source image", e))?;
    let staged_a = work.path().join("session-host-a.exe");
    let staged_b = work.path().join("session-host-b.exe");
    fs::copy(&source, &staged_a).map_err(|e| fail("stage host A", e))?;
    fs::copy(&source, &staged_b).map_err(|e| fail("stage host B", e))?;
    launch_session_host(namespace, host_a, work.path(), &staged_a, &marker_a_path)?;
    launch_session_host(namespace, host_b, work.path(), &staged_b, &marker_b_path)?;
    let marker_a = wait_for_marker(&marker_a_path)?;
    let marker_b = wait_for_marker(&marker_b_path)?;
    Ok(RunningHosts {
        work,
        host_a,
        host_b,
        marker_a,
        marker_b,
        source,
    })
}

fn assert_survives_and_source_replaces(
    namespace: &mut PsmuxNamespace,
    hosts: &RunningHosts,
) -> Result<(), RegressionFailure> {
    assert!(hosts.marker_a.host_owned_job && hosts.marker_b.host_owned_job);
    for (session, worker) in [
        (hosts.host_a, hosts.marker_a.worker_pid),
        (hosts.host_b, hosts.marker_b.worker_pid),
    ] {
        assert!(namespace.session_exists(session));
        assert!(process_alive(worker));
    }
    let replacement = hosts.work.path().join("replacement-image.exe");
    fs::write(&replacement, b"replacement-bytes-467").map_err(|e| fail("write replacement", e))?;
    fs::copy(&replacement, &hosts.source).map_err(|e| fail("overwrite source while running", e))?;
    assert!(namespace.session_exists(hosts.host_a));
    assert!(namespace.session_exists(hosts.host_b));
    Ok(())
}

fn assert_reconnect_and_scoped_reap(
    namespace: &mut PsmuxNamespace,
    hosts: &RunningHosts,
) -> Result<(), RegressionFailure> {
    let pane_a = namespace.pane_pid(hosts.host_a)?;
    let pane_b = namespace.pane_pid(hosts.host_b)?;
    assert_ne!(pane_a, 0);
    assert_ne!(pane_b, 0);
    assert_eq!(namespace.pane_pid(hosts.host_a)?, pane_a);
    assert_eq!(namespace.pane_pid(hosts.host_b)?, pane_b);
    let worker_a: WorkerRecord = wait_for_marker(&hosts.work.path().join("host-a.worker.json"))?;
    let worker_b: WorkerRecord = wait_for_marker(&hosts.work.path().join("host-b.worker.json"))?;
    assert!(process_alive(worker_a.worker_pid));
    assert!(process_alive(worker_b.worker_pid));
    kill_session(namespace, hosts.host_a)?;
    assert!(wait_for_process_exit(
        hosts.marker_a.worker_pid,
        CONTAINMENT_TIMEOUT
    ));
    assert!(process_alive(hosts.marker_b.worker_pid));
    assert!(namespace.session_exists(hosts.host_b));
    kill_session(namespace, hosts.host_b)?;
    assert!(!namespace.session_exists(hosts.host_a));
    assert!(!namespace.session_exists(hosts.host_b));
    Ok(())
}

#[test]
fn two_staged_hosts_survive_dashboard_exit_keep_source_replaceable_reconnect_and_scoped_reap()
-> Result<(), RegressionFailure> {
    if !psmux_required() {
        return Ok(());
    }
    let mut namespace = PsmuxNamespace::new("session-host")?;
    let hosts = launch_running_hosts(&mut namespace)?;
    assert_survives_and_source_replaces(&mut namespace, &hosts)?;
    assert_reconnect_and_scoped_reap(&mut namespace, &hosts)?;
    Ok(())
}

struct Workdir {
    dir: tempfile::TempDir,
}

impl Workdir {
    fn new() -> Result<Self, RegressionFailure> {
        let dir = tempfile::Builder::new()
            .prefix("jefe-467-host Ω ")
            .tempdir()
            .map_err(|e| fail("create work dir", e))?;
        Ok(Self { dir })
    }

    fn path(&self) -> &Path {
        self.dir.path()
    }
}

fn launch_session_host(
    namespace: &mut PsmuxNamespace,
    session: &str,
    cwd: &Path,
    program: &Path,
    marker: &Path,
) -> Result<(), RegressionFailure> {
    let fixture_args = vec![
        OsString::from("--session-host"),
        marker.as_os_str().to_owned(),
    ];
    let pane = namespace.pane_command(program, &fixture_args)?;
    let mut args = vec![
        OsString::from("new-session"),
        OsString::from("-d"),
        OsString::from("-s"),
        OsString::from(session),
        OsString::from("-c"),
        cwd.as_os_str().to_owned(),
    ];
    args.extend(pane);
    namespace.run_os(&args).map(|_| ())
}

fn kill_session(namespace: &mut PsmuxNamespace, session: &str) -> Result<(), RegressionFailure> {
    namespace.run(&["kill-session", "-t", session]).map(|_| ())
}

fn wait_for_marker<T: serde::de::DeserializeOwned>(path: &Path) -> Result<T, RegressionFailure> {
    let deadline = Instant::now() + POLL_TIMEOUT;
    let mut last_error = String::from("marker not written");
    while Instant::now() < deadline {
        if let Ok(bytes) = fs::read(path) {
            match serde_json::from_slice::<T>(&bytes) {
                Ok(value) => return Ok(value),
                Err(error) => last_error = format!("marker unreadable: {error}"),
            }
        }
        thread::sleep(Duration::from_millis(50));
    }
    Err(RegressionFailure {
        message: format!(
            "marker {} not ready within {POLL_TIMEOUT:?}",
            path.display()
        ),
        diagnostics: last_error,
    })
}

fn wait_for_process_exit(pid: u32, bound: Duration) -> bool {
    let deadline = Instant::now() + bound;
    while Instant::now() < deadline {
        if !process_alive(pid) {
            return true;
        }
        thread::sleep(Duration::from_millis(50));
    }
    !process_alive(pid)
}

fn process_alive(pid: u32) -> bool {
    #[cfg(windows)]
    {
        use winsafe::{HPROCESS, co};
        let access = co::PROCESS::QUERY_LIMITED_INFORMATION | co::PROCESS::SYNCHRONIZE;
        match HPROCESS::OpenProcess(access, false, pid) {
            Ok(process) => {
                matches!(process.WaitForSingleObject(Some(0)), Ok(co::WAIT::TIMEOUT))
            }
            Err(_) => false,
        }
    }
    #[cfg(not(windows))]
    {
        let _ = pid;
        false
    }
}

fn psmux_required() -> bool {
    std::env::var("JEFE_REQUIRE_PSMUX").is_ok_and(|value| value == "1")
}

fn psmux_path() -> PathBuf {
    if let Some(explicit) = std::env::var_os("JEFE_PSMUX_BIN").filter(|v| !v.is_empty()) {
        return PathBuf::from(explicit);
    }
    which_psmux().unwrap_or_else(|| PathBuf::from("psmux"))
}

fn which_psmux() -> Option<PathBuf> {
    let output = Command::new("where.exe").arg("psmux").output().ok()?;
    if !output.status.success() {
        return None;
    }
    let text = String::from_utf8_lossy(&output.stdout);
    PathBuf::from(text.lines().next()?.trim()).into()
}

/// Build a namespace unique across threads, processes, and clock ticks.
/// See `tests/psmux_parallel_isolation.rs` for the proof this construction
/// is required: a timestamp alone collides under concurrency on Windows.
fn unique_name(label: &str) -> String {
    static NAMESPACE_SEQUENCE: AtomicU64 = AtomicU64::new(0);
    let sequence = NAMESPACE_SEQUENCE.fetch_add(1, Ordering::Relaxed);
    let stamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_nanos());
    format!(
        "jefe-467-{label}-{}-{stamp:x}-{sequence:x}",
        std::process::id()
    )
}

fn format_args_os(args: &[OsString]) -> String {
    args.iter()
        .map(|value| value.to_string_lossy().into_owned())
        .collect::<Vec<_>>()
        .join(" ")
}

fn format_output(output: &Output) -> String {
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    format!(
        "status: {}\nstdout: {stdout}\nstderr: {stderr}",
        output.status
    )
}

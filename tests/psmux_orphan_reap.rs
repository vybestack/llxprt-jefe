#![cfg(all(windows, feature = "psmux-smoke"))]

//! Issue #332 real-psmux regression: a dead pane with surviving validated
//! worker descendants must be reaped to exactly the validated tree, and only
//! the target session is removed — no leaked sessions or processes.
//!
//! Requires a native Windows host with psmux >= 3.3.6 and
//! `JEFE_REQUIRE_PSMUX=1`. Skipped otherwise (mirrors `psmux_smoke.rs`).

use std::ffi::OsString;
use std::fmt::Write as FmtWrite;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};
use std::thread;
use std::time::{Duration, Instant};
use std::time::{SystemTime, UNIX_EPOCH};

use jefe::runtime::{OrphanClassification, PaneLiveness};
use serde::Deserialize;

const FIXTURE: &str = env!("CARGO_BIN_EXE_jefe-psmux-orphan-fixture");
const POLL_TIMEOUT: Duration = Duration::from_secs(8);
const COMMAND_TIMEOUT: Duration = Duration::from_secs(5);

#[derive(Debug, Deserialize)]
#[allow(dead_code)]
struct OrphanMarker {
    pid: u32,
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

struct PsmuxNamespace {
    executable: PathBuf,
    name: String,
    transcript: String,
}

impl PsmuxNamespace {
    fn new(label: &str) -> Self {
        let name = unique_name(label);
        Self {
            executable: psmux_path(),
            name,
            transcript: String::new(),
        }
    }

    fn run(&mut self, args: &[&str]) -> Result<Output, RegressionFailure> {
        let mut command = Command::new(&self.executable);
        command.arg("-L").arg(&self.name).args(args);
        for variable in ["TMUX", "TMUX_PANE", "TMUX_TMPDIR"] {
            command.env_remove(variable);
        }
        let display = format!("psmux -L {} {}", self.name, args.join(" "));
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

    fn failure(&self, message: String, details: &str) -> RegressionFailure {
        let sessions = self.available_sessions();
        let diagnostics = format!(
            "namespace: {}\n{details}\n\navailable sessions:\n{sessions}\n\ntranscript:\n{}",
            self.name, self.transcript
        );
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
        self.run_quiet(&["kill-server"]);
        let _ = fs::write(
            Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("target")
                .join("psmux-smoke")
                .join(format!("{}-transcript.txt", self.name)),
            &self.transcript,
        );
    }
}

#[test]
fn reap_removes_validated_orphan_and_only_target_session() {
    if !psmux_required() {
        return;
    }
    let mut namespace = PsmuxNamespace::new("orphan-reap");
    let work_dir = tempfile::tempdir().expect("create work dir");
    let marker_path = work_dir.path().join("orphan-marker.json");
    let session = "orphan-target";
    let bystander = "orphan-bystander";

    launch_fixture(&mut namespace, session, &work_dir, &marker_path);
    launch_fixture(&mut namespace, bystander, &work_dir, &marker_path);

    let marker = wait_for_marker(&marker_path).expect("orphan marker written");
    let orphan_identity = jefe::runtime::capture_process_identity(marker.pid)
        .expect("orphan child alive for identity capture");
    let observed = vec![jefe::runtime::ObservedDescendant::alive(orphan_identity)];

    assert!(process_alive(marker.pid), "orphan child alive before reap");
    assert!(namespace.session_exists(session));
    assert!(namespace.session_exists(bystander));

    kill_pane_leader(&mut namespace, session);
    assert!(
        process_alive(marker.pid),
        "orphan child must survive leader kill (the orphan scenario)"
    );

    assert_eq!(
        jefe::runtime::classify_orphan_state(PaneLiveness::Dead, true, &observed),
        OrphanClassification::DeadPaneWithOrphans,
        "dead pane with a validated live descendant must classify as Orphaned"
    );

    jefe::runtime::reap_orphan_tree(&[orphan_identity])
        .expect("reap of a validated orphan should succeed");
    namespace.run_quiet(&["kill-session", "-t", session]);

    assert!(
        !process_alive(marker.pid),
        "validated orphan PID {} must be terminated by the reap",
        marker.pid
    );
    assert!(!namespace.session_exists(session), "target session removed");
    assert!(
        namespace.session_exists(bystander),
        "bystander session must not be touched by the reap"
    );
}

/// Launch a fixture session. Targets (`*target`) run `--orphan-leader` with a
/// marker path; bystanders run `--orphan-child`.
fn launch_fixture(
    namespace: &mut PsmuxNamespace,
    session: &str,
    work_dir: &tempfile::TempDir,
    marker_path: &Path,
) {
    let leader = session.contains("target");
    let cwd = work_dir.path().to_string_lossy().into_owned();
    let marker = marker_path.to_string_lossy().into_owned();
    let args: Vec<OsString> = if leader {
        [
            "new-session",
            "-d",
            "-s",
            session,
            "-c",
            &cwd,
            FIXTURE,
            "--orphan-leader",
            &marker,
        ]
        .iter()
        .map(|s| OsString::from(*s))
        .collect()
    } else {
        [
            "new-session",
            "-d",
            "-s",
            session,
            "-c",
            &cwd,
            FIXTURE,
            "--orphan-child",
        ]
        .iter()
        .map(|s| OsString::from(*s))
        .collect()
    };
    let refs: Vec<&str> = args.iter().map(|s| s.to_str().expect("utf8")).collect();
    namespace
        .run(&refs)
        .unwrap_or_else(|error| panic!("launch {session}: {error}"));
}

/// Kill the pane leader of `session` to simulate the dead-pane state.
fn kill_pane_leader(namespace: &mut PsmuxNamespace, session: &str) {
    let leader_pid = namespace
        .run(&["display-message", "-p", "-t", session, "#{pane_pid}"])
        .map(|output| {
            String::from_utf8_lossy(&output.stdout)
                .trim()
                .parse::<u32>()
                .expect("pane_pid parses")
        })
        .unwrap_or_else(|error| panic!("read pane_pid: {error}"));
    let _ = Command::new("taskkill")
        .args(["/PID", &leader_pid.to_string(), "/F"])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status();
    thread::sleep(Duration::from_millis(400));
}

#[test]
fn empty_orphan_tree_leaves_unrelated_processes_and_sessions_intact() {
    // AC6/AC18 negative: reaping with no anchors and removing a session must
    // not affect other sessions or processes in the namespace.
    if !psmux_required() {
        return;
    }
    let mut namespace = PsmuxNamespace::new("orphan-empty");
    let work_dir = tempfile::tempdir().expect("create work dir");
    let session = "orphan-empty-target";
    let bystander = "orphan-empty-bystander";
    for name in [session, bystander] {
        namespace
            .run(&[
                "new-session",
                "-d",
                "-s",
                name,
                "-c",
                &work_dir.path().to_string_lossy(),
                FIXTURE,
                "--orphan-child",
            ])
            .unwrap_or_else(|error| panic!("launch {name}: {error}"));
    }
    assert!(namespace.session_exists(session));
    assert!(namespace.session_exists(bystander));

    // No validated anchors to reap; remove only the target session (mirrors the
    // session-kill half of reap_orphan_session for an empty orphan tree).
    let _ = jefe::runtime::reap_orphan_tree(&[]);
    namespace.run_quiet(&["kill-session", "-t", session]);

    assert!(!namespace.session_exists(session));
    assert!(
        namespace.session_exists(bystander),
        "empty reap must not remove unrelated sessions"
    );
}

fn wait_for_marker(path: &Path) -> Result<OrphanMarker, String> {
    let deadline = Instant::now() + POLL_TIMEOUT;
    let mut last_error = String::from("marker not written");
    while Instant::now() < deadline {
        if let Ok(bytes) = fs::read(path) {
            if let Ok(marker) = serde_json::from_slice::<OrphanMarker>(&bytes) {
                return Ok(marker);
            }
        }
        last_error = "marker unreadable".to_owned();
        thread::sleep(Duration::from_millis(50));
    }
    Err(format!(
        "orphan marker {path:?} not ready within {POLL_TIMEOUT:?}: {last_error}"
    ))
}

fn process_alive(pid: u32) -> bool {
    // Use OpenProcess to check liveness directly.
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

fn unique_name(label: &str) -> String {
    let stamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    format!("jefe-orph-{label}-{stamp}")
}

fn format_output(output: &Output) -> String {
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    format!(
        "status: {}\nstdout: {stdout}\nstderr: {stderr}",
        output.status
    )
}

// Silence unused warnings for helpers reserved for the negative path / Unix.
#[allow(dead_code)]
fn _reserved(_os: OsString) {
    let _ = COMMAND_TIMEOUT;
}

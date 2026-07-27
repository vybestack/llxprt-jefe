#![cfg(all(windows, feature = "psmux-smoke"))]

//! Issue #296: real-transport mouse-mode + page-key delivery test.
//!
//! Lives in a separate file from `tests/psmux_smoke.rs` to keep both files
//! under the project's 1000-line hard source-file limit.

use std::ffi::OsString;
use std::fmt::Write as FmtWrite;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};
use std::thread;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use jefe::runtime::{
    AttachedViewer, LocalPlatform, MultiplexerIsolation, MultiplexerPlan,
    configure_prefix_for_passthrough_with_plan,
};

/// Ceiling for a byte to traverse a real PTY and appear in a pane capture.
///
/// The wait polls every 50ms and returns as soon as the needle appears, so
/// this only bounds how long a genuine hang is tolerated; a passing run is not
/// slowed by raising it. Five seconds proved too tight on shared CI runners —
/// Windows in particular — where this test failed intermittently on commits
/// that changed no code it exercises.
const POLL_TIMEOUT: Duration = Duration::from_secs(30);
const FIXTURE: &str = env!("CARGO_BIN_EXE_jefe-psmux-smoke-fixture");

#[test]
fn psmux_attached_viewer_observes_mouse_modes_and_delivers_page_keys() {
    let Some((executable, version_text)) = qualified_psmux() else {
        return;
    };
    let mut namespace = namespace_or_panic(executable.clone(), "mouse-mode", &version_text);
    let session = "mouse-mode-fixture";
    let work_dir = tempfile::Builder::new()
        .prefix("jefe psmux mouse Ω ")
        .tempdir()
        .unwrap_or_else(|error| panic!("create mouse-mode fixture directory: {error}"));
    namespace
        .run_os(&[
            OsString::from("new-session"),
            OsString::from("-d"),
            OsString::from("-s"),
            OsString::from(session),
            OsString::from("-x"),
            OsString::from("100"),
            OsString::from("-y"),
            OsString::from("32"),
            OsString::from("-c"),
            work_dir.path().as_os_str().to_owned(),
            OsString::from(FIXTURE),
        ])
        .unwrap_or_else(|error| panic!("create mouse-mode fixture session: {error}"));
    namespace
        .wait_for_capture(session, "PSMUX_SMOKE_READY")
        .unwrap_or_else(|error| panic!("fixture never became ready: {error}"));

    let plan = MultiplexerPlan::for_platform(
        LocalPlatform::Windows,
        executable,
        MultiplexerIsolation::Namespace(namespace.name.clone()),
    )
    .unwrap_or_else(|error| panic!("construct psmux plan: {error}"));

    // Issue #465: apply the production prefix + root-table unbind policy so
    // psmux's default `PageUp -> copy-mode -u` root binding is removed before
    // the test writes Page-key sequences through the attached viewer.
    configure_prefix_for_passthrough_with_plan(session, &plan)
        .unwrap_or_else(|error| panic!("configure production prefix policy: {error}"));

    let viewer = AttachedViewer::spawn_with_plan(session, 32, 100, &plan)
        .unwrap_or_else(|error| panic!("spawn AttachedViewer: {error}"));
    assert!(
        viewer.is_alive(),
        "AttachedViewer should be alive after spawn"
    );

    assert_attached_viewer_observes_mouse_reporting(&viewer);
    assert_attached_input_ready(&mut namespace, session, &viewer);
    assert_page_keys_delivered_as_csi_tilde(&mut namespace, session, &viewer);
    assert_sgr_mouse_delivered_intact(&mut namespace, session, &viewer);

    drop(viewer);
    let _ = namespace.run(&["kill-session", "-t", session]);
}

/// Issue #296 (a): poll until the AttachedViewer's embedded terminal model
/// observes the fixture's advertised DEC private mouse modes.
fn assert_attached_viewer_observes_mouse_reporting(viewer: &AttachedViewer) {
    let deadline = Instant::now() + POLL_TIMEOUT;
    let mut observed = false;
    while Instant::now() < deadline {
        if viewer.mouse_reporting_active() {
            observed = true;
            break;
        }
        thread::sleep(Duration::from_millis(50));
    }
    assert!(
        observed,
        "AttachedViewer never observed mouse reporting after fixture advertised 1000/1002/1006"
    );
}

/// Wait for the attached psmux client to forward input before asserting the
/// semantic key sequences. Output can reach the viewer before psmux's input
/// relay is ready on a loaded Windows runner, so terminal-mode observation alone
/// is not an input-readiness barrier.
fn assert_attached_input_ready(
    namespace: &mut PsmuxNamespace,
    session: &str,
    viewer: &AttachedViewer,
) {
    write_input_until_captured(
        namespace,
        session,
        viewer,
        b"j",
        "PSMUX_BYTE_6A",
        "input-readiness probe",
    );
}

fn write_input_until_captured(
    namespace: &mut PsmuxNamespace,
    session: &str,
    viewer: &AttachedViewer,
    bytes: &[u8],
    needle: &str,
    label: &str,
) -> String {
    let deadline = Instant::now() + POLL_TIMEOUT;
    let mut last = String::new();
    while Instant::now() < deadline {
        assert!(
            viewer.is_alive(),
            "AttachedViewer exited before forwarding {label}"
        );
        for byte in bytes {
            viewer
                .write_input(std::slice::from_ref(byte))
                .unwrap_or_else(|error| panic!("write {label}: {error}"));
            thread::sleep(Duration::from_millis(10));
        }
        thread::sleep(Duration::from_millis(50));
        last = namespace
            .capture(session)
            .unwrap_or_else(|error| panic!("capture {label}: {error}"));
        if last.contains(needle) {
            return last;
        }
        thread::sleep(Duration::from_millis(50));
    }
    panic!("attached input relay did not forward {label} within {POLL_TIMEOUT:?}:\n{last}");
}

/// Issue #465: poll `capture-pane` until the expected needle appears, without
/// re-injecting input. Re-injecting semantic Page-key sequences mutates psmux
/// state and cannot recover once copy mode is active. The caller writes the
/// sequence exactly once, then this helper polls the pane capture for the
/// needle within the bounded timeout.
fn poll_for_capture(
    namespace: &mut PsmuxNamespace,
    session: &str,
    needle: &str,
    label: &str,
) -> String {
    let deadline = Instant::now() + POLL_TIMEOUT;
    let mut last = String::new();
    while Instant::now() < deadline {
        last = namespace
            .capture(session)
            .unwrap_or_else(|error| panic!("capture {label}: {error}"));
        if last.contains(needle) {
            return last;
        }
        thread::sleep(Duration::from_millis(50));
    }
    panic!("pane did not contain {needle:?} within {POLL_TIMEOUT:?} ({label}):\n{last}");
}

/// Issue #296 (b): forwarded PageUp/PageDown must arrive as `CSI 5~`/`CSI 6~`
/// (bytes 1B 5B 35/36 7E), not arrow sequences.
///
/// Issue #465: psmux 3.3.7 ships a default root-table binding
/// `PageUp -> copy-mode -u` that consumes bare PageUp events before they
/// reach the pane child. The production `configure_prefix_for_passthrough`
/// now unbinds `PageUp` from the root table on Windows, so the attached viewer
/// must deliver the bytes through the pane. This assertion writes the full
/// Page-key sequence in a single `write_input` call (no per-byte sleeps, no
/// retry re-injection), then polls `capture-pane` without re-injecting input.
/// Re-injecting once copy mode is active cannot recover — every retry is
/// consumed by copy mode — so the test must not retry.
fn assert_page_keys_delivered_as_csi_tilde(
    namespace: &mut PsmuxNamespace,
    session: &str,
    viewer: &AttachedViewer,
) {
    // Write the complete PageUp + PageDown sequence in a single call. The
    // production unbind (issue #465) prevents psmux's root-table binding from
    // intercepting PageUp, so the bytes traverse the normal passthrough path.
    viewer
        .write_input(b"\x1b[5~\x1b[6~")
        .unwrap_or_else(|error| panic!("write PageUp/PageDown: {error}"));

    let capture = poll_for_capture(namespace, session, "PSMUX_BYTE_7E", "PageUp/PageDown bytes");

    for needle in [
        "PSMUX_BYTE_1B",
        "PSMUX_BYTE_5B",
        "PSMUX_BYTE_35",
        "PSMUX_BYTE_36",
        "PSMUX_BYTE_7E",
    ] {
        assert!(
            capture.contains(needle),
            "page-key byte {needle} missing from child capture:\n{capture}"
        );
    }

    // Issue #465: psmux must not have entered copy mode after bare PageUp. If
    // the root-table binding was not removed, copy mode would activate and
    // consume every subsequent Page key. `#{pane_in_mode}` is 1 when the pane
    // is in copy mode (or any other mode), 0 in the normal pane state.
    let mode_output = namespace
        .run(&["display-message", "-p", "-t", session, "#{pane_in_mode}"])
        .unwrap_or_else(|error| panic!("query pane_in_mode after PageUp: {error}"));
    let mode_text = String::from_utf8_lossy(&mode_output.stdout);
    let in_mode = mode_text.trim();
    assert!(
        in_mode == "0",
        "psmux entered copy mode after bare PageUp (pane_in_mode={in_mode}); \
         the root-table PageUp binding was not removed. capture:\n{capture}"
    );
}

/// Issue #296 (c): forwarded SGR mouse bytes (`CSI < 0;1;1 M`) must reach the
/// child intact.
fn assert_sgr_mouse_delivered_intact(
    namespace: &mut PsmuxNamespace,
    session: &str,
    viewer: &AttachedViewer,
) {
    let capture = write_input_until_captured(
        namespace,
        session,
        viewer,
        b"\x1b[<0;1;1M",
        "PSMUX_BYTE_4D",
        "SGR mouse bytes",
    );
    for needle in ["PSMUX_BYTE_3C", "PSMUX_BYTE_30", "PSMUX_BYTE_4D"] {
        assert!(
            capture.contains(needle),
            "SGR mouse byte {needle} missing from child capture:\n{capture}"
        );
    }
}

// ── Minimal psmux namespace harness (mirrors tests/psmux_smoke.rs) ─────────
//
// Duplicated here so the mouse-mode test file stays self-contained and both
// files stay under the 1000-line source-file hard limit. The shared helpers
// (`PsmuxNamespace`, `qualified_psmux`, etc.) are private to the smoke suite;
// rather than widen that public surface for a single test, this file reuses
// the same proven harness pattern.

#[derive(Debug)]
struct SmokeFailure {
    message: String,
    diagnostics: String,
}

impl std::fmt::Display for SmokeFailure {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(formatter, "{}\n\n{}", self.message, self.diagnostics)
    }
}

impl std::error::Error for SmokeFailure {}

struct PsmuxNamespace {
    executable: PathBuf,
    name: String,
    transcript: String,
    artifact_dir: PathBuf,
}

impl PsmuxNamespace {
    fn new(executable: PathBuf, label: &str) -> Result<Self, SmokeFailure> {
        let name = unique_name(label);
        let artifact_dir = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("target")
            .join("psmux-smoke")
            .join(&name);
        fs::create_dir_all(&artifact_dir).map_err(|error| SmokeFailure {
            message: format!("failed to create artifact directory: {error}"),
            diagnostics: format!("namespace: {name}\npath: {}", artifact_dir.display()),
        })?;
        Ok(Self {
            executable,
            name,
            transcript: String::new(),
            artifact_dir,
        })
    }

    fn run(&mut self, args: &[&str]) -> Result<Output, SmokeFailure> {
        let owned = args.iter().map(OsString::from).collect::<Vec<_>>();
        self.run_os(&owned)
    }

    fn run_os(&mut self, args: &[OsString]) -> Result<Output, SmokeFailure> {
        let mut command = Command::new(&self.executable);
        command.arg("-L").arg(&self.name).args(args);
        for variable in ["TMUX", "TMUX_PANE", "TMUX_TMPDIR"] {
            command.env_remove(variable);
        }
        command.stdout(Stdio::piped()).stderr(Stdio::piped());
        let mut child = command.spawn().map_err(|error| {
            self.failure(
                format!("failed to spawn: {error}"),
                "status: not started\nstdout: \nstderr: ",
            )
        })?;
        let deadline = Instant::now() + Duration::from_secs(5);
        let output = loop {
            match child.try_wait() {
                Ok(Some(_)) => break child.wait_with_output(),
                Ok(None) if Instant::now() >= deadline => {
                    let _ = child.kill();
                    let output = child.wait_with_output();
                    let details = output
                        .as_ref()
                        .map_or_else(std::string::ToString::to_string, format_output);
                    return Err(self.failure(format!("command timed out: {details}"), ""));
                }
                Ok(None) => thread::sleep(Duration::from_millis(25)),
                Err(error) => {
                    return Err(
                        self.failure(format!("failed waiting: {error}"), "status: wait failed")
                    );
                }
            }
        }
        .map_err(|error| self.failure(format!("failed collecting: {error}"), ""))?;
        let _ = writeln!(self.transcript, "{}", format_output(&output));
        if output.status.success() {
            Ok(output)
        } else {
            Err(self.failure("command failed".to_owned(), &format_output(&output)))
        }
    }

    fn capture(&mut self, session: &str) -> Result<String, SmokeFailure> {
        let output = self.run(&["capture-pane", "-p", "-S", "-100", "-t", session])?;
        Ok(String::from_utf8_lossy(&output.stdout).into_owned())
    }

    fn wait_for_capture(&mut self, session: &str, needle: &str) -> Result<String, SmokeFailure> {
        let deadline = Instant::now() + POLL_TIMEOUT;
        let mut last = String::new();
        while Instant::now() < deadline {
            last = self.capture(session)?;
            if last.contains(needle) {
                return Ok(last);
            }
            thread::sleep(Duration::from_millis(50));
        }
        Err(self.failure(
            format!("pane did not contain {needle:?} within {POLL_TIMEOUT:?}"),
            &format!("last capture:\n{last}"),
        ))
    }

    fn failure(&self, message: String, details: &str) -> SmokeFailure {
        SmokeFailure {
            message,
            diagnostics: format!(
                "namespace: {}\nartifact: {}\n{details}\ntranscript:\n{}",
                self.name,
                self.artifact_dir.display(),
                self.transcript,
            ),
        }
    }
}

impl Drop for PsmuxNamespace {
    fn drop(&mut self) {
        let _ = Command::new(&self.executable)
            .arg("-L")
            .arg(&self.name)
            .arg("kill-server")
            .output();
        let _ = fs::write(self.artifact_dir.join("transcript.txt"), &self.transcript);
    }
}

fn qualified_psmux() -> Option<(PathBuf, String)> {
    let executable =
        std::env::var_os("JEFE_PSMUX_BIN").map_or_else(|| PathBuf::from("psmux"), PathBuf::from);
    let output = Command::new(&executable).arg("-V").output();
    let output = match output {
        Ok(output) if output.status.success() => output,
        _ => {
            assert!(
                !std::env::var("JEFE_REQUIRE_PSMUX").is_ok_and(|v| v == "1"),
                "psmux required but unavailable"
            );
            return None;
        }
    };
    let version_text = String::from_utf8_lossy(&output.stdout).trim().to_owned();
    Some((executable, version_text))
}

fn namespace_or_panic(executable: PathBuf, label: &str, _version: &str) -> PsmuxNamespace {
    match PsmuxNamespace::new(executable, label) {
        Ok(ns) => ns,
        Err(error) => panic!("{error}"),
    }
}

fn unique_name(label: &str) -> String {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_nanos());
    format!("jefe-psmux-{label}-{}-{nanos:x}", std::process::id())
}

fn format_output(output: &Output) -> String {
    format!(
        "status: {}\nstdout: {}\nstderr: {}",
        output.status,
        String::from_utf8_lossy(&output.stdout).trim(),
        String::from_utf8_lossy(&output.stderr).trim(),
    )
}

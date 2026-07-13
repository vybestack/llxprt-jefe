#![cfg(all(windows, feature = "psmux-smoke"))]

use std::ffi::OsString;
use std::path::PathBuf;
use std::process::Command;
use std::thread;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use jefe::runtime::{
    AttachedViewer, LocalPlatform, MultiplexerIsolation, MultiplexerPlan, TerminalSnapshot,
};

const FIXTURE: &str = env!("CARGO_BIN_EXE_jefe-psmux-smoke-fixture");
const TIMEOUT: Duration = Duration::from_secs(8);

struct ServerCleanup {
    plan: MultiplexerPlan,
}

impl Drop for ServerCleanup {
    fn drop(&mut self) {
        let mut cleanup = self.plan.command();
        let _ = cleanup.arg("kill-server").status();
    }
}

#[test]
fn native_psmux_attachment_preserves_terminal_contract_and_session() {
    let executable =
        std::env::var_os("JEFE_PSMUX_BIN").map_or_else(|| PathBuf::from("psmux"), PathBuf::from);
    if Command::new(&executable).arg("-V").output().is_err() {
        assert!(
            std::env::var_os("JEFE_REQUIRE_PSMUX").is_none(),
            "psmux is required but {executable:?} is unavailable"
        );
        return;
    }
    let namespace = unique_namespace();
    let plan = MultiplexerPlan::for_platform(
        LocalPlatform::Windows,
        executable,
        MultiplexerIsolation::Namespace(namespace.clone()),
    )
    .unwrap_or_else(|error| panic!("construct psmux plan: {error}"));
    let cleanup = ServerCleanup { plan: plan.clone() };
    let session = "jefe-attach-contract";
    let mut create = plan.command();
    create.args([
        OsString::from("new-session"),
        OsString::from("-d"),
        OsString::from("-s"),
        OsString::from(session),
        OsString::from("-x"),
        OsString::from("100"),
        OsString::from("-y"),
        OsString::from("32"),
        OsString::from(FIXTURE),
    ]);
    let status = create
        .status()
        .unwrap_or_else(|error| panic!("create psmux fixture session: {error}"));
    assert!(
        status.success(),
        "fixture session creation failed: {status}"
    );
    for (option, value) in [
        ("prefix", "F12"),
        ("prefix2", "None"),
        ("allow-passthrough", "on"),
    ] {
        let mut configure = plan.command();
        // psmux reads prefix options from the private server's global scope
        // when an attached client starts; session-scoped values are ignored.
        let status = configure
            .args(["set-option", "-g", option, value])
            .status()
            .unwrap_or_else(|error| panic!("configure {option}: {error}"));
        assert!(status.success(), "configure {option} failed: {status}");
    }

    let result = exercise_attachment(&plan, session);
    drop(cleanup);
    if let Err(error) = result {
        panic!("{error}");
    }
}

fn exercise_attachment(plan: &MultiplexerPlan, session: &str) -> Result<(), String> {
    let viewer = AttachedViewer::spawn_with_plan(session, 32, 100, plan)
        .map_err(|error| format!("attach through production viewer: {error}"))?;
    let initial = wait_for_snapshot(&viewer, "ALT_SCREEN")?;
    assert_initial_render(&viewer, &initial)?;
    exercise_input(&viewer)?;
    assert_resize(&viewer, plan, session)?;
    drop(viewer);
    let mut has_session = plan.command();
    let status = has_session
        .args(["has-session", "-t", session])
        .status()
        .map_err(|error| format!("probe persistent session: {error}"))?;
    if !status.success() {
        return Err("dropping attach client killed persistent session".to_owned());
    }
    Ok(())
}

fn assert_initial_render(
    viewer: &AttachedViewer,
    initial: &TerminalSnapshot,
) -> Result<(), String> {
    if initial.rows != 32 || initial.cols != 100 {
        return Err(format!(
            "initial terminal model geometry was {}x{}, expected 100x32",
            initial.cols, initial.rows
        ));
    }
    let text = snapshot_text(initial);
    for expected in ["COLOR_RED", "UNICODE_Ω_界", "_e", "CURSOR_A!", "ALT_SCREEN"] {
        if !text.contains(expected) {
            return Err(format!("snapshot missing {expected:?}:\n{text}"));
        }
    }
    if !viewer.mouse_reporting_active() || !viewer.bracketed_paste_active() {
        return Err("fixture terminal modes were not propagated".to_owned());
    }
    if !initial
        .cells
        .iter()
        .flatten()
        .any(|cell| cell.style.fg != iocraft::Color::Reset)
    {
        return Err("ANSI color was not represented in the terminal snapshot".to_owned());
    }
    Ok(())
}

fn exercise_input(viewer: &AttachedViewer) -> Result<(), String> {
    for (index, input) in [
        b"\r".as_slice(),
        b"\x1b",
        b"\t",
        b"\x1b[A",
        b"\x1bOP",
        b"\x1bx",
        b"\x03",
        b"\x18",
        b"\x02",
        b"\x1b[200~line 1\nUnicode \xce\xa9 & (%)\x1b[201~",
    ]
    .into_iter()
    .enumerate()
    {
        viewer
            .write_input(input)
            .map_err(|error| format!("write production viewer input {index}: {error}"))?;
        thread::sleep(Duration::from_millis(100));
        if !viewer.is_alive() {
            return Err(format!("attach client exited after input {index}"));
        }
    }
    viewer
        .write_input(b"\x12")
        .map_err(|error| format!("request fixture report: {error}"))?;
    let final_snapshot = wait_for_snapshot(viewer, "MAIN_SCREEN")?;
    let final_text = snapshot_text(&final_snapshot);
    for bytes in [
        "INPUT_HEX_0D_1B_09_1B_5B_41_1B_4F_50_1B_78_03_18_02",
        "_6C_69_6E_65_20_31_0D_55_6E_69_63_6F_64_65_20_CE",
        "A9_20_26_20_28_25_29_12",
    ] {
        if !final_text.contains(bytes) {
            return Err(format!("fixture did not receive {bytes}:\n{final_text}"));
        }
    }
    Ok(())
}

fn assert_resize(
    viewer: &AttachedViewer,
    plan: &MultiplexerPlan,
    session: &str,
) -> Result<(), String> {
    viewer
        .resize(28, 90)
        .map_err(|error| format!("resize production viewer: {error}"))?;
    // psmux reserves one attached-client status row; the pane receives the
    // remaining rows while Jefe's terminal model retains the requested 90x28.
    wait_for_dimensions(plan, session, "90x27")?;
    let resized = viewer
        .snapshot()
        .ok_or_else(|| "resized viewer snapshot unavailable".to_owned())?;
    if resized.cells.len() != 28 || resized.cells.first().map_or(0, Vec::len) != 90 {
        return Err(format!(
            "terminal model did not resize to 90x28: {}x{}",
            resized.cells.first().map_or(0, Vec::len),
            resized.cells.len()
        ));
    }
    Ok(())
}

fn wait_for_snapshot(viewer: &AttachedViewer, needle: &str) -> Result<TerminalSnapshot, String> {
    let deadline = Instant::now() + TIMEOUT;
    let mut latest = String::new();
    while Instant::now() < deadline {
        if let Some(snapshot) = viewer.snapshot() {
            latest = snapshot_text(&snapshot);
            if latest.contains(needle) {
                return Ok(snapshot);
            }
        }
        thread::sleep(Duration::from_millis(50));
    }
    Err(format!(
        "viewer snapshot did not contain {needle:?}:\n{latest}"
    ))
}

fn wait_for_dimensions(
    plan: &MultiplexerPlan,
    session: &str,
    expected: &str,
) -> Result<(), String> {
    let deadline = Instant::now() + TIMEOUT;
    while Instant::now() < deadline {
        let mut command = plan.command();
        let output = command
            .args([
                "display-message",
                "-p",
                "-t",
                session,
                "#{pane_width}x#{pane_height}",
            ])
            .output()
            .map_err(|error| format!("query pane geometry: {error}"))?;
        if String::from_utf8_lossy(&output.stdout).trim() == expected {
            return Ok(());
        }
        thread::sleep(Duration::from_millis(50));
    }
    let mut command = plan.command();
    let output = command
        .args([
            "display-message",
            "-p",
            "-t",
            session,
            "#{pane_width}x#{pane_height}",
        ])
        .output()
        .map_err(|error| format!("query final pane geometry: {error}"))?;
    Err(format!(
        "pane geometry did not become {expected}; final={:?}",
        String::from_utf8_lossy(&output.stdout).trim()
    ))
}

fn snapshot_text(snapshot: &TerminalSnapshot) -> String {
    snapshot
        .cells
        .iter()
        .flat_map(|row| row.iter().map(|cell| cell.ch).chain(std::iter::once('\n')))
        .collect()
}

fn unique_namespace() -> String {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |duration| duration.as_nanos());
    format!("jefe-attach-{}-{nanos:x}", std::process::id())
}

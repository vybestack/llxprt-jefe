//! Test-only control sidecar for the provider fixture binary
//! (issue #704, slice S2).
//!
//! The product composition spawns providers with no argv, so a staged copy of
//! the fixture selects its behavior from a `<executable>.control` file next
//! to the copied executable — one `key=value` line per setting (`mode`,
//! `record_dir`, `spawn_marker`). The sidecar is consulted only when argv
//! supplies no mode, so every test that drives the fixture through explicit
//! argv (all of issue #390) is unaffected. `spawn_marker` names a file
//! touched the instant the fixture starts: the fail-if-spawned trap for
//! providers the host must never start.

use std::io::Write as _;

/// Resolve this invocation's fixture mode and record directory.
///
/// Explicit argv always wins, so every test that drives the fixture through a
/// command line is unaffected by the sidecar. The product composition supplies
/// no argv, so a staged copy of this fixture instead reads its mode, record
/// directory, and spawn marker from the control sidecar next to the
/// executable; with neither, the standalone `happy` default applies.
pub fn resolve_invocation() -> (String, Option<String>) {
    let argv_mode = std::env::args().nth(1);
    let argv_record_dir = std::env::args().nth(2);
    let control = if argv_mode.is_none() {
        read_control_sidecar()
    } else {
        None
    };
    if let Some(marker) = control
        .as_ref()
        .and_then(|sidecar| sidecar.spawn_marker.as_deref())
    {
        touch_spawn_marker(marker);
    }
    let mode = argv_mode
        .or_else(|| control.as_ref().and_then(|sidecar| sidecar.mode.clone()))
        .unwrap_or_else(|| "happy".to_owned());
    // The sidecar owns the record directory only when argv supplied none.
    let record_dir =
        argv_record_dir.or_else(|| control.and_then(|sidecar| sidecar.record_dir.clone()));
    (mode, record_dir)
}

/// The settings a `<executable>.control` sidecar may carry. Every field is
/// optional; an absent sidecar leaves the fixture's defaults untouched.
struct ControlSidecar {
    mode: Option<String>,
    record_dir: Option<String>,
    spawn_marker: Option<String>,
}

/// Read the `<executable>.control` sidecar next to the running executable.
///
/// Returns `None` when the file is absent (the standalone default). A present
/// but malformed control file exits loudly: a half-staged trap or mode that
/// silently falls back to `happy` would turn a regression into a hang.
fn read_control_sidecar() -> Option<ControlSidecar> {
    let exe = std::env::current_exe().ok()?;
    let name = exe.file_name()?.to_str()?;
    let text = std::fs::read_to_string(exe.with_file_name(format!("{name}.control"))).ok()?;
    Some(parse_control_sidecar(&text))
}

/// Parse `key=value` control lines into a [`ControlSidecar`].
fn parse_control_sidecar(text: &str) -> ControlSidecar {
    let mut sidecar = ControlSidecar {
        mode: None,
        record_dir: None,
        spawn_marker: None,
    };
    for line in text.lines() {
        let trimmed = line.trim_end_matches('\r');
        let Some((key, value)) = trimmed.split_once('=') else {
            malformed_control_line(trimmed);
        };
        let slot = match key {
            "mode" => &mut sidecar.mode,
            "record_dir" => &mut sidecar.record_dir,
            "spawn_marker" => &mut sidecar.spawn_marker,
            _ => malformed_control_line(trimmed),
        };
        *slot = Some(value.to_owned());
    }
    sidecar
}

/// Reject an unknown or malformed control line by exiting with a distinct
/// code, so a staging defect is observed instead of silently ignored.
fn malformed_control_line(line: &str) -> ! {
    let mut stderr = std::io::stderr().lock();
    let _ = writeln!(stderr, "fixture: malformed control line: {line}");
    std::process::exit(4);
}

/// Record that this fixture was spawned the instant it starts, before any
/// protocol traffic: the fail-if-spawned trap for providers the host must
/// never start. A trap that cannot record its own spawn is useless, so a
/// write failure exits loudly rather than hanging silently.
fn touch_spawn_marker(path: &str) {
    let marker = std::path::Path::new(path);
    if let Some(parent) = marker.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    if std::fs::write(marker, b"1").is_err() {
        let mut stderr = std::io::stderr().lock();
        let _ = writeln!(stderr, "fixture: cannot write spawn marker {path}");
        std::process::exit(4);
    }
}

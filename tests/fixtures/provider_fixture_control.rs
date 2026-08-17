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
        match read_control_sidecar() {
            Ok(control) => control,
            Err(error) => invalid_control(&error),
        }
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
fn read_control_sidecar() -> Result<Option<ControlSidecar>, String> {
    let exe = std::env::current_exe()
        .map_err(|error| format!("cannot resolve fixture executable: {error}"))?;
    read_control_sidecar_from(&exe)
}

fn read_control_sidecar_from(exe: &std::path::Path) -> Result<Option<ControlSidecar>, String> {
    let name = exe
        .file_name()
        .and_then(std::ffi::OsStr::to_str)
        .ok_or_else(|| {
            format!(
                "fixture executable has no UTF-8 file name: {}",
                exe.display()
            )
        })?;
    let path = exe.with_file_name(format!("{name}.control"));
    let text = match std::fs::read_to_string(&path) {
        Ok(text) => text,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(error) => {
            return Err(format!(
                "cannot read control sidecar {}: {error}",
                path.display()
            ));
        }
    };
    parse_control_sidecar(&text).map(Some)
}

/// Parse `key=value` control lines into a [`ControlSidecar`].
fn parse_control_sidecar(text: &str) -> Result<ControlSidecar, String> {
    if text.is_empty() {
        return Err("control sidecar is empty".to_owned());
    }
    let mut sidecar = ControlSidecar {
        mode: None,
        record_dir: None,
        spawn_marker: None,
    };
    for line in text.lines() {
        let trimmed = line.trim_end_matches('\r');
        let Some((key, value)) = trimmed.split_once('=') else {
            return Err(format!("malformed control line: {trimmed}"));
        };
        if value.is_empty() {
            return Err(format!("empty control value: {trimmed}"));
        }
        let slot = match key {
            "mode" => &mut sidecar.mode,
            "record_dir" => &mut sidecar.record_dir,
            "spawn_marker" => &mut sidecar.spawn_marker,
            _ => return Err(format!("unknown control key: {key}")),
        };
        if slot.is_some() {
            return Err(format!("duplicate control key: {key}"));
        }
        *slot = Some(value.to_owned());
    }
    Ok(sidecar)
}

/// Reject an unreadable or malformed control sidecar by exiting with a distinct
/// code, so a staging defect is observed instead of silently ignored.
fn invalid_control(error: &str) -> ! {
    let mut stderr = std::io::stderr().lock();
    let _ = writeln!(stderr, "fixture: invalid control sidecar: {error}");
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

#[cfg(test)]
mod tests {
    use super::{parse_control_sidecar, read_control_sidecar_from};

    #[test]
    fn empty_duplicate_and_unknown_controls_are_rejected() {
        for text in ["", "mode=\n", "mode=happy\nmode=other\n", "unknown=value\n"] {
            assert!(
                parse_control_sidecar(text).is_err(),
                "control must be rejected: {text:?}"
            );
        }
    }

    #[test]
    fn absent_sidecar_is_distinct_from_an_unreadable_present_sidecar() {
        let temp = tempfile::tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
        let exe = temp.path().join("fixture");
        assert!(matches!(read_control_sidecar_from(&exe), Ok(None)));

        std::fs::write(exe.with_file_name("fixture.control"), [0xff])
            .unwrap_or_else(|error| panic!("write invalid control: {error}"));
        assert!(read_control_sidecar_from(&exe).is_err());
    }
}

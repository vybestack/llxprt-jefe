//! Capture shim fixture executable for the schema-1 harness (issue #380).
//!
//! Invoked in place of a captured tool (e.g. `gh`). Locates its behavior
//! file beside its own executable (no env lookup, no PATH search, no shell),
//! claims a start ordinal atomically, records exact process-boundary fields
//! (argv, sorted env pairs, cwd, bounded stdin), emits the configured
//! stdout/stderr, and exits with the configured code. `hang` blocks forever
//! for timeout/reaping tests; `spawn_child_hang` additionally leaves a
//! hanging grandchild so escalation must reap the whole tree.

use std::io::{Read, Write};
use std::path::PathBuf;
use std::process::ExitCode;

use jefe::harness::v1::capture::{
    BEHAVIOR_SUFFIX, CaptureRecord, EXHAUSTED_EXIT, HANG_CHILD_ENV, HANG_CHILD_ROLE,
    ShimBehaviorFile,
};
use jefe::harness::v1::limits::MAX_PROCESSES_PER_CAPTURE;

fn main() -> ExitCode {
    if std::env::var(HANG_CHILD_ENV).as_deref() == Ok(HANG_CHILD_ROLE) {
        hang_forever();
    }
    match run() {
        Ok(code) => ExitCode::from(code),
        Err(message) => {
            let _ = std::io::stderr().write_all(message.as_bytes());
            let _ = std::io::stderr().write_all(b"\n");
            ExitCode::from(97)
        }
    }
}

fn run() -> Result<u8, String> {
    let behavior = load_behavior()?;
    let records_dir = PathBuf::from(&behavior.records_dir);
    let Some(ordinal) = claim_ordinal(&records_dir)? else {
        return Ok(EXHAUSTED_EXIT);
    };
    let stdin = read_bounded_stdin(behavior.stdin_limit)?;
    let mut record = CaptureRecord {
        ordinal,
        pid: std::process::id(),
        child_pid: None,
        argv: std::env::args().collect(),
        env: sorted_env(),
        cwd: current_dir_string()?,
        stdin,
        stdout: behavior.stdout.clone(),
        stderr: behavior.stderr.clone(),
        exit_code: behavior.exit_code,
        completed: false,
    };
    if behavior.spawn_child_hang {
        record.child_pid = Some(spawn_hang_child()?);
    }
    write_record(&records_dir, ordinal, "start", &record)?;
    let _ = std::io::stdout().write_all(behavior.stdout.as_bytes());
    let _ = std::io::stderr().write_all(behavior.stderr.as_bytes());
    if behavior.hang {
        hang_forever();
    }
    record.completed = true;
    write_record(&records_dir, ordinal, "done", &record)?;
    Ok(behavior.exit_code)
}

fn load_behavior() -> Result<ShimBehaviorFile, String> {
    let exe = std::env::current_exe().map_err(|err| format!("current_exe: {err}"))?;
    let mut name = exe
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .ok_or_else(|| "shim executable has no file name".to_string())?;
    name.push_str(BEHAVIOR_SUFFIX);
    let behavior_path = exe.with_file_name(name);
    let bytes = std::fs::read(&behavior_path)
        .map_err(|err| format!("read behavior '{}': {err}", behavior_path.display()))?;
    serde_json::from_slice(&bytes).map_err(|err| format!("decode behavior: {err}"))
}

/// Atomically claim the lowest free ordinal with `create_new` lock files.
/// Returns `None` when every slot is taken (the per-capture process bound).
fn claim_ordinal(records_dir: &std::path::Path) -> Result<Option<u64>, String> {
    for ordinal in 1..=(MAX_PROCESSES_PER_CAPTURE as u64) {
        let lock = records_dir.join(format!("{ordinal}.lock"));
        match std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&lock)
        {
            Ok(_) => return Ok(Some(ordinal)),
            Err(err) if err.kind() == std::io::ErrorKind::AlreadyExists => {}
            Err(err) => return Err(format!("claim ordinal {ordinal}: {err}")),
        }
    }
    Ok(None)
}

fn read_bounded_stdin(limit: u64) -> Result<String, String> {
    if limit == 0 {
        return Ok(String::new());
    }
    let mut buffer = Vec::new();
    std::io::stdin()
        .lock()
        .take(limit)
        .read_to_end(&mut buffer)
        .map_err(|err| format!("read stdin: {err}"))?;
    Ok(String::from_utf8_lossy(&buffer).into_owned())
}

fn sorted_env() -> Vec<(String, String)> {
    let mut pairs: Vec<(String, String)> = std::env::vars().collect();
    pairs.sort();
    pairs
}

fn current_dir_string() -> Result<String, String> {
    std::env::current_dir()
        .map(|dir| dir.to_string_lossy().into_owned())
        .map_err(|err| format!("current_dir: {err}"))
}

fn spawn_hang_child() -> Result<u32, String> {
    let exe = std::env::current_exe().map_err(|err| format!("current_exe: {err}"))?;
    let child = std::process::Command::new(exe)
        .env(HANG_CHILD_ENV, HANG_CHILD_ROLE)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn()
        .map_err(|err| format!("spawn hang child: {err}"))?;
    Ok(child.id())
}

fn write_record(
    records_dir: &std::path::Path,
    ordinal: u64,
    stage: &str,
    record: &CaptureRecord,
) -> Result<(), String> {
    let serialized = serde_json::to_vec(record).map_err(|err| format!("encode record: {err}"))?;
    let file_name = if stage == "done" {
        format!("{ordinal}.json")
    } else {
        format!("{ordinal}.start.json")
    };
    std::fs::write(records_dir.join(file_name), serialized)
        .map_err(|err| format!("write record: {err}"))
}

fn hang_forever() -> ! {
    loop {
        std::thread::sleep(std::time::Duration::from_secs(3600));
    }
}

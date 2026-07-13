use std::io::{Read, Write};
use std::path::Path;
use std::process::ExitCode;

use serde::Serialize;

fn main() -> ExitCode {
    if let Err(error) = run() {
        let _ = writeln!(std::io::stderr(), "psmux smoke fixture failed: {error}");
        return ExitCode::FAILURE;
    }
    ExitCode::SUCCESS
}

fn run() -> Result<(), Box<dyn std::error::Error>> {
    let mut args = std::env::args().skip(1);
    if args.next().as_deref() == Some("--record") {
        let output = args.next().ok_or("record mode requires an output path")?;
        return record(Path::new(&output), args.collect());
    }

    crossterm::terminal::enable_raw_mode()?;
    let _raw_mode = RawModeGuard;
    let mut output = std::io::stdout().lock();
    output.write_all(b"PSMUX_SMOKE_READY\r\n")?;
    output.flush()?;

    let mut byte = [0_u8; 1];
    loop {
        std::io::stdin().read_exact(&mut byte)?;
        writeln!(output, "PSMUX_BYTE_{:02X}", byte[0])?;
        output.flush()?;
        if byte[0] == 0x04 {
            return Ok(());
        }
    }
}

fn record(path: &Path, args: Vec<String>) -> Result<(), Box<dyn std::error::Error>> {
    let observation = LaunchObservation {
        args,
        cwd: std::env::current_dir()?.to_string_lossy().into_owned(),
        selected_environment: std::env::var("JEFE_FIXTURE_VALUE").ok(),
        tmux: std::env::var("TMUX").ok(),
        tmux_pane: std::env::var("TMUX_PANE").ok(),
        tmux_tmpdir: std::env::var("TMUX_TMPDIR").ok(),
    };
    std::fs::write(path, serde_json::to_vec(&observation)?)?;
    Ok(())
}

#[derive(Serialize)]
struct LaunchObservation {
    args: Vec<String>,
    cwd: String,
    selected_environment: Option<String>,
    tmux: Option<String>,
    tmux_pane: Option<String>,
    tmux_tmpdir: Option<String>,
}

struct RawModeGuard;

impl Drop for RawModeGuard {
    fn drop(&mut self) {
        let _ = crossterm::terminal::disable_raw_mode();
    }
}

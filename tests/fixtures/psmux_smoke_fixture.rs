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
    let marker = fixture_marker(&mut args)?;
    if marker.as_deref() == Some("--record") {
        let output = args.next().ok_or("record mode requires an output path")?;
        return record(Path::new(&output), args.collect());
    }

    crossterm::terminal::enable_raw_mode()?;
    let _raw_mode = RawModeGuard;
    #[cfg(windows)]
    let _input = {
        let input = winsafe::HSTD::GetStdHandle(winsafe::co::STD_HANDLE::INPUT)?;
        let mode = input.GetConsoleMode()?;
        input.SetConsoleMode(
            (mode
                & !(winsafe::co::CONSOLE::ENABLE_LINE_INPUT
                    | winsafe::co::CONSOLE::ENABLE_ECHO_INPUT
                    | winsafe::co::CONSOLE::ENABLE_PROCESSED_INPUT))
                | winsafe::co::CONSOLE::ENABLE_VIRTUAL_TERMINAL_INPUT,
        )?;
        input
    };
    let mut output = std::io::stdout().lock();
    for line in 0..80 {
        writeln!(output, "SCROLLBACK_{line:03}")?;
    }
    output.write_all(b"\x1b[31mCOLOR_RED\x1b[0m\r\n")?;
    output.write_all("UNICODE_Ω_界_e\u{301}\r\n".as_bytes())?;
    output.write_all(b"CURSOR_AB\x1b[D!\r\n")?;
    output.write_all(b"\x1b[?1000h\x1b[?1006h\x1b[?2004h\x1b[?1049hALT_SCREEN\r\n")?;
    output.write_all(b"\x1b[31mCOLOR_RED\x1b[0m\r\n")?;
    output.write_all("UNICODE_Ω_界_e\u{301}\r\n".as_bytes())?;
    output.write_all(b"CURSOR_AB\x1b[D!\r\n")?;
    output.write_all(b"\x1b]52;c;bmF0aXZlIGNsaXBib2FyZA==\x07")?;
    output.write_all(b"PSMUX_SMOKE_READY\r\n")?;
    if let Some(marker) = marker {
        writeln!(output, "PSMUX_MARKER_{marker}")?;
    }
    output.flush()?;

    let mut byte = [0_u8; 1];
    let mut received = Vec::new();
    loop {
        std::io::stdin().read_exact(&mut byte)?;
        received.push(byte[0]);
        writeln!(output, "PSMUX_BYTE_{:02X} BYTE_{:02X}", byte[0], byte[0])?;
        if byte[0] == 0x12 {
            output.write_all(b"\x1b[?1049lMAIN_SCREEN\r\nINPUT_HEX")?;
            for value in &received {
                write!(output, "_{value:02X}")?;
            }
            output.write_all(b"\r\n")?;
        }
        output.flush()?;
        if byte[0] == 0x04 {
            return Ok(());
        }
    }
}

fn fixture_marker(
    args: &mut impl Iterator<Item = String>,
) -> Result<Option<String>, Box<dyn std::error::Error>> {
    match args.next().as_deref() {
        Some("--marker") => Ok(Some(args.next().ok_or("marker mode requires a value")?)),
        Some("--record") => Ok(Some("--record".to_owned())),
        _ => Ok(None),
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

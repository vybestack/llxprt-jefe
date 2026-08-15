//! Deterministic app-under-test for the schema-1 harness ledger fixtures
//! (issue #380).
//!
//! A tiny full-screen terminal program with fully predictable output:
//! - prints `PROBE READY <cols>x<rows>` on start and after every resize
//!   (SIGWINCH is unavailable without unsafe, so it polls the PTY size);
//! - echoes unrecognized lines as `INPUT: <line>`;
//! - `run <name> [args..]` executes `<name>` from PATH (captures) and
//!   reports `RUN[<seq>] EXIT <code>` with a per-run sequence number so
//!   waits on consecutive identical commands stay deterministic;
//! - `write <path> <text>` writes a durable file relative to the cwd;
//! - `print-env <NAME>` echoes one environment variable;
//! - `exit` terminates with code 0.
//!
//! It never clears the screen, so frame assertions are plain line matching.

use std::io::{BufRead, Write};
use std::process::ExitCode;

#[path = "../panic_capture.rs"]
mod panic_capture;

#[path = "jefe-harness-probe/issue382_acceptance.rs"]
mod issue382_acceptance;

fn init_diagnostics() {
    jefe::logging::init();
    panic_capture::install_panic_hook();
}

fn main() -> ExitCode {
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(err) => {
            let _ = writeln!(std::io::stderr(), "probe I/O failure: {err}");
            ExitCode::from(4)
        }
    }
}

fn run() -> Result<(), String> {
    let mut last_size = terminal_size();
    print_line(&format!("PROBE READY {}x{}", last_size.0, last_size.1))?;
    print_line(&format!("PROBE PID {}", std::process::id()))?;
    let stdin = std::io::stdin();
    let (sender, receiver) = std::sync::mpsc::channel::<String>();
    std::thread::spawn(move || {
        for line in stdin.lock().lines() {
            match line {
                Ok(text) => {
                    if sender.send(text).is_err() {
                        break;
                    }
                }
                Err(_) => break,
            }
        }
    });
    let mut run_sequence = 0u64;
    loop {
        let size = terminal_size();
        if size != last_size {
            last_size = size;
            print_line(&format!("PROBE READY {}x{}", size.0, size.1))?;
        }
        match receiver.recv_timeout(std::time::Duration::from_millis(50)) {
            Ok(line) => {
                if handle_line(&line, &mut run_sequence)? {
                    return Ok(());
                }
            }
            Err(std::sync::mpsc::RecvTimeoutError::Timeout) => {}
            Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => return Ok(()),
        }
    }
}

/// Handle one input line; returns `true` on `exit`.
fn handle_line(line: &str, run_sequence: &mut u64) -> Result<bool, String> {
    let mut parts = line.split_whitespace();
    match parts.next() {
        Some("exit") => {
            print_line("PROBE EXITING")?;
            return Ok(true);
        }
        Some("run") => {
            *run_sequence += 1;
            run_command(
                *run_sequence,
                &parts.map(str::to_string).collect::<Vec<_>>(),
            )?;
        }
        Some("spawn") => {
            *run_sequence += 1;
            spawn_command(
                *run_sequence,
                &parts.map(str::to_string).collect::<Vec<_>>(),
            )?;
        }
        Some("write") => {
            let args: Vec<String> = parts.map(str::to_string).collect();
            write_file(&args)?;
        }
        Some("print-env") => {
            let name = parts.next().unwrap_or_default();
            let value = std::env::var(name).unwrap_or_else(|_| "<unset>".to_string());
            print_line(&format!("ENV {name}={value}"))?;
        }
        Some("issue382-stale-generation") => {
            let diagnostic = issue382_acceptance::stale_generation_diagnostic()?;
            print_line(&diagnostic)?;
        }
        Some("issue382-preflight") => {
            let diagnostic = issue382_acceptance::preflight_diagnostic()?;
            print_line(&diagnostic)?;
        }
        Some("panic-errors") => render_panic_errors()?,
        Some(_) | None => print_line(&format!("INPUT: {line}"))?,
    }
    Ok(false)
}
fn render_panic_errors() -> Result<(), String> {
    const PANIC_MARKER: &str = "issue-496-ui-panic";
    init_diagnostics();
    let _ = panic_capture::drain_panic_reports();
    let unwind = std::panic::catch_unwind(|| std::panic::panic_any(PANIC_MARKER.to_owned()));
    if unwind.is_ok() {
        return Err("panic fixture did not unwind".to_owned());
    }
    let mut state = jefe::state::AppState::default();
    let reports = panic_capture::drain_panic_reports();
    if reports.is_empty() {
        return Err("panic fixture did not capture a report".to_owned());
    }
    for report in reports {
        jefe::state::transition::commit_pure_site(
            &mut state,
            jefe::messages::AppMessage::Errors(report.into_errors_message()),
        );
    }
    print_line(&format!(
        "STATUS ERROR TITLE {}",
        state.last_error_title().as_deref().unwrap_or("<none>")
    ))?;
    state.errors_state.selected_index = Some(0);
    let content = jefe::selection::pane_content_lines(
        jefe::selection::SelectablePane::ErrorDetail,
        &state,
        None,
        &[],
        100,
        30,
    );
    for line in content.lines {
        print_line(&line)?;
    }
    print_line("PANIC ERRORS READY")
}

fn run_command(sequence: u64, args: &[String]) -> Result<(), String> {
    let Some(program) = args.first() else {
        return print_line(&format!("RUN[{sequence}] ERROR: missing program"));
    };
    let output = std::process::Command::new(program)
        .args(&args[1..])
        .stdin(std::process::Stdio::null())
        .output();
    match output {
        Ok(output) => {
            print_line(&format!(
                "RUN[{sequence}] EXIT {}",
                output.status.code().unwrap_or(-1)
            ))?;
            let stdout = String::from_utf8_lossy(&output.stdout);
            for line in stdout.lines() {
                print_line(&format!("RUN[{sequence}] OUT {line}"))?;
            }
            let stderr = String::from_utf8_lossy(&output.stderr);
            for line in stderr.lines() {
                print_line(&format!("RUN[{sequence}] ERR {line}"))?;
            }
        }
        Err(err) => print_line(&format!("RUN[{sequence}] ERROR: {err}"))?,
    }
    Ok(())
}

fn spawn_command(sequence: u64, args: &[String]) -> Result<(), String> {
    let Some(program) = args.first() else {
        return print_line(&format!("RUN[{sequence}] ERROR: missing program"));
    };
    match std::process::Command::new(program)
        .args(&args[1..])
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::inherit())
        .stderr(std::process::Stdio::inherit())
        .spawn()
    {
        Ok(child) => print_line(&format!("RUN[{sequence}] STARTED {}", child.id())),
        Err(err) => print_line(&format!("RUN[{sequence}] ERROR: {err}")),
    }
}
fn write_file(args: &[String]) -> Result<(), String> {
    let Some(path) = args.first() else {
        return print_line("WRITE ERROR: missing path");
    };
    let rest = &args[1..];
    let text = rest.join(" ");
    match std::fs::write(path, &text) {
        Ok(()) => print_line(&format!("WROTE {path}")),
        Err(err) => print_line(&format!("WRITE ERROR: {err}")),
    }
}

/// Poll the terminal size via `stty size` on the controlling terminal, with a
/// fixed default when `stty` is unavailable. `stty` is a fixed-path POSIX tool;
/// the harness gives the probe a real PTY so `stty size` reflects resize immediately.
fn terminal_size() -> (u16, u16) {
    for candidate in ["/bin/stty", "/usr/bin/stty"] {
        let Ok(output) = std::process::Command::new(candidate)
            .arg("size")
            .stdin(std::process::Stdio::inherit())
            .output()
        else {
            continue;
        };
        let text = String::from_utf8_lossy(&output.stdout);
        let mut parts = text.split_whitespace();
        if let (Some(rows), Some(cols)) = (parts.next(), parts.next())
            && let (Ok(rows), Ok(cols)) = (rows.parse::<u16>(), cols.parse::<u16>())
        {
            return (cols, rows);
        }
    }
    (80, 24)
}

fn print_line(text: &str) -> Result<(), String> {
    let mut stdout = std::io::stdout().lock();
    stdout
        .write_all(text.as_bytes())
        .and_then(|()| stdout.write_all(b"\r\n"))
        .and_then(|()| stdout.flush())
        .map_err(|err| format!("write stdout: {err}"))
}

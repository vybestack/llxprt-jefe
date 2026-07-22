//! Deterministic app-under-test for the schema-1 harness ledger fixtures
//! (issue #380).
//!
//! A tiny full-screen terminal program with fully predictable output:
//! - prints `PROBE READY <cols>x<rows>` on start and after every resize
//!   (SIGWINCH is unavailable without unsafe, so it polls the PTY size);
//! - echoes typed lines as `INPUT: <line>` and key bytes as `KEY: <hex>`;
//! - `run <name> [args..]` executes `<name>` from PATH (captures);
//! - `write <path> <text>` writes a durable file relative to the cwd;
//! - `print-env <NAME>` echoes one environment variable;
//! - `exit` terminates with code 0.
//!
//! It never clears the screen, so frame assertions are plain line matching.

use std::io::{BufRead, Write};
use std::process::ExitCode;

fn main() -> ExitCode {
    let mut last_size = terminal_size();
    print_line(&format!("PROBE READY {}x{}", last_size.0, last_size.1));
    print_line(&format!("PROBE PID {}", std::process::id()));
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
    loop {
        let size = terminal_size();
        if size != last_size {
            last_size = size;
            print_line(&format!("PROBE READY {}x{}", size.0, size.1));
        }
        match receiver.recv_timeout(std::time::Duration::from_millis(50)) {
            Ok(line) => {
                if handle_line(&line) {
                    return ExitCode::SUCCESS;
                }
            }
            Err(std::sync::mpsc::RecvTimeoutError::Timeout) => {}
            Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => return ExitCode::SUCCESS,
        }
    }
}

/// Handle one input line; returns `true` on `exit`.
fn handle_line(line: &str) -> bool {
    let mut parts = line.split_whitespace();
    match parts.next() {
        Some("exit") => {
            print_line("PROBE EXITING");
            return true;
        }
        Some("run") => run_command(&parts.map(str::to_string).collect::<Vec<_>>()),
        Some("write") => {
            let args: Vec<String> = parts.map(str::to_string).collect();
            write_file(&args);
        }
        Some("print-env") => {
            let name = parts.next().unwrap_or_default();
            let value = std::env::var(name).unwrap_or_else(|_| "<unset>".to_string());
            print_line(&format!("ENV {name}={value}"));
        }
        Some(_) | None => print_line(&format!("INPUT: {line}")),
    }
    false
}

fn run_command(args: &[String]) {
    let Some(program) = args.first() else {
        print_line("RUN ERROR: missing program");
        return;
    };
    let output = std::process::Command::new(program)
        .args(&args[1..])
        .output();
    match output {
        Ok(output) => {
            print_line(&format!("RUN EXIT {}", output.status.code().unwrap_or(-1)));
            let stdout = String::from_utf8_lossy(&output.stdout);
            for line in stdout.lines() {
                print_line(&format!("RUN OUT {line}"));
            }
        }
        Err(err) => print_line(&format!("RUN ERROR: {err}")),
    }
}

fn write_file(args: &[String]) {
    let (Some(path), rest) = (args.first(), &args[1.min(args.len())..]) else {
        print_line("WRITE ERROR: missing path");
        return;
    };
    let text = rest.join(" ");
    match std::fs::write(path, &text) {
        Ok(()) => print_line(&format!("WROTE {path}")),
        Err(err) => print_line(&format!("WRITE ERROR: {err}")),
    }
}

/// Poll the terminal size via the COLUMNS/LINES fallback and `stty size` on
/// the controlling terminal. `stty` is a fixed-path POSIX tool; the harness
/// gives the probe a real PTY so `stty size` reflects resize immediately.
fn terminal_size() -> (u16, u16) {
    for candidate in ["/bin/stty", "/usr/bin/stty"] {
        if !std::path::Path::new(candidate).exists() {
            continue;
        }
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

fn print_line(text: &str) {
    let mut stdout = std::io::stdout().lock();
    let _ = stdout.write_all(text.as_bytes());
    let _ = stdout.write_all(b"\r\n");
    let _ = stdout.flush();
}

//! Schema-1 harness entry point (issue #380).
//!
//! Reads one schema-1 scenario, executes it synchronously in a real PTY, and
//! prints exactly one deterministic redacted report (schema 1) to stdout.
//! Exit codes: 0 success, 2 validation, 4 I/O/process/assertion, 124
//! timeout. Schema-1 input is the only accepted format: a missing or wrong
//! `schema` field is a validation error. There is no legacy adapter (see
//! issue #397 for the forward migration of pre-schema scenarios).

#![cfg_attr(not(unix), allow(dead_code))]

use std::collections::BTreeSet;
use std::io::Write;
use std::path::PathBuf;
use std::process::ExitCode;

#[cfg(unix)]
use jefe::harness::v1::contract::Platform;
#[cfg(unix)]
use jefe::harness::v1::parse_scenario_v1;
#[cfg(unix)]
use jefe::harness::v1::redact::Redactor;
#[cfg(unix)]
use jefe::harness::v1::run;
#[cfg(unix)]
use jefe::harness::v1::runner::RunnerConfig;
use jefe::harness::v1::validate::is_valid_id;

#[cfg(not(unix))]
fn main() -> ExitCode {
    emit_stderr(
        "HAR-E005: the schema-1 harness requires a Unix PTY (macOS/Linux)
",
        4,
    )
}

#[cfg(unix)]
fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match parse_args(&args) {
        Ok(Command::Help) => emit_stdout(&format!("{}\n", usage()), 0),
        Ok(Command::Run(config)) => execute(&config),
        Err(message) => emit_stderr(&format!("HAR-E001: {message}\n\n{}\n", usage()), 2),
    }
}

enum Command {
    Help,
    Run(CliConfig),
}

struct CliConfig {
    scenario: PathBuf,
    shim_binary: PathBuf,
    installs: Vec<(String, PathBuf)>,
}

fn parse_args(args: &[String]) -> Result<Command, String> {
    let mut scenario: Option<PathBuf> = None;
    let mut shim: Option<PathBuf> = None;
    let mut installs: Vec<(String, PathBuf)> = Vec::new();
    let mut install_names = BTreeSet::new();
    let mut iter = args.iter();
    while let Some(arg) = iter.next() {
        match arg.as_str() {
            "--help" | "-h" => return Ok(Command::Help),
            "--scenario" => {
                let value = iter
                    .next()
                    .ok_or_else(|| "missing value for --scenario".to_string())?;
                scenario = Some(PathBuf::from(value));
            }
            "--shim-bin" => {
                let value = iter
                    .next()
                    .ok_or_else(|| "missing value for --shim-bin".to_string())?;
                shim = Some(PathBuf::from(value));
            }
            "--install" => {
                let value = iter
                    .next()
                    .ok_or_else(|| "missing value for --install".to_string())?;
                let (name, path) = value
                    .split_once('=')
                    .ok_or_else(|| format!("--install expects <name>=<path>, got '{value}'"))?;
                if !is_valid_id(name) || path.is_empty() {
                    return Err(format!(
                        "--install name must be a 1..=64 character identifier of [A-Za-z0-9._-], not '.' or '..': '{name}'"
                    ));
                }
                if !install_names.insert(name.to_string()) {
                    return Err(format!("duplicate --install name: '{name}'"));
                }
                installs.push((name.to_string(), PathBuf::from(path)));
            }
            other => return Err(format!("unknown argument: {other}")),
        }
    }
    let scenario = scenario.ok_or_else(|| "missing required --scenario".to_string())?;
    let shim_binary = match shim {
        Some(path) => path,
        None => default_shim_path()?,
    };
    Ok(Command::Run(CliConfig {
        scenario,
        shim_binary,
        installs,
    }))
}

/// The capture shim ships beside this binary; resolve it from the current
/// executable's directory (no PATH lookup).
fn default_shim_path() -> Result<PathBuf, String> {
    let exe = std::env::current_exe().map_err(|err| format!("current_exe: {err}"))?;
    let dir = exe
        .parent()
        .ok_or_else(|| "executable has no parent directory".to_string())?;
    Ok(dir.join("jefe-capture-shim"))
}

#[cfg(unix)]
fn execute(config: &CliConfig) -> ExitCode {
    let bytes = match std::fs::read(&config.scenario) {
        Ok(bytes) => bytes,
        Err(err) => {
            return emit_stderr(
                &format!(
                    "HAR-E005: read scenario '{}': {err}\n",
                    config.scenario.display()
                ),
                4,
            );
        }
    };
    let scenario = match parse_scenario_v1(&bytes) {
        Ok(scenario) => scenario,
        Err(err) => return emit_stderr(&format!("{err}\n"), err.exit_code()),
    };
    if Platform::current() != Some(scenario.platform) {
        return emit_stderr(
            "HAR-E005: scenario targets another platform; skipping is not success\n",
            4,
        );
    }
    let redactor = Redactor::new(&scenario.secrets);
    let outcome = run(
        &scenario,
        &RunnerConfig {
            shim_binary: config.shim_binary.clone(),
            installs: config.installs.clone(),
        },
    );
    let rendered = match outcome.report.to_redacted_json(&redactor) {
        Ok(rendered) => rendered,
        Err(err) => {
            let (detail, _) = redactor.redact(&err.to_string());
            return emit_stderr(&format!("{detail}\n"), err.exit_code());
        }
    };
    if let Err(err) = write_stdout(&format!("{rendered}\n")) {
        let (detail, _) = redactor.redact(&format!("HAR-E005: write report: {err}"));
        return emit_stderr(&format!("{detail}\n"), 4);
    }
    match outcome.error {
        None => ExitCode::SUCCESS,
        Some(err) => {
            let (detail, _) = redactor.redact(&err.to_string());
            emit_stderr(&format!("{detail}\n"), err.exit_code())
        }
    }
}

fn usage() -> String {
    "\
tmux_scenario - run a schema-1 jefe harness scenario in a real PTY

Usage: tmux_scenario --scenario <FILE> [--shim-bin <PATH>] [--install <name>=<path>]...

Options:
  --scenario <FILE>       Schema-1 scenario JSON file (required)
  --shim-bin <PATH>       Capture shim binary (default: jefe-capture-shim
                          beside this executable)
  --install <name>=<path> Copy a host binary into the workspace bin/<name>
                          so the hermetic PATH can resolve it (repeatable)
  -h, --help              Print this help and exit

Exit codes: 0 success, 2 validation, 4 I/O/process/assertion, 124 timeout."
        .to_string()
}

fn emit_stdout(message: &str, success_code: u8) -> ExitCode {
    match write_stdout(message) {
        Ok(()) => ExitCode::from(success_code),
        Err(err) => emit_stderr(
            &format!(
                "HAR-E005: write stdout: {err}
"
            ),
            4,
        ),
    }
}

fn emit_stderr(message: &str, exit_code: u8) -> ExitCode {
    let _ = write_stderr(message);
    ExitCode::from(exit_code)
}

fn write_stdout(message: &str) -> std::io::Result<()> {
    let mut stdout = std::io::stdout().lock();
    stdout.write_all(message.as_bytes())?;
    stdout.flush()
}

fn write_stderr(message: &str) -> std::io::Result<()> {
    let mut stderr = std::io::stderr().lock();
    stderr.write_all(message.as_bytes())?;
    stderr.flush()
}

#[cfg(test)]
mod tests {
    use super::parse_args;

    fn parse_error(args: &[&str]) -> String {
        let owned: Vec<String> = args.iter().map(|value| (*value).to_string()).collect();
        match parse_args(&owned) {
            Ok(_) => panic!("arguments must be rejected: {args:?}"),
            Err(error) => error,
        }
    }

    #[test]
    fn install_names_are_single_safe_identifiers() {
        let over_limit = "a".repeat(65);
        for name in [".", "..", "a/b", r"a\b", "/absolute", &over_limit] {
            let install = format!("{name}=/bin/true");
            let error = parse_error(&["--install", &install]);
            assert!(
                error.contains("--install name"),
                "unexpected error: {error}"
            );
        }
    }

    #[test]
    fn install_names_are_unique() {
        let error = parse_error(&[
            "--install",
            "tool=/bin/true",
            "--install",
            "tool=/bin/false",
        ]);
        assert_eq!(error, "duplicate --install name: 'tool'");
    }
}

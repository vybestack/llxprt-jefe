//! Thin CLI entry for the tmux-backed TUI harness.
//!
//! @plan PLAN-20260629-TMUX-HARNESS.P04
//! @requirement REQ-TMUX-HARNESS-004

use std::env;
use std::fs;
use std::io::Write;
use std::path::PathBuf;
use std::process::ExitCode;

use jefe::harness::{TmuxPaneSize, TmuxStartRequest, parse_scenario, run_tmux_scenario};

fn main() -> ExitCode {
    match CliArgs::parse(env::args().skip(1)) {
        Ok(args) => run(args),
        Err(ParseError::Help) => {
            write_stdout(&format!("{}\n", usage()));
            ExitCode::SUCCESS
        }
        Err(ParseError::Message(message)) => {
            write_stderr(&format!("{message}\n\n{}\n", usage()));
            ExitCode::from(2)
        }
    }
}

fn run(args: CliArgs) -> ExitCode {
    let json = match fs::read_to_string(&args.scenario) {
        Ok(value) => value,
        Err(err) => {
            write_stderr(&format!(
                "failed to read scenario '{}': {err}\n",
                args.scenario.display()
            ));
            return ExitCode::from(1);
        }
    };
    let scenario = match parse_scenario(&json) {
        Ok(value) => value,
        Err(err) => {
            write_stderr(&format!("failed to parse scenario: {err}\n"));
            return ExitCode::from(1);
        }
    };
    let request = start_request(&scenario, &args);
    match request.and_then(|req| run_tmux_scenario(&scenario, &req, args.out_dir.as_deref())) {
        Ok(summary) => {
            write_stdout(&format!("ok: {} steps\n", summary.steps_run));
            ExitCode::SUCCESS
        }
        Err(err) => {
            write_stderr(&format!("scenario failed: {err}\n"));
            ExitCode::from(1)
        }
    }
}

fn start_request(
    scenario: &jefe::harness::Scenario,
    args: &CliArgs,
) -> Result<TmuxStartRequest, jefe::harness::RunnerError> {
    let dims = TmuxPaneSize::new(
        scenario.config.cols,
        scenario.config.rows,
        scenario.config.history_limit,
    );
    let request = TmuxStartRequest::jefe(
        args.session.clone(),
        args.jefe_bin.clone(),
        args.config_dir.clone(),
        args.working_dir.clone(),
        dims,
    )
    .map_err(|err| jefe::harness::RunnerError::Driver(err.to_string()))?;
    Ok(request.with_keep_session(args.keep_session))
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct CliArgs {
    scenario: PathBuf,
    jefe_bin: PathBuf,
    config_dir: PathBuf,
    working_dir: PathBuf,
    session: String,
    out_dir: Option<PathBuf>,
    keep_session: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum ParseError {
    Help,
    Message(String),
}

impl ParseError {
    fn message(value: impl Into<String>) -> Self {
        Self::Message(value.into())
    }

    #[cfg(test)]
    fn contains(&self, needle: &str) -> bool {
        match self {
            Self::Help => false,
            Self::Message(message) => message.contains(needle),
        }
    }
}

impl std::fmt::Display for ParseError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Help => f.write_str("help requested"),
            Self::Message(message) => f.write_str(message),
        }
    }
}
impl CliArgs {
    fn parse<I>(mut args: I) -> Result<Self, ParseError>
    where
        I: Iterator<Item = String>,
    {
        let mut parsed = ParsedOptions::default();
        while let Some(arg) = args.next() {
            match arg.as_str() {
                "--scenario" => parsed.scenario = Some(next_value(&mut args, "--scenario")?),
                "--jefe-bin" => parsed.jefe_bin = Some(next_value(&mut args, "--jefe-bin")?),
                "--config" => parsed.config_dir = Some(next_value(&mut args, "--config")?),
                "--working-dir" => {
                    parsed.working_dir = Some(next_value(&mut args, "--working-dir")?);
                }
                "--session" => parsed.session = Some(next_value(&mut args, "--session")?),
                "--out-dir" => parsed.out_dir = Some(next_value(&mut args, "--out-dir")?),
                "--keep-session" => parsed.keep_session = true,
                "--help" | "-h" => return Err(ParseError::Help),
                other => apply_equals_option(&mut parsed, other)?,
            }
        }
        parsed.finish()
    }
}

#[derive(Default)]
struct ParsedOptions {
    scenario: Option<String>,
    jefe_bin: Option<String>,
    config_dir: Option<String>,
    working_dir: Option<String>,
    session: Option<String>,
    out_dir: Option<String>,
    keep_session: bool,
}

impl ParsedOptions {
    fn finish(self) -> Result<CliArgs, ParseError> {
        let working_dir = match self.working_dir {
            Some(value) => PathBuf::from(value),
            None => env::current_dir().map_err(|err| ParseError::message(err.to_string()))?,
        };
        Ok(CliArgs {
            scenario: required_path(self.scenario, "--scenario")?,
            jefe_bin: required_path(self.jefe_bin, "--jefe-bin")?,
            config_dir: required_path(self.config_dir, "--config")?,
            working_dir,
            session: self
                .session
                .unwrap_or_else(|| "jefe-harness-cli".to_string()),
            out_dir: self.out_dir.map(PathBuf::from),
            keep_session: self.keep_session,
        })
    }
}

fn next_value<I>(args: &mut I, flag: &str) -> Result<String, ParseError>
where
    I: Iterator<Item = String>,
{
    let value = args
        .next()
        .ok_or_else(|| ParseError::message(format!("missing value for {flag}")))?;
    if value.is_empty() || value.starts_with('-') {
        return Err(ParseError::message(format!("missing value for {flag}")));
    }
    Ok(value)
}

fn apply_equals_option(parsed: &mut ParsedOptions, arg: &str) -> Result<(), ParseError> {
    let Some((flag, value)) = arg.split_once('=') else {
        return Err(ParseError::message(format!("unknown argument: {arg}")));
    };
    if value.is_empty() || value.starts_with('-') {
        return Err(ParseError::message(format!("missing value for {flag}")));
    }
    match flag {
        "--scenario" => parsed.scenario = Some(value.to_string()),
        "--jefe-bin" => parsed.jefe_bin = Some(value.to_string()),
        "--config" => parsed.config_dir = Some(value.to_string()),
        "--working-dir" => parsed.working_dir = Some(value.to_string()),
        "--session" => parsed.session = Some(value.to_string()),
        "--out-dir" => parsed.out_dir = Some(value.to_string()),
        other => return Err(ParseError::message(format!("unknown argument: {other}"))),
    }
    Ok(())
}

fn required_path(value: Option<String>, flag: &str) -> Result<PathBuf, ParseError> {
    value
        .map(PathBuf::from)
        .ok_or_else(|| ParseError::message(format!("missing required {flag}")))
}

fn usage() -> String {
    "\
jefe-tmux-harness - run jefe TUI scenarios in tmux

Usage: jefe-tmux-harness --scenario <FILE> --jefe-bin <PATH> --config <DIR> [OPTIONS]

Options:
  --scenario <FILE>       Scenario JSON file to run
  --jefe-bin <PATH>       Path to the jefe binary under test
  --config <DIR>          Isolated jefe config directory for the run
  --working-dir <DIR>     Working directory for the tmux session
  --session <NAME>        Tmux session name (default: jefe-harness-cli)
  --out-dir <DIR>         Artifact output directory
  --keep-session          Keep tmux session alive after completion
  -h, --help              Print this help message and exit"
        .to_string()
}

fn write_stdout(message: &str) {
    let _ = std::io::stdout().write_all(message.as_bytes());
}

fn write_stderr(message: &str) {
    let _ = std::io::stderr().write_all(message.as_bytes());
}

#[cfg(test)]
mod tests {
    use super::CliArgs;

    #[test]
    fn parses_required_cli_args() {
        let args = CliArgs::parse(
            [
                "--scenario",
                "scenario.json",
                "--jefe-bin",
                "target/debug/jefe",
                "--config",
                "tmp-config",
                "--session",
                "s",
                "--out-dir",
                "artifacts",
                "--keep-session",
            ]
            .into_iter()
            .map(str::to_string),
        )
        .unwrap_or_else(|err| panic!("parse should succeed: {err}"));

        assert_eq!(args.session, "s");
        assert!(args.keep_session);
        assert!(args.out_dir.is_some());
    }

    #[test]
    fn parses_equals_style_cli_args() {
        let args = CliArgs::parse(
            [
                "--scenario=scenario.json",
                "--jefe-bin=target/debug/jefe",
                "--config=tmp-config",
                "--working-dir=.",
                "--session=s",
                "--out-dir=artifacts",
            ]
            .into_iter()
            .map(str::to_string),
        )
        .unwrap_or_else(|err| panic!("parse should succeed: {err}"));

        assert_eq!(args.session, "s");
        assert!(args.out_dir.is_some());
    }

    #[test]
    fn value_flag_rejects_following_flag_as_missing_value() {
        let err = CliArgs::parse(["--scenario", "--jefe-bin"].into_iter().map(str::to_string))
            .err()
            .unwrap_or_else(|| panic!("parse should fail"));
        assert!(err.contains("--scenario"));
    }

    #[test]
    fn equals_style_value_rejects_flag_like_value() {
        let err = CliArgs::parse(std::iter::once("--scenario=--jefe-bin").map(str::to_string))
            .err()
            .unwrap_or_else(|| panic!("parse should fail"));
        assert!(err.contains("--scenario"));
    }

    #[test]
    fn value_flag_rejects_help_flag_as_missing_value() {
        let err = CliArgs::parse(["--scenario", "--help"].into_iter().map(str::to_string))
            .err()
            .unwrap_or_else(|| panic!("parse should fail"));
        assert!(err.contains("--scenario"));
    }

    #[test]
    fn missing_required_cli_arg_fails() {
        let err = CliArgs::parse(std::iter::empty())
            .err()
            .unwrap_or_else(|| panic!("parse should fail"));
        assert!(err.contains("--scenario"));
    }
}

//! Tutorial-capture CLI: orchestrate documentation capture runs.
//!
//! Provides subcommands for preparing isolated run environments, running
//! deterministic local TUI capture, planning opt-in GitHub fixture mutations,
//! generating evidence reports, and cleaning up manifest-owned resources.
//!
//! @requirement REQ-TUTORIAL-CAPTURE-001

mod cleanup_cmd;
mod cli;
mod commands;
mod plan_github;
mod svg_helpers;
mod tmux_helpers;

#[cfg(test)]
#[path = "cli_parsing_tests.rs"]
mod cli_parsing_tests;

#[cfg(test)]
mod tests;

use std::env;
use std::process::ExitCode;

use cli::{CliArgs, Command, ParseError, usage, write_stderr, write_stdout};

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
    match args.command {
        Command::Prepare(opts) => commands::run_prepare(opts),
        Command::CaptureLocal(opts) => commands::run_capture_local(opts),
        Command::CaptureGithub(opts) => commands::run_capture_github(opts),
        Command::PlanGithub(opts) => commands::run_plan_github(opts),
        Command::Render(opts) => commands::run_render(&opts),
        Command::Report(opts) => commands::run_report(&opts.manifest_path),
        Command::Cleanup(opts) => cleanup_cmd::run_cleanup(&opts),
        Command::ValidateRuntime(opts) => commands::run_validate_runtime(opts),
    }
}

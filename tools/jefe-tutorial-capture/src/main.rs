//! Tutorial-capture CLI: orchestrate documentation capture runs.
//!
//! Provides subcommands for preparing isolated run environments, running
//! deterministic local TUI capture, planning opt-in GitHub fixture mutations,
//! generating evidence reports, and cleaning up manifest-owned resources.
//!
//! @requirement REQ-TUTORIAL-CAPTURE-001

mod cli_cmd;

#[cfg(test)]
#[path = "cli_cmd/cli_parsing_tests.rs"]
mod cli_parsing_tests;

#[cfg(test)]
#[path = "cli_cmd/tmux_request_tests.rs"]
mod tmux_request_tests;

#[cfg(test)]
#[path = "cli_cmd/cli_subcommand_tests.rs"]
mod cli_subcommand_tests;

#[cfg(test)]
#[path = "cli_cmd/status_suppress_hook_tests.rs"]
mod status_suppress_hook_tests;

use std::env;
use std::process::ExitCode;

use cli_cmd::cli::{CliArgs, Command, ParseError, usage, write_stderr, write_stdout};

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
        Command::Prepare(opts) => cli_cmd::commands::run_prepare(opts),
        Command::CaptureLocal(opts) => cli_cmd::commands::run_capture_local(opts),
        Command::CaptureGithub(opts) => cli_cmd::commands::run_capture_github(opts),
        Command::PlanGithub(opts) => cli_cmd::commands::run_plan_github(opts),
        Command::Render(opts) => cli_cmd::commands::run_render(&opts),
        Command::Report(opts) => cli_cmd::commands::run_report(&opts.manifest_path),
        Command::Cleanup(opts) => cli_cmd::cleanup_cmd::run_cleanup(&opts),
        Command::ValidateRuntime(opts) => cli_cmd::commands::run_validate_runtime(opts),
    }
}

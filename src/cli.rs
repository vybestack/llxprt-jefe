//! Command-line argument parsing.
//!
//! Minimal hand-rolled parser (no external dependency) consistent with the
//! existing approach. Supports `--version`/`-V`, `--help`/`-h`, and
//! `--config <dir>`/`-c <dir>` so multiple instances can run against fully
//! isolated config/state directories.

use std::path::PathBuf;

/// Parsed command-line arguments.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct CliArgs {
    /// `--version` / `-V` was requested.
    pub version: bool,
    /// `--help` / `-h` was requested.
    pub help: bool,
    /// Explicit config directory from `--config <dir>` / `-c <dir>`.
    pub config_dir: Option<PathBuf>,
    /// `doctor` subcommand was requested (issue #264).
    ///
    /// When true, the dispatcher in `main` runs the read-only diagnostic
    /// report before logging/TUI initialization and exits with the typed
    /// `DoctorOutcome` exit code.
    pub doctor: bool,
}

impl CliArgs {
    /// Whether the parsed arguments request the `doctor` subcommand.
    ///
    /// Provided as a focused predicate so dispatch sites read intent rather
    /// than touching the public field directly (issue #264).
    #[must_use]
    pub const fn is_doctor(&self) -> bool {
        self.doctor
    }
}

/// Error produced while parsing command-line arguments.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CliError {
    /// A flag that expects a value was given none.
    MissingValue(String),
    /// An unrecognized argument was encountered.
    UnknownArgument(String),
}

impl std::fmt::Display for CliError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::MissingValue(flag) => write!(f, "{flag} requires a path argument"),
            Self::UnknownArgument(arg) => write!(f, "unknown argument: {arg}"),
        }
    }
}

impl std::error::Error for CliError {}

/// Usage text shown for `--help`.
pub const USAGE: &str = "\
jefe - terminal manager for multiple llxprt coding agents

Usage: jefe [OPTIONS] [COMMAND]

Commands:
  doctor              Run read-only local readiness diagnostics and exit

Options:
  -c, --config <DIR>  Use <DIR> for settings.toml, state.json, and themes/,
                      isolating this instance from the default config/state
  -V, --version       Print version information and exit
  -h, --help          Print this help message and exit";

/// Parse command-line arguments from an iterator of program arguments
/// (excluding the program name).
///
/// Recognises `--version`/`-V`, `--help`/`-h`, the global `--config <dir>` /
/// `-c <dir>` flag (including the `=` forms), and the first-class `doctor`
/// subcommand (issue #264). The `doctor` subcommand accepts only the global
/// `--config` flag and rejects positional operands and any unsupported option
/// (in particular there is no `--json` and no `--copy`).
///
/// # Errors
///
/// Returns [`CliError`] if a value-taking flag is missing its value or an
/// unknown argument is supplied.
pub fn parse_args<I, S>(args: I) -> Result<CliArgs, CliError>
where
    I: IntoIterator<Item = S>,
    S: Into<String>,
{
    let mut result = CliArgs::default();
    let mut iter = args.into_iter().map(Into::into).peekable();

    while let Some(arg) = iter.next() {
        match arg.as_str() {
            "doctor" => {
                result.doctor = true;
                parse_doctor_flags(&mut iter, &mut result)?;
            }
            "--version" | "-V" => result.version = true,
            "--help" | "-h" => result.help = true,
            "--config" | "-c" => {
                set_config_value(&mut result, &arg, iter.next())?;
            }
            other => {
                if let Some(value) = other
                    .strip_prefix("--config=")
                    .or_else(|| other.strip_prefix("-c="))
                {
                    set_config_equals(&mut result, other, value)?;
                } else {
                    return Err(CliError::UnknownArgument(other.to_string()));
                }
            }
        }
    }

    Ok(result)
}

/// Parse the trailing flags accepted by the `doctor` subcommand.
///
/// `doctor` takes only the global `--config <dir>` / `-c <dir>` flag (and its
/// `=` forms). Positional operands and any other option are rejected so the
/// diagnostic surface stays minimal and JSON/clipboard flags remain explicit
/// non-goals (issue #264, decision D-06).
fn parse_doctor_flags<I>(
    iter: &mut std::iter::Peekable<I>,
    result: &mut CliArgs,
) -> Result<(), CliError>
where
    I: Iterator<Item = String>,
{
    while let Some(arg) = iter.next() {
        match arg.as_str() {
            "--config" | "-c" => set_config_value(result, &arg, iter.next())?,
            other => {
                if let Some(value) = other
                    .strip_prefix("--config=")
                    .or_else(|| other.strip_prefix("-c="))
                {
                    set_config_equals(result, other, value)?;
                } else {
                    return Err(CliError::UnknownArgument(other.to_string()));
                }
            }
        }
    }
    Ok(())
}

/// Apply a `--config <dir>` / `-c <dir>` flag value, rejecting empty/flag-like
/// tokens so a typo cannot silently swallow a flag.
fn set_config_value(
    result: &mut CliArgs,
    flag: &str,
    next: Option<String>,
) -> Result<(), CliError> {
    let value = next.ok_or_else(|| CliError::MissingValue(flag.to_string()))?;
    if value.is_empty() || value.starts_with('-') {
        return Err(CliError::MissingValue(flag.to_string()));
    }
    result.config_dir = Some(PathBuf::from(value));
    Ok(())
}

/// Apply the `--config=<dir>` / `-c=<dir>` equals form.
fn set_config_equals(result: &mut CliArgs, raw: &str, value: &str) -> Result<(), CliError> {
    if value.is_empty() {
        let flag = raw.split('=').next().unwrap_or(raw);
        return Err(CliError::MissingValue(flag.to_string()));
    }
    result.config_dir = Some(PathBuf::from(value));
    Ok(())
}

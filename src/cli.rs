//! Command-line argument parsing.
//!
//! Minimal hand-rolled parser (no external dependency) consistent with the
//! existing approach. Supports `--version`/`-V`, `--help`/`-h`, and
//! `--config <dir>`/`-c <dir>` so multiple instances can run against fully
//! isolated config/state directories.

use std::path::PathBuf;

/// Provider-free configuration recovery operation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConfigCommand {
    /// Print resolved persistence paths and provenance.
    Path,
    /// Parse and statically validate configuration documents.
    Validate,
    /// Print redacted effective configuration.
    ShowEffective { provenance: bool },
    /// Open the settings document in the configured editor.
    Edit,
    /// Check or rewrite owned settings syntax.
    Format { check: bool, migrate: bool },
    /// Migrate/import state through the atomic writer.
    MigrateState,
}

/// Arguments owned by `jefe explain binding`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExplainBindingArgs {
    /// Chord text to normalize and resolve.
    pub chord: String,
    /// Optional highest-precedence context.
    pub context: Option<String>,
}

/// Parsed command-line arguments.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct CliArgs {
    /// `--version` / `-V` was requested.
    pub version: bool,
    /// `--help` / `-h` was requested.
    pub help: bool,
    /// Explicit config directory from `--config <dir>` / `-c <dir>`.
    pub config_dir: Option<PathBuf>,
    /// Provider-free recovery command, when selected.
    pub command: Option<ConfigCommand>,
    /// Provider-free binding explanation, when selected.
    pub explain_binding: Option<ExplainBindingArgs>,
    /// `doctor` subcommand was requested (issue #264).
    pub doctor: bool,
}

impl CliArgs {
    /// Whether the parsed arguments request the `doctor` subcommand.
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
    /// A command that expects an operand was given none.
    MissingOperand(String),
    /// An unrecognized argument was encountered.
    UnknownArgument(String),
}

impl CliError {
    /// Return the standard command-line usage error exit code.
    #[must_use]
    pub const fn exit_code(&self) -> u8 {
        64
    }
}

impl std::fmt::Display for CliError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::MissingValue(flag) => write!(f, "{flag} requires a path argument"),
            Self::MissingOperand(command) => write!(f, "{command} requires a command operand"),
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
  config <COMMAND>    Run provider-free configuration recovery
  explain binding CHORD [--context ID]
                      Explain one resolved binding without starting the TUI

Options:
  -c, --config <DIR>  Use <DIR> for settings.toml, state.json, and themes/,
                      isolating this instance from the default config/state
  -V, --version       Print version information and exit
  -h, --help          Print this help message and exit";

/// Parse command-line arguments from an iterator of program arguments
/// (excluding the program name).
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
            "--config" | "-c" => set_config_value(&mut result, &arg, iter.next())?,
            "config" => parse_config_command(&mut result, &mut iter)?,
            "explain" => parse_explain_command(&mut result, &mut iter)?,
            other => parse_config_equals(&mut result, other)?,
        }
    }

    Ok(result)
}

/// Parse the trailing flags accepted by the `doctor` subcommand.
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
            other => parse_config_equals(result, other)?,
        }
    }
    Ok(())
}

fn set_config_value(
    result: &mut CliArgs,
    flag: &str,
    value: Option<String>,
) -> Result<(), CliError> {
    let value = value.ok_or_else(|| CliError::MissingValue(flag.to_owned()))?;
    if value.is_empty() || value.starts_with('-') {
        return Err(CliError::MissingValue(flag.to_owned()));
    }
    result.config_dir = Some(PathBuf::from(value));
    Ok(())
}

fn parse_config_equals(result: &mut CliArgs, argument: &str) -> Result<(), CliError> {
    let Some(value) = argument
        .strip_prefix("--config=")
        .or_else(|| argument.strip_prefix("-c="))
    else {
        return Err(CliError::UnknownArgument(argument.to_owned()));
    };
    if value.is_empty() {
        let flag = argument.split('=').next().unwrap_or(argument);
        return Err(CliError::MissingValue(flag.to_owned()));
    }
    result.config_dir = Some(PathBuf::from(value));
    Ok(())
}

#[derive(Default)]
struct RecoveryFlags {
    provenance: bool,
    check: bool,
    migrate: bool,
}

fn parse_config_command(
    result: &mut CliArgs,
    iter: &mut impl Iterator<Item = String>,
) -> Result<(), CliError> {
    let name = iter
        .next()
        .ok_or_else(|| CliError::MissingOperand("config".to_owned()))?;
    let mut flags = RecoveryFlags::default();
    while let Some(argument) = iter.next() {
        match argument.as_str() {
            "--config" | "-c" => set_config_value(result, &argument, iter.next())?,
            "--provenance" if name == "show-effective" => flags.provenance = true,
            "--check" if name == "format" => flags.check = true,
            "--migrate" if name == "format" => flags.migrate = true,
            other => parse_config_equals(result, other)?,
        }
    }
    result.command = Some(config_command(&name, flags)?);
    Ok(())
}

fn config_command(name: &str, flags: RecoveryFlags) -> Result<ConfigCommand, CliError> {
    match name {
        "path" => Ok(ConfigCommand::Path),
        "validate" => Ok(ConfigCommand::Validate),
        "show-effective" => Ok(ConfigCommand::ShowEffective {
            provenance: flags.provenance,
        }),
        "edit" => Ok(ConfigCommand::Edit),
        "format" => Ok(ConfigCommand::Format {
            check: flags.check,
            migrate: flags.migrate,
        }),
        "migrate-state" => Ok(ConfigCommand::MigrateState),
        other => Err(CliError::UnknownArgument(other.to_owned())),
    }
}

fn parse_explain_command(
    result: &mut CliArgs,
    iter: &mut impl Iterator<Item = String>,
) -> Result<(), CliError> {
    let subject = iter
        .next()
        .ok_or_else(|| CliError::MissingOperand("explain".to_owned()))?;
    if subject != "binding" {
        return Err(CliError::UnknownArgument(subject));
    }
    let chord = iter
        .next()
        .filter(|value| !value.starts_with('-'))
        .ok_or_else(|| CliError::MissingOperand("explain binding".to_owned()))?;
    let mut context = None;
    while let Some(argument) = iter.next() {
        match argument.as_str() {
            "--context" => context = Some(required_flag_value("--context", iter.next())?),
            "--config" | "-c" => set_config_value(result, &argument, iter.next())?,
            other => parse_config_equals(result, other)?,
        }
    }
    result.explain_binding = Some(ExplainBindingArgs { chord, context });
    Ok(())
}

fn required_flag_value(flag: &str, value: Option<String>) -> Result<String, CliError> {
    value
        .filter(|value| !value.is_empty() && !value.starts_with('-'))
        .ok_or_else(|| CliError::MissingValue(flag.to_owned()))
}

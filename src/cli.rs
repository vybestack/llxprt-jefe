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

Usage: jefe [OPTIONS]

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
/// Returns [`CliError`] if a value-taking flag is missing its value or if an
/// unknown argument is supplied.
pub fn parse_args<I, S>(args: I) -> Result<CliArgs, CliError>
where
    I: IntoIterator<Item = S>,
    S: Into<String>,
{
    let mut result = CliArgs::default();
    let mut iter = args.into_iter().map(Into::into);

    while let Some(arg) = iter.next() {
        match arg.as_str() {
            "--version" | "-V" => result.version = true,
            "--help" | "-h" => result.help = true,
            "--config" | "-c" => set_config_value(&mut result, &arg, iter.next())?,
            "config" => parse_config_command(&mut result, &mut iter)?,
            other => parse_config_equals(&mut result, other)?,
        }
    }

    Ok(result)
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

#[cfg(test)]
mod tests {
    use super::*;

    trait TestResultExt<T, E> {
        fn value_or_panic(self, context: &str) -> T;
        fn error_or_panic(self, context: &str) -> E;
    }

    impl<T, E: std::fmt::Debug> TestResultExt<T, E> for Result<T, E> {
        fn value_or_panic(self, context: &str) -> T {
            match self {
                Ok(value) => value,
                Err(error) => panic!("{context}: {error:?}"),
            }
        }

        fn error_or_panic(self, context: &str) -> E {
            match self {
                Ok(_) => panic!("{context}: expected error"),
                Err(error) => error,
            }
        }
    }

    fn parse(args: &[&str]) -> Result<CliArgs, CliError> {
        parse_args(args.iter().map(|s| (*s).to_string()))
    }

    #[test]
    fn empty_args_yield_defaults() {
        let parsed = parse(&[]).value_or_panic("should parse");
        assert_eq!(parsed, CliArgs::default());
        assert!(!parsed.version);
        assert!(!parsed.help);
        assert!(parsed.config_dir.is_none());
    }

    #[test]
    fn version_long_and_short() {
        assert!(parse(&["--version"]).value_or_panic("parse").version);
        assert!(parse(&["-V"]).value_or_panic("parse").version);
    }

    #[test]
    fn help_long_and_short() {
        assert!(parse(&["--help"]).value_or_panic("parse").help);
        assert!(parse(&["-h"]).value_or_panic("parse").help);
    }

    #[test]
    fn config_long_with_separate_value() {
        let parsed = parse(&["--config", "/tmp/jefe-dev"]).value_or_panic("parse");
        assert_eq!(parsed.config_dir, Some(PathBuf::from("/tmp/jefe-dev")));
    }

    #[test]
    fn config_short_with_separate_value() {
        let parsed = parse(&["-c", "/tmp/jefe-dev"]).value_or_panic("parse");
        assert_eq!(parsed.config_dir, Some(PathBuf::from("/tmp/jefe-dev")));
    }

    #[test]
    fn config_equals_form() {
        let parsed = parse(&["--config=/tmp/jefe-dev"]).value_or_panic("parse");
        assert_eq!(parsed.config_dir, Some(PathBuf::from("/tmp/jefe-dev")));

        let parsed = parse(&["-c=/tmp/jefe-dev"]).value_or_panic("parse");
        assert_eq!(parsed.config_dir, Some(PathBuf::from("/tmp/jefe-dev")));
    }

    #[test]
    fn config_missing_value_errors() {
        let err = parse(&["--config"]).error_or_panic("should error");
        assert_eq!(err, CliError::MissingValue("--config".to_string()));

        let err = parse(&["-c"]).error_or_panic("should error");
        assert_eq!(err, CliError::MissingValue("-c".to_string()));
    }

    #[test]
    fn config_rejects_following_flag_as_value() {
        let err = parse(&["--config", "--help"]).error_or_panic("should error");
        assert_eq!(err, CliError::MissingValue("--config".to_string()));

        let err = parse(&["-c", "-V"]).error_or_panic("should error");
        assert_eq!(err, CliError::MissingValue("-c".to_string()));
    }

    #[test]
    fn config_equals_form_allows_leading_dash_dir() {
        // The explicit `=value` form is unambiguous, so a directory whose name
        // starts with a dash is still accepted there.
        let parsed = parse(&["--config=-weird-dir"]).value_or_panic("parse");
        assert_eq!(parsed.config_dir, Some(PathBuf::from("-weird-dir")));
    }

    #[test]
    fn config_empty_equals_value_errors() {
        let err = parse(&["--config="]).error_or_panic("should error");
        assert_eq!(err, CliError::MissingValue("--config".to_string()));
    }

    #[test]
    fn unknown_argument_errors() {
        let err = parse(&["--nope"]).error_or_panic("should error");
        assert_eq!(err, CliError::UnknownArgument("--nope".to_string()));
    }

    #[test]
    fn combined_flags_parse() {
        let parsed = parse(&["--config", "/tmp/x", "--version"]).value_or_panic("parse");
        assert!(parsed.version);
        assert_eq!(parsed.config_dir, Some(PathBuf::from("/tmp/x")));
    }

    #[test]
    fn later_config_overrides_earlier() {
        let parsed = parse(&["-c", "/tmp/a", "-c", "/tmp/b"]).value_or_panic("parse");
        assert_eq!(parsed.config_dir, Some(PathBuf::from("/tmp/b")));
    }

    #[test]
    fn config_recovery_commands_parse_with_only_their_owned_flags() {
        let path =
            parse(&["config", "path", "--config", "/tmp/recovery"]).value_or_panic("path command");
        assert_eq!(path.config_dir, Some(PathBuf::from("/tmp/recovery")));
        assert_eq!(path.command, Some(ConfigCommand::Path));

        let effective = parse(&["config", "show-effective", "--provenance"])
            .value_or_panic("effective command");
        assert_eq!(
            effective.command,
            Some(ConfigCommand::ShowEffective { provenance: true })
        );

        let format =
            parse(&["config", "format", "--check", "--migrate"]).value_or_panic("format command");
        assert_eq!(
            format.command,
            Some(ConfigCommand::Format {
                check: true,
                migrate: true,
            })
        );

        for (name, expected) in [
            ("validate", ConfigCommand::Validate),
            ("edit", ConfigCommand::Edit),
            ("migrate-state", ConfigCommand::MigrateState),
        ] {
            assert_eq!(
                parse(&["config", name]).value_or_panic(name).command,
                Some(expected)
            );
        }
    }

    #[test]
    fn config_recovery_rejects_missing_command_and_foreign_flags_with_exit_64() {
        for args in [
            vec!["config"],
            vec!["config", "path", "--check"],
            vec!["config", "validate", "--provenance"],
            vec!["config", "format", "--provenance"],
            vec!["config", "unknown"],
        ] {
            let error = parse(&args).error_or_panic("invalid recovery syntax");
            assert_eq!(error.exit_code(), 64, "args: {args:?}");
        }
    }
}

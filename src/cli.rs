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
            "--config" | "-c" => {
                let value = iter
                    .next()
                    .ok_or_else(|| CliError::MissingValue(arg.clone()))?;
                // Reject empty values and flag-like tokens (e.g. a following
                // `--help`) so they aren't silently swallowed as a directory.
                if value.is_empty() || value.starts_with('-') {
                    return Err(CliError::MissingValue(arg.clone()));
                }
                result.config_dir = Some(PathBuf::from(value));
            }
            other => {
                // Support `--config=<dir>` / `-c=<dir>` forms.
                if let Some(value) = other
                    .strip_prefix("--config=")
                    .or_else(|| other.strip_prefix("-c="))
                {
                    if value.is_empty() {
                        return Err(CliError::MissingValue(
                            other.split('=').next().unwrap_or(other).to_string(),
                        ));
                    }
                    result.config_dir = Some(PathBuf::from(value));
                } else {
                    return Err(CliError::UnknownArgument(other.to_string()));
                }
            }
        }
    }

    Ok(result)
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
}

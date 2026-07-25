//! CLI argument parsing tests moved out of the lib target to stay under the
//! Clippy `large_stack_arrays` test-descriptor ceiling (issue #307).

use jefe::cli::{CliArgs, CliError, parse_args};
use std::path::PathBuf;

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

//! CLI argument parsing tests moved out of the lib target to stay under the
//! Clippy `large_stack_arrays` test-descriptor ceiling (issue #307).

use jefe::cli::{CliArgs, CliError, ConfigCommand, parse_args};
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

#[test]
fn config_recovery_commands_parse_with_only_their_owned_flags() {
    let path =
        parse(&["config", "path", "--config", "/tmp/recovery"]).value_or_panic("path command");
    assert_eq!(path.config_dir, Some(PathBuf::from("/tmp/recovery")));
    assert_eq!(path.command, Some(ConfigCommand::Path));

    let effective =
        parse(&["config", "show-effective", "--provenance"]).value_or_panic("effective command");
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

#[test]
fn explain_binding_parses_chord_context_and_config_in_owned_order() {
    let parsed = parse(&[
        "explain",
        "binding",
        "ctrl+j",
        "--context",
        "issues.list",
        "--config",
        "/tmp/explain",
    ])
    .value_or_panic("explain binding must parse");
    let Some(explain) = parsed.explain_binding else {
        panic!("explain binding arguments must be retained");
    };
    assert_eq!(explain.chord, "ctrl+j");
    assert_eq!(explain.context.as_deref(), Some("issues.list"));
    assert_eq!(parsed.config_dir, Some(PathBuf::from("/tmp/explain")));
}

#[test]
fn explain_binding_usage_errors_exit_64() {
    for args in [
        vec!["explain"],
        vec!["explain", "binding"],
        vec!["explain", "other", "j"],
        vec!["explain", "binding", "j", "--context"],
        vec!["explain", "binding", "j", "extra"],
    ] {
        let error = parse(&args).error_or_panic("invalid explain syntax");
        assert_eq!(error.exit_code(), 64, "args: {args:?}");
    }
}

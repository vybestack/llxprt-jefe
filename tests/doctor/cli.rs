//! RED contract: typed CLI parsing of the `jefe doctor` subcommand (issue #264,
//! decision D-06 / AC-04).
//!
//! These tests pin the *intended* parser surface for `doctor` before any
//! production code exists. They reference APIs under `jefe::cli` that the
//! implementation must expose; the target is expected not to compile until the
//! `doctor` command and its typed arguments land.
//!
//! Contract summarized from the plan:
//! - `doctor` is a first-class subcommand recognised by the hand-written parser.
//! - It accepts the existing global `--config <dir>` / `-c <dir>` flag (and the
//!   `=` forms) so diagnostics can target an isolated config/state directory.
//! - It rejects positional operands, repeated `--config`, and any unsupported
//!   option. In particular there is no `--json` and no `--copy` (non-goals).

use std::path::PathBuf;

use jefe::cli::{CliError, parse_args};

use crate::support::TestResultExt;

/// Parse a slice of program arguments (excluding the binary name) through the
/// shared `parse_args` entry point, matching the style of `tests/cli.rs`.
fn parse(args: &[&str]) -> Result<jefe::cli::CliArgs, CliError> {
    parse_args(args.iter().map(|s| (*s).to_string()))
}

#[test]
fn doctor_alone_parses_as_doctor_command() {
    let parsed = parse(&["doctor"]).test_unwrap("doctor alone should parse");
    assert!(parsed.is_doctor(), "doctor subcommand must be recognised");
    assert!(
        parsed.config_dir.is_none(),
        "doctor without --config must carry no config dir"
    );
}

#[test]
fn doctor_with_long_config_carries_config_dir() {
    let parsed = parse(&["doctor", "--config", "/tmp/jefe-doctor"])
        .test_unwrap("doctor --config <dir> should parse");
    assert!(parsed.is_doctor());
    assert_eq!(
        parsed.config_dir,
        Some(PathBuf::from("/tmp/jefe-doctor")),
        "doctor must propagate --config <dir>"
    );
}

#[test]
fn doctor_with_short_config_carries_config_dir() {
    let parsed =
        parse(&["doctor", "-c", "/tmp/jefe-doctor"]).test_unwrap("doctor -c <dir> should parse");
    assert!(parsed.is_doctor());
    assert_eq!(parsed.config_dir, Some(PathBuf::from("/tmp/jefe-doctor")));
}

#[test]
fn doctor_with_config_equals_form_carries_config_dir() {
    let parsed = parse(&["doctor", "--config=/tmp/jefe-doctor"])
        .test_unwrap("doctor --config=<dir> should parse");
    assert!(parsed.is_doctor());
    assert_eq!(parsed.config_dir, Some(PathBuf::from("/tmp/jefe-doctor")));

    let parsed =
        parse(&["doctor", "-c=/tmp/jefe-doctor"]).test_unwrap("doctor -c=<dir> should parse");
    assert!(parsed.is_doctor());
    assert_eq!(parsed.config_dir, Some(PathBuf::from("/tmp/jefe-doctor")));
}

#[test]
fn doctor_config_missing_value_errors() {
    let err = parse(&["doctor", "--config"])
        .err()
        .unwrap_or_else(|| panic!("doctor --config without a value must error"));
    assert_eq!(
        err,
        CliError::MissingValue("--config".to_string()),
        "doctor must reuse the global MissingValue contract for --config"
    );

    let err = parse(&["doctor", "-c"])
        .err()
        .unwrap_or_else(|| panic!("doctor -c without a value must error"));
    assert_eq!(err, CliError::MissingValue("-c".to_string()));
}

#[test]
fn doctor_rejects_extra_positional_operand() {
    // `doctor extra` is not a valid invocation: doctor takes no operands.
    let err = parse(&["doctor", "extra"])
        .err()
        .unwrap_or_else(|| panic!("doctor with an extra operand must error"));
    assert!(
        matches!(err, CliError::UnknownArgument(ref arg) if arg == "extra"),
        "extra operand must be reported as unknown, got {err:?}"
    );
}

#[test]
fn doctor_rejects_unsupported_option() {
    let err = parse(&["doctor", "--diagnose-everything"])
        .err()
        .unwrap_or_else(|| panic!("doctor with an unsupported option must error"));
    assert!(
        matches!(err, CliError::UnknownArgument(ref arg) if arg == "--diagnose-everything"),
        "unsupported option must be reported as unknown, got {err:?}"
    );
}

#[test]
fn doctor_rejects_json_flag() {
    // --json is an explicit non-goal (decision D-06).
    let err = parse(&["doctor", "--json"])
        .err()
        .unwrap_or_else(|| panic!("doctor --json must be rejected"));
    assert!(
        matches!(err, CliError::UnknownArgument(ref arg) if arg == "--json"),
        "--json is not supported, got {err:?}"
    );
}

#[test]
fn doctor_rejects_copy_flag() {
    // --copy is an explicit non-goal (decision D-06).
    let err = parse(&["doctor", "--copy"])
        .err()
        .unwrap_or_else(|| panic!("doctor --copy must be rejected"));
    assert!(
        matches!(err, CliError::UnknownArgument(ref arg) if arg == "--copy"),
        "--copy is not supported, got {err:?}"
    );
}

#[test]
fn doctor_does_not_set_version_or_help_flags() {
    let parsed = parse(&["doctor"]).test_unwrap("doctor alone should parse");
    assert!(
        !parsed.version,
        "doctor must not be confused with --version"
    );
    assert!(!parsed.help, "doctor must not be confused with --help");
}

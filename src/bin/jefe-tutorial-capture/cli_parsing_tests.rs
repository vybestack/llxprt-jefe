//! CLI argument-parsing tests for `jefe-tutorial-capture`.
//!
//! Extracted from `tests.rs` to keep file sizes under the project limit.

use crate::cli::{CliArgs, Command, ParseError, parse_runtime_profile};
use jefe::tutorial_capture::RuntimeProfile;
use std::path::PathBuf;

fn parse(args: &[&str]) -> Result<CliArgs, ParseError> {
    CliArgs::parse(args.iter().map(std::string::ToString::to_string))
}

#[test]
fn missing_subcommand_fails() {
    let err = parse(&[]).err().unwrap_or_else(|| panic!("should fail"));
    assert!(err.contains("missing subcommand"));
}

#[test]
fn unknown_subcommand_fails() {
    let err = parse(&["frobnicate"])
        .err()
        .unwrap_or_else(|| panic!("should fail"));
    assert!(err.contains("unknown subcommand"));
}

#[test]
fn prepare_defaults_run_id_and_base_dir() {
    let args = parse(&["prepare"]).unwrap_or_else(|e| panic!("parse: {e}"));
    match args.command {
        Command::Prepare(opts) => {
            assert!(opts.run_id.starts_with("tutorial-"));
            assert_eq!(opts.base_dir, PathBuf::from("/tmp/jefe-tutorial-capture"));
            assert_eq!(opts.scenario_name, "tutorial-capture-local");
            assert_eq!(opts.runtime_profile, "shim");
        }
        other => panic!("expected Prepare, got {other:?}"),
    }
}

#[test]
fn prepare_accepts_runtime_profile() {
    let args = parse(&[
        "prepare",
        "--run-id",
        "my-run",
        "--runtime-profile",
        "real-llxprt",
    ])
    .unwrap_or_else(|e| panic!("parse: {e}"));
    match args.command {
        Command::Prepare(opts) => {
            assert_eq!(opts.runtime_profile, "real-llxprt");
        }
        other => panic!("expected Prepare, got {other:?}"),
    }
}

#[test]
fn parse_runtime_profile_maps_strings_correctly() {
    assert!(matches!(
        parse_runtime_profile("shim"),
        Ok(RuntimeProfile::Shim)
    ));
    assert!(matches!(
        parse_runtime_profile("real-llxprt"),
        Ok(RuntimeProfile::RealLlxprt)
    ));
    assert!(matches!(
        parse_runtime_profile("real-code-puppy"),
        Ok(RuntimeProfile::RealCodePuppy)
    ));
    assert!(matches!(
        parse_runtime_profile("REAL_LLXPRT"),
        Ok(RuntimeProfile::RealLlxprt)
    ));
    assert!(parse_runtime_profile("unknown").is_err());
}

#[test]
fn prepare_accepts_all_options() {
    let args = parse(&["prepare", "--run-id", "my-run", "--base-dir", "/tmp/runs"])
        .unwrap_or_else(|e| panic!("parse: {e}"));
    match args.command {
        Command::Prepare(opts) => {
            assert_eq!(opts.run_id, "my-run");
            assert_eq!(opts.base_dir, PathBuf::from("/tmp/runs"));
        }
        other => panic!("expected Prepare, got {other:?}"),
    }
}

/// Finding: --shim-availability is parsed and persisted.
#[test]
fn prepare_accepts_shim_availability() {
    use jefe::tutorial_capture::ShimAvailability;
    let args = parse(&[
        "prepare",
        "--run-id",
        "my-run",
        "--shim-availability",
        "llxprt-only",
    ])
    .unwrap_or_else(|e| panic!("parse: {e}"));
    match args.command {
        Command::Prepare(opts) => {
            assert_eq!(opts.shim_availability, ShimAvailability::LlxprtOnly);
        }
        other => panic!("expected Prepare, got {other:?}"),
    }
}

/// Finding: --shim-availability defaults to both.
#[test]
fn prepare_shim_availability_defaults_to_both() {
    use jefe::tutorial_capture::ShimAvailability;
    let args = parse(&["prepare"]).unwrap_or_else(|e| panic!("parse: {e}"));
    match args.command {
        Command::Prepare(opts) => {
            assert_eq!(opts.shim_availability, ShimAvailability::Both);
        }
        other => panic!("expected Prepare, got {other:?}"),
    }
}

/// Finding: invalid --shim-availability is rejected.
#[test]
fn prepare_rejects_invalid_shim_availability() {
    let err = parse(&["prepare", "--shim-availability", "all"])
        .err()
        .unwrap_or_else(|| panic!("should fail"));
    assert!(err.contains("unknown shim availability"));
}

#[test]
fn capture_local_requires_manifest_and_scenario_and_jefe_bin() {
    let err = parse(&["capture-local"])
        .err()
        .unwrap_or_else(|| panic!("should fail"));
    assert!(err.contains("--manifest"));
}

#[test]
fn capture_local_parses_all_options() {
    let args = parse(&[
        "capture-local",
        "--manifest",
        "manifest.json",
        "--scenario",
        "scenario.json",
        "--jefe-bin",
        "target/debug/jefe",
        "--keep-session",
    ])
    .unwrap_or_else(|e| panic!("parse: {e}"));
    match args.command {
        Command::CaptureLocal(opts) => {
            assert_eq!(opts.manifest_path, PathBuf::from("manifest.json"));
            assert_eq!(opts.scenario_path, PathBuf::from("scenario.json"));
            assert_eq!(opts.jefe_bin, PathBuf::from("target/debug/jefe"));
            assert!(opts.keep_session);
        }
        other => panic!("expected CaptureLocal, got {other:?}"),
    }
}

#[test]
fn plan_github_requires_fixture_repo_and_run_id() {
    let err = parse(&["plan-github"])
        .err()
        .unwrap_or_else(|| panic!("should fail"));
    assert!(err.contains("--fixture-repo"));
}

#[test]
fn plan_github_parses_options() {
    let args = parse(&[
        "plan-github",
        "--fixture-repo",
        "fixture/test-repo",
        "--run-id",
        "run-001",
        "--allow-merge",
        "--dry-run",
        "--confirm-disposable",
    ])
    .unwrap_or_else(|e| panic!("parse: {e}"));
    match args.command {
        Command::PlanGithub(opts) => {
            assert_eq!(opts.fixture_repo, "fixture/test-repo");
            assert_eq!(opts.run_id, "run-001");
            assert!(opts.allow_merge);
            assert!(opts.dry_run);
            assert!(opts.confirm_disposable);
            assert!(opts.allow_repos.is_empty());
            assert!(opts.manifest_path.is_none());
        }
        other => panic!("expected PlanGithub, got {other:?}"),
    }
}

/// Finding #1: --allow-repo flag is parsed and collected into a list.
#[test]
fn plan_github_parses_allow_repo_flags() {
    let args = parse(&[
        "plan-github",
        "--fixture-repo",
        "fixture/target",
        "--run-id",
        "run-001",
        "--allow-repo",
        "fixture/allowed-a",
        "--allow-repo",
        "fixture/allowed-b",
        "--dry-run",
    ])
    .unwrap_or_else(|e| panic!("parse: {e}"));
    match args.command {
        Command::PlanGithub(opts) => {
            assert_eq!(opts.allow_repos.len(), 2);
            assert!(opts.allow_repos.contains(&"fixture/allowed-a".to_string()));
            assert!(opts.allow_repos.contains(&"fixture/allowed-b".to_string()));
        }
        other => panic!("expected PlanGithub, got {other:?}"),
    }
}

/// Finding #1: --manifest flag is parsed for plan-github.
#[test]
fn plan_github_parses_manifest_flag() {
    let args = parse(&[
        "plan-github",
        "--fixture-repo",
        "fixture/test",
        "--run-id",
        "run-001",
        "--manifest",
        "/tmp/run/run-manifest.json",
        "--dry-run",
    ])
    .unwrap_or_else(|e| panic!("parse: {e}"));
    match args.command {
        Command::PlanGithub(opts) => {
            assert_eq!(
                opts.manifest_path.as_deref(),
                Some(std::path::Path::new("/tmp/run/run-manifest.json"))
            );
        }
        other => panic!("expected PlanGithub, got {other:?}"),
    }
}

/// Finding #1: An arbitrary unconfigured target repo is refused because
/// the allowlist is built only from independent sources (env, file,
/// --allow-repo), never from the --fixture-repo target itself.
#[test]
fn plan_github_refuses_arbitrary_unconfigured_target() {
    use jefe::tutorial_capture::build_allowlist_from_sources;
    // Build allowlist WITHOUT the target repo — simulating no env/file/flag.
    let allowlist = build_allowlist_from_sources(None, None, &[]);
    assert!(
        !allowlist.is_allowed("arbitrary/unconfigured-repo"),
        "arbitrary unconfigured repo must be refused"
    );
    assert!(
        !allowlist.is_allowed("fixture/target"),
        "target repo must not self-allow"
    );
    // Even when --allow-repo lists a different repo, target is not auto-allowed.
    let allowlist2 = build_allowlist_from_sources(None, None, &["fixture/other"]);
    assert!(
        !allowlist2.is_allowed("fixture/target"),
        "target must not be auto-allowed when only a different repo is in the allowlist"
    );
    // When the target IS in the allowlist via --allow-repo, it is allowed.
    let allowlist3 = build_allowlist_from_sources(None, None, &["fixture/target"]);
    assert!(allowlist3.is_allowed("fixture/target"));
}

/// Finding #3: cleanup --dry-run and --confirm flags are parsed.
#[test]
fn cleanup_parses_dry_run_and_confirm_flags() {
    let args = parse(&[
        "cleanup",
        "--manifest",
        "manifest.json",
        "--dry-run",
        "--confirm",
        "--purge-evidence",
    ])
    .unwrap_or_else(|e| panic!("parse: {e}"));
    match args.command {
        Command::Cleanup(opts) => {
            assert!(opts.dry_run);
            assert!(opts.confirm);
            assert!(opts.purge_evidence);
        }
        other => panic!("expected Cleanup, got {other:?}"),
    }
}

#[test]
fn report_requires_manifest() {
    let err = parse(&["report"])
        .err()
        .unwrap_or_else(|| panic!("should fail"));
    assert!(err.contains("--manifest"));
}

#[test]
fn cleanup_requires_manifest() {
    let err = parse(&["cleanup"])
        .err()
        .unwrap_or_else(|| panic!("should fail"));
    assert!(err.contains("--manifest"));
}

#[test]
fn help_flag_returns_help_error() {
    assert!(matches!(parse(&["--help"]), Err(ParseError::Help)));
}

/// Finding: plan-github with invalid repo format (no slash) parses but the
/// CLI handler rejects it. This test verifies the format validator catches
/// malformed repo strings.
#[test]
fn plan_github_invalid_repo_format_detected_by_validator() {
    use jefe::tutorial_capture::is_valid_repo_format;
    assert!(!is_valid_repo_format("not-a-repo"));
    assert!(!is_valid_repo_format("owner/"));
    assert!(!is_valid_repo_format("/repo"));
    assert!(is_valid_repo_format("owner/repo"));
}

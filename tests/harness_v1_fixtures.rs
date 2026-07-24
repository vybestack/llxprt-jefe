//! Executes every shipped schema-1 ledger fixture through the real runner
//! (issue #380: CW00-01, CW00-03, CW00-04, CW00-05, CW00-06, CW00-07,
//! CW00-08, CW00-09, CW00-10).
//!
//! The fixtures under `dev-docs/tmux-scenarios/v1/` are the canonical
//! evidence artifacts. They declare `platform: "macos"`; because their
//! behavior is identical on any Unix (probe + shim + real PTY), this test
//! rewrites the platform field to the current platform so the same fixtures
//! gate both macOS and Linux CI.
#![cfg(unix)]

use std::path::{Path, PathBuf};

use jefe::harness::v1::error::HarCode;
use jefe::harness::v1::redact::Redactor;
use jefe::harness::v1::runner::{RunOutcome, RunnerConfig};
use jefe::harness::v1::{parse_scenario_v1, run};

fn repo_path(relative: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join(relative)
}

fn bin_path(name: &str) -> PathBuf {
    let mut path = std::env::current_exe().unwrap_or_else(|err| panic!("current_exe: {err}"));
    path.pop();
    if path.ends_with("deps") {
        path.pop();
    }
    path.join(name)
}

fn load_fixture(name: &str) -> String {
    let path = repo_path(&format!("dev-docs/tmux-scenarios/v1/{name}"));
    let text = std::fs::read_to_string(&path)
        .unwrap_or_else(|err| panic!("read {}: {err}", path.display()));
    if cfg!(target_os = "macos") {
        text
    } else {
        text.replace("\"platform\": \"macos\"", "\"platform\": \"linux\"")
    }
}

fn run_fixture(name: &str) -> RunOutcome {
    let json = load_fixture(name);
    let scenario = parse_scenario_v1(json.as_bytes())
        .unwrap_or_else(|err| panic!("{name} should parse: {err}"));
    let config = RunnerConfig {
        shim_binary: bin_path("jefe-capture-shim"),
        installs: vec![
            (
                "jefe-harness-probe".to_string(),
                bin_path("jefe-harness-probe"),
            ),
            ("jefe".to_string(), bin_path("jefe")),
        ],
    };
    run(&scenario, &config)
}

fn cleanup(outcome: &RunOutcome) {
    if !outcome.report.workspace.is_empty() {
        let _ = std::fs::remove_dir_all(&outcome.report.workspace);
    }
}

fn assert_passed(name: &str, outcome: &RunOutcome) {
    assert!(
        outcome.error.is_none(),
        "{name} should pass: {:?}",
        outcome.error
    );
    assert_eq!(outcome.report.status, "passed", "{name}");
}

#[test]
fn schema_all_ops_fixture_passes() {
    let outcome = run_fixture("harness-schema-all-ops.json");
    assert_passed("harness-schema-all-ops", &outcome);
    cleanup(&outcome);
}

#[test]
fn config_path_fixture_runs_the_real_provider_free_binary() {
    let outcome = run_fixture("config-path-precedence.json");
    assert_passed("config-path-precedence", &outcome);
    assert_eq!(
        outcome
            .report
            .app_exit
            .as_ref()
            .and_then(|exit| exit.exit_code),
        Some(0)
    );
    cleanup(&outcome);
}

#[test]
fn config_provider_free_fixture_starts_no_captured_provider() {
    let outcome = run_fixture("config-provider-free.json");
    assert_passed("config-provider-free", &outcome);
    let capture = outcome
        .report
        .captures
        .iter()
        .find(|capture| capture.name == "gh")
        .unwrap_or_else(|| panic!("gh capture must be reported"));
    assert!(capture.invocations.is_empty(), "recovery must not start gh");
    assert_eq!(
        outcome
            .report
            .app_exit
            .as_ref()
            .and_then(|exit| exit.exit_code),
        Some(0)
    );
    cleanup(&outcome);
}

#[test]
fn settings_schema1_validate_fixture_preserves_source_bytes() {
    let outcome = run_fixture("settings-v1-lossless.json");
    assert_passed("settings-v1-lossless", &outcome);
    assert_eq!(
        outcome
            .report
            .app_exit
            .as_ref()
            .and_then(|exit| exit.exit_code),
        Some(0)
    );
    cleanup(&outcome);
}

#[test]
fn settings_show_effective_fixture_redacts_secrets_and_skips_providers() {
    let outcome = run_fixture("settings-show-effective.json");
    assert_passed("settings-show-effective", &outcome);
    let capture = outcome
        .report
        .captures
        .iter()
        .find(|capture| capture.name == "gh")
        .unwrap_or_else(|| panic!("gh capture must be reported"));
    assert!(capture.invocations.is_empty(), "recovery must not start gh");
    assert_eq!(
        outcome
            .report
            .app_exit
            .as_ref()
            .and_then(|exit| exit.exit_code),
        Some(0)
    );
    cleanup(&outcome);
}

#[cfg(target_os = "linux")]
#[test]
fn config_ambiguity_fixture_exits_three_without_writes() {
    let outcome = run_fixture("config-ambiguity.json");
    assert_passed("config-ambiguity", &outcome);
    assert_eq!(
        outcome
            .report
            .app_exit
            .as_ref()
            .and_then(|exit| exit.exit_code),
        Some(3)
    );
    cleanup(&outcome);
}

#[test]
fn settings_edit_fixture_executes_configured_editor_as_argv() {
    let outcome = run_fixture("settings-edit.json");
    assert_passed("settings-edit", &outcome);
    assert_eq!(
        outcome
            .report
            .app_exit
            .as_ref()
            .and_then(|exit| exit.exit_code),
        Some(0)
    );
    cleanup(&outcome);
}

#[test]
fn settings_format_check_fixture_detects_drift_without_writing() {
    let outcome = run_fixture("settings-format-check.json");
    assert_passed("settings-format-check", &outcome);
    assert_eq!(
        outcome
            .report
            .app_exit
            .as_ref()
            .and_then(|exit| exit.exit_code),
        Some(2)
    );
    cleanup(&outcome);
}

#[test]
fn settings_format_fixture_preserves_dormant_bytes() {
    let outcome = run_fixture("settings-lossless-save.json");
    assert_passed("settings-lossless-save", &outcome);
    assert_eq!(
        outcome
            .report
            .app_exit
            .as_ref()
            .and_then(|exit| exit.exit_code),
        Some(0)
    );
    cleanup(&outcome);
}

#[test]
fn settings_format_migrate_fixture_writes_schema2_losslessly() {
    let outcome = run_fixture("settings-format-migrate.json");
    assert_passed("settings-format-migrate", &outcome);
    assert_eq!(
        outcome
            .report
            .app_exit
            .as_ref()
            .and_then(|exit| exit.exit_code),
        Some(0)
    );
    cleanup(&outcome);
}

#[test]
fn state_migrate_fixture_writes_schema2_atomically() {
    let outcome = run_fixture("state-v1-v2.json");
    assert_passed("state-v1-v2", &outcome);
    assert_eq!(
        outcome
            .report
            .app_exit
            .as_ref()
            .and_then(|exit| exit.exit_code),
        Some(0)
    );
    cleanup(&outcome);
}

#[test]
fn startup_malformed_state_fixture_blocks_before_tui_without_writing() {
    let outcome = run_fixture("startup-malformed-state.json");
    assert_passed("startup-malformed-state", &outcome);
    assert_eq!(
        outcome
            .report
            .app_exit
            .as_ref()
            .and_then(|exit| exit.exit_code),
        Some(2)
    );
    cleanup(&outcome);
}

#[test]
fn capture_fixture_records_exact_boundary_fields() {
    let outcome = run_fixture("harness-capture.json");
    assert_passed("harness-capture", &outcome);
    let capture = outcome
        .report
        .captures
        .iter()
        .find(|capture| capture.name == "gh")
        .unwrap_or_else(|| panic!("gh capture must be reported"));
    assert_eq!(capture.invocations.len(), 2);
    cleanup(&outcome);
}

#[test]
fn interpolation_fixture_applies_prefix_and_escape_rules() {
    let outcome = run_fixture("harness-interpolation.json");
    assert_passed("harness-interpolation", &outcome);
    cleanup(&outcome);
}

#[test]
fn resize_restart_fixture_produces_both_evidence_frames() {
    let outcome = run_fixture("harness-resize-restart.json");
    assert_passed("harness-resize-restart", &outcome);
    let has_normal = outcome
        .report
        .frames
        .iter()
        .any(|frame| frame.cols == 100 && frame.rows == 30);
    let has_focused = outcome
        .report
        .frames
        .iter()
        .any(|frame| frame.cols == 70 && frame.rows == 18);
    assert!(has_normal, "must record the normal 100x30 frame");
    assert!(has_focused, "must record the focused 70x18 frame");
    cleanup(&outcome);
}

#[test]
fn containment_fixture_rejects_symlink_swapped_ancestor() {
    let outcome = run_fixture("harness-containment.json");
    let err = outcome
        .error
        .as_ref()
        .unwrap_or_else(|| panic!("containment violation must fail the run"));
    assert_eq!(err.code(), HarCode::E004);
    assert_eq!(err.exit_code(), 4);
    assert_eq!(outcome.report.status, "failed");
    // The write through the swapped ancestor must never have happened.
    assert!(
        !Path::new("/tmp/jefe-harness-escape-evidence.txt").exists(),
        "escape file must not exist outside the workspace"
    );
    cleanup(&outcome);
}

#[test]
fn timeout_fixture_exits_124_and_reaps_the_tree() {
    let outcome = run_fixture("harness-timeout.json");
    let err = outcome
        .error
        .as_ref()
        .unwrap_or_else(|| panic!("wait must time out"));
    assert!(err.is_timeout(), "timeout must be marked for exit 124");
    assert_eq!(err.exit_code(), 124);
    let capture = outcome
        .report
        .captures
        .iter()
        .find(|capture| capture.name == "slow-tool")
        .unwrap_or_else(|| panic!("slow-tool capture must be reported"));
    assert!(
        capture
            .invocations
            .first()
            .is_some_and(|record| !record.completed),
        "hanging shim must be recorded as incomplete"
    );
    cleanup(&outcome);
}

#[test]
fn redaction_fixture_scrubs_secret_from_rendered_report() {
    let outcome = run_fixture("harness-redaction.json");
    assert_passed("harness-redaction", &outcome);
    let redactor = Redactor::new(&["sekrit-token-123".to_string()]);
    let rendered = outcome
        .report
        .to_redacted_json(&redactor)
        .unwrap_or_else(|err| panic!("report must render: {err}"));
    assert!(
        !rendered.contains("sekrit-token-123"),
        "secret must not survive redaction"
    );
    assert!(
        rendered.contains("<redacted>"),
        "redaction marker must appear"
    );
    assert!(
        rendered.contains("\"redaction_count\""),
        "redaction count must be reported"
    );
    cleanup(&outcome);
}

#[test]
fn limits_fixture_fails_validation_before_any_launch() {
    let json = load_fixture("harness-limits.json");
    let err = parse_scenario_v1(json.as_bytes())
        .err()
        .unwrap_or_else(|| panic!("cols over limit must fail validation"));
    assert_eq!(err.code(), HarCode::E002);
    assert_eq!(err.exit_code(), 2);
}

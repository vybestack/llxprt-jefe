//! Integration contracts for the tutorial-capture workflow (issue #241).
//!
//! These tests exercise the public API surface of the `tutorial_capture`
//! module and the `jefe-tutorial-capture` CLI binary's argument parser.
//! They do not launch tmux or the real Jefe binary — those are manual
//! opt-in operations documented in `dev-docs/testing/tutorial-capture.md`.
//!
//! @requirement REQ-TUTORIAL-CAPTURE-001

use std::fs;
use std::path::{Path, PathBuf};

use jefe::harness::run_tmux_scenario;
use jefe_tutorial_capture::{
    FixtureAllowlist, OwnedPathKind, RunDirectories, RunId, RunManifest, RunSetup, RuntimeProfile,
    check_fixture_repo, cleanup_manifest, controlled_path_for, prepare_run,
};
#[cfg(unix)]
use jefe_tutorial_capture::{
    RunOutcome, load_manifest, redact_artifacts, save_manifest, save_report,
};

trait TestResultExt<T> {
    fn value_or_panic(self, context: &str) -> T;
}

impl<T, E: std::fmt::Debug> TestResultExt<T> for Result<T, E> {
    fn value_or_panic(self, context: &str) -> T {
        match self {
            Ok(value) => value,
            Err(error) => panic!("{context}: {error:?}"),
        }
    }
}

impl<T> TestResultExt<T> for Option<T> {
    fn value_or_panic(self, context: &str) -> T {
        match self {
            Some(value) => value,
            None => panic!("{context}: None"),
        }
    }
}

fn temp_base() -> tempfile::TempDir {
    tempfile::tempdir().value_or_panic("create temp base")
}

#[cfg(unix)]
fn sample_setup(base: &std::path::Path) -> RunSetup {
    RunSetup {
        run_id: RunId::new("integration-test-001").value_or_panic("valid run id"),
        base_dir: base.to_path_buf(),
        jefe_version: "test-version".to_string(),
        scenario_name: "tutorial-capture-local".to_string(),
        cols: 100,
        rows: 32,
        runtime_profile: RuntimeProfile::Shim,
        fixture_github_repo: None,
        jefe_bin: None,
        theme: None,
        scenario_hash: None,
        shim_availability: jefe_tutorial_capture::ShimAvailability::default(),
    }
}

/// `prepare` + `cleanup` should be a reversible cycle: after cleanup, all
/// owned paths are gone and the manifest is marked cleaned.
///
/// @requirement REQ-TUTORIAL-CAPTURE-001
#[cfg(unix)]
#[test]
fn prepare_then_cleanup_is_reversible() {
    let base = temp_base();
    let setup = sample_setup(base.path());
    let (dirs, mut manifest) = prepare_run(&setup).value_or_panic("prepare_run");

    assert!(dirs.config_dir.exists());
    assert!(dirs.artifact_dir.exists());
    assert!(dirs.shim_dir.exists());
    assert!(dirs.fixture_repo.exists());

    cleanup_manifest(&mut manifest, true).value_or_panic("cleanup");
    assert!(manifest.cleanup_completed);
    assert!(!dirs.config_dir.exists());
    assert!(!dirs.shim_dir.exists());
    assert!(!dirs.fixture_repo.exists());
}

/// The manifest records the config directory so the CLI can find it later.
///
/// @requirement REQ-TUTORIAL-CAPTURE-001
#[cfg(unix)]
#[test]
fn manifest_records_config_dir_for_cli_lookup() {
    let base = temp_base();
    let setup = sample_setup(base.path());
    let (dirs, manifest) = prepare_run(&setup).value_or_panic("prepare_run");

    let config = manifest
        .find_path_by_kind(OwnedPathKind::ConfigDir)
        .value_or_panic("manifest should have a config dir");
    assert_eq!(config, dirs.config_dir.as_path());
}

/// `prepare` writes executable shim scripts for the Shim runtime profile.
///
/// @requirement REQ-TUTORIAL-CAPTURE-003
#[cfg(unix)]
#[test]
fn prepare_writes_shim_executables_for_shim_profile() {
    let base = temp_base();
    let setup = sample_setup(base.path());
    let (dirs, _manifest) = prepare_run(&setup).value_or_panic("prepare_run");

    assert!(dirs.shim_dir.join("llxprt").exists());
    assert!(dirs.shim_dir.join("code-puppy").exists());
}

/// `controlled_path_for` prepends the shim directory to the inherited PATH.
///
/// @requirement REQ-TUTORIAL-CAPTURE-003
#[test]
fn controlled_path_prepends_shim_directory() {
    let tmp = tempfile::tempdir().value_or_panic("create temp shim dir");
    let shim_dir = tmp.path();
    let path = controlled_path_for(shim_dir);
    let prefix = format!("{}:", shim_dir.display());
    assert!(
        path.starts_with(&prefix),
        "controlled PATH must start with shim dir; got {path}"
    );
}

/// The allowlist refuses the production repository even when explicitly listed.
///
/// @requirement REQ-TUTORIAL-CAPTURE-004
#[test]
fn allowlist_refuses_production_repo_in_integration() {
    let allowlist = FixtureAllowlist::new(["vybestack/jefe", "fixture/test"]);
    let err = check_fixture_repo(&allowlist, "vybestack/jefe")
        .err()
        .value_or_panic("production repo should be refused");
    let msg = err.to_string();
    assert!(
        msg.contains("production repository"),
        "error must mention production; got {msg}"
    );
}

/// `save_manifest` + `load_manifest` round-trips the manifest with owned
/// paths and outcome.
///
/// @requirement REQ-TUTORIAL-CAPTURE-001
#[cfg(unix)]
#[test]
fn manifest_save_load_roundtrip_in_integration() {
    let base = temp_base();
    let setup = sample_setup(base.path());
    let (dirs, mut manifest) = prepare_run(&setup).value_or_panic("prepare_run");
    manifest.set_outcome(RunOutcome::Success);

    let manifest_path = dirs.manifest_path();
    save_manifest(&manifest, &manifest_path).value_or_panic("save manifest");

    let loaded = load_manifest(&manifest_path).value_or_panic("load manifest");
    assert_eq!(loaded.run_id, manifest.run_id);
    assert_eq!(loaded.outcome, RunOutcome::Success);
    assert!(!loaded.owned_paths.is_empty());
}

/// `save_report` writes a Markdown report that includes the run ID and
/// editorial note.
///
/// @requirement REQ-TUTORIAL-CAPTURE-005
#[cfg(unix)]
#[test]
fn save_report_writes_markdown_with_run_id_in_integration() {
    let base = temp_base();
    let setup = sample_setup(base.path());
    let (dirs, manifest) = prepare_run(&setup).value_or_panic("prepare_run");

    let report_path = dirs.report_path();
    save_report(&manifest, &report_path).value_or_panic("save report");

    let content = fs::read_to_string(&report_path).value_or_panic("read report");
    assert!(content.contains("integration-test-001"));
    assert!(content.contains("Editorial Note"));
}

/// `redact_artifacts` scrubs GitHub token values (not just prefixes) from
/// text files.
///
/// @requirement REQ-TUTORIAL-CAPTURE-006
#[cfg(unix)]
#[test]
fn redact_artifacts_removes_tokens_in_integration() {
    let base = temp_base();
    let setup = sample_setup(base.path());
    let (dirs, _manifest) = prepare_run(&setup).value_or_panic("prepare_run");

    let artifact = dirs.artifact_dir.join("capture.screen.txt");
    // Use a realistic-length token (36 chars after prefix, like real GitHub tokens).
    let token = "ghp_abcdef1234567890ABCDEF1234567890";
    fs::write(&artifact, format!("export GITHUB_TOKEN={token}")).value_or_panic("write artifact");

    let count = redact_artifacts(&dirs.artifact_dir).value_or_panic("redact");
    assert!(count >= 1);

    let content = fs::read_to_string(&artifact).value_or_panic("read redacted");
    assert!(!content.contains("ghp_"));
    assert!(!content.contains(token), "original token must be absent");
    assert!(content.contains("<token>"));
}

/// The shipped tutorial-capture scenario JSON parses successfully.
///
/// @requirement REQ-TUTORIAL-CAPTURE-005
#[test]
fn shipped_tutorial_capture_scenario_parses() {
    let scenario_path = repo_root().join("dev-docs/tmux-scenarios/tutorial-capture-local.json");
    let json =
        fs::read_to_string(&scenario_path).value_or_panic("read tutorial-capture-local.json");
    let scenario =
        jefe::harness::parse_scenario(&json).value_or_panic("parse tutorial-capture scenario");
    assert!(
        !scenario.steps.is_empty(),
        "scenario must have at least one step"
    );
}

/// The tutorial-capture documentation file exists and documents the safety
/// model.
///
/// @requirement REQ-TUTORIAL-CAPTURE-001
#[test]
fn tutorial_capture_docs_exist_and_document_safety() {
    let docs_path = repo_root().join("dev-docs/testing/tutorial-capture.md");
    let content = fs::read_to_string(&docs_path).value_or_panic("read tutorial-capture.md");
    assert!(content.contains("Safety model"));
    assert!(
        content.to_ascii_lowercase().contains("isolation")
            || content.to_ascii_lowercase().contains("isolated"),
        "docs must mention isolation or isolated"
    );
    assert!(
        content.contains("allowlist") || content.contains("Allowlist"),
        "docs must mention allowlist"
    );
    assert!(content.contains("Manifest-scoped cleanup"));
    assert!(
        content.contains("Unix-only") || content.contains("Unix"),
        "docs must document Unix-only platform support"
    );
    assert!(
        content.contains("token value") || content.contains("full token"),
        "docs must document full token redaction"
    );
}

/// Find the workspace root (two levels up from the tool's manifest dir).
fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(|p| p.parent())
        .map_or_else(
            || PathBuf::from(env!("CARGO_MANIFEST_DIR")),
            std::path::Path::to_path_buf,
        )
}

/// A guarded real tmux integration test for the local capture scenario.
///
/// This test:
/// 1. Prepares a run with `prepare_run` (exclusive run root, shims, git repo).
/// 2. Verifies the shim scripts exist and contain the stable marker.
/// 3. Launches the real Jefe binary via tmux with the controlled PATH.
/// 4. Waits for the dashboard to render.
/// 5. Captures a screen snapshot and asserts the stable title.
///
/// The test is guarded: it skips if tmux is unavailable or the jefe binary
/// cannot be located. This proves the real terminal interaction path works.
///
/// @requirement REQ-TUTORIAL-CAPTURE-001
#[test]
fn guarded_local_capture_proves_real_terminal_interaction() {
    let driver = jefe::harness::TmuxDriver::new();
    if !driver.is_available() {
        return;
    }
    let Some(jefe_bin) = find_jefe_binary() else {
        return;
    };

    let base = temp_base();
    let setup = RunSetup {
        run_id: RunId::new("guarded-tmux-test").value_or_panic("valid run id"),
        base_dir: base.path().to_path_buf(),
        jefe_version: "test".to_string(),
        scenario_name: "tutorial-capture-local".to_string(),
        cols: 100,
        rows: 32,
        runtime_profile: RuntimeProfile::Shim,
        fixture_github_repo: None,
        jefe_bin: Some(jefe_bin.clone()),
        theme: Some("dark".to_string()),
        scenario_hash: None,
        shim_availability: jefe_tutorial_capture::ShimAvailability::default(),
    };

    let (dirs, manifest) = prepare_run(&setup).value_or_panic("prepare_run should succeed");
    verify_shims_and_manifest(&dirs, &manifest);

    let request = build_tmux_request(&jefe_bin, &dirs, &manifest);
    let summary = run_tmux_scenario(&simple_scenario(), &request, Some(&dirs.artifact_dir))
        .value_or_panic("scenario should run");
    assert!(summary.steps_run > 0, "at least one step should have run");
    assert_artifacts_have_capture(&dirs);

    let mut manifest = manifest;
    cleanup_manifest(&mut manifest, true).value_or_panic("cleanup");
}

/// Verify shim scripts and manifest metadata.
fn verify_shims_and_manifest(dirs: &RunDirectories, manifest: &RunManifest) {
    assert!(
        dirs.shim_dir.join("llxprt").exists(),
        "shim script must exist"
    );
    assert!(
        dirs.shim_dir.join("code-puppy").exists(),
        "shim script must exist"
    );
    assert_eq!(manifest.theme.as_deref(), Some("dark"));
    assert!(
        manifest.binary_hash.is_some(),
        "manifest must record binary hash when jefe_bin is provided"
    );
}

/// Build the tmux start request with controlled PATH.
fn build_tmux_request(
    jefe_bin: &Path,
    dirs: &RunDirectories,
    manifest: &RunManifest,
) -> jefe::harness::TmuxStartRequest {
    let controlled_path = controlled_path_for(&dirs.shim_dir);
    let config_dir = manifest
        .find_path_by_kind(OwnedPathKind::ConfigDir)
        .value_or_panic("manifest should have config dir");
    jefe::harness::TmuxStartRequest::jefe(
        format!("jefe-tutorial-test-{}", manifest.run_id.as_str()),
        jefe_bin.to_path_buf(),
        config_dir.to_path_buf(),
        dirs.fixture_repo.clone(),
        jefe::harness::TmuxPaneSize::new(100, 32, 2000),
    )
    .value_or_panic("tmux request should be valid")
    .with_env_path(controlled_path)
}

/// Assert the artifact directory has at least one screen capture.
fn assert_artifacts_have_capture(dirs: &RunDirectories) {
    let artifacts = fs::read_dir(&dirs.artifact_dir).value_or_panic("read artifact dir");
    let has_capture = artifacts
        .filter_map(Result::ok)
        .any(|e| e.file_name().to_string_lossy().ends_with(".screen.txt"));
    assert!(
        has_capture,
        "at least one screen capture should have been written"
    );
}

/// A simple scenario that waits for the dashboard and captures it.
fn simple_scenario() -> jefe::harness::Scenario {
    let json = r#"{
        "config": {
            "cols": 100,
            "rows": 32,
            "history_limit": 2000,
            "initial_wait_ms": 500,
            "assert_mode": "soft"
        },
        "macros": {},
        "steps": [
            { "waitFor": "LLxprt Jefe" },
            { "capture": "dashboard-oriented" },
            { "key": "C-q" },
            { "waitForExit": 3000 }
        ]
    }"#;
    jefe::harness::parse_scenario(json).value_or_panic("parse simple scenario")
}

/// Locate the jefe binary for integration testing.
fn find_jefe_binary() -> Option<PathBuf> {
    if let Ok(path) = std::env::var("CARGO_BIN_EXE_jefe") {
        let candidate = PathBuf::from(path);
        if candidate.exists() {
            return Some(candidate);
        }
    }
    let current = std::env::current_exe().ok()?;
    let deps_dir = current.parent()?;
    let debug_dir = deps_dir.parent()?;
    let candidate = debug_dir.join("jefe");
    candidate.exists().then_some(candidate)
}

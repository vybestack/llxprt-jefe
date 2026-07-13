//! Tmux helper and orchestration tests for `jefe-tutorial-capture`.
//!
//! Extracted from the original `tests.rs` to keep file sizes under the
//! project limit. CLI parsing tests live in `cli_parsing_tests.rs`.

use crate::cli::{CliArgs, Command, ParseError};
use crate::tmux_helpers;
use jefe::tutorial_capture::{
    ArtifactKind, OwnedPathKind, RunId, RunManifest, RunOutcome, RuntimeProfile, load_manifest,
    save_manifest,
};
use std::path::{Path, PathBuf};

fn parse(args: &[&str]) -> Result<CliArgs, ParseError> {
    CliArgs::parse(args.iter().map(std::string::ToString::to_string))
}

// ── build_tmux_request ────────────────────────────────────────────────

fn sample_manifest_for_tmux_request() -> RunManifest {
    let mut manifest = RunManifest::new(
        RunId::new("tmux-req-test").unwrap_or_else(|| panic!("valid run id")),
        "0.0.28",
        "tutorial-capture-local",
        100,
        32,
        RuntimeProfile::Shim,
    );
    manifest.add_owned_path(
        OwnedPathKind::ConfigDir,
        PathBuf::from("/tmp/test-run/config"),
    );
    manifest.set_fixture_repo(PathBuf::from("/tmp/test-run/fixture-repo"));
    manifest
}

#[test]
fn build_tmux_request_succeeds_with_config_dir() {
    let manifest = sample_manifest_for_tmux_request();
    let request = tmux_helpers::build_tmux_request(
        &manifest,
        Path::new("/tmp/jefe-bin"),
        "/tmp/shims:/usr/bin",
        false,
    )
    .unwrap_or_else(|e| panic!("build_tmux_request should succeed: {e}"));

    assert!(request.env_path.is_some());
    assert_eq!(request.env_path.as_deref(), Some("/tmp/shims:/usr/bin"));
    assert!(request.command.contains(&"--config".to_string()));
    assert!(
        request
            .command
            .contains(&"/tmp/test-run/config".to_string())
    );
    assert_eq!(
        request.working_dir,
        PathBuf::from("/tmp/test-run/fixture-repo")
    );
    assert!(!request.keep_session);
}

#[test]
fn build_tmux_request_fails_without_config_dir() {
    let mut manifest = sample_manifest_for_tmux_request();
    manifest.owned_paths.clear();

    let result = tmux_helpers::build_tmux_request(
        &manifest,
        Path::new("/tmp/jefe-bin"),
        "/tmp/shims:/usr/bin",
        false,
    );
    assert!(result.is_err());
    let err = result.err().unwrap_or_else(|| panic!("should be Err"));
    assert!(err.contains("config directory"));
}

#[test]
fn build_tmux_request_sets_keep_session_flag() {
    let manifest = sample_manifest_for_tmux_request();
    let request = tmux_helpers::build_tmux_request(
        &manifest,
        Path::new("/tmp/jefe-bin"),
        "/tmp/shims:/usr/bin",
        true,
    )
    .unwrap_or_else(|e| panic!("build_tmux_request should succeed: {e}"));
    assert!(request.keep_session);
}

// ── render subcommand ────────────────────────────────────────────────

#[test]
fn render_requires_manifest() {
    let err = parse(&["render"])
        .err()
        .unwrap_or_else(|| panic!("should fail"));
    assert!(err.contains("--manifest"));
}

#[test]
fn render_parses_manifest() {
    let args = parse(&["render", "--manifest", "run-manifest.json"])
        .unwrap_or_else(|e| panic!("parse: {e}"));
    match args.command {
        Command::Render(opts) => {
            assert_eq!(opts.manifest_path, PathBuf::from("run-manifest.json"));
        }
        other => panic!("expected Render, got {other:?}"),
    }
}

// ── capture-github subcommand ────────────────────────────────────────

#[test]
fn capture_github_requires_manifest_and_scenario_and_jefe_bin() {
    let err = parse(&["capture-github"])
        .err()
        .unwrap_or_else(|| panic!("should fail"));
    assert!(err.contains("--manifest"));
}

#[test]
fn capture_github_parses_all_options() {
    let args = parse(&[
        "capture-github",
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
        Command::CaptureGithub(opts) => {
            assert_eq!(opts.manifest_path, PathBuf::from("manifest.json"));
            assert_eq!(opts.scenario_path, PathBuf::from("scenario.json"));
            assert_eq!(opts.jefe_bin, PathBuf::from("target/debug/jefe"));
            assert!(opts.keep_session);
        }
        other => panic!("expected CaptureGithub, got {other:?}"),
    }
}

// ── finalize_manifest_success ────────────────────────────────────────

#[test]
fn finalize_manifest_success_adds_artifacts_and_sets_outcome() {
    let dir = tempfile::tempdir().unwrap_or_else(|e| panic!("tempdir: {e:?}"));
    let manifest_path = dir.path().join("run-manifest.json");
    let mut manifest = RunManifest::new(
        RunId::new("reg-test").unwrap_or_else(|| panic!("valid run id")),
        "0.0.28",
        "tutorial-capture-local",
        100,
        32,
        RuntimeProfile::Shim,
    );
    manifest.add_owned_path(OwnedPathKind::ConfigDir, dir.path().join("config"));
    save_manifest(&manifest, &manifest_path).unwrap_or_else(|e| panic!("save manifest: {e:?}"));
    tmux_helpers::finalize_manifest_success(
        &manifest,
        &manifest_path,
        &["dashboard-oriented".to_string()],
    );
    let reloaded = load_manifest(&manifest_path).unwrap_or_else(|e| panic!("load manifest: {e:?}"));
    assert_eq!(reloaded.artifacts.len(), 1);
    assert_eq!(reloaded.artifacts[0].label, "dashboard-oriented");
    assert_eq!(reloaded.artifacts[0].kind, ArtifactKind::ScreenCapture);
    assert_eq!(reloaded.outcome, RunOutcome::Success);
}

/// Finding #8: observed actions are derived from the actual scenario steps
/// executed (key presses, typed text, captures), not just from capture labels.
fn assert_observed_actions_from_scenario(reloaded: &RunManifest) {
    assert!(
        reloaded.observed_actions.len() >= 4,
        "should have at least 4 observed actions from scenario steps, got {}: {:?}",
        reloaded.observed_actions.len(),
        reloaded.observed_actions
    );
    assert!(
        reloaded
            .observed_actions
            .iter()
            .any(|a| a.keybinding == "n"),
        "key 'n' must be in observed actions: {:?}",
        reloaded.observed_actions
    );
    assert!(
        reloaded
            .observed_actions
            .iter()
            .any(|a| a.keybinding == "Enter"),
        "key 'Enter' must be in observed actions: {:?}",
        reloaded.observed_actions
    );
    assert!(
        reloaded
            .observed_actions
            .iter()
            .any(|a| a.keybinding.contains("fixture-repo")),
        "type step must be in observed actions: {:?}",
        reloaded.observed_actions
    );
    assert!(
        reloaded
            .observed_actions
            .iter()
            .any(|a| a.checkpoint.as_deref() == Some("dashboard")),
        "capture checkpoint must be in observed actions: {:?}",
        reloaded.observed_actions
    );
}

#[test]
fn finalize_manifest_with_scenario_derives_observed_actions_from_steps() {
    let dir = tempfile::tempdir().unwrap_or_else(|e| panic!("tempdir: {e:?}"));
    let manifest_path = dir.path().join("run-manifest.json");
    let mut manifest = RunManifest::new(
        RunId::new("obs-test").unwrap_or_else(|| panic!("valid run id")),
        "0.0.28",
        "tutorial-capture-local",
        100,
        32,
        RuntimeProfile::Shim,
    );
    manifest.add_owned_path(OwnedPathKind::ConfigDir, dir.path().join("config"));
    save_manifest(&manifest, &manifest_path).unwrap_or_else(|e| panic!("save manifest: {e:?}"));

    let scenario_json = r#"{
        "config": { "cols": 100, "rows": 32, "history_limit": 2000 },
        "macros": {},
        "steps": [
            { "waitFor": "LLxprt Jefe" },
            { "key": "n" },
            { "type": "fixture-repo" },
            { "key": "Enter" },
            { "capture": "dashboard" }
        ]
    }"#;
    let scenario = jefe::harness::parse_scenario(scenario_json)
        .unwrap_or_else(|e| panic!("parse scenario: {e}"));

    tmux_helpers::finalize_manifest_with_scenario(
        &manifest,
        &manifest_path,
        &["dashboard".to_string()],
        &[],
        Some(&scenario),
    );

    let reloaded = load_manifest(&manifest_path).unwrap_or_else(|e| panic!("load manifest: {e:?}"));
    assert_observed_actions_from_scenario(&reloaded);
}

// ── Tier B state seeding verification (issue #241 task #1) ───────────

/// Verify that `run_capture_github` seeds the isolated Jefe config so the
/// isolated Jefe sees the seeded repo/agent. Since we cannot use live
/// GitHub, we test the seeding path directly: create a fixture-clone
/// directory, seed state, and verify the config files contain the expected
/// repo/agent.
#[test]
fn seed_tier_b_state_makes_isolated_config_see_repo_and_agent() {
    use jefe::persistence::State;
    use jefe::tutorial_capture::{TierBStateSeed, seed_tier_b_state};

    let dir = tempfile::tempdir().unwrap_or_else(|e| panic!("tempdir: {e:?}"));
    let config_dir = dir.path().join("config");
    let fixture_clone = dir.path().join("fixture-clone");
    std::fs::create_dir_all(&config_dir).unwrap_or_else(|e| panic!("mkdir config: {e:?}"));
    std::fs::create_dir_all(&fixture_clone).unwrap_or_else(|e| panic!("mkdir clone: {e:?}"));

    let seed = TierBStateSeed {
        config_dir: config_dir.clone(),
        fixture_clone_path: fixture_clone.clone(),
        fixture_github_repo: "fixture/test-repo".to_string(),
        theme: "green-screen".to_string(),
        agent_name: "TutorialAgent".to_string(),
        agent_kind: jefe::domain::AgentKind::Llxprt,
    };
    let result = seed_tier_b_state(&seed).unwrap_or_else(|e| panic!("seed: {e:?}"));

    // Verify state.json has the repo pointing at fixture-clone.
    let state_json = std::fs::read_to_string(config_dir.join("state.json"))
        .unwrap_or_else(|e| panic!("read state.json: {e:?}"));
    let state: State =
        serde_json::from_str(&state_json).unwrap_or_else(|e| panic!("parse state.json: {e:?}"));
    assert_eq!(state.repositories.len(), 1, "must have 1 repository");
    assert_eq!(
        state.repositories[0].base_dir, fixture_clone,
        "repo base_dir must be fixture-clone"
    );
    assert_eq!(
        state.repositories[0].github_repo, "fixture/test-repo",
        "must have github repo association"
    );
    assert_eq!(state.agents.len(), 1, "must have 1 agent");
    assert_eq!(state.agents[0].name, "TutorialAgent");
    assert_eq!(
        state.agents[0].repository_id, result.repository_id,
        "agent must be bound to seeded repository"
    );

    // Verify settings.toml has the theme.
    let settings = std::fs::read_to_string(config_dir.join("settings.toml"))
        .unwrap_or_else(|e| panic!("read settings.toml: {e:?}"));
    assert!(
        settings.contains("green-screen"),
        "settings must contain green-screen theme"
    );
}

/// Verify that `build_github_tmux_request` uses fixture-clone as the
/// working directory (not fixture-repo), which is the directory the
/// seeded state points at.
#[test]
fn build_github_tmux_request_uses_fixture_clone_as_working_dir() {
    let manifest = sample_manifest_for_tmux_request();
    let manifest_dir = Path::new("/tmp/test-run");
    let request = tmux_helpers::build_github_tmux_request(
        &manifest,
        Path::new("/tmp/jefe-bin"),
        "/tmp/shims:/usr/bin",
        false,
        manifest_dir,
    )
    .unwrap_or_else(|e| panic!("build_github_tmux_request: {e}"));

    assert_eq!(
        request.working_dir,
        PathBuf::from("/tmp/test-run/fixture-clone"),
        "GitHub capture must launch from fixture-clone, not fixture-repo"
    );
    assert!(request.command.contains(&"--config".to_string()));
    assert!(
        request
            .command
            .contains(&"/tmp/test-run/config".to_string())
    );
}

// ── Task #4: Binary SHA-256, scenario SHA-256/name, resolved theme ──

/// Setup helper for enrichment tests: creates temp binary and scenario files.
fn setup_enrichment_test() -> (
    tempfile::TempDir,
    std::path::PathBuf,
    std::path::PathBuf,
    String,
) {
    use std::io::Write;
    let dir = tempfile::tempdir().unwrap_or_else(|e| panic!("tempdir: {e:?}"));

    let bin_path = dir.path().join("fake-jefe");
    let mut file = std::fs::File::create(&bin_path).unwrap_or_else(|e| panic!("create: {e}"));
    let bin_content = b"fake binary content for hashing";
    file.write_all(bin_content)
        .unwrap_or_else(|e| panic!("write: {e}"));
    drop(file);

    let scenario_path = dir.path().join("test-scenario.json");
    std::fs::write(
        &scenario_path,
        r#"{"config":{"cols":80,"rows":24,"history_limit":2000},"macros":{},"steps":[]}"#,
    )
    .unwrap_or_else(|e| panic!("write scenario: {e}"));

    let expected_hash = {
        use sha2::{Digest, Sha256};
        let mut hasher = Sha256::new();
        hasher.update(bin_content);
        format!("{:x}", hasher.finalize())
    };

    (dir, bin_path, scenario_path, expected_hash)
}

/// Test that the enrichment function computes the actual binary SHA-256
/// and scenario SHA-256, and resolves the theme.
#[test]
fn enrich_manifest_computes_binary_hash_scenario_hash_and_theme() {
    use crate::tmux_helpers;
    use jefe::tutorial_capture::RunManifest;

    let (_dir, bin_path, scenario_path, expected_hash) = setup_enrichment_test();

    let mut manifest = RunManifest::new(
        RunId::new("hash-test").unwrap_or_else(|| panic!("valid run id")),
        "0.0.28",
        "",
        80,
        24,
        RuntimeProfile::Shim,
    );

    tmux_helpers::enrich_manifest_with_capture_metadata(&mut manifest, &bin_path, &scenario_path);

    let hash = manifest
        .binary_hash
        .as_deref()
        .unwrap_or_else(|| panic!("binary hash must be computed"));
    assert_eq!(hash.len(), 64, "binary hash must be SHA-256 (64 hex chars)");
    assert_eq!(
        hash, expected_hash,
        "binary hash must match actual SHA-256 of the file content"
    );

    let scenario_hash = manifest
        .scenario_hash
        .as_deref()
        .unwrap_or_else(|| panic!("scenario hash must be computed"));
    assert_eq!(
        scenario_hash.len(),
        64,
        "scenario hash must be SHA-256 (64 hex chars)"
    );
    assert_eq!(manifest.scenario_name, "test-scenario");
    assert_eq!(
        manifest.theme.as_deref(),
        Some("green-screen"),
        "theme must be resolved (default: green-screen)"
    );
}

/// Test that enrichment always overwrites metadata for the current invocation.
/// This ensures re-runs with a different binary or scenario are accurately
/// reflected in the manifest.
#[test]
fn enrich_manifest_overwrites_metadata_for_current_invocation() {
    use crate::tmux_helpers;
    use jefe::tutorial_capture::RunManifest;

    let dir = tempfile::tempdir().unwrap_or_else(|e| panic!("tempdir: {e:?}"));
    let bin_path = dir.path().join("fake-jefe");
    std::fs::write(&bin_path, b"new content").unwrap_or_else(|_e| panic!("write bin"));
    let scenario_path = dir.path().join("scenario.json");
    std::fs::write(
        &scenario_path,
        r#"{"config":{"cols":80,"rows":24,"history_limit":2000},"macros":{},"steps":[]}"#,
    )
    .unwrap_or_else(|_| panic!("write scenario"));

    let mut manifest = RunManifest::new(
        RunId::new("overwrite-test").unwrap_or_else(|| panic!("valid run id")),
        "0.0.28",
        "existing-scenario",
        80,
        24,
        RuntimeProfile::Shim,
    );
    manifest.binary_hash = Some("preexisting-hash".to_string());
    manifest.scenario_hash = Some("preexisting-scenario-hash".to_string());
    manifest.theme = Some("light".to_string());

    tmux_helpers::enrich_manifest_with_capture_metadata(&mut manifest, &bin_path, &scenario_path);

    // Binary hash must be overwritten with the current binary's hash.
    assert_ne!(
        manifest.binary_hash.as_deref(),
        Some("preexisting-hash"),
        "must overwrite binary hash for current invocation"
    );
    assert!(manifest.binary_hash.is_some());

    // Scenario hash must be overwritten with the current scenario's hash.
    assert_ne!(
        manifest.scenario_hash.as_deref(),
        Some("preexisting-scenario-hash"),
        "must overwrite scenario hash for current invocation"
    );

    // Scenario name must be overwritten from the file stem.
    assert_eq!(
        manifest.scenario_name, "scenario",
        "must overwrite scenario name from file stem"
    );

    // Theme is preserved if already set (it's not metadata that changes
    // between invocations — it's a presentation choice).
    assert_eq!(
        manifest.theme.as_deref(),
        Some("light"),
        "theme must be preserved when explicitly set"
    );
}

/// Scenario hash failure clears the stale hash and records a discrepancy.
/// This prevents a stale hash from being trusted as the current scenario.
#[test]
fn enrich_manifest_clears_stale_hash_on_scenario_hash_failure() {
    let mut manifest = sample_manifest_for_tmux_request();
    // Set a stale hash from a prior run.
    manifest.scenario_hash = Some("stale-hash-from-prior-run".to_string());
    assert!(manifest.discrepancies.is_empty());

    // Use a nonexistent scenario path so hash computation fails.
    let bad_scenario = std::path::PathBuf::from("/nonexistent/scenario.json");
    let jefe_bin = std::path::PathBuf::from("/bin/true");
    crate::tmux_helpers::enrich_manifest_with_capture_metadata(
        &mut manifest,
        &jefe_bin,
        &bad_scenario,
    );

    // Stale hash must be cleared.
    assert!(
        manifest.scenario_hash.is_none(),
        "stale scenario hash must be cleared when recompute fails"
    );
    // A discrepancy must be recorded.
    assert!(
        !manifest.discrepancies.is_empty(),
        "must record a discrepancy for stale hash clearance"
    );
    assert!(
        manifest.discrepancies.iter().any(|d| d.contains("stale")),
        "discrepancy must mention stale hash: {:?}",
        manifest.discrepancies
    );
}

// ── Hard capture failure artifact discovery (Finding) ───────────────────

/// On hard capture failure, all produced screen captures (.screen.txt) and
/// ANSI captures (.screen.ansi) must be discovered and registered in the
/// manifest before the atomic save.
#[test]
fn discover_artifacts_registers_screen_captures_on_failure() {
    let base = tempfile::tempdir().unwrap_or_else(|e| panic!("temp dir: {e}"));
    let artifact_dir = base.path().join("artifacts");
    std::fs::create_dir_all(&artifact_dir).unwrap_or_else(|e| panic!("mkdir: {e}"));
    // Simulate artifacts produced before failure.
    std::fs::write(artifact_dir.join("step1.screen.txt"), "capture 1")
        .unwrap_or_else(|e| panic!("write step1: {e}"));
    std::fs::write(
        artifact_dir.join("step1.screen.ansi"),
        "\x1b[32mcapture\x1b[0m",
    )
    .unwrap_or_else(|e| panic!("write step1 ansi: {e}"));
    std::fs::write(artifact_dir.join("step2.screen.txt"), "capture 2")
        .unwrap_or_else(|e| panic!("write step2: {e}"));

    let mut manifest = sample_manifest_for_tmux_request();
    crate::tmux_helpers::discover_and_register_artifacts(&mut manifest, &artifact_dir);

    let labels: Vec<&str> = manifest
        .artifacts
        .iter()
        .map(|a| a.label.as_str())
        .collect();
    assert!(
        labels.contains(&"step1"),
        "must register step1 screen capture: {labels:?}"
    );
    assert!(
        labels.contains(&"step1-ansi"),
        "must register step1 ANSI capture: {labels:?}"
    );
    assert!(
        labels.contains(&"step2"),
        "must register step2 screen capture: {labels:?}"
    );
}

/// On hard failure, the manifest must reflect outcome=Failed but still
/// contain the discovered artifacts (strict late failure: artifacts
/// registered, outcome set to failed).
#[test]
fn discover_artifacts_strict_late_failure_registers_and_marks_failed() {
    let base = tempfile::tempdir().unwrap_or_else(|e| panic!("temp dir: {e}"));
    let artifact_dir = base.path().join("artifacts");
    std::fs::create_dir_all(&artifact_dir).unwrap_or_else(|e| panic!("mkdir: {e}"));
    std::fs::write(
        artifact_dir.join("before-crash.screen.txt"),
        "partial evidence",
    )
    .unwrap_or_else(|e| panic!("write before-crash: {e}"));

    let mut manifest = sample_manifest_for_tmux_request();
    crate::tmux_helpers::discover_and_register_artifacts(&mut manifest, &artifact_dir);
    manifest.set_outcome(RunOutcome::Failed);

    assert_eq!(manifest.outcome, RunOutcome::Failed);
    assert!(
        manifest.artifacts.iter().any(|a| a.label == "before-crash"),
        "must register partial evidence: {:?}",
        manifest
            .artifacts
            .iter()
            .map(|a| a.label.as_str())
            .collect::<Vec<_>>()
    );
}
// ── Finding #3: capture-github generates scenario from manifest ────────

/// Verify that `extract_scenario_params` generates a scenario with exact
/// manifest issue/PR identity (title, number), and that the scenario asserts
/// on these exact values before any action. This is the production
/// capture-github path: generate from manifest, not from a static generic file.
#[test]
fn finding3_capture_github_generates_scenario_from_manifest_exact_resources() {
    use jefe::tutorial_capture::{
        GitHubResource, GitHubResourceKind, extract_scenario_params, generate_tier_b_scenario,
    };

    let mut manifest = sample_manifest_for_tmux_request();
    manifest.run_id = RunId::new("run-241").unwrap_or_else(|| panic!("valid id"));
    manifest.add_github_resource(GitHubResource {
        kind: GitHubResourceKind::Issue,
        repository: "fixture/test".to_string(),
        identifier: "42".to_string(),
        url: Some("https://github.com/fixture/test/issues/42".to_string()),
        title: String::new(),
    });
    manifest.add_github_resource(GitHubResource {
        kind: GitHubResourceKind::Branch,
        repository: "fixture/test".to_string(),
        identifier: "tutorial-capture/run-241".to_string(),
        url: None,
        title: String::new(),
    });
    manifest.add_github_resource(GitHubResource {
        kind: GitHubResourceKind::PullRequest,
        repository: "fixture/test".to_string(),
        identifier: "7".to_string(),
        url: Some("https://github.com/fixture/test/pull/7".to_string()),
        title: String::new(),
    });

    let params = extract_scenario_params(&manifest, "TutorialAgent")
        .unwrap_or_else(|| panic!("should extract params from manifest"));
    let scenario_json = generate_tier_b_scenario(&params);

    // The scenario must inject the exact issue number and title.
    assert!(
        scenario_json.contains("\"42\""),
        "scenario must contain exact issue number: {scenario_json}"
    );
    assert!(
        scenario_json.contains("run-241"),
        "scenario must contain run-id in titles: {scenario_json}"
    );
    // The scenario must use filter+assert on exact identity BEFORE action.
    assert!(
        scenario_json.contains("\"waitFor\": \"42\""),
        "scenario must waitFor exact issue number before action: {scenario_json}"
    );
    assert!(
        scenario_json.contains("\"expect\""),
        "scenario must assert exact identity: {scenario_json}"
    );
}

//! Tests for `build_tmux_request` and manifest finalization.
//!
//! Extracted from `tests.rs` to keep file sizes under the project limit.

use crate::cli_cmd::tmux_helpers;
use jefe_tutorial_capture::{
    ArtifactKind, OwnedPathKind, RunId, RunManifest, RunOutcome, RuntimeProfile, load_manifest,
    save_manifest,
};
use std::path::PathBuf;

// ── build_tmux_request ────────────────────────────────────────────────

/// Setup for tmux request tests: creates a real temp directory tree so the
/// binary and working-directory existence checks pass.
struct TmuxRequestSetup {
    _dir: tempfile::TempDir,
    bin: PathBuf,
    config_dir: PathBuf,
    fixture_repo: PathBuf,
    fixture_clone: PathBuf,
    root: PathBuf,
}

impl TmuxRequestSetup {
    fn config_str(&self) -> String {
        self.config_dir.to_string_lossy().into_owned()
    }
}

fn tmux_request_setup() -> TmuxRequestSetup {
    let dir = tempfile::tempdir().unwrap_or_else(|e| panic!("create temp dir: {e:?}"));
    let root = dir.path().to_path_buf();
    let config_dir = root.join("config");
    let fixture_repo = root.join("fixture-repo");
    let fixture_clone = root.join("fixture-clone");
    std::fs::create_dir_all(&config_dir).unwrap_or_else(|e| panic!("create config dir: {e:?}"));
    std::fs::create_dir_all(&fixture_repo).unwrap_or_else(|e| panic!("create fixture-repo: {e:?}"));
    std::fs::create_dir_all(&fixture_clone)
        .unwrap_or_else(|e| panic!("create fixture-clone: {e:?}"));
    let bin = root.join("jefe");
    std::fs::write(
        &bin,
        b"#!/bin/sh
",
    )
    .unwrap_or_else(|e| panic!("write dummy bin: {e:?}"));
    TmuxRequestSetup {
        _dir: dir,
        bin,
        config_dir,
        fixture_repo,
        fixture_clone,
        root,
    }
}

fn manifest_from_setup(setup: &TmuxRequestSetup) -> RunManifest {
    let mut manifest = RunManifest::new(
        RunId::new("tmux-req-test").unwrap_or_else(|| panic!("valid run id")),
        "0.0.28",
        "tutorial-capture-local",
        100,
        32,
        RuntimeProfile::Shim,
    );
    manifest.add_owned_path(OwnedPathKind::ConfigDir, setup.config_dir.clone());
    manifest.add_owned_path(OwnedPathKind::FixtureClone, setup.fixture_clone.clone());
    manifest.set_fixture_repo(setup.fixture_repo.clone());
    manifest
}

#[test]
fn build_tmux_request_succeeds_with_config_dir() {
    let setup = tmux_request_setup();
    let manifest = manifest_from_setup(&setup);
    let request =
        tmux_helpers::build_tmux_request(&manifest, &setup.bin, "/tmp/shims:/usr/bin", false)
            .unwrap_or_else(|e| panic!("build_tmux_request should succeed: {e}"));

    assert!(request.env_path.is_some());
    assert_eq!(request.env_path.as_deref(), Some("/tmp/shims:/usr/bin"));
    assert!(request.command.contains(&"--config".to_string()));
    assert!(request.command.contains(&setup.config_str()));
    assert_eq!(request.working_dir, setup.fixture_repo);
    assert!(!request.keep_session);
    assert!(request.suppress_status_bar);
}

#[test]
fn build_tmux_request_fails_without_config_dir() {
    let setup = tmux_request_setup();
    let mut manifest = manifest_from_setup(&setup);
    manifest.owned_paths.clear();

    let result =
        tmux_helpers::build_tmux_request(&manifest, &setup.bin, "/tmp/shims:/usr/bin", false);
    assert!(result.is_err());
    let err = result.err().unwrap_or_else(|| panic!("should be Err"));
    assert!(err.contains("config directory"));
}

#[test]
fn build_tmux_request_sets_keep_session_flag() {
    let setup = tmux_request_setup();
    let manifest = manifest_from_setup(&setup);
    let request =
        tmux_helpers::build_tmux_request(&manifest, &setup.bin, "/tmp/shims:/usr/bin", true)
            .unwrap_or_else(|e| panic!("build_tmux_request should succeed: {e}"));
    assert!(request.keep_session);
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
    )
    .unwrap_or_else(|e| panic!("finalize manifest: {e}"));
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
    )
    .unwrap_or_else(|e| panic!("finalize manifest: {e}"));

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
    use jefe_tutorial_capture::{TierBStateSeed, seed_tier_b_state};

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
    let setup = tmux_request_setup();
    let manifest = manifest_from_setup(&setup);
    let request = tmux_helpers::build_github_tmux_request(
        &manifest,
        &setup.bin,
        "/tmp/shims:/usr/bin",
        false,
        &setup.root,
    )
    .unwrap_or_else(|e| panic!("build_github_tmux_request: {e}"));

    assert_eq!(
        request.working_dir, setup.fixture_clone,
        "GitHub capture must launch from fixture-clone, not fixture-repo"
    );
    assert!(request.command.contains(&"--config".to_string()));
    assert!(request.command.contains(&setup.config_str()));
    assert!(request.suppress_status_bar);
    assert!(request.extra_env.iter().any(|(key, value)| {
        key == "JEFE_SOCKET_PATH"
            && value == &setup.root.join("jefe-runtime.sock").to_string_lossy()
    }));
}

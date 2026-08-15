//! CWR1-01/CWR1-05/CWR1-07: the one publication boundary.

use std::fs;
use std::sync::Arc;

use jefe::persistence::State;
use jefe::persistence::paths::plan_state_import_source;
use jefe::startup_commit::{StartupCommitFailure, commit_candidate};

use super::support::{build, config_root, publish_settings, resolve_paths, scan_roots};
use super::transaction_support::{
    EmptyEnv, Scene, assert_nothing_spawned, fast_bounds, process_budget, process_is_gone, read_pid,
};

fn state_import_plan(root: &std::path::Path) -> jefe::persistence::paths::StateImportPlan {
    let source = root.join("legacy-state.json");
    let target = root.join("config").join("state.json");
    let bytes = serde_json::to_vec_pretty(&State::default_with_version())
        .unwrap_or_else(|error| panic!("state serialization: {error}"));
    fs::write(&source, bytes).unwrap_or_else(|error| panic!("legacy state write: {error}"));
    plan_state_import_source(&source, &target)
        .unwrap_or_else(|error| panic!("state import plan: {error:?}"))
}

#[test]
fn provider_free_commit_installs_deferred_state_and_returns_one_aggregate_identity() {
    let temp = tempfile::tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
    let config = config_root(temp.path());
    fs::create_dir_all(&config).unwrap_or_else(|error| panic!("config dir: {error}"));
    let paths = resolve_paths(&config);
    let inventory = scan_roots(&[]);
    let settings = publish_settings(&inventory, "settings_schema = 2\n");
    let candidate = build(&paths, &inventory, &settings, temp.path())
        .unwrap_or_else(|error| panic!("candidate: {error}"));
    let plan = state_import_plan(temp.path());

    assert!(!paths.state.path.exists(), "planning must not write state");
    let commit = commit_candidate(candidate, Some(plan), &fast_bounds(), &EmptyEnv)
        .unwrap_or_else(|error| panic!("commit: {error}"));

    assert!(
        paths.state.path.is_file(),
        "commit installs state exactly once"
    );
    assert!(!commit.providers.has_persistent());
    assert!(commit.workbench.provider_catalog().is_empty());
    assert!(commit.workbench.provider_ready().is_some());
    let same_identity = Arc::clone(&commit.workbench);
    assert!(Arc::ptr_eq(&commit.workbench, &same_identity));
}

#[test]
fn final_import_conflict_returns_no_commit_and_preserves_conflicting_bytes() {
    let temp = tempfile::tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
    let config = config_root(temp.path());

    fs::create_dir_all(&config).unwrap_or_else(|error| panic!("config dir: {error}"));
    let paths = resolve_paths(&config);
    let inventory = scan_roots(&[]);
    let settings = publish_settings(&inventory, "settings_schema = 2\n");
    let candidate = build(&paths, &inventory, &settings, temp.path())
        .unwrap_or_else(|error| panic!("candidate: {error}"));
    let plan = state_import_plan(temp.path());
    let conflict = b"concurrent durable state";
    fs::write(&paths.state.path, conflict)
        .unwrap_or_else(|error| panic!("conflict write: {error}"));

    let Err(error) = commit_candidate(candidate, Some(plan), &fast_bounds(), &EmptyEnv) else {
        panic!("an import conflict must publish no commit");
    };

    assert!(matches!(error, StartupCommitFailure::StateImport(_)));
    assert_eq!(
        fs::read(&paths.state.path).unwrap_or_else(|error| panic!("conflict read: {error}")),
        conflict
    );
}

#[test]
fn one_shot_commit_starts_no_process_and_owns_no_persistent_session() {
    let _process_budget = process_budget();
    let scene = Scene::new();
    scene.stage_one_shot("plugin.oneshot");
    let candidate = scene.build_workbench(&["plugin.oneshot"]);

    let commit = commit_candidate(candidate, None, &fast_bounds(), &EmptyEnv)
        .unwrap_or_else(|error| panic!("one-shot commit: {error}"));

    assert!(!commit.providers.has_persistent());
    assert_nothing_spawned(&scene);
}

#[test]
fn provider_failure_leaves_the_deferred_state_target_absent() {
    let _process_budget = process_budget();
    let scene = Scene::new();
    scene.stage_missing_binary("plugin.required");
    let candidate = scene.build_workbench(&["plugin.required"]);
    let target = scene.config.join("state.json");
    let plan = state_import_plan(
        scene
            .config
            .parent()
            .unwrap_or_else(|| panic!("scene config has a parent")),
    );

    let Err(error) = commit_candidate(candidate, Some(plan), &fast_bounds(), &EmptyEnv) else {
        panic!("provider preparation failure must publish no commit");
    };
    assert!(matches!(error, StartupCommitFailure::Provider(_)));
    assert!(!target.exists(), "provider failure must not install state");
}

#[test]
fn final_import_conflict_reaps_every_ready_provider_before_refusal() {
    let _process_budget = process_budget();
    let scene = Scene::new();
    scene.stage_required("plugin.required", "persistent-ready");
    let candidate = scene.build_workbench(&["plugin.required"]);
    let root = scene
        .config
        .parent()
        .unwrap_or_else(|| panic!("scene config has a parent"));
    let plan = state_import_plan(root);
    let target = scene.config.join("state.json");
    fs::write(&target, b"concurrent durable state")
        .unwrap_or_else(|error| panic!("conflict write: {error}"));

    let Err(error) = commit_candidate(candidate, Some(plan), &fast_bounds(), &EmptyEnv) else {
        panic!("an import conflict must publish no commit");
    };
    let pid = read_pid(&scene.record_dir, "plugin.required");

    assert!(matches!(error, StartupCommitFailure::StateImport(_)));
    assert!(
        process_is_gone(pid),
        "provider {pid} must be reaped before refusal is observable"
    );
    assert_eq!(
        fs::read(&target).unwrap_or_else(|error| panic!("conflict read: {error}")),
        b"concurrent durable state"
    );
}

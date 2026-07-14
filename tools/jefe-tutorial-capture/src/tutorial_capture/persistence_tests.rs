//! Tests for the persistence module.
//!
//! Extracted from `persistence.rs` to keep the implementation file under
//! the source-file-size limit.
//!
//! @requirement REQ-TUTORIAL-CAPTURE-007

use super::*;

pub(super) trait TestResultExt<T> {
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

pub(super) fn error_or_panic<T: std::fmt::Debug, E>(result: Result<T, E>, context: &str) -> E {
    match result {
        Err(error) => error,
        Ok(value) => panic!("{context}: unexpectedly succeeded with {value:?}"),
    }
}

/// Finding #5: Return the TempDir RAII guard so it is automatically cleaned
/// up when dropped, instead of calling `.keep()` which leaks the directory.
pub(super) fn temp_dir() -> tempfile::TempDir {
    tempfile::tempdir().value_or_panic("create temp dir")
}

pub(super) fn make_run_root(base: &Path, name: &str) -> PathBuf {
    base.join(name)
}

/// Create a run root with an exclusive sentinel bound to the manifest's run ID,
/// so cleanup_with_containment's sentinel verification passes.
pub(super) fn create_run_root_with_sentinel(run_root: &Path, run_id: &str) {
    fs::create_dir_all(run_root).value_or_panic("create run root");
    let sentinel = run_root.join(EXCLUSIVE_SENTINEL);
    let time = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    let content = format!("pid={}\ntime={time}\nrun_id={run_id}\n", std::process::id());
    atomic_write(&sentinel, &content).value_or_panic("write sentinel");
}

pub(super) fn make_valid_manifest(run_root: &Path) -> RunManifest {
    let id = RunId::new("test-persist-001").value_or_panic("valid id");
    let mut manifest =
        RunManifest::new(id, "0.0.28", "test-scenario", 100, 32, RuntimeProfile::Shim);
    manifest.add_owned_path(OwnedPathKind::ConfigDir, run_root.join("config"));
    manifest.add_owned_path(OwnedPathKind::ArtifactDir, run_root.join("artifacts"));
    manifest.add_owned_path(OwnedPathKind::ShimDir, run_root.join("shims"));
    manifest.add_owned_path(OwnedPathKind::FixtureRepo, run_root.join("fixture-repo"));
    manifest.set_fixture_repo(run_root.join("fixture-repo"));
    manifest
}

// ─── Atomic save + load roundtrip ───────────────────────────────────────

#[test]
fn save_and_load_roundtrip_preserves_manifest() {
    let base = temp_dir();
    let run_root = make_run_root(base.path(), "run-001");
    fs::create_dir_all(&run_root).value_or_panic("create run root");

    let manifest = make_valid_manifest(&run_root);
    save_manifest_atomic(&manifest, &run_root).value_or_panic("save");

    let loaded = load_and_validate(&run_root).value_or_panic("load");
    assert_eq!(loaded.run_id, manifest.run_id);
    assert_eq!(loaded.jefe_version, manifest.jefe_version);
    assert_eq!(loaded.owned_paths.len(), manifest.owned_paths.len());
}

#[test]
fn loaded_manifest_has_correct_schema_version() {
    let base = temp_dir();
    let run_root = make_run_root(base.path(), "run-001");
    fs::create_dir_all(&run_root).value_or_panic("create run root");

    let manifest = make_valid_manifest(&run_root);
    save_manifest_atomic(&manifest, &run_root).value_or_panic("save");

    let path = manifest_path(&run_root);
    let json = fs::read_to_string(&path).value_or_panic("read json");
    assert!(
        json.contains("\"schema_version\": 1"),
        "manifest must contain schema_version field"
    );
}

// ─── Schema version mismatch ────────────────────────────────────────────

#[test]
fn load_rejects_unknown_schema_version() {
    let base = temp_dir();
    let run_root = make_run_root(base.path(), "run-001");
    fs::create_dir_all(&run_root).value_or_panic("create run root");

    let bad_json = r#"{
        "schema_version": 999,
        "run_id": "test-run",
        "jefe_version": "0.0.28",
        "scenario_name": "test",
        "cols": 100,
        "rows": 32,
        "runtime_profile": "shim",
        "owned_paths": [],
        "github_resources": [],
        "artifacts": [],
        "outcome": "pending",
        "cleanup_completed": false
    }"#;
    atomic_write(&manifest_path(&run_root), bad_json).value_or_panic("write bad manifest");

    let err = error_or_panic(load_and_validate(&run_root), "should reject");
    assert!(
        matches!(err, PersistenceError::SchemaVersion { found: 999, .. }),
        "should be schema version error: {err:?}"
    );
}

// ─── Path containment ───────────────────────────────────────────────────

#[test]
fn validate_rejects_path_outside_run_root() {
    let base = temp_dir();
    let run_root = make_run_root(base.path(), "run-001");
    let id = RunId::new("escape-test").value_or_panic("valid id");
    let mut manifest = RunManifest::new(id, "0.0.28", "test", 100, 32, RuntimeProfile::Shim);
    manifest.add_owned_path(OwnedPathKind::ConfigDir, PathBuf::from("/etc/passwd"));

    let err = error_or_panic(
        validate_owned_paths(&manifest.owned_paths, &run_root),
        "should reject path outside run root",
    );
    assert!(
        matches!(err, PersistenceError::PathNotContained { .. }),
        "should be PathNotContained: {err:?}"
    );
}

#[test]
fn validate_rejects_traversal_path() {
    let base = temp_dir();
    let run_root = make_run_root(base.path(), "run-001");
    let id = RunId::new("traversal-test").value_or_panic("valid id");
    let mut manifest = RunManifest::new(id, "0.0.28", "test", 100, 32, RuntimeProfile::Shim);
    let escaped = run_root.join("../../etc");
    manifest.add_owned_path(OwnedPathKind::ConfigDir, escaped);

    let err = error_or_panic(
        validate_owned_paths(&manifest.owned_paths, &run_root),
        "should reject traversal",
    );
    assert!(
        matches!(
            err,
            PersistenceError::PathNotContained { .. } | PersistenceError::UnexpectedSubdir { .. }
        ),
        "should reject traversal: {err:?}"
    );
}

#[test]
fn validate_rejects_unexpected_subdir_name() {
    let base = temp_dir();
    let run_root = make_run_root(base.path(), "run-001");
    let id = RunId::new("subdir-test").value_or_panic("valid id");
    let mut manifest = RunManifest::new(id, "0.0.28", "test", 100, 32, RuntimeProfile::Shim);
    manifest.add_owned_path(
        OwnedPathKind::ConfigDir,
        run_root.join("not-a-valid-subdir"),
    );

    let err = error_or_panic(
        validate_owned_paths(&manifest.owned_paths, &run_root),
        "should reject unexpected subdir",
    );
    assert!(
        matches!(err, PersistenceError::UnexpectedSubdir { .. }),
        "should be UnexpectedSubdir: {err:?}"
    );
}

#[test]
fn validate_rejects_duplicate_paths() {
    let base = temp_dir();
    let run_root = make_run_root(base.path(), "run-001");
    let owned = vec![
        OwnedPath {
            kind: OwnedPathKind::ConfigDir,
            path: run_root.join("config"),
        },
        OwnedPath {
            kind: OwnedPathKind::ConfigDir,
            path: run_root.join("config"),
        },
    ];

    let err = error_or_panic(
        validate_owned_paths(&owned, &run_root),
        "should reject duplicate",
    );
    assert!(
        matches!(err, PersistenceError::DuplicatePath { .. }),
        "should be DuplicatePath: {err:?}"
    );
}

// ─── Symlink rejection ──────────────────────────────────────────────────

#[cfg(unix)]
#[test]
fn validate_rejects_symlink_owned_path() {
    use std::os::unix::fs::symlink;
    let base = temp_dir();
    let run_root = make_run_root(base.path(), "run-001");
    fs::create_dir_all(&run_root).value_or_panic("create run root");
    let target = base.path().join("outside-target");
    fs::create_dir_all(&target).value_or_panic("create target");
    let link = run_root.join("config");
    symlink(&target, &link).value_or_panic("create symlink");

    let id = RunId::new("symlink-test").value_or_panic("valid id");
    let mut manifest = RunManifest::new(id, "0.0.28", "test", 100, 32, RuntimeProfile::Shim);
    manifest.add_owned_path(OwnedPathKind::ConfigDir, link);

    let err = error_or_panic(
        validate_owned_paths(&manifest.owned_paths, &run_root),
        "should reject symlink",
    );
    assert!(
        matches!(err, PersistenceError::SymlinkFound { .. }),
        "should be SymlinkFound: {err:?}"
    );
}

// ─── NUL byte rejection ─────────────────────────────────────────────────

#[test]
fn validate_rejects_nul_in_path() {
    use std::ffi::OsString;
    use std::os::unix::ffi::OsStringExt;
    let base = temp_dir();
    let run_root = make_run_root(base.path(), "run-001");

    let id = RunId::new("nul-test").value_or_panic("valid id");
    let mut manifest = RunManifest::new(id, "0.0.28", "test", 100, 32, RuntimeProfile::Shim);
    let bad_bytes = b"/tmp/run-\0root/config";
    let bad_path = PathBuf::from(OsString::from_vec(bad_bytes.to_vec()));
    manifest.add_owned_path(OwnedPathKind::ConfigDir, bad_path);

    let err = error_or_panic(
        validate_owned_paths(&manifest.owned_paths, &run_root),
        "should reject NUL path",
    );
    assert!(
        matches!(err, PersistenceError::NulInPath { .. }),
        "should be NulInPath: {err:?}"
    );
}

// ─── Exclusive run-root creation ────────────────────────────────────────

#[test]
fn create_run_root_exclusive_succeeds_on_new_dir() {
    let base = temp_dir();
    let run_root = make_run_root(base.path(), "exclusive-001");

    create_run_root_exclusive(&run_root).value_or_panic("should create exclusively");
    assert!(run_root.exists());
    assert!(run_root.join(EXCLUSIVE_SENTINEL).exists());
}

#[test]
fn create_run_root_exclusive_fails_on_collision() {
    let base = temp_dir();
    let run_root = make_run_root(base.path(), "exclusive-002");
    fs::create_dir_all(&run_root).value_or_panic("pre-create");

    let err = error_or_panic(
        create_run_root_exclusive(&run_root),
        "should detect collision",
    );
    assert!(
        matches!(err, PersistenceError::RunRootCollision { .. }),
        "should be RunRootCollision: {err:?}"
    );
}

// ─── Cleanup with containment ───────────────────────────────────────────

#[test]
fn cleanup_with_containment_removes_owned_paths() {
    let base = temp_dir();
    let run_root = make_run_root(base.path(), "cleanup-001");
    let manifest = make_valid_manifest(&run_root);
    create_run_root_with_sentinel(&run_root, manifest.run_id.as_str());

    let mut manifest = manifest;
    for entry in &manifest.owned_paths {
        fs::create_dir_all(&entry.path).value_or_panic("create subdir");
    }

    let records =
        cleanup_with_containment(&mut manifest, &run_root, true).value_or_panic("cleanup");
    assert!(records.iter().any(|r| r.outcome == CleanupOutcome::Removed));
    assert!(manifest.cleanup_completed);
}

#[test]
fn cleanup_preserves_evidence_by_default() {
    let base = temp_dir();
    let run_root = make_run_root(base.path(), "cleanup-002");
    let manifest = make_valid_manifest(&run_root);
    create_run_root_with_sentinel(&run_root, manifest.run_id.as_str());

    let mut manifest = manifest;
    for entry in &manifest.owned_paths {
        fs::create_dir_all(&entry.path).value_or_panic("create subdir");
    }

    let records =
        cleanup_with_containment(&mut manifest, &run_root, false).value_or_panic("cleanup");

    let artifact_record = records
        .iter()
        .find(|r| r.kind == OwnedPathKind::ArtifactDir)
        .value_or_panic("should have artifact record");
    assert_eq!(artifact_record.outcome, CleanupOutcome::Retained);
}

#[test]
fn cleanup_fails_on_path_not_contained() {
    let base = temp_dir();
    let run_root = make_run_root(base.path(), "cleanup-003");
    let id = RunId::new("bad-cleanup").value_or_panic("valid id");
    let mut manifest = RunManifest::new(id, "0.0.28", "test", 100, 32, RuntimeProfile::Shim);
    manifest.add_owned_path(OwnedPathKind::ConfigDir, PathBuf::from("/etc"));
    create_run_root_with_sentinel(&run_root, manifest.run_id.as_str());

    let err = error_or_panic(
        cleanup_with_containment(&mut manifest, &run_root, true),
        "should fail containment",
    );
    assert!(
        matches!(err, PersistenceError::PathNotContained { .. }),
        "should be PathNotContained: {err:?}"
    );
}

// ─── Finding #4: shim_availability persistence in all variants ──────────

/// Finding #4: shim_availability must round-trip through the versioned DTO
/// for all variants (llxprt_only, code_puppy_only, both).
#[test]
fn shim_availability_both_round_trips_through_dto() {
    let base = temp_dir();
    let run_root = make_run_root(base.path(), "shim-both");
    fs::create_dir_all(&run_root).value_or_panic("create run root");
    create_run_root_with_sentinel(&run_root, "test-persist-001");

    let mut manifest = make_valid_manifest(&run_root);
    manifest.shim_availability = crate::path_shim::ShimAvailability::Both;
    save_manifest_atomic(&manifest, &run_root).value_or_panic("save");

    let loaded = load_and_validate(&run_root).value_or_panic("load");
    assert_eq!(
        loaded.shim_availability,
        crate::path_shim::ShimAvailability::Both,
        "shim_availability Both must round-trip"
    );
}

#[test]
fn shim_availability_llxprt_only_round_trips_through_dto() {
    let base = temp_dir();
    let run_root = make_run_root(base.path(), "shim-llxprt");
    fs::create_dir_all(&run_root).value_or_panic("create run root");
    create_run_root_with_sentinel(&run_root, "test-persist-001");

    let mut manifest = make_valid_manifest(&run_root);
    manifest.shim_availability = crate::path_shim::ShimAvailability::LlxprtOnly;
    save_manifest_atomic(&manifest, &run_root).value_or_panic("save");

    let loaded = load_and_validate(&run_root).value_or_panic("load");
    assert_eq!(
        loaded.shim_availability,
        crate::path_shim::ShimAvailability::LlxprtOnly,
        "shim_availability LlxprtOnly must round-trip"
    );
}

#[test]
fn shim_availability_code_puppy_only_round_trips_through_dto() {
    let base = temp_dir();
    let run_root = make_run_root(base.path(), "shim-cp");
    fs::create_dir_all(&run_root).value_or_panic("create run root");
    create_run_root_with_sentinel(&run_root, "test-persist-001");

    let mut manifest = make_valid_manifest(&run_root);
    manifest.shim_availability = crate::path_shim::ShimAvailability::CodePuppyOnly;
    save_manifest_atomic(&manifest, &run_root).value_or_panic("save");

    let loaded = load_and_validate(&run_root).value_or_panic("load");
    assert_eq!(
        loaded.shim_availability,
        crate::path_shim::ShimAvailability::CodePuppyOnly,
        "shim_availability CodePuppyOnly must round-trip"
    );
}

/// Finding #4: shim_availability must appear in the serialized JSON.
#[test]
fn shim_availability_appears_in_serialized_json() {
    let base = temp_dir();
    let run_root = make_run_root(base.path(), "shim-json");
    fs::create_dir_all(&run_root).value_or_panic("create run root");
    create_run_root_with_sentinel(&run_root, "test-persist-001");

    let mut manifest = make_valid_manifest(&run_root);
    manifest.shim_availability = crate::path_shim::ShimAvailability::LlxprtOnly;
    save_manifest_atomic(&manifest, &run_root).value_or_panic("save");

    let path = manifest_path(&run_root);
    let json = fs::read_to_string(&path).value_or_panic("read json");
    assert!(
        json.contains(r#""shim_availability""#),
        "serialized JSON must contain shim_availability field: {json}"
    );
    assert!(
        json.contains(r#""llxprt_only""#),
        "serialized JSON must contain llxprt_only variant: {json}"
    );
}

/// Finding #4: shim_availability defaults to Both when missing from old JSON.
#[test]
fn shim_availability_defaults_when_missing_from_json() {
    let base = temp_dir();
    let run_root = make_run_root(base.path(), "shim-default");
    fs::create_dir_all(&run_root).value_or_panic("create run root");
    create_run_root_with_sentinel(&run_root, "test-persist-001");

    // Manually write a manifest without shim_availability (simulating old schema).
    let json_without_shim = r#"{
        "schema_version": 1,
        "run_id": "test-persist-001",
        "jefe_version": "0.0.28",
        "scenario_name": "test",
        "cols": 100,
        "rows": 32,
        "runtime_profile": "shim",
        "fixture_repo_path": null,
        "fixture_github_repo": null,
        "owned_paths": [],
        "github_resources": [],
        "artifacts": [],
        "outcome": "pending",
        "cleanup_completed": false
    }"#;
    let manifest_path = run_root.join(super::dto::MANIFEST_FILENAME);
    fs::write(&manifest_path, json_without_shim).value_or_panic("write json");

    let loaded = load_and_validate(&run_root).value_or_panic("load with default shim");
    assert_eq!(
        loaded.shim_availability,
        crate::path_shim::ShimAvailability::Both,
        "missing shim_availability should default to Both"
    );
}

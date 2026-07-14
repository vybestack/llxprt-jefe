//! Sentinel, field-validation, and merge-authorization persistence tests.
//!
//! Extracted from `persistence_tests.rs` to keep the file under the
//! source-file-size warning limit.
//!
//! @requirement REQ-TUTORIAL-CAPTURE-007

use super::*;
// Import shared test helpers from the sibling `tests` module.
use super::tests::{
    TestResultExt, create_run_root_with_sentinel, error_or_panic, make_run_root,
    make_valid_manifest, temp_dir,
};

// ─── Sentinel ownership verification ─────────────────────────────────────

/// Cleanup must fail when the sentinel's run_id doesn't match the manifest's
/// run_id, preventing forged manifests from triggering cleanup.
#[test]
fn cleanup_rejects_forged_manifest_with_mismatched_run_id() {
    let base = temp_dir();
    let run_root = make_run_root(base.path(), "forged-001");
    let manifest = make_valid_manifest(&run_root);
    // Create sentinel with a DIFFERENT run_id.
    create_run_root_with_sentinel(&run_root, "different-run-id");

    let mut manifest = manifest;
    for entry in &manifest.owned_paths {
        fs::create_dir_all(&entry.path).value_or_panic("create subdir");
    }

    let err = error_or_panic(
        cleanup_with_containment(&mut manifest, &run_root, true),
        "should reject forged manifest",
    );
    assert!(
        matches!(err, PersistenceError::InvalidField { .. }),
        "should be InvalidField for sentinel mismatch: {err:?}"
    );
}

/// Cleanup must fail when no sentinel exists (possible forged manifest).
#[test]
fn cleanup_rejects_missing_sentinel() {
    let base = temp_dir();
    let run_root = make_run_root(base.path(), "forged-002");
    fs::create_dir_all(&run_root).value_or_panic("create run root");

    let mut manifest = make_valid_manifest(&run_root);
    for entry in &manifest.owned_paths {
        fs::create_dir_all(&entry.path).value_or_panic("create subdir");
    }

    let err = error_or_panic(
        cleanup_with_containment(&mut manifest, &run_root, true),
        "should reject missing sentinel",
    );
    assert!(
        matches!(err, PersistenceError::Io { .. }),
        "should be Io for missing sentinel: {err:?}"
    );
}

/// `verify_sentinel_ownership` succeeds when run_id matches.
#[test]
fn verify_sentinel_succeeds_on_matching_run_id() {
    let base = temp_dir();
    let run_root = make_run_root(base.path(), "sentinel-ok");
    create_run_root_with_sentinel(&run_root, "test-persist-001");

    verify_sentinel_ownership(&run_root, "test-persist-001").value_or_panic("should verify");
}

/// `verify_sentinel_ownership` fails when run_id doesn't match.
#[test]
fn verify_sentinel_fails_on_mismatched_run_id() {
    let base = temp_dir();
    let run_root = make_run_root(base.path(), "sentinel-mismatch");
    create_run_root_with_sentinel(&run_root, "run-A");

    let err = error_or_panic(
        verify_sentinel_ownership(&run_root, "run-B"),
        "should fail mismatch",
    );
    assert!(
        matches!(err, PersistenceError::InvalidField { .. }),
        "should be InvalidField: {err:?}"
    );
}

/// `create_run_root_with_run_id` binds the run_id to the sentinel.
#[test]
fn create_run_root_with_run_id_binds_sentinel() {
    let base = temp_dir();
    let run_root = make_run_root(base.path(), "bound-001");

    create_run_root_with_run_id(&run_root, Some("my-run-id")).value_or_panic("create");

    let sentinel = run_root.join(EXCLUSIVE_SENTINEL);
    let content = fs::read_to_string(&sentinel).value_or_panic("read sentinel");
    assert!(
        content.contains("run_id=my-run-id"),
        "sentinel must contain run_id binding: {content}"
    );
}

// ─── lexical_canonical ──────────────────────────────────────────────────

#[test]
fn lexical_canonical_resolves_dot() {
    let path = Path::new("/tmp/./a/./b");
    assert_eq!(lexical_canonical(path), PathBuf::from("/tmp/a/b"));
}

#[test]
fn lexical_canonical_resolves_dot_dot() {
    let path = Path::new("/tmp/a/../b");
    assert_eq!(lexical_canonical(path), PathBuf::from("/tmp/b"));
}

#[test]
fn lexical_canonical_combined() {
    let path = Path::new("/tmp/a/./b/../c");
    assert_eq!(lexical_canonical(path), PathBuf::from("/tmp/a/c"));
}

// ─── Field validation ───────────────────────────────────────────────────

#[test]
fn dto_rejects_empty_jefe_version() {
    let base = temp_dir();
    let run_root = make_run_root(base.path(), "field-test");
    let dto = ManifestDto {
        schema_version: MANIFEST_SCHEMA_VERSION,
        run_id: "test-run".to_string(),
        jefe_version: String::new(),
        git_commit: None,
        scenario_name: "test".to_string(),
        scenario_hash: None,
        cols: 100,
        rows: 32,
        runtime_profile: RuntimeProfile::Shim,
        fixture_repo_path: None,
        fixture_github_repo: None,
        owned_paths: Vec::new(),
        github_resources: Vec::new(),
        artifacts: Vec::new(),
        outcome: RunOutcome::Pending,
        cleanup_completed: false,
        binary_hash: None,
        theme: None,
        tool_versions: None,
        observed_actions: Vec::new(),
        discrepancies: Vec::new(),
        creation_allowlist: Vec::new(),
        merge_authorized: false,
        shim_availability: crate::path_shim::ShimAvailability::default(),
    };

    let err = error_or_panic(dto_to_manifest(&dto, &run_root), "should reject");
    assert!(
        matches!(err, PersistenceError::InvalidField { .. }),
        "should be InvalidField: {err:?}"
    );
}

#[test]
fn dto_rejects_zero_cols() {
    let base = temp_dir();
    let run_root = make_run_root(base.path(), "field-test");
    let dto = ManifestDto {
        schema_version: MANIFEST_SCHEMA_VERSION,
        run_id: "test-run".to_string(),
        jefe_version: "0.0.28".to_string(),
        git_commit: None,
        scenario_name: "test".to_string(),
        scenario_hash: None,
        cols: 0,
        rows: 32,
        runtime_profile: RuntimeProfile::Shim,
        fixture_repo_path: None,
        fixture_github_repo: None,
        owned_paths: Vec::new(),
        github_resources: Vec::new(),
        artifacts: Vec::new(),
        outcome: RunOutcome::Pending,
        cleanup_completed: false,
        binary_hash: None,
        theme: None,
        tool_versions: None,
        observed_actions: Vec::new(),
        discrepancies: Vec::new(),
        creation_allowlist: Vec::new(),
        merge_authorized: false,
        shim_availability: crate::path_shim::ShimAvailability::default(),
    };

    let err = error_or_panic(dto_to_manifest(&dto, &run_root), "should reject");
    assert!(
        matches!(&err, PersistenceError::InvalidField { field, .. } if field == "cols"),
        "should be InvalidField cols: {err:?}"
    );
}

// ─── Validated path roundtrip ───────────────────────────────────────────

#[test]
fn valid_manifest_with_all_subdirs_passes_containment() {
    let base = temp_dir();
    let run_root = make_run_root(base.path(), "valid-001");
    let manifest = make_valid_manifest(&run_root);

    validate_owned_paths(&manifest.owned_paths, &run_root)
        .value_or_panic("valid manifest should pass");
}

// ─── Finding #1: Tier-B clone ownership kind/path match ──────────────

/// FixtureClone kind must validate against the `fixture-clone` sub-directory.
/// This prevents the kind/path mismatch where a clone destination was recorded
/// as FixtureRepo (which expects `fixture-repo`).
#[test]
fn fixture_clone_validates_against_correct_subdir() {
    let base = temp_dir();
    let run_root = make_run_root(base.path(), "clone-valid");
    let mut manifest = make_valid_manifest(&run_root);
    manifest.add_owned_path(OwnedPathKind::FixtureClone, run_root.join("fixture-clone"));

    validate_owned_paths(&manifest.owned_paths, &run_root)
        .value_or_panic("FixtureClone at fixture-clone should pass containment");
}

/// FixtureClone kind must be REJECTED when it points at `fixture-repo`
/// (a mismatch). The kind and path must agree.
#[test]
fn fixture_clone_rejects_fixture_repo_subdir() {
    let base = temp_dir();
    let run_root = make_run_root(base.path(), "clone-mismatch");
    let mut manifest = make_valid_manifest(&run_root);
    // Remove the default fixture-repo (which is kind FixtureRepo) so we can
    // add fixture-repo path as FixtureClone without dedup.
    manifest
        .owned_paths
        .retain(|p| p.kind != OwnedPathKind::FixtureRepo);
    manifest.add_owned_path(OwnedPathKind::FixtureClone, run_root.join("fixture-repo"));

    let err = error_or_panic(
        validate_owned_paths(&manifest.owned_paths, &run_root),
        "should reject FixtureClone at fixture-repo",
    );
    assert!(
        matches!(err, PersistenceError::UnexpectedSubdir { .. }),
        "should be UnexpectedSubdir for kind/path mismatch: {err:?}"
    );
}

/// FixtureRepo kind must be REJECTED when it points at `fixture-clone`.
#[test]
fn fixture_repo_rejects_fixture_clone_subdir() {
    let base = temp_dir();
    let run_root = make_run_root(base.path(), "repo-mismatch");
    let mut manifest = make_valid_manifest(&run_root);
    // Remove the default fixture-repo and try with fixture-clone path.
    manifest
        .owned_paths
        .retain(|p| p.kind != OwnedPathKind::FixtureRepo);
    manifest.add_owned_path(OwnedPathKind::FixtureRepo, run_root.join("fixture-clone"));

    let err = error_or_panic(
        validate_owned_paths(&manifest.owned_paths, &run_root),
        "should reject FixtureRepo at fixture-clone",
    );
    assert!(
        matches!(err, PersistenceError::UnexpectedSubdir { .. }),
        "should be UnexpectedSubdir: {err:?}"
    );
}

// ─── Finding #10: symlinked parent production containment ──────────────

/// On macOS, `/tmp` is a symlink to `/private/tmp`. The production checkout
/// containment check must canonicalize the path so that run roots under
/// `/tmp` (or `/var`) are correctly checked against production repos at
/// their canonical locations.
#[cfg(target_os = "macos")]
#[test]
fn production_check_handles_macos_tmp_symlink() {
    let run_root = PathBuf::from("/tmp/jefe-tutorial-test-symlink-check-001");
    let _ = fs::remove_dir_all(&run_root);
    create_run_root_with_run_id(&run_root, Some("symlink-test"))
        .value_or_panic("should create under /tmp symlink");
    assert!(run_root.exists());
    let _ = fs::remove_dir_all(&run_root);
}

/// On macOS, `/var` is a symlink to `/private/var`. Verify that the
/// canonicalization handles this correctly.
#[cfg(target_os = "macos")]
#[test]
fn production_check_handles_macos_var_symlink() {
    let run_root = PathBuf::from("/var/tmp/jefe-tutorial-test-var-symlink-001");
    let _ = fs::remove_dir_all(&run_root);
    create_run_root_with_run_id(&run_root, Some("var-symlink-test"))
        .value_or_panic("should create under /var symlink");
    assert!(run_root.exists());
    let _ = fs::remove_dir_all(&run_root);
}

/// Canonicalize an existing ancestor resolves symlinks.
#[cfg(target_os = "macos")]
#[test]
fn canonicalize_existing_ancestor_resolves_macos_tmp() {
    use super::run_root::canonicalize_existing_ancestor;
    let path = Path::new("/tmp/jefe-test-nonexistent-dir-001");
    let canon = canonicalize_existing_ancestor(path);
    assert!(
        canon.starts_with("/private/tmp") || canon.starts_with("/tmp"),
        "canonicalized path should resolve symlink: {}",
        canon.display()
    );
}

// ─── merge_authorized versioned roundtrip (Finding: versioned ManifestDto) ──

/// `merge_authorized` must survive the full save_manifest_atomic →
/// load_and_validate roundtrip through the versioned DTO, both when true
/// and false.
#[test]
fn merge_authorized_roundtrips_through_versioned_dto_when_true() {
    let base = temp_dir();
    let run_root = make_run_root(base.path(), "merge-auth-true");
    create_run_root_with_sentinel(&run_root, "merge-auth-true-001");
    let mut manifest = make_valid_manifest(&run_root);
    manifest.set_merge_authorized(true);
    save_manifest_atomic(&manifest, &run_root).value_or_panic("save manifest atomic");
    let loaded = load_and_validate(&run_root).value_or_panic("load and validate");
    assert!(
        loaded.merge_authorized,
        "merge_authorized=true must survive versioned DTO roundtrip"
    );
}

/// `merge_authorized=false` (the default) must also survive the roundtrip.
#[test]
fn merge_authorized_roundtrips_through_versioned_dto_when_false() {
    let base = temp_dir();
    let run_root = make_run_root(base.path(), "merge-auth-false");
    create_run_root_with_sentinel(&run_root, "merge-auth-false-001");
    let manifest = make_valid_manifest(&run_root);
    assert!(!manifest.merge_authorized, "default must be false");
    save_manifest_atomic(&manifest, &run_root).value_or_panic("save manifest atomic");
    let loaded = load_and_validate(&run_root).value_or_panic("load and validate");
    assert!(
        !loaded.merge_authorized,
        "merge_authorized=false must survive versioned DTO roundtrip"
    );
}

/// A manifest DTO missing `merge_authorized` (legacy v1 file) must
/// deserialize with `merge_authorized=false` (serde default).
#[test]
fn merge_authorized_defaults_to_false_when_absent_in_json() {
    let base = temp_dir();
    let run_root = make_run_root(base.path(), "merge-legacy");
    create_run_root_with_sentinel(&run_root, "merge-legacy-001");
    let json = format!(
        r#"{{
            "schema_version": 1,
            "run_id": "merge-legacy-001",
            "jefe_version": "0.0.28",
            "git_commit": null,
            "scenario_name": "test-scenario",
            "scenario_hash": null,
            "cols": 100,
            "rows": 32,
            "runtime_profile": "shim",
            "fixture_repo_path": "{}",
            "fixture_github_repo": null,
            "owned_paths": [],
            "github_resources": [],
            "artifacts": [],
            "outcome": "pending",
            "cleanup_completed": false,
            "binary_hash": null,
            "theme": null,
            "tool_versions": null
        }}"#,
        run_root.join("fixture-repo").display()
    );
    let manifest_path = manifest_path(&run_root);
    atomic_write(&manifest_path, &json).value_or_panic("write legacy manifest");
    let loaded = load_and_validate(&run_root).value_or_panic("load legacy manifest");
    assert!(
        !loaded.merge_authorized,
        "legacy manifest without merge_authorized must default to false"
    );
}

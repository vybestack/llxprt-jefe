//! Tests for the run manifest module.
//!
//! Extracted from `manifest.rs` to keep file sizes under the recommended
//! 750-line threshold.

use super::*;

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

fn error_or_panic<T: std::fmt::Debug, E>(result: Result<T, E>, context: &str) -> E {
    match result {
        Err(error) => error,
        Ok(value) => panic!("{context}: unexpectedly succeeded with {value:?}"),
    }
}

// ── RunId validation ──────────────────────────────────────────────────

#[test]
fn run_id_accepts_alphanumeric_and_hyphens() {
    let id = RunId::new("tutorial-2026-07-12").value_or_panic("valid run id");
    assert_eq!(id.as_str(), "tutorial-2026-07-12");
}

#[test]
fn run_id_rejects_empty() {
    assert!(RunId::new("").is_none());
}

#[test]
fn run_id_rejects_underscores() {
    assert!(RunId::new("run_id").is_none());
}

#[test]
fn run_id_rejects_spaces() {
    assert!(RunId::new("run id").is_none());
}

#[test]
fn run_id_rejects_slashes() {
    assert!(RunId::new("run/id").is_none());
}

#[test]
fn run_id_rejects_too_long() {
    let long = "a".repeat(65);
    assert!(RunId::new(&long).is_none());
}

#[test]
fn run_id_accepts_max_length() {
    let max = "a".repeat(64);
    assert!(RunId::new(&max).is_some());
}

// ── Manifest ownership ────────────────────────────────────────────────

#[test]
fn manifest_starts_empty() {
    let manifest = RunManifest::new(
        RunId::new("test-run").value_or_panic("valid id"),
        "0.0.28",
        "tutorial-capture-local",
        100,
        32,
        RuntimeProfile::Shim,
    );
    assert!(manifest.owned_paths.is_empty());
    assert!(manifest.github_resources.is_empty());
    assert!(manifest.artifacts.is_empty());
    assert_eq!(manifest.outcome, RunOutcome::Pending);
    assert!(!manifest.cleanup_completed);
}

#[test]
fn manifest_records_owned_path() {
    let mut manifest = RunManifest::new(
        RunId::new("test-run").value_or_panic("valid id"),
        "0.0.28",
        "scenario",
        100,
        32,
        RuntimeProfile::Shim,
    );
    let path = PathBuf::from("/tmp/jefe-tutorial/test-run/config");
    manifest.add_owned_path(OwnedPathKind::ConfigDir, path.clone());

    assert_eq!(manifest.owned_paths.len(), 1);
    assert!(manifest.owns_path(&path));
}

#[test]
fn manifest_deduplicates_owned_paths() {
    let mut manifest = RunManifest::new(
        RunId::new("test-run").value_or_panic("valid id"),
        "0.0.28",
        "scenario",
        100,
        32,
        RuntimeProfile::Shim,
    );
    let path = PathBuf::from("/tmp/jefe-tutorial/test-run/config");
    manifest.add_owned_path(OwnedPathKind::ConfigDir, path.clone());
    manifest.add_owned_path(OwnedPathKind::ConfigDir, path.clone());

    assert_eq!(manifest.owned_paths.len(), 1);
}

#[test]
fn manifest_does_not_own_unrelated_path() {
    let manifest = RunManifest::new(
        RunId::new("test-run").value_or_panic("valid id"),
        "0.0.28",
        "scenario",
        100,
        32,
        RuntimeProfile::Shim,
    );
    assert!(!manifest.owns_path(&PathBuf::from("/usr/local/jefe")));
}

#[test]
fn manifest_records_github_resource() {
    let mut manifest = RunManifest::new(
        RunId::new("test-run").value_or_panic("valid id"),
        "0.0.28",
        "scenario",
        100,
        32,
        RuntimeProfile::Shim,
    );
    manifest.add_github_resource(GitHubResource {
        kind: GitHubResourceKind::Issue,
        repository: "fixture/repo".to_string(),
        identifier: "42".to_string(),
        url: Some("https://github.com/fixture/repo/issues/42".to_string()),
        title: String::new(),
    });

    assert_eq!(manifest.github_resources.len(), 1);
    assert_eq!(manifest.github_resources[0].kind, GitHubResourceKind::Issue);
}

#[test]
fn manifest_serializes_and_deserializes_roundtrip() {
    let mut manifest = RunManifest::new(
        RunId::new("tutorial-run-001").value_or_panic("valid id"),
        "0.0.28",
        "tutorial-capture-local",
        100,
        32,
        RuntimeProfile::Shim,
    );
    manifest.add_owned_path(
        OwnedPathKind::ConfigDir,
        PathBuf::from("/tmp/jefe-tutorial/tutorial-run-001/config"),
    );
    manifest.add_owned_path(
        OwnedPathKind::FixtureRepo,
        PathBuf::from("/tmp/jefe-tutorial/tutorial-run-001/repo"),
    );
    manifest.add_artifact(ArtifactEntry {
        label: "dashboard-oriented".to_string(),
        relative_path: PathBuf::from("dashboard-oriented.screen.txt"),
        kind: ArtifactKind::ScreenCapture,
    });
    manifest.set_outcome(RunOutcome::Success);
    manifest.mark_cleanup_completed();

    let json = manifest.to_json().value_or_panic("serialize manifest");
    let restored = RunManifest::from_json(&json).value_or_panic("deserialize manifest");

    assert_eq!(restored.run_id, manifest.run_id);
    assert_eq!(restored.owned_paths.len(), 2);
    assert_eq!(restored.artifacts.len(), 1);
    assert_eq!(restored.outcome, RunOutcome::Success);
    assert!(restored.cleanup_completed);
}

#[test]
fn manifest_fixture_github_repo_matching_is_case_insensitive() {
    let mut manifest = RunManifest::new(
        RunId::new("test-run").value_or_panic("valid id"),
        "0.0.28",
        "scenario",
        100,
        32,
        RuntimeProfile::Shim,
    );
    manifest.set_fixture_github_repo("Fixture/Repo");
    assert!(manifest.is_fixture_github_repo("fixture/repo"));
    assert!(manifest.is_fixture_github_repo("FIXTURE/REPO"));
    assert!(!manifest.is_fixture_github_repo("other/repo"));
}

#[test]
fn manifest_json_roundtrip_preserves_github_resources() {
    let mut manifest = RunManifest::new(
        RunId::new("gh-run-001").value_or_panic("valid id"),
        "0.0.28",
        "tutorial-capture-github",
        100,
        32,
        RuntimeProfile::Shim,
    );
    manifest.set_fixture_github_repo("fixture/test-repo");
    manifest.add_github_resource(GitHubResource {
        kind: GitHubResourceKind::Issue,
        repository: "fixture/test-repo".to_string(),
        identifier: "1".to_string(),
        url: Some("https://github.com/fixture/test-repo/issues/1".to_string()),
        title: String::new(),
    });
    manifest.add_github_resource(GitHubResource {
        kind: GitHubResourceKind::PullRequest,
        repository: "fixture/test-repo".to_string(),
        identifier: "2".to_string(),
        url: Some("https://github.com/fixture/test-repo/pull/2".to_string()),
        title: String::new(),
    });

    let json = manifest.to_json().value_or_panic("serialize");
    let restored = RunManifest::from_json(&json).value_or_panic("deserialize");

    assert_eq!(restored.github_resources.len(), 2);
    assert_eq!(restored.github_resources[0].kind, GitHubResourceKind::Issue);
    assert_eq!(
        restored.github_resources[1].kind,
        GitHubResourceKind::PullRequest
    );
}

#[test]
fn manifest_from_malformed_json_returns_error() {
    let err = error_or_panic(RunManifest::from_json("{ not json"), "should fail");
    let _ = err;
}

/// Finding #8: Visual artifact kind exists and serializes correctly.
#[test]
fn visual_artifact_kind_serializes_correctly() {
    let mut manifest = RunManifest::new(
        RunId::new("visual-test").value_or_panic("valid id"),
        "0.0.28",
        "scenario",
        100,
        32,
        RuntimeProfile::Shim,
    );
    manifest.add_artifact(ArtifactEntry {
        label: "dashboard-svg".to_string(),
        relative_path: PathBuf::from("artifacts/svg/dashboard.svg"),
        kind: ArtifactKind::Visual,
    });
    let json = manifest.to_json().value_or_panic("serialize");
    assert!(
        json.contains("\"visual\""),
        "Visual kind must serialize as 'visual': {json}"
    );
    let restored = RunManifest::from_json(&json).value_or_panic("deserialize");
    assert_eq!(restored.artifacts[0].kind, ArtifactKind::Visual);
}

#[test]
fn runtime_profile_is_shim_flag() {
    assert!(RuntimeProfile::Shim.is_shim());
    assert!(!RuntimeProfile::RealLlxprt.is_shim());
    assert!(!RuntimeProfile::RealCodePuppy.is_shim());
}

#[test]
fn run_id_display() {
    let id = RunId::new("abc-123").value_or_panic("valid id");
    assert_eq!(format!("{id}"), "abc-123");
}

#[test]
fn run_id_into_inner() {
    let id = RunId::new("abc-123").value_or_panic("valid id");
    assert_eq!(id.into_inner(), "abc-123");
}

#[test]
fn find_path_by_kind_returns_matching_path() {
    let mut manifest = RunManifest::new(
        RunId::new("test-run").value_or_panic("valid id"),
        "0.0.28",
        "scenario",
        100,
        32,
        RuntimeProfile::Shim,
    );
    let config_path = PathBuf::from("/tmp/jefe-tutorial/test/config");
    manifest.add_owned_path(OwnedPathKind::ConfigDir, config_path.clone());
    manifest.add_owned_path(
        OwnedPathKind::ArtifactDir,
        PathBuf::from("/tmp/jefe-tutorial/test/artifacts"),
    );

    let found = manifest.find_path_by_kind(OwnedPathKind::ConfigDir);
    assert_eq!(found, Some(config_path.as_path()));
}

#[test]
fn find_path_by_kind_returns_none_when_absent() {
    let manifest = RunManifest::new(
        RunId::new("test-run").value_or_panic("valid id"),
        "0.0.28",
        "scenario",
        100,
        32,
        RuntimeProfile::Shim,
    );
    assert!(
        manifest
            .find_path_by_kind(OwnedPathKind::ConfigDir)
            .is_none()
    );
}

// ── Task #5: ArtifactEntry path normalization roundtrip ────────────

/// Verify that artifact relative_path roundtrips through JSON
/// serialization/deserialization correctly. Paths are relative to
/// ArtifactDir (no "artifacts/" prefix).
#[test]
fn artifact_entry_relative_path_roundtrip_preserves_relative_to_artifact_dir() {
    let mut manifest = RunManifest::new(
        RunId::new("roundtrip-test").value_or_panic("valid id"),
        "0.0.28",
        "scenario",
        100,
        32,
        RuntimeProfile::Shim,
    );
    manifest.add_artifact(ArtifactEntry {
        label: "dashboard".to_string(),
        relative_path: PathBuf::from("dashboard.screen.txt"),
        kind: ArtifactKind::ScreenCapture,
    });
    manifest.add_artifact(ArtifactEntry {
        label: "svg".to_string(),
        relative_path: PathBuf::from("svg").join("dashboard.svg"),
        kind: ArtifactKind::Visual,
    });
    manifest.add_artifact(ArtifactEntry {
        label: "report".to_string(),
        relative_path: PathBuf::from("run-report.md"),
        kind: ArtifactKind::Report,
    });

    let json = manifest.to_json().value_or_panic("serialize");
    let restored = RunManifest::from_json(&json).value_or_panic("deserialize");

    assert_eq!(restored.artifacts.len(), 3, "all artifacts must roundtrip");
    assert_eq!(
        restored.artifacts[0].relative_path,
        PathBuf::from("dashboard.screen.txt"),
        "screen capture path must be relative to ArtifactDir"
    );
    assert_eq!(
        restored.artifacts[1].relative_path,
        PathBuf::from("svg").join("dashboard.svg"),
        "SVG path must be relative to ArtifactDir"
    );
    assert_eq!(
        restored.artifacts[2].relative_path,
        PathBuf::from("run-report.md"),
        "report path must be relative to ArtifactDir"
    );
}

/// Verify that paths are stored relative to ArtifactDir (not including
/// "artifacts/" prefix) and can be resolved back to the full path.
#[test]
fn artifact_entry_path_resolves_to_full_artifact_dir_path() {
    let mut manifest = RunManifest::new(
        RunId::new("resolve-test").value_or_panic("valid id"),
        "0.0.28",
        "scenario",
        100,
        32,
        RuntimeProfile::Shim,
    );
    manifest.add_artifact(ArtifactEntry {
        label: "capture".to_string(),
        relative_path: PathBuf::from("capture.screen.txt"),
        kind: ArtifactKind::ScreenCapture,
    });

    let artifact_dir = PathBuf::from("/tmp/run-root/artifacts");
    let full_path = artifact_dir.join(&manifest.artifacts[0].relative_path);
    assert_eq!(
        full_path,
        PathBuf::from("/tmp/run-root/artifacts/capture.screen.txt"),
        "relative_path + artifact_dir must resolve to full path"
    );
}

/// Verify that paths with parent-dir traversal are still rejected
/// (security invariant maintained after normalization).
#[test]
fn artifact_entry_rejects_traversal_in_normalized_path() {
    let mut manifest = RunManifest::new(
        RunId::new("traversal-test").value_or_panic("valid id"),
        "0.0.28",
        "scenario",
        100,
        32,
        RuntimeProfile::Shim,
    );
    manifest.add_artifact(ArtifactEntry {
        label: "bad".to_string(),
        relative_path: PathBuf::from("../../../etc/passwd"),
        kind: ArtifactKind::ScreenCapture,
    });
    assert!(
        manifest.artifacts.is_empty(),
        "traversal paths must be rejected"
    );
}

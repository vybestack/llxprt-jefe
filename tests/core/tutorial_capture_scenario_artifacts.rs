//! Tier-B generated scenario artifact contracts (issue #241, Finding #4).
//!
//! These tests verify that generated Tier-B scenarios are:
//! - Written under `artifacts/scenarios/`
//! - Registered in the manifest with `ArtifactKind::Scenario`
//! - Written atomically (temp file + rename)
//! - Redacted (no tokens/secrets in scenario content)
//! - Included in cleanup/evidence tracking
//!
//! @requirement REQ-TUTORIAL-CAPTURE-004

use jefe::tutorial_capture::{
    ArtifactKind, GitHubResource, GitHubResourceKind, RunId, RunManifest, RuntimeProfile,
    TierBScenarioParams, generate_tier_b_merge_scenario, generate_tier_b_scenario,
    write_artifact_atomic,
};

trait ResultExt<T> {
    fn value_or_panic(self, context: &str) -> T;
}

impl<T, E: std::fmt::Debug> ResultExt<T> for Result<T, E> {
    fn value_or_panic(self, context: &str) -> T {
        match self {
            Ok(value) => value,
            Err(error) => panic!("{context}: {error:?}"),
        }
    }
}

impl<T> ResultExt<T> for Option<T> {
    fn value_or_panic(self, context: &str) -> T {
        match self {
            Some(value) => value,
            None => panic!("{context}: None"),
        }
    }
}

fn sample_params() -> TierBScenarioParams {
    TierBScenarioParams {
        issue_title: "[tutorial-capture:run-001] fixture issue".to_string(),
        pr_title: "[tutorial-capture:run-001] fixture pull request".to_string(),
        branch_name: "tutorial-capture/run-001".to_string(),
        issue_number: "42".to_string(),
        pr_number: "7".to_string(),
        agent_name: "TutorialAgent".to_string(),
    }
}

fn sample_manifest_with_resources() -> RunManifest {
    let run_id = RunId::new("scenario-run-001").value_or_panic("valid run id");
    let mut manifest = RunManifest::new(
        run_id,
        "0.0.28",
        "tutorial-capture-github",
        100,
        32,
        RuntimeProfile::Shim,
    );
    manifest.set_fixture_github_repo("fixture/test");
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
        identifier: "tutorial-capture/run-001".to_string(),
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
    manifest
}

/// Finding #4: The generated scenario is written under `artifacts/scenarios/`
/// and registered in the manifest as `ArtifactKind::Scenario`.
#[test]
fn scenario_written_under_artifacts_scenarios_and_registered() {
    let base = tempfile::tempdir().value_or_panic("temp dir");
    let artifact_dir = base.path().join("artifacts");

    let mut manifest = sample_manifest_with_resources();
    let params = sample_params();
    let scenario_json = generate_tier_b_scenario(&params);

    let scenario_rel = std::path::Path::new("scenarios/generated-github-scenario.json");
    write_artifact_atomic(
        &artifact_dir,
        scenario_rel,
        &scenario_json,
        &mut manifest,
        "generated-github-scenario",
        ArtifactKind::Scenario,
    )
    .value_or_panic("write artifact atomically");

    // The file must exist under artifacts/scenarios/.
    let scenario_path = artifact_dir.join(scenario_rel);
    assert!(
        scenario_path.exists(),
        "scenario file must exist at {}",
        scenario_path.display()
    );
    assert!(
        scenario_path.starts_with(&artifact_dir),
        "scenario must be under artifacts dir"
    );
    assert!(
        scenario_path
            .components()
            .any(|c| c.as_os_str() == "scenarios"),
        "scenario path must contain 'scenarios' subdirectory"
    );

    // The manifest must register the scenario as an ArtifactKind::Scenario.
    let scenario_artifact = manifest
        .artifacts
        .iter()
        .find(|a| a.kind == ArtifactKind::Scenario)
        .value_or_panic("manifest must have a Scenario artifact");
    assert!(
        scenario_artifact.label.contains("github-scenario"),
        "scenario artifact label must identify the scenario: {:?}",
        scenario_artifact.label
    );

    let _ = std::fs::remove_dir_all(&base);
}

/// Finding #4: The merge scenario is also written under `artifacts/scenarios/`
/// and registered as `ArtifactKind::Scenario`.
#[test]
fn merge_scenario_written_under_artifacts_scenarios_and_registered() {
    let base = tempfile::tempdir().value_or_panic("temp dir");
    let artifact_dir = base.path().join("artifacts");

    let mut manifest = sample_manifest_with_resources();
    let params = sample_params();
    let scenario_json = generate_tier_b_merge_scenario(&params);

    let scenario_rel = std::path::Path::new("scenarios/generated-github-merge-scenario.json");
    write_artifact_atomic(
        &artifact_dir,
        scenario_rel,
        &scenario_json,
        &mut manifest,
        "generated-github-merge-scenario",
        ArtifactKind::Scenario,
    )
    .value_or_panic("write merge scenario atomically");

    // The file must exist.
    let scenario_path = artifact_dir.join(scenario_rel);
    assert!(scenario_path.exists(), "merge scenario must exist");

    // Registered with Scenario kind.
    let count = manifest
        .artifacts
        .iter()
        .filter(|a| a.kind == ArtifactKind::Scenario)
        .count();
    assert_eq!(count, 1, "must have exactly one Scenario artifact");

    let _ = std::fs::remove_dir_all(&base);
}

/// Finding #4: Scenario content must be redacted — it must not contain
/// GitHub token values or other common secret patterns. The scenario is
/// generated from fixture metadata only; injecting a token into a title
/// should not survive redaction.
#[test]
fn scenario_content_is_redacted_of_tokens() {
    use jefe::tutorial_capture::RedactionSet;

    let mut redaction = RedactionSet::new();
    redaction.add_token_prefix("ghp_", "REDACTED", 10);

    let params = TierBScenarioParams {
        issue_title:
            "[tutorial-capture:run-001] issue with token ghp_AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA"
                .to_string(),
        pr_title: "[tutorial-capture:run-001] fixture pull request".to_string(),
        branch_name: "tutorial-capture/run-001".to_string(),
        issue_number: "42".to_string(),
        pr_number: "7".to_string(),
        agent_name: "TutorialAgent".to_string(),
    };
    let scenario_json = generate_tier_b_scenario(&params);

    // Redact the entire scenario content using the RedactionSet apply method.
    let redacted = redaction.apply(&scenario_json);

    assert!(
        !redacted.contains("ghp_AAAA"),
        "redacted scenario must not contain token value: {redacted}"
    );
    assert!(
        redacted.contains("REDACTED"),
        "redacted scenario must contain replacement marker"
    );
}

/// Finding #4: Atomic write means the file is complete on disk — there is
/// no partial content. Writing then reading should produce the same content.
#[test]
fn scenario_atomic_write_produces_complete_file() {
    let base = tempfile::tempdir().value_or_panic("temp dir");
    let artifact_dir = base.path().join("artifacts");

    let mut manifest = sample_manifest_with_resources();
    let params = sample_params();
    let scenario_json = generate_tier_b_scenario(&params);

    let scenario_rel = std::path::Path::new("scenarios/generated-github-scenario.json");
    write_artifact_atomic(
        &artifact_dir,
        scenario_rel,
        &scenario_json,
        &mut manifest,
        "generated-github-scenario",
        ArtifactKind::Scenario,
    )
    .value_or_panic("write atomically");

    let scenario_path = artifact_dir.join(scenario_rel);
    let read_content = std::fs::read_to_string(&scenario_path).value_or_panic("read scenario");
    assert_eq!(
        read_content, scenario_json,
        "file content must match exactly (atomic write)"
    );

    // Verify it's valid JSON.
    let _: serde_json::Value =
        serde_json::from_str(&read_content).value_or_panic("scenario must be valid JSON");

    let _ = std::fs::remove_dir_all(&base);
}

/// Finding #4: The scenario artifact path is relative to the artifact
/// directory and validated for safety (no traversal, no absolute path).
#[test]
fn scenario_artifact_path_is_validated_relative() {
    let base = tempfile::tempdir().value_or_panic("temp dir");
    let artifact_dir = base.path().join("artifacts");

    let mut manifest = sample_manifest_with_resources();
    let scenario_json = generate_tier_b_scenario(&sample_params());

    // Valid relative path under scenarios/.
    let scenario_rel = std::path::Path::new("scenarios/generated-github-scenario.json");
    write_artifact_atomic(
        &artifact_dir,
        scenario_rel,
        &scenario_json,
        &mut manifest,
        "generated-github-scenario",
        ArtifactKind::Scenario,
    )
    .value_or_panic("valid path should succeed");

    // Path traversal must be rejected.
    let bad_rel = std::path::Path::new("../../etc/passwd");
    let result = write_artifact_atomic(
        &artifact_dir,
        bad_rel,
        &scenario_json,
        &mut manifest,
        "bad-scenario",
        ArtifactKind::Scenario,
    );
    assert!(
        result.is_err(),
        "path traversal must be rejected for scenario artifacts"
    );

    let _ = std::fs::remove_dir_all(&base);
}

/// Finding #4: The scenario artifact is tracked in the manifest so cleanup
/// includes it in evidence. The manifest's artifact list must include the
/// scenario with the correct relative path.
#[test]
fn scenario_artifact_tracked_for_cleanup_evidence() {
    let base = tempfile::tempdir().value_or_panic("temp dir");
    let artifact_dir = base.path().join("artifacts");

    let mut manifest = sample_manifest_with_resources();
    let scenario_json = generate_tier_b_scenario(&sample_params());

    let scenario_rel = std::path::Path::new("scenarios/generated-github-scenario.json");
    write_artifact_atomic(
        &artifact_dir,
        scenario_rel,
        &scenario_json,
        &mut manifest,
        "generated-github-scenario",
        ArtifactKind::Scenario,
    )
    .value_or_panic("write scenario");

    // The artifact entry must record the relative path matching what was written.
    let scenario_artifact = manifest
        .artifacts
        .iter()
        .find(|a| a.kind == ArtifactKind::Scenario)
        .value_or_panic("scenario artifact must be tracked");
    assert_eq!(
        scenario_artifact.relative_path, scenario_rel,
        "artifact relative path must match the written path"
    );

    // Cleanup includes artifacts registered in the manifest. The scenario
    // must be in the artifact list so cleanup tracks it.
    assert!(
        manifest
            .artifacts
            .iter()
            .any(|a| a.kind == ArtifactKind::Scenario),
        "scenario must be in manifest artifacts for cleanup tracking"
    );

    let _ = std::fs::remove_dir_all(&base);
}

/// Finding #4: After writing a scenario + registering it, the manifest
/// round-trips through serialization with the Scenario artifact intact.
#[test]
fn scenario_artifact_survives_manifest_serialization_roundtrip() {
    let mut manifest = sample_manifest_with_resources();
    let scenario_json = generate_tier_b_scenario(&sample_params());

    // Use a temp dir for the artifact dir.
    let base = tempfile::tempdir().value_or_panic("temp dir");
    let artifact_dir = base.path().join("artifacts");

    let scenario_rel = std::path::Path::new("scenarios/generated-github-scenario.json");
    write_artifact_atomic(
        &artifact_dir,
        scenario_rel,
        &scenario_json,
        &mut manifest,
        "generated-github-scenario",
        ArtifactKind::Scenario,
    )
    .value_or_panic("write scenario");

    let json = manifest.to_json().value_or_panic("serialize manifest");
    let reloaded = RunManifest::from_json(&json).value_or_panic("deserialize manifest");

    let scenario_artifact = reloaded
        .artifacts
        .iter()
        .find(|a| a.kind == ArtifactKind::Scenario)
        .value_or_panic("Scenario artifact must survive serialization roundtrip");
    assert_eq!(scenario_artifact.relative_path, scenario_rel);

    let _ = std::fs::remove_dir_all(&base);
}

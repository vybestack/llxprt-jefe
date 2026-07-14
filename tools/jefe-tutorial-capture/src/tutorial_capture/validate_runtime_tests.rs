//! Tests for validate-runtime scenario generation.
//!
//! **Finding #2**: Validates that the scenario JSON opens New Agent and
//! asserts on the actual runtime label text as shown by the Jefe form.
//!
//! **Finding #7**: Tests assert on actual choices, not title only.
//!
//! @requirement REQ-TUTORIAL-CAPTURE-003

use super::*;
use crate::manifest::RuntimeProfile;

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

// ── runtime_binary_name ──────────────────────────────────────────────

#[test]
fn runtime_binary_name_real_llxprt() {
    assert_eq!(
        runtime_binary_name(RuntimeProfile::RealLlxprt),
        Some("llxprt")
    );
}

#[test]
fn runtime_binary_name_real_code_puppy() {
    assert_eq!(
        runtime_binary_name(RuntimeProfile::RealCodePuppy),
        Some("code-puppy")
    );
}

#[test]
fn runtime_binary_name_shim_returns_none() {
    assert_eq!(runtime_binary_name(RuntimeProfile::Shim), None);
}

// ── runtime_label (Finding #2) ───────────────────────────────────────

#[test]
fn runtime_label_real_llxprt() {
    assert_eq!(runtime_label(RuntimeProfile::RealLlxprt), Some("LLxprt"));
}

#[test]
fn runtime_label_real_code_puppy() {
    assert_eq!(
        runtime_label(RuntimeProfile::RealCodePuppy),
        Some("code_puppy")
    );
}

#[test]
fn runtime_label_shim_returns_none() {
    assert_eq!(runtime_label(RuntimeProfile::Shim), None);
}

// ── generate_validate_runtime_scenario ───────────────────────────────

#[test]
fn scenario_for_real_llxprt_contains_llxprt_label() {
    let json = generate_validate_runtime_scenario(RuntimeProfile::RealLlxprt)
        .value_or_panic("should generate scenario for real-llxprt");
    assert!(
        json.contains("\"LLxprt\""),
        "scenario must contain exact LLxprt runtime label: {json}"
    );
}

#[test]
fn scenario_for_real_code_puppy_contains_code_puppy_label() {
    let json = generate_validate_runtime_scenario(RuntimeProfile::RealCodePuppy)
        .value_or_panic("should generate scenario for real-code-puppy");
    assert!(
        json.contains("\"code_puppy\""),
        "scenario must contain exact code_puppy runtime label: {json}"
    );
}

#[test]
fn scenario_for_shim_returns_none() {
    assert!(
        generate_validate_runtime_scenario(RuntimeProfile::Shim).is_none(),
        "shim profile should not produce a validate-runtime scenario"
    );
}

#[test]
fn scenario_is_valid_json() {
    for profile in [RuntimeProfile::RealLlxprt, RuntimeProfile::RealCodePuppy] {
        let json = generate_validate_runtime_scenario(profile)
            .value_or_panic(&format!("scenario for {profile:?} should generate"));
        let parsed: serde_json::Value = serde_json::from_str(&json)
            .value_or_panic(&format!("scenario JSON must parse for {profile:?}"));
        assert!(parsed.is_object(), "scenario must be a JSON object");
    }
}

/// Finding #7: The scenario asserts on the actual runtime label in the
/// chooser via an `expect` step, not just a title or heading.
#[test]
fn scenario_uses_expect_on_actual_runtime_label() {
    let json = generate_validate_runtime_scenario(RuntimeProfile::RealLlxprt)
        .value_or_panic("should generate scenario");
    assert!(
        json.contains("\"expect\": \"LLxprt\""),
        "scenario must use expect on the exact runtime label: {json}"
    );
}

/// Finding #2: The scenario captures semantic evidence checkpoints.
#[test]
fn scenario_captures_semantic_evidence_checkpoints() {
    let json = generate_validate_runtime_scenario(RuntimeProfile::RealCodePuppy)
        .value_or_panic("should generate scenario");
    assert!(
        json.contains("\"capture\": \"validate-dashboard\""),
        "scenario must capture dashboard evidence: {json}"
    );
    assert!(
        json.contains("\"capture\": \"validate-runtime-chooser\""),
        "scenario must capture chooser evidence: {json}"
    );
}

/// Finding #2: The scenario opens New Agent (lowercase `n`) after creating
/// a repo (uppercase `N`) to trigger the runtime chooser. Jefe routes
/// lowercase `n` to New Agent when a repo exists.
#[test]
fn scenario_opens_new_agent() {
    let json = generate_validate_runtime_scenario(RuntimeProfile::RealLlxprt)
        .value_or_panic("should generate scenario");
    // Must create a repo first so lowercase n maps to New Agent.
    assert!(
        json.contains("\"key\": \"N\""),
        "scenario must press 'N' to create a repo first: {json}"
    );
    assert!(
        json.contains("\"waitFor\": \"New Repository\""),
        "scenario must wait for New Repository form: {json}"
    );
    // Then lowercase n opens New Agent.
    assert!(
        json.contains("\"key\": \"n\""),
        "scenario must press 'n' to open new agent: {json}"
    );
    assert!(
        json.contains("\"waitFor\": \"New Agent\""),
        "scenario must wait for New Agent form: {json}"
    );
    assert!(
        json.contains("\"waitFor\": \"Agent Runtime\""),
        "scenario must wait for Agent Runtime selector: {json}"
    );
}

/// Finding #2: The scenario does NOT start an agent — it only proves
/// detection by asserting the chooser shows the runtime label. The step
/// immediately after the chooser capture must be the quit macro (not any
/// agent-starting action).
#[test]
fn scenario_does_not_start_agent() {
    let json = generate_validate_runtime_scenario(RuntimeProfile::RealLlxprt)
        .value_or_panic("should generate scenario");

    // Parse the JSON and verify the step after the chooser capture is quit.
    let parsed: serde_json::Value =
        serde_json::from_str(&json).value_or_panic("scenario JSON must parse");
    let steps = parsed["steps"]
        .as_array()
        .value_or_panic("scenario must have steps array");

    let chooser_idx = steps
        .iter()
        .position(|s| {
            s.get("capture")
                .is_some_and(|c| c.as_str().is_some_and(|v| v == "validate-runtime-chooser"))
        })
        .value_or_panic("must have chooser capture step");

    assert!(
        chooser_idx + 1 < steps.len(),
        "there must be a step after the chooser capture"
    );

    let next_step = &steps[chooser_idx + 1];
    assert!(
        next_step
            .get("macro")
            .is_some_and(|m| { m.as_str().is_some_and(|v| v == "quit") }),
        "the step immediately after chooser capture must be macro=quit, got: {next_step}"
    );
    assert!(
        next_step.get("key").is_none(),
        "the next step must not contain any key action"
    );

    // Also verify the quit macro itself includes the Escape and C-q keys.
    let quit_steps = parsed["macros"]["quit"]["steps"]
        .as_array()
        .value_or_panic("quit macro must have steps");
    assert!(
        quit_steps
            .iter()
            .any(|s| { s.get("key").is_some_and(|k| k.as_str() == Some("Escape")) }),
        "quit macro must include Escape key: {json}"
    );
    assert!(
        quit_steps
            .iter()
            .any(|s| { s.get("key").is_some_and(|k| k.as_str() == Some("C-q")) }),
        "quit macro must include C-q key: {json}"
    );
}

/// Finding #2: The scenario uses strict assert mode so detection failures
/// fail the validation.
#[test]
fn scenario_uses_strict_assert_mode() {
    let json = generate_validate_runtime_scenario(RuntimeProfile::RealLlxprt)
        .value_or_panic("should generate scenario");
    assert!(
        json.contains("\"assert_mode\": \"strict\""),
        "scenario must use strict assert mode: {json}"
    );
}

// ── Finding #2: full Agent Runtime row assertion + absence of opposite ──

/// Finding #2: The scenario must assert on the full "Agent Runtime" label
/// row text (not just the runtime label), proving the chooser rendered the
/// runtime selector row.
#[test]
fn scenario_asserts_full_agent_runtime_row() {
    let json = generate_validate_runtime_scenario(RuntimeProfile::RealLlxprt)
        .value_or_panic("should generate scenario");
    assert!(
        json.contains("\"expect\": \"Agent Runtime\""),
        "scenario must assert on the full Agent Runtime row label: {json}"
    );
}

/// Finding #2: The scenario for llxprt must assert that the opposite runtime
/// label `code_puppy` is ABSENT via `waitForNot`, the supported harness step
/// that blocks until a pattern no longer appears on screen.
#[test]
fn scenario_for_llxprt_asserts_code_puppy_absent() {
    let json = generate_validate_runtime_scenario(RuntimeProfile::RealLlxprt)
        .value_or_panic("should generate scenario");
    assert!(
        json.contains("\"waitForNot\": \"code_puppy\""),
        "scenario must use waitForNot for code_puppy label: {json}"
    );
}

/// Finding #2: The scenario for code-puppy must assert that the opposite
/// runtime label `LLxprt` is ABSENT via `waitForNot`.
#[test]
fn scenario_for_code_puppy_asserts_llxprt_absent() {
    let json = generate_validate_runtime_scenario(RuntimeProfile::RealCodePuppy)
        .value_or_panic("should generate scenario");
    assert!(
        json.contains("\"waitForNot\": \"LLxprt\""),
        "scenario must use waitForNot for LLxprt label: {json}"
    );
}

/// Finding #2: The scenario must NOT use the unsupported `expectAbsent` step
/// kind — the harness only supports `waitForNot` for absence checking.
#[test]
fn scenario_does_not_use_expectabsent() {
    for profile in [RuntimeProfile::RealLlxprt, RuntimeProfile::RealCodePuppy] {
        let json = generate_validate_runtime_scenario(profile)
            .value_or_panic(&format!("scenario for {profile:?} should generate"));
        assert!(
            !json.contains("expectAbsent"),
            "scenario must not use unsupported expectAbsent step kind: {json}"
        );
    }
}

/// Finding #2: The scenario must NOT reference binary names (e.g. `llxprt`,
/// `code-puppy`) — these are internal identifiers never displayed in the TUI
/// form. Only visible UI labels (`LLxprt`, `code_puppy`) should be used.
#[test]
fn scenario_does_not_reference_binary_names() {
    for profile in [RuntimeProfile::RealLlxprt, RuntimeProfile::RealCodePuppy] {
        let json = generate_validate_runtime_scenario(profile)
            .value_or_panic(&format!("scenario for {profile:?} should generate"));
        assert!(
            !json.contains("\"expect\": \"llxprt\""),
            "scenario must not assert on binary name 'llxprt': {json}"
        );
        assert!(
            !json.contains("\"expect\": \"code-puppy\""),
            "scenario must not assert on binary name 'code-puppy': {json}"
        );
        assert!(
            !json.contains("\"waitForNot\": \"llxprt\""),
            "scenario must not assert on binary name 'llxprt': {json}"
        );
        assert!(
            !json.contains("\"waitForNot\": \"code-puppy\""),
            "scenario must not assert on binary name 'code-puppy': {json}"
        );
    }
}

/// Finding #2: The scenario for llxprt must still contain the selected
/// runtime label `LLxprt` as a positive assertion.
#[test]
fn scenario_for_llxprt_contains_llxprt_label_positive() {
    let json = generate_validate_runtime_scenario(RuntimeProfile::RealLlxprt)
        .value_or_panic("should generate scenario");
    assert!(
        json.contains("\"expect\": \"LLxprt\""),
        "scenario must positively assert LLxprt is present: {json}"
    );
}

/// Finding #2: The scenario for code-puppy must still contain the selected
/// runtime label `code_puppy` as a positive assertion.
#[test]
fn scenario_for_code_puppy_contains_code_puppy_label_positive() {
    let json = generate_validate_runtime_scenario(RuntimeProfile::RealCodePuppy)
        .value_or_panic("should generate scenario");
    assert!(
        json.contains("\"expect\": \"code_puppy\""),
        "scenario must positively assert code_puppy is present: {json}"
    );
}

/// Finding #2: Both runtime profiles must assert on the actual runtime label
/// via `expect`, not just `waitFor` (assertion proves detection, not just
/// that the UI appeared).
#[test]
fn scenario_uses_expect_not_just_waitfor_for_runtime_label() {
    for profile in [RuntimeProfile::RealLlxprt, RuntimeProfile::RealCodePuppy] {
        let json = generate_validate_runtime_scenario(profile)
            .value_or_panic(&format!("scenario for {profile:?} should generate"));
        let label = runtime_label(profile).value_or_panic("real profile has a label");
        let expect_pattern = format!("\"expect\": \"{label}\"");
        assert!(
            json.contains(&expect_pattern),
            "scenario for {profile:?} must use expect on exact runtime label: {json}"
        );
    }
}

/// The generated scenario JSON must parse successfully through the harness
/// `parse_scenario` function — proving the scenario uses only supported step
/// kinds and well-formed structure.
#[test]
fn scenario_parses_through_harness_parse_scenario() {
    for profile in [RuntimeProfile::RealLlxprt, RuntimeProfile::RealCodePuppy] {
        let json = generate_validate_runtime_scenario(profile)
            .value_or_panic(&format!("scenario for {profile:?} should generate"));
        jefe::harness::parse_scenario(&json).value_or_panic(&format!(
            "scenario for {profile:?} must parse through harness"
        ));
    }
}

// ── Finding #1: atomic scenario registration under artifacts/scenarios/ ──

/// Helper: create a run root with sentinel and a manifest with owned paths.
fn setup_test_run(
    run_label: &str,
    profile: RuntimeProfile,
) -> (
    tempfile::TempDir,
    std::path::PathBuf,
    std::path::PathBuf,
    crate::manifest::RunManifest,
) {
    let base = tempfile::tempdir().value_or_panic("temp dir");
    let run_root = base.path().join(run_label);
    let artifact_dir = run_root.join("artifacts");
    // Create the run root EXCLUSIVELY first (before any sub-directories),
    // otherwise create_dir_all on artifact_dir implicitly creates run_root
    // and the exclusive creation fails with RunRootCollision.
    super::persistence::create_run_root_with_run_id(&run_root, Some(run_label))
        .value_or_panic("create run root");
    std::fs::create_dir_all(&artifact_dir).value_or_panic("create artifact dir");
    let run_id = crate::manifest::RunId::new(run_label).value_or_panic("valid run id");
    let mut manifest =
        crate::manifest::RunManifest::new(run_id, "0.0.28", "validate-runtime", 100, 32, profile);
    manifest.add_owned_path(
        crate::manifest::OwnedPathKind::ArtifactDir,
        artifact_dir.clone(),
    );
    manifest.add_owned_path(
        crate::manifest::OwnedPathKind::ConfigDir,
        run_root.join("config"),
    );
    (base, run_root, artifact_dir, manifest)
}

/// Finding #1: `prepare_validate_runtime_scenario` writes the scenario under
/// `artifacts/scenarios/`, registers it in the manifest as
/// `ArtifactKind::Scenario`, and persists the manifest.
#[test]
fn prepare_scenario_writes_under_artifacts_scenarios_and_registers() {
    let (_base, run_root, artifact_dir, mut manifest) =
        setup_test_run("vr-001", RuntimeProfile::RealLlxprt);

    let scenario_path = prepare_validate_runtime_scenario(&mut manifest, &artifact_dir, &run_root)
        .value_or_panic("prepare scenario");

    // File must exist under artifacts/scenarios/.
    assert!(
        scenario_path.starts_with(&artifact_dir),
        "scenario must be under artifacts dir: {}",
        scenario_path.display()
    );
    assert!(
        scenario_path
            .components()
            .any(|c| c.as_os_str() == "scenarios"),
        "scenario path must contain 'scenarios' subdirectory"
    );
    assert!(
        scenario_path.exists(),
        "scenario file must exist: {}",
        scenario_path.display()
    );

    // Manifest must register it as ArtifactKind::Scenario.
    let scenario_artifact = manifest
        .artifacts
        .iter()
        .find(|a| a.kind == crate::manifest::ArtifactKind::Scenario)
        .value_or_panic("manifest must have a Scenario artifact");

    // Manifest must be persisted to disk with the scenario registered.
    let loaded =
        super::persistence::load_and_validate(&run_root).value_or_panic("load persisted manifest");
    assert!(
        loaded
            .artifacts
            .iter()
            .any(|a| a.kind == crate::manifest::ArtifactKind::Scenario),
        "persisted manifest must contain the Scenario artifact"
    );
    assert_eq!(
        loaded.artifacts.len(),
        manifest.artifacts.len(),
        "persisted manifest artifacts must match in-memory manifest"
    );
    assert!(
        scenario_artifact
            .label
            .contains("validate-runtime-scenario"),
        "scenario artifact label must identify the validate-runtime scenario"
    );
}

/// Finding #1: The scenario file is written atomically (complete content, no
/// partial write).
#[test]
fn prepare_scenario_atomic_write_produces_complete_file() {
    let (_base, run_root, artifact_dir, mut manifest) =
        setup_test_run("vr-002", RuntimeProfile::RealCodePuppy);

    let scenario_path = prepare_validate_runtime_scenario(&mut manifest, &artifact_dir, &run_root)
        .value_or_panic("prepare scenario");

    let content = std::fs::read_to_string(&scenario_path).value_or_panic("read scenario");
    let _: serde_json::Value =
        serde_json::from_str(&content).value_or_panic("scenario must be valid JSON");
}

/// Finding #1: `prepare_validate_runtime_scenario` fails for Shim profile.
#[test]
fn prepare_scenario_fails_for_shim_profile() {
    let (_base, run_root, artifact_dir, mut manifest) =
        setup_test_run("vr-003", RuntimeProfile::Shim);

    let result = prepare_validate_runtime_scenario(&mut manifest, &artifact_dir, &run_root);
    assert!(
        result.is_err(),
        "Shim profile must not produce a validate-runtime scenario"
    );
}

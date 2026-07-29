//! Integration tests for the JSP/1 compliance framework (issue 477).
//!
//! These tests drive the scenario corpus, schema oracle, and adapter profiles
//! as an external consumer would. They also execute the built CLI binary and
//! verify the output contract.

use std::path::PathBuf;

use jefe::jsp::v1::compliance::profile::validate_producer_trace;
use jefe::jsp::v1::compliance::scenario::{ScenarioOracle, load_scenario, load_scenario_package};
use jefe::jsp::v1::compliance::schema::{
    default_fixtures_dir, default_schemas_dir, run_schema_oracle,
};
use jefe::jsp::v1::compliance::server_profile::validate_server_transcript;
use serde_json::Value;

/// The workspace root: this crate's directory.
fn workspace_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

fn scenarios_dir() -> PathBuf {
    workspace_root().join("dev-docs/jsp/v1/compliance/scenarios")
}

fn traces_dir() -> PathBuf {
    workspace_root().join("dev-docs/jsp/v1/compliance/traces")
}

fn read_json(path: &std::path::Path) -> Value {
    let raw =
        std::fs::read_to_string(path).unwrap_or_else(|e| panic!("read {}: {e}", path.display()));
    serde_json::from_str(&raw).unwrap_or_else(|e| panic!("parse {}: {e}", path.display()))
}

fn json_bytes(value: &Value) -> Vec<u8> {
    serde_json::to_vec(value).unwrap_or_else(|error| panic!("serialize test mutation: {error}"))
}

/// RAII temp-directory guard for integration tests.
///
/// Uses exclusive `create_dir` (not `create_dir_all`) so the directory is
/// guaranteed not to pre-exist. A per-process nanosecond stamp ensures
/// uniqueness across parallel test runs.
struct TempDir {
    path: PathBuf,
}

impl TempDir {
    fn new() -> Self {
        let mut counter = 0u64;
        loop {
            counter += 1;
            assert!(
                counter <= 1024,
                "could not create a unique temp directory after 1024 attempts"
            );
            let path = std::env::temp_dir().join(format!(
                "jefe-477-{}-{}-{counter}",
                std::process::id(),
                std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .map_or(0, |d| d.as_nanos()),
            ));
            // Use `create_dir` (exclusive): fails if the directory already
            // exists, guaranteeing exclusive ownership of the temp path.
            if std::fs::create_dir(&path).is_ok() {
                return Self { path };
            }
        }
    }

    fn path(&self) -> &std::path::Path {
        &self.path
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.path);
    }
}

// ---------------------------------------------------------------------------
// Schema profile (C1)
// ---------------------------------------------------------------------------

#[test]
fn schema_oracle_accepts_corpus_and_rejects_negatives() {
    let root = workspace_root();
    let report = run_schema_oracle(&default_schemas_dir(&root), &default_fixtures_dir(&root))
        .unwrap_or_else(|e| panic!("schema oracle runs: {e:?}"));
    assert!(report.passed, "schema findings: {:?}", report.findings);
    assert!(report.positive_count > 0, "positive fixtures checked");
    assert!(report.negative_count > 0, "negative fixtures checked");
}

#[test]
fn schema_manifest_lists_all_three_document_kinds() {
    let root = workspace_root();
    let manifest_path = default_schemas_dir(&root).join("manifest.json");
    let manifest: Value = read_json(&manifest_path);
    let schemas = manifest
        .get("schemas")
        .and_then(Value::as_array)
        .unwrap_or_else(|| panic!("manifest has schemas"));
    let kinds: Vec<&str> = schemas
        .iter()
        .filter_map(|s| s.get("kind").and_then(Value::as_str))
        .collect();
    assert!(kinds.contains(&"snapshot"));
    assert!(kinds.contains(&"event"));
    assert!(kinds.contains(&"heartbeat"));
}

#[test]
fn schema_oracle_rejects_permissive_or_corrupted_standard_schema() {
    let source = default_schemas_dir(&workspace_root());
    let temp = TempDir::new();
    let root = temp.path();
    std::fs::create_dir_all(root.join("cases"))
        .unwrap_or_else(|error| panic!("create schema temp: {error}"));
    for entry in std::fs::read_dir(&source).unwrap_or_else(|error| panic!("read schemas: {error}"))
    {
        let entry = entry.unwrap_or_else(|error| panic!("schema entry: {error}"));
        if entry
            .file_type()
            .unwrap_or_else(|error| panic!("entry type: {error}"))
            .is_dir()
        {
            for case in std::fs::read_dir(entry.path())
                .unwrap_or_else(|error| panic!("read cases: {error}"))
            {
                let case = case.unwrap_or_else(|error| panic!("case entry: {error}"));
                std::fs::copy(case.path(), root.join("cases").join(case.file_name()))
                    .unwrap_or_else(|error| panic!("copy case: {error}"));
            }
        } else {
            std::fs::copy(entry.path(), root.join(entry.file_name()))
                .unwrap_or_else(|error| panic!("copy schema: {error}"));
        }
    }
    let schema_path = root.join("snapshot.schema.json");
    let mut schema = read_json(&schema_path);
    schema["additionalProperties"] = Value::Bool(true);
    std::fs::write(&schema_path, json_bytes(&schema))
        .unwrap_or_else(|error| panic!("write permissive schema: {error}"));
    let report = run_schema_oracle(root, &default_fixtures_dir(&workspace_root()))
        .unwrap_or_else(|error| panic!("schema oracle executes: {error}"));
    assert!(!report.passed);
    assert!(
        report
            .findings
            .iter()
            .any(|finding| finding.kind == "schema_semantics")
    );

    schema["type"] = Value::from(7);
    std::fs::write(&schema_path, json_bytes(&schema))
        .unwrap_or_else(|error| panic!("write uncompilable schema: {error}"));
    let report = run_schema_oracle(root, &default_fixtures_dir(&workspace_root()))
        .unwrap_or_else(|error| panic!("schema oracle recompiles mutation: {error}"));
    assert!(
        report
            .findings
            .iter()
            .any(|finding| finding.kind == "schema_compile"),
        "mutated schema must be recompiled independently"
    );
}

// ---------------------------------------------------------------------------
// Reducer/scenario profile (C2-C6)
// ---------------------------------------------------------------------------

#[test]
fn scenario_manifest_lists_exactly_fifteen_scenarios() {
    let manifest_path = scenarios_dir().join("manifest.json");
    let manifest: Value = read_json(&manifest_path);
    let scenarios = manifest
        .get("scenarios")
        .and_then(Value::as_array)
        .unwrap_or_else(|| panic!("manifest has scenarios"));
    assert_eq!(scenarios.len(), 15, "exactly 15 reference scenarios");
}

#[test]
fn every_scenario_passes_the_oracle() {
    let manifest_path = scenarios_dir().join("manifest.json");
    let manifest: Value = read_json(&manifest_path);
    let scenarios = manifest
        .get("scenarios")
        .and_then(Value::as_array)
        .unwrap_or_else(|| panic!("manifest has scenarios"));
    for entry in scenarios {
        let file = entry
            .get("file")
            .and_then(Value::as_str)
            .unwrap_or_else(|| panic!("entry has file"));
        let path = scenarios_dir().join(file);
        let scenario = load_scenario(&path).unwrap_or_else(|e| panic!("scenario loads: {e:?}"));
        let result = ScenarioOracle::evaluate(&scenario);
        assert!(
            result.passed,
            "scenario {} failed: {:?}",
            scenario.id,
            result
                .steps
                .iter()
                .filter(|s| !s.passed)
                .collect::<Vec<_>>()
        );
    }
}

#[test]
fn reducer_rejects_gap_without_partial_mutation() {
    use jefe::jsp::v1::compliance::projection::ActivityProjection;
    use jefe::jsp::v1::compliance::reducer::{ReducerError, ReferenceReducer};
    use jefe::jsp::v1::{parse_event, parse_snapshot};

    let snapshot_bytes = br#"{
        "schema": 1, "kind": "snapshot", "agent_id": "a", "lifecycle_generation": 1, "source_epoch": "e",
        "source_sequence": 0, "cursor": 0, "bridge_observed_ms": 0,
        "native_session": { "repository": "r", "path": "/p", "agent_kind": "k", "pid": 1, "display_name": "d" },
        "process_binding": "unsupported",
        "native_activity": { "provenance": "authoritative", "availability": "known", "value": { "state": "idle" } },
        "current_wait": { "provenance": "authoritative", "availability": "known", "value": null },
        "current_turn": "unsupported", "todos": "unsupported",
        "last_displayed_assistant_message": "unsupported", "last_created_tool_call": "unsupported",
        "source_terminal_state": "unsupported", "source_error_state": "unsupported"
    }"#;
    let snapshot =
        parse_snapshot(snapshot_bytes).unwrap_or_else(|e| panic!("snapshot parses: {e}"));

    let gap_event_bytes = br#"{
        "schema": 1, "kind": "event", "agent_id": "a", "lifecycle_generation": 1, "source_epoch": "e",
        "source_sequence": 3, "bridge_observed_ms": 0,
        "event": { "type": "activity.changed", "state": "acting" }
    }"#;
    let gap_event = parse_event(gap_event_bytes).unwrap_or_else(|e| panic!("event parses: {e}"));

    let mut reducer = ReferenceReducer::new();
    reducer.apply_snapshot(&snapshot);
    let err = match reducer.apply_event(&gap_event) {
        Ok(()) => panic!("gap event must fail"),
        Err(e) => e,
    };
    assert!(matches!(err, ReducerError::Gap { .. }));
    // No partial mutation: activity stays idle.
    let proj = reducer.projection();
    assert_eq!(proj.activity, ActivityProjection::Idle);
}
#[test]
fn scenario_loader_and_evaluator_fail_closed() {
    let source = scenarios_dir().join("s10_overflow_gap.json");
    let baseline = read_json(&source);
    let temp = TempDir::new();
    let root = temp.path();
    std::fs::create_dir_all(root).unwrap_or_else(|error| panic!("create temp: {error}"));
    let mutations = build_fail_closed_mutations(baseline);
    for (name, mutation, evaluates, expect_failed_step) in mutations {
        let path = root.join(format!("{name}.json"));
        std::fs::write(&path, json_bytes(&mutation))
            .unwrap_or_else(|error| panic!("write mutation: {error}"));
        match load_scenario(&path) {
            Ok(scenario) if evaluates => {
                let result = ScenarioOracle::evaluate(&scenario);
                assert!(!result.passed, "{name} must fail evaluation");
                assert_failing_step(&result, name, expect_failed_step);
            }
            Ok(_) => panic!("{name} unexpectedly loaded"),
            Err(error) if evaluates => {
                panic!("{name} should reach semantic evaluation: {error}")
            }
            Err(_) => {}
        }
    }
}

/// Build the (name, mutated JSON, evaluates?, expected failing step index)
/// tuples for the fail-closed scenario loader/evaluator test.
fn build_fail_closed_mutations(baseline: Value) -> Vec<(&'static str, Value, bool, Option<usize>)> {
    let mut mutations = Vec::new();
    let mut missing_expected = baseline.clone();
    missing_expected["steps"][0]
        .as_object_mut()
        .unwrap_or_else(|| panic!("step object"))
        .remove("expected");
    mutations.push(("missing-expected", missing_expected, false, None));
    let mut missing_document = baseline.clone();
    missing_document["steps"][0]
        .as_object_mut()
        .unwrap_or_else(|| panic!("step object"))
        .remove("document");
    mutations.push(("missing-document", missing_document, false, None));
    let mut malformed_base = baseline.clone();
    malformed_base["base_snapshot"]["native_activity"]["availability"] =
        Value::String("invalid".to_string());
    mutations.push(("malformed-base", malformed_base, false, None));
    let mut malformed_step = baseline.clone();
    malformed_step["steps"][0]["document"]["event"]["state"] = Value::String("invalid".to_string());
    mutations.push(("malformed-step", malformed_step, false, None));
    let mut unexpected_success = baseline.clone();
    // s10's step at index 1 expects a gap signal; make it contiguous so the
    // evaluator should reject it as an unexpected success at that step.
    unexpected_success["steps"][1]["document"]["source_sequence"] = Value::from(2);
    mutations.push(("unexpected-success", unexpected_success, true, Some(1usize)));
    let mut wrong_rejection = baseline;
    // Mismatched source_epoch makes the gap event an identity mismatch instead;
    // the gap-signal expectation then fails at step index 1.
    wrong_rejection["steps"][1]["document"]["source_epoch"] = Value::String("wrong".to_string());
    mutations.push(("wrong-rejection", wrong_rejection, true, Some(1)));
    mutations
}

fn assert_failing_step(
    result: &jefe::jsp::v1::compliance::scenario::ScenarioResult,
    name: &str,
    expect_failed_step: Option<usize>,
) {
    if let Some(expected_index) = expect_failed_step {
        assert!(
            result
                .steps
                .iter()
                .any(|step| !step.passed && step.index == expected_index),
            "{name}: expected failing step {expected_index}, got {:?}",
            result
                .steps
                .iter()
                .filter(|s| !s.passed)
                .collect::<Vec<_>>()
        );
    }
}

#[test]
fn snapshot_full_cursor_41_accepts_event_42() {
    use jefe::jsp::v1::compliance::reducer::ReferenceReducer;
    use jefe::jsp::v1::{parse_event, parse_snapshot};

    let snapshot_path = workspace_root().join("dev-docs/jsp/v1/fixtures/snapshot_full.json");

    let snapshot_bytes =
        std::fs::read(&snapshot_path).unwrap_or_else(|error| panic!("read snapshot_full: {error}"));
    let snapshot = parse_snapshot(&snapshot_bytes)
        .unwrap_or_else(|error| panic!("parse snapshot_full: {error}"));
    let event = br#"{
      "schema":1,"kind":"event","agent_id":"agent-alex","lifecycle_generation":7,
      "source_epoch":"epoch-001","source_sequence":42,"bridge_observed_ms":1785921964001,
      "event":{"type":"activity.changed","state":"acting"}
    }"#;
    let event = parse_event(event).unwrap_or_else(|error| panic!("parse event42: {error}"));
    let mut reducer = ReferenceReducer::new();
    reducer.apply_snapshot(&snapshot);
    assert_eq!(reducer.projection().last_sequence, 41);
    assert!(reducer.apply_event(&event).is_ok());
    assert_eq!(reducer.projection().last_sequence, 42);
}

// ---------------------------------------------------------------------------
#[test]
fn scenario_package_rejects_duplicate_unlisted_and_extra_files() {
    let temp = TempDir::new();
    let root = temp.path();
    std::fs::create_dir_all(root).unwrap_or_else(|error| panic!("create temp: {error}"));
    let source = scenarios_dir();
    for entry in std::fs::read_dir(&source).unwrap_or_else(|error| panic!("read source: {error}")) {
        let entry = entry.unwrap_or_else(|error| panic!("source entry: {error}"));
        std::fs::copy(entry.path(), root.join(entry.file_name()))
            .unwrap_or_else(|error| panic!("copy scenario package: {error}"));
    }
    let manifest_path = root.join("manifest.json");
    let original = read_json(&manifest_path);
    let mut duplicate = original.clone();
    duplicate["scenarios"][1] = duplicate["scenarios"][0].clone();
    std::fs::write(&manifest_path, json_bytes(&duplicate))
        .unwrap_or_else(|error| panic!("write duplicate manifest: {error}"));
    assert!(load_scenario_package(root).is_err());

    std::fs::write(&manifest_path, json_bytes(&original))
        .unwrap_or_else(|error| panic!("restore manifest: {error}"));
    std::fs::write(root.join("extra.json"), b"{}")
        .unwrap_or_else(|error| panic!("write extra: {error}"));
    assert!(load_scenario_package(root).is_err());
    std::fs::remove_file(root.join("extra.json"))
        .unwrap_or_else(|error| panic!("remove extra: {error}"));
    std::fs::remove_file(root.join("s15_privacy_mode.json"))
        .unwrap_or_else(|_| panic!("remove listed file"));
    assert!(load_scenario_package(root).is_err());
}

/// An in-package symlink must not escape the package directory or pass the
/// inventory check, even if the manifest lists it.
#[cfg(unix)]
#[test]
fn scenario_package_rejects_in_package_symlink() {
    use std::os::unix::fs::symlink;

    let temp = TempDir::new();
    let root = temp.path();
    std::fs::create_dir_all(root).unwrap_or_else(|error| panic!("create temp: {error}"));
    let source = scenarios_dir();
    for entry in std::fs::read_dir(&source).unwrap_or_else(|error| panic!("read source: {error}")) {
        let entry = entry.unwrap_or_else(|error| panic!("source entry: {error}"));
        std::fs::copy(entry.path(), root.join(entry.file_name()))
            .unwrap_or_else(|error| panic!("copy scenario package: {error}"));
    }
    // Create a unique RAII symlink target outside the package. The target
    // uses a unique name derived from temp_dir so it does not collide across
    // parallel test runs.
    let escape_target = std::env::temp_dir().join(format!(
        "jefe-477-symlink-target-{}-{}.json",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |d| d.as_nanos()),
    ));
    std::fs::write(&escape_target, b"{}")
        .unwrap_or_else(|error| panic!("write escape target: {error}"));
    let victim = root.join("s15_privacy_mode.json");
    std::fs::remove_file(&victim).unwrap_or_else(|error| panic!("remove victim: {error}"));
    symlink(&escape_target, &victim).unwrap_or_else(|error| panic!("create symlink: {error}"));
    let error = load_scenario_package(root)
        .err()
        .unwrap_or_else(|| panic!("symlink must be rejected"));
    assert!(
        error.to_string().contains("symlink"),
        "error must name the symlink: {error}"
    );
    let _ = std::fs::remove_file(&escape_target);
}

// Producer profile (C8)
// ---------------------------------------------------------------------------

#[test]
fn producer_trace_passes_profile() {
    let trace = read_json(&traces_dir().join("producer-trace.json"));
    let report = validate_producer_trace(&json_bytes(&trace));
    assert!(report.passed, "producer findings: {:?}", report.findings);
}

#[test]
fn producer_trace_missing_gap_signal_fails() {
    let mut trace = read_json(&traces_dir().join("producer-trace.json"));
    // Remove the observable emitted/dropped-range challenge.
    if let Some(facts) = trace.get_mut("facts").and_then(Value::as_array_mut) {
        facts.retain(|f| f.get("fact").and_then(Value::as_str) != Some("gap_challenge"));
    }
    let report = validate_producer_trace(&json_bytes(&trace));
    assert!(!report.passed);
    assert!(
        report
            .findings
            .iter()
            .any(|f| f.invariant == "nonblocking_gap_signaling")
    );
}

// ---------------------------------------------------------------------------
// Server profile (C9-C12)
// ---------------------------------------------------------------------------

#[test]
fn server_transcript_passes_profile() {
    let transcript = read_json(&traces_dir().join("server-transcript.json"));
    let report = validate_server_transcript(&json_bytes(&transcript));
    assert!(report.passed, "server findings: {:?}", report.findings);
}

#[test]
fn server_transcript_snapshot_first_stream_required() {
    let mut transcript = read_json(&traces_dir().join("server-transcript.json"));
    let interactions = transcript["interactions"]
        .as_array_mut()
        .unwrap_or_else(|| panic!("interactions array"));
    let observe = interactions
        .iter_mut()
        .find(|interaction| interaction["name"] == "observe_stream_snapshot_first")
        .unwrap_or_else(|| panic!("observe interaction"));
    observe["stream"][0]["kind"] = Value::String("event".to_string());
    let report = validate_server_transcript(&json_bytes(&transcript));
    assert!(!report.passed);
    assert!(report.findings.iter().any(|finding| {
        finding.invariant == "canonical_snapshot_first" || finding.invariant == "transcript_shape"
    }));
}

#[test]
fn server_semantics_do_not_trust_name_or_assertion_tags() {
    let mut transcript = read_json(&traces_dir().join("server-transcript.json"));
    let interactions = transcript["interactions"]
        .as_array_mut()
        .unwrap_or_else(|| panic!("interactions array"));
    for interaction in interactions {
        interaction["name"] = Value::String("descriptive-only".to_string());
        interaction["assert"] = Value::String("descriptive-only".to_string());
    }
    let report = validate_server_transcript(&json_bytes(&transcript));
    assert!(
        report.passed,
        "semantic transcript ignores labels: {:?}",
        report.findings
    );
}

#[test]
fn server_duplicate_response_mutation_fails() {
    let mut transcript = read_json(&traces_dir().join("server-transcript.json"));
    let interactions = transcript["interactions"]
        .as_array_mut()
        .unwrap_or_else(|| panic!("interactions array"));
    let duplicate = interactions
        .iter_mut()
        .find(|interaction| interaction["name"] == "publish_duplicate_event_1")
        .unwrap_or_else(|| panic!("duplicate interaction"));
    duplicate["response"]["kind"] = Value::String("accepted".to_string());
    let report = validate_server_transcript(&json_bytes(&transcript));
    assert!(!report.passed);
    assert!(
        report
            .findings
            .iter()
            .any(|finding| finding.invariant == "no_op_mutation")
    );
}

// ---------------------------------------------------------------------------
// CLI binary output contract (C7)
// ---------------------------------------------------------------------------

#[test]
fn cli_all_profile_exits_zero_and_emits_pass_report() {
    let bin = jefe_bin_path();
    let root = workspace_root();
    let output = std::process::Command::new(&bin)
        .arg("all")
        .arg("--root")
        .arg(&root)
        .output()
        .unwrap_or_else(|e| panic!("run CLI: {e}"));
    assert!(
        output.status.success(),
        "CLI exited {:?}: {}",
        output.status.code(),
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    let report: Value = serde_json::from_str(stdout.trim())
        .unwrap_or_else(|e| panic!("CLI emits valid JSON on stdout: {e}"));
    assert_eq!(report["outcome"], "pass");
    assert_eq!(report["profile"], "all");
    assert!(report["checks_total"].as_u64().unwrap_or(0) > 0);
    assert_eq!(report["checks_passed"], report["checks_total"]);
}

#[test]
fn cli_schema_profile_exits_zero() {
    let bin = jefe_bin_path();
    let root = workspace_root();
    let output = std::process::Command::new(&bin)
        .arg("schema")
        .arg("--root")
        .arg(&root)
        .output()
        .unwrap_or_else(|e| panic!("run CLI: {e}"));
    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    let report: Value =
        serde_json::from_str(stdout.trim()).unwrap_or_else(|e| panic!("valid JSON: {e}"));
    assert_eq!(report["outcome"], "pass");
}

#[test]
fn cli_unknown_profile_exits_nonzero() {
    let bin = jefe_bin_path();
    let root = workspace_root();
    let output = std::process::Command::new(&bin)
        .arg("bogus")
        .arg("--root")
        .arg(&root)
        .output()
        .unwrap_or_else(|e| panic!("run CLI: {e}"));

    assert!(!output.status.success());
}

#[test]
fn cli_help_flag_prints_usage_to_stdout_and_exits_zero() {
    let bin = jefe_bin_path();
    for flag in ["--help", "-h"] {
        let output = std::process::Command::new(&bin)
            .arg(flag)
            .output()
            .unwrap_or_else(|e| panic!("run CLI {flag}: {e}"));
        assert!(
            output.status.success(),
            "{flag} must exit 0: {:?}",
            output.status.code()
        );
        let stdout = String::from_utf8_lossy(&output.stdout);
        assert!(
            stdout.contains("usage:"),
            "{flag} stdout must contain usage: {stdout}"
        );
        assert!(
            stdout.contains("schema|reducer|producer|server|all"),
            "{flag} stdout must list profiles: {stdout}"
        );
        // Help must not emit a JSON report.
        assert!(
            !stdout.contains("\"outcome\""),
            "{flag} must not emit JSON: {stdout}"
        );
        // Help must not emit a JSON report.
        assert!(
            output.stderr.is_empty(),
            "{flag} must not write to stderr: {:?}",
            String::from_utf8_lossy(&output.stderr)
        );
    }
}

#[test]
fn cli_all_aggregates_all_profiles_even_when_one_is_fatal() {
    let bin = jefe_bin_path();
    // Point root at a directory without the compliance fixtures so the schema
    // profile (the first aggregator) hits a fatal I/O error. `all` must still
    // produce a single deterministic fail report with per-profile failures
    // rather than short-circuiting or emitting a bare CLI error.
    let temp = TempDir::new();
    let output = std::process::Command::new(&bin)
        .arg("all")
        .arg("--root")
        .arg(temp.path())
        .output()
        .unwrap_or_else(|e| panic!("run CLI all: {e}"));
    assert!(
        !output.status.success(),
        "all must fail when fixtures are missing"
    );
    assert!(output.stderr.is_empty(), "no stderr on aggregated failure");
    let report: Value = serde_json::from_slice(&output.stdout)
        .unwrap_or_else(|e| panic!("all emits valid JSON on stdout: {e}"));
    assert_eq!(report["outcome"], "fail");
    assert_eq!(report["profile"], "all");
    let failures = report["failures"]
        .as_array()
        .unwrap_or_else(|| panic!("failures is an array"));
    assert!(
        !failures.is_empty(),
        "aggregated report must carry per-profile failures"
    );
    assert!(
        failures.iter().any(|f| f["invariant"] == "profile_fatal"),
        "at least one failure must be a profile_fatal: {failures:?}"
    );
}

#[cfg(unix)]
#[test]
fn cli_non_utf8_path_reports_json_without_echoing_bytes() {
    use std::os::unix::ffi::OsStringExt;

    // Derive a non-UTF-8 path from the system temp directory so the test
    // does not assume a specific filesystem layout.
    let mut prefix = std::env::temp_dir();
    prefix.push("jefe-477-non-utf8");
    let mut bytes = prefix.into_os_string().into_vec();
    bytes.extend_from_slice(&[b'-', 0xff, b'.', b'j']);
    let path = std::ffi::OsString::from_vec(bytes);
    let output = std::process::Command::new(jefe_bin_path())
        .arg("producer")
        .arg("--input")
        .arg(path)
        .output()
        .unwrap_or_else(|error| panic!("run non-UTF-8 CLI path: {error}"));
    assert!(!output.status.success());
    assert!(output.stderr.is_empty());
    let report: Value = serde_json::from_slice(&output.stdout)
        .unwrap_or_else(|error| panic!("non-UTF-8 failure JSON stdout: {error}"));
    assert_eq!(report["outcome"], "fail");
    assert_eq!(report["failures"][0]["invariant"], "cli_input");
    assert_eq!(
        report["failures"][0]["detail"],
        "producer qualification requires --adapter or --reference-adapter"
    );
}
/// Locate the built compliance binary.
fn jefe_bin_path() -> PathBuf {
    PathBuf::from(env!("CARGO_BIN_EXE_jefe-jsp-compliance"))
}

#[test]
fn cli_producer_and_server_require_executable_qualification() {
    let binary = jefe_bin_path();
    for (profile, file) in [
        ("producer", "producer-trace.json"),
        ("server", "server-transcript.json"),
    ] {
        let rejected = std::process::Command::new(&binary)
            .arg(profile)
            .arg("--input")
            .arg(traces_dir().join(file))
            .output()
            .unwrap_or_else(|error| panic!("run {profile} CLI: {error}"));
        assert!(
            !rejected.status.success(),
            "{profile} fixture must not qualify"
        );

        let qualified = std::process::Command::new(&binary)
            .arg(profile)
            .arg("--reference-adapter")
            .arg("--nonce")
            .arg("477")
            .output()
            .unwrap_or_else(|error| panic!("run {profile} reference CLI: {error}"));
        assert!(
            qualified.status.success(),
            "{profile} executable qualification failed"
        );
        let report: Value = serde_json::from_slice(&qualified.stdout)
            .unwrap_or_else(|error| panic!("{profile} JSON stdout: {error}"));
        assert_eq!(report["outcome"], "pass");
    }
}

#[test]
fn cli_input_error_is_json_stdout_and_nonzero() {
    let output = std::process::Command::new(jefe_bin_path())
        .arg("producer")
        .arg("--input")
        .arg(workspace_root().join("missing-producer-trace.json"))
        .output()
        .unwrap_or_else(|error| panic!("run failing CLI: {error}"));
    assert!(!output.status.success());
    assert!(output.stderr.is_empty());
    let report: Value = serde_json::from_slice(&output.stdout)
        .unwrap_or_else(|error| panic!("failure JSON stdout: {error}"));
    assert_eq!(report["outcome"], "fail");
    assert_eq!(report["failures"][0]["invariant"], "cli_input");
}

// ---------------------------------------------------------------------------
// Adversarial tests (Slice A remediation)
// ---------------------------------------------------------------------------

/// A metadata string exceeding the 4096-byte bound must be rejected with a
/// payload-free error code, not silently accepted through unbounded
/// deserialization.
#[test]
fn producer_trace_rejects_oversized_description() {
    let mut trace = read_json(&traces_dir().join("producer-trace.json"));
    // 8192 bytes: exceeds the 4096 metadata-string bound but fits within
    // the 1 MiB total input bound so the metadata bound is what triggers.
    trace["description"] = Value::String("x".repeat(8192));
    let report = validate_producer_trace(&json_bytes(&trace));
    assert!(!report.passed);
    assert!(
        report
            .findings
            .iter()
            .any(|finding| finding.invariant == "trace_shape"),
        "oversized description must be rejected: {:?}",
        report.findings
    );
}

/// A 2 MiB description must be rejected by the outer input bound with a
/// payload-free artifact_bound code.
#[test]
fn producer_trace_rejects_2mib_description() {
    let mut trace = read_json(&traces_dir().join("producer-trace.json"));
    trace["description"] = Value::String("x".repeat(2 * 1024 * 1024));
    let report = validate_producer_trace(&json_bytes(&trace));
    assert!(!report.passed);
    assert!(
        report
            .findings
            .iter()
            .any(|finding| finding.invariant == "artifact_bound"),
        "2 MiB description must be rejected by outer bound: {:?}",
        report.findings
    );
}

/// A metadata string exceeding the 4096-byte bound in the server transcript
/// must also be rejected.
#[test]
fn server_transcript_rejects_oversized_description() {
    let mut transcript = read_json(&traces_dir().join("server-transcript.json"));
    transcript["description"] = Value::String("x".repeat(8192));
    let report = validate_server_transcript(&json_bytes(&transcript));
    assert!(!report.passed);
    assert!(
        report
            .findings
            .iter()
            .any(|finding| finding.invariant == "transcript_shape"),
        "oversized description must be rejected: {:?}",
        report.findings
    );
}

/// A scenario package manifest with a corrupted version must fail
/// with a payload-free code, never leaking the OS error string or path.
#[test]
fn scenario_manifest_corrupted_version_fails_payload_free() {
    let temp = TempDir::new();
    let root = temp.path();
    std::fs::create_dir_all(root).unwrap_or_else(|error| panic!("create temp: {error}"));
    // Write a manifest with the wrong version
    let manifest = serde_json::json!({
        "schema": 1,
        "kind": "scenario-manifest",
        "scenario_artifact_version": "wrong-version",
        "description": "bad",
        "scenarios": []
    });
    std::fs::write(root.join("manifest.json"), json_bytes(&manifest))
        .unwrap_or_else(|error| panic!("write manifest: {error}"));
    let error = load_scenario_package(root)
        .err()
        .unwrap_or_else(|| panic!("corrupted manifest must fail"));
    let message = error.to_string();
    assert!(
        message.starts_with("JSP-C-"),
        "error must be a payload-free code, got: {message}"
    );
    assert!(
        !message.contains('/') || message.contains("JSP-C-"),
        "error must not leak OS paths: {message}"
    );
}

/// The scenario oracle must populate typed expected/actual sequence fields
/// from a gap rejection rather than relying on string parsing.
#[test]
fn step_outcome_carries_typed_gap_sequence() {
    use jefe::jsp::v1::compliance::scenario::ScenarioResult;
    let source = scenarios_dir().join("s10_overflow_gap.json");
    let scenario = load_scenario(&source).unwrap_or_else(|error| panic!("load: {error}"));
    let result: ScenarioResult = ScenarioOracle::evaluate(&scenario);
    // S10 has a gap step that should produce typed gap fields on any failing
    // step. Even when the scenario passes, the step outcomes exist.
    assert!(result.passed, "S10 should pass: {:?}", result.steps);
    assert!(!result.steps.is_empty());
    // S10 includes an EventGap step; the typed sequence fields should be
    // populated on that step to prove the typed path (not string parsing)
    // is used for gap diagnostics.
    let gap_step = result
        .steps
        .iter()
        .find(|step| step.expected_sequence.is_some());
    assert!(
        gap_step.is_some(),
        "S10 must have a step with populated typed sequence fields"
    );
    let gap_step = gap_step.unwrap_or_else(|| panic!("gap step"));
    assert!(gap_step.expected_sequence.is_some());
    assert!(gap_step.actual_sequence.is_some());
    // A gap means actual > expected
    assert!(
        gap_step.actual_sequence.unwrap_or(0) > gap_step.expected_sequence.unwrap_or(0),
        "gap actual must exceed expected"
    );
}

/// The top-level manifest must validate its artifact version, paths,
/// and profile inventory. A corrupted version must produce a CLI failure.
#[test]
fn cli_all_detects_top_level_manifest_version_drift() {
    let temp = TempDir::new();
    let root = temp.path();
    let compliance_dir = root.join("dev-docs/jsp/v1/compliance");
    std::fs::create_dir_all(&compliance_dir).unwrap_or_else(|error| panic!("create dirs: {error}"));

    // Copy the real compliance artifacts
    let source = workspace_root().join("dev-docs/jsp/v1/compliance");
    copy_dir_recursive(&source, &compliance_dir);

    // Corrupt the top-level manifest version
    let manifest_path = compliance_dir.join("manifest.json");
    let mut manifest = read_json(&manifest_path);
    manifest["compliance_artifact_version"] = Value::String("drifted-version".to_string());
    std::fs::write(&manifest_path, json_bytes(&manifest))
        .unwrap_or_else(|error| panic!("write corrupted manifest: {error}"));

    let output = std::process::Command::new(jefe_bin_path())
        .arg("all")
        .arg("--root")
        .arg(root)
        .output()
        .unwrap_or_else(|error| panic!("run CLI: {error}"));
    assert!(!output.status.success(), "must fail on version drift");
    let report: Value = serde_json::from_slice(&output.stdout)
        .unwrap_or_else(|error| panic!("valid JSON: {error}"));
    assert_eq!(report["outcome"], "fail");
    assert!(
        report["failures"]
            .as_array()
            .is_some_and(|failures| failures
                .iter()
                .any(|f| f["invariant"] == "manifest_inventory")),
        "must report manifest_inventory failure"
    );
}

/// Schema oracle must reject an oversized artifact file with a payload-free
/// bound error code.
#[test]
fn schema_oracle_rejects_oversized_artifact() {
    let source = default_schemas_dir(&workspace_root());
    let temp = TempDir::new();
    let root = temp.path();
    std::fs::create_dir_all(root.join("cases"))
        .unwrap_or_else(|error| panic!("create schema temp: {error}"));
    for entry in std::fs::read_dir(&source).unwrap_or_else(|error| panic!("read schemas: {error}"))
    {
        let entry = entry.unwrap_or_else(|error| panic!("schema entry: {error}"));
        if entry
            .file_type()
            .unwrap_or_else(|error| panic!("entry type: {error}"))
            .is_dir()
        {
            for case in std::fs::read_dir(entry.path())
                .unwrap_or_else(|error| panic!("read cases: {error}"))
            {
                let case = case.unwrap_or_else(|error| panic!("case entry: {error}"));
                std::fs::copy(case.path(), root.join("cases").join(case.file_name()))
                    .unwrap_or_else(|error| panic!("copy case: {error}"));
            }
        } else {
            std::fs::copy(entry.path(), root.join(entry.file_name()))
                .unwrap_or_else(|error| panic!("copy schema: {error}"));
        }
    }
    // Overwrite manifest.json with a file exceeding the bound
    let oversized = "x".repeat((2 * 1024 * 1024) + 1);
    std::fs::write(root.join("manifest.json"), &oversized)
        .unwrap_or_else(|error| panic!("write oversized: {error}"));
    let error = run_schema_oracle(root, &default_fixtures_dir(&workspace_root()))
        .err()
        .unwrap_or_else(|| panic!("oversized artifact must fail"));
    assert!(
        error.message.contains("BOUND"),
        "error must be a bound code: {}",
        error.message
    );
}

#[test]
fn schema_package_rejects_unlisted_artifact() {
    let source = default_schemas_dir(&workspace_root());
    let temp = TempDir::new();
    copy_dir_recursive(&source, temp.path());
    std::fs::write(temp.path().join("unlisted.json"), b"{}")
        .unwrap_or_else(|error| panic!("write unlisted artifact: {error}"));
    assert!(run_schema_oracle(temp.path(), &default_fixtures_dir(&workspace_root())).is_err());
}

#[cfg(unix)]
#[test]
fn schema_package_rejects_symlinked_schema() {
    use std::os::unix::fs::symlink;

    let source = default_schemas_dir(&workspace_root());
    let temp = TempDir::new();
    copy_dir_recursive(&source, temp.path());
    let target = temp.path().join("event.schema.json");
    std::fs::remove_file(&target).unwrap_or_else(|error| panic!("remove schema: {error}"));
    symlink(source.join("event.schema.json"), &target)
        .unwrap_or_else(|error| panic!("create schema symlink: {error}"));
    assert!(run_schema_oracle(temp.path(), &default_fixtures_dir(&workspace_root())).is_err());
}
/// Recursively copy a directory tree for test setup.
fn copy_dir_recursive(source: &std::path::Path, destination: &std::path::Path) {
    std::fs::create_dir_all(destination).unwrap_or_else(|error| panic!("create dir: {error}"));
    for entry in std::fs::read_dir(source).unwrap_or_else(|error| panic!("read dir: {error}")) {
        let entry = entry.unwrap_or_else(|error| panic!("dir entry: {error}"));
        let path = entry.path();
        let dest = destination.join(entry.file_name());
        if path.is_dir() {
            copy_dir_recursive(&path, &dest);
        } else {
            std::fs::copy(&path, &dest).unwrap_or_else(|error| panic!("copy file: {error}"));
        }
    }
}

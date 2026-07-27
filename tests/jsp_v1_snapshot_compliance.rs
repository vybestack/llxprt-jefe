//! JSP/1 snapshot compliance integration test (issue 476, J1 slice).
//!
//! This is the language-neutral fixture-manifest oracle. It loads
//! `dev-docs/jsp/v1/fixtures/manifest.json`, enumerates every listed fixture,
//! and asserts each one parses to the manifest-declared expected result. A
//! missing or unlisted fixture fails the integration test (S6). Credential and
//! control fields are attempted in `snapshot_forbidden_fields.json` and must
//! fail closed (S5/S6).
//!
//! Canonical fixtures live outside the crate so external implementations can
//! consume the exact same corpus from another repository/language.

use std::collections::HashSet;
use std::fs;
use std::path::PathBuf;

use jefe::domain::observation::{FieldState, NativeActivityState, NativeActivityValue, Provenance};
use jefe::jsp::v1::error::JspCode;
use jefe::jsp::v1::parse_snapshot;

/// Resolve the fixtures directory under the workspace root.
fn fixtures_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("dev-docs")
        .join("jsp")
        .join("v1")
        .join("fixtures")
}

/// One manifest fixture entry, mirroring `manifest.json`.
#[derive(serde::Deserialize)]
struct ManifestEntry {
    name: String,
    file: String,
    expected: String,
    #[serde(default)]
    error_code: Option<String>,
}

/// The manifest document.
#[derive(serde::Deserialize)]
struct Manifest {
    fixtures: Vec<ManifestEntry>,
}

/// Load and parse the manifest document.
fn load_manifest() -> Manifest {
    let manifest_path = fixtures_dir().join("manifest.json");
    let raw = fs::read_to_string(&manifest_path)
        .unwrap_or_else(|_| panic!("manifest must exist at {}", manifest_path.display()));
    serde_json::from_str(&raw)
        .unwrap_or_else(|_| panic!("manifest must be valid JSON at {}", manifest_path.display()))
}

/// Assert that every JSON file on disk is listed in the manifest (S6: a
/// missing/unlisted fixture fails the integration test).
#[test]
fn manifest_enumerates_every_fixture_on_disk() {
    let manifest = load_manifest();
    let listed: HashSet<String> = manifest.fixtures.iter().map(|e| e.file.clone()).collect();
    let mut on_disk: HashSet<String> = HashSet::new();
    let dir = fs::read_dir(fixtures_dir())
        .unwrap_or_else(|err| panic!("fixtures directory readable: {err}"));
    for entry in dir {
        let entry = entry.unwrap_or_else(|err| panic!("readable dir entry: {err}"));
        let path = entry.path();
        if path.extension().is_some_and(|ext| ext == "json") {
            let name = path
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or_else(|| panic!("utf-8 fixture name for {}", path.display()));
            if name != "manifest.json" {
                on_disk.insert(name.to_string());
            }
        }
    }
    assert_eq!(
        listed, on_disk,
        "every fixture on disk must be listed in the manifest and vice versa"
    );
}

/// Drive every manifest entry through the JSP/1 parser and assert the declared
/// result. This is the core compliance oracle (S1-S6).
#[test]
fn manifest_fixtures_match_expected_results() {
    let manifest = load_manifest();
    assert!(
        !manifest.fixtures.is_empty(),
        "manifest must enumerate at least one fixture"
    );

    for entry in &manifest.fixtures {
        let path = fixtures_dir().join(&entry.file);
        let bytes = fs::read(&path)
            .unwrap_or_else(|_| panic!("fixture {} exists at {}", entry.name, path.display()));

        match entry.expected.as_str() {
            "ok" => match parse_snapshot(&bytes) {
                Ok(snapshot) => {
                    assert_expected_identity(&entry.name, &snapshot);
                }
                Err(error) => panic!(
                    "fixture {} expected ok but failed: {} ({})",
                    entry.name,
                    error.code(),
                    error.code().as_str()
                ),
            },
            "error" => {
                let expected_code = entry.error_code.as_deref().unwrap_or_else(|| {
                    panic!(
                        "fixture {} declares error_code for error expected",
                        entry.name
                    )
                });
                match parse_snapshot(&bytes) {
                    Ok(_) => panic!(
                        "fixture {} expected error {} but parsed successfully",
                        entry.name, expected_code
                    ),
                    Err(error) => {
                        let actual = error.code();
                        assert_eq!(
                            actual.as_str(),
                            expected_code,
                            "fixture {} expected code {} but got {}",
                            entry.name,
                            expected_code,
                            actual.as_str()
                        );
                    }
                }
            }
            other => panic!("fixture {} has unknown expected value: {other}", entry.name),
        }
    }
}

/// Cross-check that the parsed identity is non-empty and well-formed (S1/S3).
fn assert_expected_identity(fixture_name: &str, snapshot: &jefe::jsp::v1::Snapshot) {
    assert!(
        !snapshot.identity.agent_id.as_str().is_empty(),
        "{fixture_name}: agent_id must not be empty"
    );
    assert!(
        snapshot.identity.lifecycle_generation >= 1,
        "{fixture_name}: lifecycle_generation must be positive"
    );
}

/// S1: the canonical full snapshot yields exact typed identity, cursor, and
/// semantic fields with provenance and availability.
#[test]
fn s1_canonical_full_snapshot_has_exact_typed_fields() {
    let path = fixtures_dir().join("snapshot_full.json");
    let bytes =
        fs::read(&path).unwrap_or_else(|_| panic!("full fixture exists at {}", path.display()));
    let snapshot =
        parse_snapshot(&bytes).unwrap_or_else(|err| panic!("full snapshot must parse: {err}"));

    assert_eq!(snapshot.identity.agent_id.as_str(), "agent-alex");
    assert_eq!(snapshot.identity.lifecycle_generation, 7);
    assert_eq!(snapshot.identity.source_epoch.as_str(), "epoch-001");
    assert_eq!(snapshot.cursor, 41);
    assert_eq!(snapshot.source_sequence, 42);
    assert_eq!(snapshot.bridge_observed_ms, 1_785_921_964_000);

    assert_eq!(
        snapshot.native_session.repository.as_str(),
        "vybestack/llxprt-jefe"
    );
    assert_eq!(snapshot.native_session.path.as_str(), "/Users/dev/src/jefe");
    assert_eq!(snapshot.native_session.agent_kind.as_str(), "llxprt");
    assert_eq!(snapshot.native_session.pid, 12_345);
    assert_eq!(snapshot.native_session.display_name.as_str(), "main-worker");

    let FieldState::Supported {
        provenance,
        availability,
    } = &snapshot.process_binding
    else {
        panic!("process_binding must be a supported field state");
    };
    assert_eq!(*provenance, Provenance::Authoritative);
    let binding = availability
        .known_value()
        .unwrap_or_else(|| panic!("process_binding must carry a known value"));
    assert_eq!(binding.pid, 12_345);
    assert_eq!(binding.started_at_ms, 1_785_921_000_000);

    assert_eq!(
        snapshot.native_activity,
        FieldState::known(
            Provenance::Authoritative,
            NativeActivityValue {
                state: NativeActivityState::Idle
            }
        ),
        "native activity is source-owned and authoritative in the canonical fixture"
    );
}

/// S2: closed grammar rejects unknown fields, wrong schema, wrong kind, and
/// wrong types with deterministic JSP-E001/JSP-E003 and no echoed payload.
#[test]
fn s2_closed_grammar_rejects_unknown_and_version_violations() {
    let path = fixtures_dir().join("snapshot_closed_grammar.json");
    let unknown =
        fs::read(&path).unwrap_or_else(|_| panic!("closed grammar fixture at {}", path.display()));
    let error = parse_snapshot(&unknown)
        .err()
        .unwrap_or_else(|| panic!("unknown fields must fail"));
    assert_eq!(error.code(), JspCode::EClosedShape);
}

/// S2: schema 0 and schema 2 both fail with JSP-E003.
#[test]
fn s2_unsupported_schema_version_fails() {
    let schema0 = br#"{"schema":0,"kind":"snapshot","agent_id":"a","lifecycle_generation":1,"source_epoch":"e","source_sequence":1,"cursor":0,"bridge_observed_ms":1,"native_session":{"repository":"r","path":"p","agent_kind":"llxprt","pid":1,"display_name":"d"},"process_binding":"unsupported","native_activity":"unsupported","current_wait":"unsupported","current_turn":"unsupported","todos":"unsupported","last_displayed_assistant_message":"unsupported","last_created_tool_call":"unsupported","source_terminal_state":"unsupported","source_error_state":"unsupported"}"#;
    let error = parse_snapshot(schema0)
        .err()
        .unwrap_or_else(|| panic!("schema 0 must fail"));
    assert_eq!(error.code(), JspCode::EUnsupportedVersion);

    let mut schema2 = schema0.to_vec();
    // Replace the '0' after "schema": with '2' (byte index 10).
    schema2[10] = b'2';
    let error = parse_snapshot(&schema2)
        .err()
        .unwrap_or_else(|| panic!("schema 2 must fail"));
    assert_eq!(error.code(), JspCode::EUnsupportedVersion);
}

/// S2: empty input, truncated JSON, and non-utf8 all fail with JSP-E001.
#[test]
fn s2_malformed_input_fails_closed() {
    let cases: &[&[u8]] = &[b"", b"{", b"{\"schema\"", &[0xFF, 0xFE, 0xFD]];
    for input in cases {
        let error = parse_snapshot(input)
            .err()
            .unwrap_or_else(|| panic!("malformed input must fail"));
        assert_eq!(error.code(), JspCode::EClosedShape);
    }
}

/// S2: a 129-byte agent_id exceeds the 128-byte bound and fails with JSP-E002.
#[test]
fn s2_over_limit_agent_id_fails() {
    let too_long = "a".repeat(129);
    let json = format!(
        r#"{{"schema":1,"kind":"snapshot","agent_id":"{too_long}","lifecycle_generation":1,"source_epoch":"e","source_sequence":1,"cursor":0,"bridge_observed_ms":1,"native_session":{{"repository":"r","path":"p","agent_kind":"llxprt","pid":1,"display_name":"d"}},"process_binding":"unsupported","native_activity":"unsupported","current_wait":"unsupported","current_turn":"unsupported","todos":"unsupported","last_displayed_assistant_message":"unsupported","last_created_tool_call":"unsupported","source_terminal_state":"unsupported","source_error_state":"unsupported"}}"#
    );
    let error = parse_snapshot(json.as_bytes())
        .err()
        .unwrap_or_else(|| panic!("over-limit id must fail"));
    assert_eq!(error.code(), JspCode::EBound);
}

/// S3: two snapshots in the same worktree with distinct agent/generation/epoch
/// produce distinct live keys.
#[test]
fn s3_distinct_identity_remains_distinct() {
    let path_a = fixtures_dir().join("snapshot_full.json");
    let a = fs::read(&path_a).unwrap_or_else(|_| panic!("full fixture at {}", path_a.display()));
    let path_b = fixtures_dir().join("snapshot_identity_distinct.json");
    let b =
        fs::read(&path_b).unwrap_or_else(|_| panic!("identity fixture at {}", path_b.display()));
    let snap_a = parse_snapshot(&a).unwrap_or_else(|err| panic!("full parses: {err}"));
    let snap_b = parse_snapshot(&b).unwrap_or_else(|err| panic!("identity parses: {err}"));

    assert_ne!(
        snap_a.identity.agent_id, snap_b.identity.agent_id,
        "distinct agent ids"
    );
    assert_ne!(
        snap_a.identity.lifecycle_generation, snap_b.identity.lifecycle_generation,
        "distinct generations"
    );
    assert_ne!(
        snap_a.identity.source_epoch, snap_b.identity.source_epoch,
        "distinct epochs"
    );
}

/// S3: zero generation is an invalid identity and fails with JSP-E004.
#[test]
fn s3_zero_generation_is_invalid_identity() {
    let json = br#"{"schema":1,"kind":"snapshot","agent_id":"a","lifecycle_generation":0,"source_epoch":"e","source_sequence":1,"cursor":0,"bridge_observed_ms":1,"native_session":{"repository":"r","path":"p","agent_kind":"llxprt","pid":1,"display_name":"d"},"process_binding":"unsupported","native_activity":"unsupported","current_wait":"unsupported","current_turn":"unsupported","todos":"unsupported","last_displayed_assistant_message":"unsupported","last_created_tool_call":"unsupported","source_terminal_state":"unsupported","source_error_state":"unsupported"}"#;
    let error = parse_snapshot(json)
        .err()
        .unwrap_or_else(|| panic!("zero generation must fail"));
    assert_eq!(error.code(), JspCode::EIdentity);
}

/// S4: producer-supplied stale state is rejected (stale is a local overlay,
/// not a producer value).
#[test]
fn s4_producer_stale_state_rejected() {
    let path = fixtures_dir().join("snapshot_semantic_failure.json");
    let bytes =
        fs::read(&path).unwrap_or_else(|_| panic!("semantic fixture at {}", path.display()));
    let error = parse_snapshot(&bytes)
        .err()
        .unwrap_or_else(|| panic!("producer stale must fail"));
    assert_eq!(error.code(), JspCode::EFieldState);
}

/// S5/S6: forbidden credential and control fields fail closed with JSP-E001
/// and no payload text in the diagnostic.
#[test]
fn s5_forbidden_fields_fail_closed() {
    let path = fixtures_dir().join("snapshot_forbidden_fields.json");
    let bytes =
        fs::read(&path).unwrap_or_else(|_| panic!("forbidden fixture at {}", path.display()));
    let error = parse_snapshot(&bytes)
        .err()
        .unwrap_or_else(|| panic!("forbidden fields must fail"));
    assert_eq!(error.code(), JspCode::EClosedShape);
    // Diagnostic must not echo any payload value (S5/S6).
    let detail = error.detail();
    assert!(
        !detail.contains("supersecret"),
        "diagnostic must not echo credential payload"
    );
    assert!(
        !detail.contains("leaked"),
        "diagnostic must not echo forbidden payload"
    );
    assert!(
        !detail.contains("kill"),
        "diagnostic must not echo control payload"
    );
}

/// Build a snapshot document whose `native_activity` field carries the supplied
/// raw JSON field-state, with every other field `unsupported`.
fn snapshot_with_activity_state(field_state: &str) -> Vec<u8> {
    format!(
        r#"{{"schema":1,"kind":"snapshot","agent_id":"a","lifecycle_generation":1,"source_epoch":"e","source_sequence":1,"cursor":0,"bridge_observed_ms":1,"native_session":{{"repository":"r","path":"p","agent_kind":"llxprt","pid":1,"display_name":"d"}},"process_binding":"unsupported","native_activity":{field_state},"current_wait":"unsupported","current_turn":"unsupported","todos":"unsupported","last_displayed_assistant_message":"unsupported","last_created_tool_call":"unsupported","source_terminal_state":"unsupported","source_error_state":"unsupported"}}"#
    )
    .into_bytes()
}

/// S4: the field-state algebra is closed — `known` requires `value` and
/// forbids the degraded members; `unknown` forbids every value member;
/// `degraded` requires its anchor members and forbids `value`.
#[test]
fn s4_field_state_algebra_rejects_illegal_member_combinations() {
    let illegal = [
        // `known` without a value.
        r#"{"provenance":"authoritative","availability":"known"}"#,
        // `known` carrying degraded-only members.
        r#"{"provenance":"authoritative","availability":"known","value":{"state":"idle"},"as_of_ms":5}"#,
        r#"{"provenance":"authoritative","availability":"known","value":{"state":"idle"},"last_value":{"state":"idle"}}"#,
        r#"{"provenance":"authoritative","availability":"known","value":{"state":"idle"},"diagnostic_code":"X"}"#,
        // `unknown` carrying any value member.
        r#"{"provenance":"inferred","availability":"unknown","value":{"state":"idle"}}"#,
        r#"{"provenance":"inferred","availability":"unknown","as_of_ms":5}"#,
        // `degraded` missing its required anchors, or using `value`.
        r#"{"provenance":"inferred","availability":"degraded","last_value":{"state":"idle"}}"#,
        r#"{"provenance":"inferred","availability":"degraded","last_value":{"state":"idle"},"as_of_ms":5}"#,
        r#"{"provenance":"inferred","availability":"degraded","value":{"state":"idle"},"as_of_ms":5,"diagnostic_code":"X"}"#,
    ];
    for field_state in illegal {
        let error = parse_snapshot(&snapshot_with_activity_state(field_state))
            .err()
            .unwrap_or_else(|| panic!("illegal field state must fail: {field_state}"));
        assert_eq!(
            error.code(),
            JspCode::EClosedShape,
            "illegal field state must fail closed: {field_state}"
        );
    }
}

/// S4: every legal availability form is accepted.
#[test]
fn s4_field_state_algebra_accepts_every_legal_form() {
    let legal = [
        r#"{"provenance":"authoritative","availability":"known","value":{"state":"idle"}}"#,
        r#"{"provenance":"inferred","availability":"unknown"}"#,
        r#"{"provenance":"inferred","availability":"degraded","last_value":{"state":"acting"},"as_of_ms":5,"diagnostic_code":"STALE_NATIVE"}"#,
    ];
    for field_state in legal {
        parse_snapshot(&snapshot_with_activity_state(field_state))
            .unwrap_or_else(|err| panic!("legal field state must parse: {field_state}: {err}"));
    }
}

/// S4: an unknown activity state is a field-state violation, not a shape error.
#[test]
fn s4_unknown_activity_state_is_field_state_violation() {
    let error = parse_snapshot(&snapshot_with_activity_state(
        r#"{"provenance":"authoritative","availability":"known","value":{"state":"bogus"}}"#,
    ))
    .err()
    .unwrap_or_else(|| panic!("unknown activity state must fail"));
    assert_eq!(error.code(), JspCode::EFieldState);
}

/// S2: duplicate top-level fields and trailing data fail closed.
#[test]
fn s2_duplicate_fields_and_trailing_data_fail_closed() {
    let base = String::from_utf8(snapshot_with_activity_state(r#""unsupported""#))
        .unwrap_or_else(|err| panic!("fixture is utf-8: {err}"));

    let duplicated = base.replacen(r#""schema":1,"#, r#""schema":1,"schema":1,"#, 1);
    let error = parse_snapshot(duplicated.as_bytes())
        .err()
        .unwrap_or_else(|| panic!("duplicate field must fail"));
    assert_eq!(error.code(), JspCode::EClosedShape);

    let trailing = format!("{base} trailing");
    let error = parse_snapshot(trailing.as_bytes())
        .err()
        .unwrap_or_else(|| panic!("trailing data must fail"));
    assert_eq!(error.code(), JspCode::EClosedShape);
}

/// S5: an unknown kind fails with JSP-E003.
#[test]
fn s5_unknown_kind_fails() {
    let json = br#"{"schema":1,"kind":"nope","agent_id":"a","lifecycle_generation":1,"source_epoch":"e","source_sequence":1,"cursor":0,"bridge_observed_ms":1}"#;
    let error = parse_snapshot(json)
        .err()
        .unwrap_or_else(|| panic!("unknown kind must fail"));
    assert_eq!(error.code(), JspCode::EUnsupportedVersion);
}

/// S6: manifest-driven negative parity check — the error detail never echoes
/// any of the forbidden payload values across all error fixtures.
#[test]
fn s6_error_diagnostics_never_echo_payload() {
    let manifest = load_manifest();
    let forbidden_tokens = ["supersecret", "leaked", "kill", "raw", "draft", "control"];
    for entry in &manifest.fixtures {
        if entry.expected != "error" {
            continue;
        }
        let path = fixtures_dir().join(&entry.file);
        let bytes = fs::read(&path)
            .unwrap_or_else(|_| panic!("fixture {} exists at {}", entry.name, path.display()));
        if let Err(error) = parse_snapshot(&bytes) {
            let detail = error.detail();
            for token in forbidden_tokens {
                assert!(
                    !detail.contains(token),
                    "fixture {} diagnostic leaked token '{token}': {detail}",
                    entry.name
                );
            }
        }
    }
}

/// Sanity: the error type implements the expected stable-code surface.
#[test]
fn error_code_surface_is_stable() {
    assert_eq!(JspCode::EClosedShape.as_str(), "JSP-E001");
    assert_eq!(JspCode::EBound.as_str(), "JSP-E002");
    assert_eq!(JspCode::EUnsupportedVersion.as_str(), "JSP-E003");
    assert_eq!(JspCode::EIdentity.as_str(), "JSP-E004");
    assert_eq!(JspCode::EFieldState.as_str(), "JSP-E005");
    assert_eq!(JspCode::ESemantic.as_str(), "JSP-E006");
}

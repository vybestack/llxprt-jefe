//! Helpers for the CW02-13 schema-1 migration acceptance test.
//!
//! These helpers keep `tests/issue382_behavior.rs` under the source-size hard
//! limit and prove the one-way schema-1 -> schema-2 selector migration is
//! production-connected: legacy LLxprt and Code Puppy selector fields migrate
//! losslessly into each agent's generic typed `version_selector` map value,
//! direct/blank semantics are preserved, unknown schema-1 agent records remain
//! byte-exact dormant data, and the projection is deterministic/idempotent.

use jefe::domain::canonical_values::typed_field;
use jefe::domain::{AgentKind, Id};
use jefe::persistence::migration::migrate_state;

trait ResultExt<T, E> {
    fn unwrap_ctx(self, context: &str) -> T;
}

impl<T, E: std::fmt::Debug> ResultExt<T, E> for Result<T, E> {
    fn unwrap_ctx(self, context: &str) -> T {
        match self {
            Ok(value) => value,
            Err(error) => panic!("{context}: {error:?}"),
        }
    }
}

impl<T> ResultExt<T, ()> for Option<T> {
    fn unwrap_ctx(self, context: &str) -> T {
        match self {
            Some(value) => value,
            None => panic!("{context}: value was absent"),
        }
    }
}

fn parse_id(value: &str) -> Id {
    Id::parse(value).unwrap_ctx("test id must parse")
}

/// Schema-1 fixture carrying one LLxprt agent (with a version selector), one
/// Execute the complete production-connected migration acceptance matrix.
pub fn assert_migration_contract() {
    schema1_selector_migration_is_lossless_into_version_selector();
    schema1_unknown_agent_kind_becomes_byte_exact_dormant_record();
    schema1_selector_migration_is_deterministic_and_idempotent();
    migrated_version_selector_restores_runtime_selector_fields();
    schema1_empty_document_is_rejected_with_typed_diagnostic();
}

/// Code Puppy agent (with a version selector), one blank-selector LLxprt agent
/// (direct launch), one unknown-kind agent (dormant), and an unknown field.
fn schema1_source(temp: &std::path::Path) -> Vec<u8> {
    let path = temp.to_string_lossy().into_owned();
    serde_json::json!({
        "schema_version": 1,
        "repositories": [schema1_repository(&path)],
        "agents": schema1_agents(&path),
        "selected_repository_index": 0,
        "selected_agent_index": 0
    })
    .to_string()
    .into_bytes()
}

fn schema1_repository(path: &str) -> serde_json::Value {
    serde_json::json!({
        "id": "repo", "name": "Repo", "slug": "repo", "base_dir": path,
        "default_profile": "", "default_llxprt_version": "nightly",
        "agent_ids": ["llxprt-agent", "puppy-agent", "direct-agent", "unknown-agent"]
    })
}

fn schema1_agents(path: &str) -> Vec<serde_json::Value> {
    vec![
        serde_json::json!({
            "id": "llxprt-agent", "display_id": "L", "repository_id": "repo",
            "name": "LLxprt Agent", "work_dir": path, "agent_kind": "llxprt",
            "llxprt_version": "0.10.0", "status": "Queued", "runtime_binding": null,
            "unknown_selector_field": {"kept": true}
        }),
        serde_json::json!({
            "id": "puppy-agent", "display_id": "P", "repository_id": "repo",
            "name": "Puppy Agent", "work_dir": path, "agent_kind": "code_puppy",
            "code_puppy_version": "1.2.3", "status": "Queued", "runtime_binding": null
        }),
        serde_json::json!({
            "id": "direct-agent", "display_id": "D", "repository_id": "repo",
            "name": "Direct Agent", "work_dir": path, "agent_kind": "llxprt",
            "llxprt_version": null, "status": "Queued", "runtime_binding": null
        }),
        serde_json::json!({
            "id": "unknown-agent", "display_id": "U", "repository_id": "repo",
            "name": "Unknown Agent", "work_dir": path, "agent_kind": "future-agent",
            "status": "Queued", "runtime_binding": null
        }),
    ]
}

/// Assert a typed map value equals a string option (`None` => absent or null).
fn assert_string_value(values: &jefe::domain::TypedMap, field: &str, expected: Option<&str>) {
    let actual = match typed_field(values, field) {
        Some(jefe::domain::TypedValue::String(value)) => Some(value.as_str()),
        _ => None,
    };
    assert_eq!(
        actual, expected,
        "{field} typed value must match the lossless migration"
    );
}

/// CW02-13: legacy selectors migrate losslessly into `version_selector`.
#[test]
fn schema1_selector_migration_is_lossless_into_version_selector() {
    let temp = tempfile::tempdir().unwrap_ctx("temporary repository root");
    let source = schema1_source(temp.path());

    let migrated = migrate_state(&source).unwrap_ctx("schema-1 state must migrate");
    let state = migrated.state();
    assert!(migrated.was_migrated());

    let llxprt = state
        .agents
        .iter()
        .find(|agent| agent.id == parse_id("llxprt-agent"))
        .unwrap_ctx("llxprt agent id preserved through migration");
    let puppy = state
        .agents
        .iter()
        .find(|agent| agent.id == parse_id("puppy-agent"))
        .unwrap_ctx("puppy agent id preserved through migration");
    let direct = state
        .agents
        .iter()
        .find(|agent| agent.id == parse_id("direct-agent"))
        .unwrap_ctx("direct agent id preserved through migration");

    // LLxprt selector moves into the generic `version_selector` field.
    assert_string_value(&llxprt.values, "version_selector", Some("0.10.0"));
    assert_eq!(llxprt.type_id, parse_id("core.llxprt"));
    // Code Puppy selector moves into the same generic field.
    assert_string_value(&puppy.values, "version_selector", Some("1.2.3"));
    assert_eq!(puppy.type_id, parse_id("core.code-puppy"));
    // A blank/null selector migrates to a blank `version_selector` (direct).
    assert_string_value(&direct.values, "version_selector", Some(""));
    // No legacy selector field names survive in the typed map: the generic
    // field is authoritative and no runtime adapter reads the old names.
    assert!(
        typed_field(&llxprt.values, "llxprt_version").is_none(),
        "legacy llxprt_version must not survive as a separate typed field"
    );
    assert!(
        typed_field(&puppy.values, "code_puppy_version").is_none(),
        "legacy code_puppy_version must not survive as a separate typed field"
    );
}

/// CW02-13: unknown schema-1 agent kind becomes a dormant durable record.
#[test]
fn schema1_unknown_agent_kind_becomes_byte_exact_dormant_record() {
    let temp = tempfile::tempdir().unwrap_ctx("temporary repository root");
    let source = schema1_source(temp.path());

    let migrated = migrate_state(&source).unwrap_ctx("schema-1 state must migrate");
    let state = migrated.state();

    // The unknown-kind agent must NOT become an executable schema-2 agent.
    assert!(
        !state
            .agents
            .iter()
            .any(|agent| agent.id == parse_id("unknown-agent")),
        "an unknown schema-1 agent kind must not migrate into an executable agent"
    );
    // It must be retained as dormant data so the user does not lose the record.
    let dormant = state
        .dormant_records
        .iter()
        .find(|record| record.kind.contains("schema1.agent.unknown-kind"))
        .unwrap_ctx("unknown schema-1 agent must be retained as a dormant record");
    assert_eq!(dormant.raw_schema, 1);
    let source_value: serde_json::Value =
        serde_json::from_slice(&source).unwrap_ctx("schema-1 fixture must decode");
    let raw_agent = source_value["agents"][3].clone();
    assert_eq!(
        dormant.raw_value, raw_agent,
        "dormant unknown agent must preserve the exact source JSON value"
    );
    assert!(
        !dormant.reason.is_empty(),
        "dormant record must explain why it is unavailable"
    );
}

/// CW02-13: migration is deterministic and reapplying schema-2 is a no-op.
#[test]
fn schema1_selector_migration_is_deterministic_and_idempotent() {
    let temp = tempfile::tempdir().unwrap_ctx("temporary repository root");
    let source = schema1_source(temp.path());

    let first = migrate_state(&source).unwrap_ctx("first migration");
    let second = migrate_state(&source).unwrap_ctx("second migration");
    assert_eq!(first.state(), second.state());

    let schema2 = serde_json::to_vec(first.state()).unwrap_ctx("encode schema 2");
    let reapplied = migrate_state(&schema2).unwrap_ctx("schema 2 reapplication");
    assert!(!reapplied.was_migrated());
    assert_eq!(reapplied.state(), first.state());
}

/// CW02-13: restoring the migrated typed map repopulates the runtime selector
/// fields from `version_selector`, definition-driven by the agent type id.
#[test]
fn migrated_version_selector_restores_runtime_selector_fields() {
    let temp = tempfile::tempdir().unwrap_ctx("temporary repository root");
    let source = schema1_source(temp.path());

    let migrated = migrate_state(&source).unwrap_ctx("schema-1 state must migrate");
    let restored =
        jefe::state::durable_restore::from_durable_state(migrated.state()).unwrap_ctx("restore");

    let llxprt = restored
        .agents
        .iter()
        .find(|agent| agent.display_id == "L")
        .unwrap_ctx("llxprt agent restored");
    assert_eq!(llxprt.agent_kind, AgentKind::Llxprt);
    assert_eq!(
        llxprt
            .llxprt_version
            .as_ref()
            .map(|selector| selector.as_str().to_owned()),
        Some("0.10.0".to_owned()),
        "llxprt_version is derived from the authoritative version_selector"
    );

    let puppy = restored
        .agents
        .iter()
        .find(|agent| agent.display_id == "P")
        .unwrap_ctx("puppy agent restored");
    assert_eq!(puppy.agent_kind, AgentKind::CodePuppy);
    assert_eq!(
        puppy.code_puppy_version, "1.2.3",
        "code_puppy_version is derived from the authoritative version_selector"
    );

    // The repository default selector also migrates into the generic field and
    // restores into the runtime default.
    let repository = &restored.repositories[0];
    assert_eq!(
        repository
            .default_llxprt_version
            .as_ref()
            .map(|selector| selector.as_str().to_owned()),
        Some("nightly".to_owned()),
        "repository default_llxprt_version is derived from the generic selector"
    );
}

/// CW02-13: an empty/malformed schema-1 document is rejected with a typed
/// diagnostic rather than silently accepted.
#[test]
fn schema1_empty_document_is_rejected_with_typed_diagnostic() {
    let empty = b"";
    let diagnostics = migrate_state(empty)
        .err()
        .unwrap_ctx("empty document must be rejected before migration");
    assert!(
        !diagnostics.is_empty(),
        "an empty document produces a typed migration diagnostic"
    );
}

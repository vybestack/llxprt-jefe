//! One-way schema-1 state migration contract tests.

use std::path::Path;

use serde_json::json;

use super::diagnostic::CfgCode;
use super::migration::migrate_state;
use crate::domain::{Id, LastKnownRuntime, RepositoryLocation, TypedValue};

trait TestResultExt<T, E> {
    fn value_or_panic(self, context: &str) -> T;
}

impl<T, E: std::fmt::Debug> TestResultExt<T, E> for Result<T, E> {
    fn value_or_panic(self, context: &str) -> T {
        match self {
            Ok(value) => value,
            Err(error) => panic!("{context}: {error:?}"),
        }
    }
}

fn id(value: &str) -> Id {
    Id::parse(value).value_or_panic("test id must be valid")
}

fn schema1_state(local_path: &Path) -> Vec<u8> {
    let path = local_path.to_string_lossy();
    let encoded = serde_json::to_string(path.as_ref()).value_or_panic("encode fixture path");
    let escaped = encoded
        .strip_prefix('"')
        .and_then(|value| value.strip_suffix('"'))
        .unwrap_or_else(|| panic!("encoded path must be a JSON string"));
    include_str!("fixtures/schema1-state-full.json")
        .replace("__LOCAL_PATH__", escaped)
        .into_bytes()
}
fn assert_primary_migration(state: &crate::domain::StateV2) {
    assert_eq!(state.state_schema, 2);
    assert_eq!(state.revision, 1);
    assert_eq!(state.repositories.len(), 3);
    assert_eq!(state.agents.len(), 3);
    assert_ne!(state.repositories[0].id, state.repositories[1].id);
    assert!(matches!(
        state.repositories[0].location,
        RepositoryLocation::Local(_)
    ));
    assert_eq!(
        state.repositories[2].location,
        RepositoryLocation::Remote(crate::domain::RemoteRepositoryLocation {
            remote_target: "3:dev11:example.com4:22226:runner12:/srv/project".to_owned(),
        })
    );
    assert_eq!(
        state.repositories[0].agent_defaults.type_id,
        id("core.llxprt")
    );
    assert_eq!(
        state.repositories[1].agent_defaults.type_id,
        id("core.code-puppy")
    );
    assert_eq!(
        state.repositories[0]
            .agent_defaults
            .values
            .get(&id("default-profile")),
        Some(&TypedValue::String("review".to_owned()))
    );
}

fn assert_agent_migration(state: &crate::domain::StateV2) {
    assert_eq!(state.agents[0].type_id, id("core.llxprt"));
    assert_eq!(state.agents[1].type_id, id("core.code-puppy"));
    assert_eq!(state.agents[2].type_id, id("core.llxprt"));
    assert_eq!(
        state.agents[0].runtime.session_id.as_deref(),
        Some("jefe-agent-a")
    );
    assert_eq!(state.agents[0].runtime.invocation_generation, 9);
    // A schema-1 agent recorded as Running is carried across as Running, so
    // startup reconciliation still checks it against live sessions instead of
    // silently forgetting it was launched.
    assert_eq!(
        state.agents[0].runtime.last_known,
        LastKnownRuntime::Running
    );
    assert_eq!(
        state.agents[1].runtime.last_known,
        LastKnownRuntime::Unknown,
        "a queued agent was never launched"
    );
    assert_eq!(
        state.agents[0].values.get(&id("runtime-binding")),
        None,
        "PID and process evidence must not enter typed product values"
    );
}

fn assert_selection_and_preferences(state: &crate::domain::StateV2) {
    assert_eq!(
        state.selection.repository_id.as_ref(),
        Some(&state.repositories[0].id)
    );
    assert_eq!(state.selection.agent_id.as_ref(), Some(&state.agents[0].id));
    assert_eq!(
        state
            .preferences
            .repository_preferences
            .get(&state.repositories[0].id)
            .and_then(|values| values.get(&id("issue-search-query"))),
        Some(&TypedValue::String("crash".to_owned()))
    );
    assert_eq!(state.last_selected_agent_by_repo.len(), 2);
}

fn assert_dormant_migration(state: &crate::domain::StateV2) {
    assert_eq!(state.dormant_records.len(), 3);
    assert!(state.dormant_records.iter().any(|record| {
        record.kind == "schema1.root.future-root-field"
            && record.raw_value == json!({"opaque": true})
    }));
    assert!(state.dormant_records.iter().any(|record| {
        record.kind == "schema1.repository.future-repository-field"
            && record.stable_id.as_ref() == Some(&state.repositories[0].id)
    }));
    assert!(state.dormant_records.iter().any(|record| {
        record.kind == "schema1.agent.future-agent-field"
            && record.stable_id.as_ref() == Some(&state.agents[0].id)
    }));
}

#[test]
fn schema1_migration_preserves_typed_product_state_and_dormant_unknowns() {
    let temp = tempfile::tempdir().value_or_panic("temporary repository root");
    let source = schema1_state(temp.path());

    let migrated = migrate_state(&source).value_or_panic("schema-1 state must migrate");
    let state = migrated.state();

    assert!(migrated.was_migrated());
    assert_primary_migration(state);
    assert_agent_migration(state);
    assert_selection_and_preferences(state);
    assert_dormant_migration(state);
}

#[test]
fn schema1_migration_is_deterministic_and_schema2_reapplication_is_a_noop() {
    let temp = tempfile::tempdir().value_or_panic("temporary repository root");
    let source = schema1_state(temp.path());

    let first = migrate_state(&source).value_or_panic("first migration");
    let second = migrate_state(&source).value_or_panic("second migration");
    assert_eq!(first.state(), second.state());

    let schema2 = serde_json::to_vec(first.state()).value_or_panic("encode schema 2");
    let reapplied = migrate_state(&schema2).value_or_panic("schema 2 reapplication");
    assert!(!reapplied.was_migrated());
    assert_eq!(reapplied.state(), first.state());
    assert_eq!(reapplied.state().revision, 1);
}

#[test]
fn invalid_schema1_indices_are_repaired_with_sorted_warnings() {
    let source = serde_json::to_vec(&json!({
        "schema_version": 1,
        "repositories": [],
        "agents": [],
        "selected_repository_index": 3,
        "selected_agent_index": 4
    }))
    .value_or_panic("repair fixture");

    let migrated = migrate_state(&source).value_or_panic("repair migration");

    assert!(migrated.state().selection.repository_id.is_none());
    assert!(migrated.state().selection.agent_id.is_none());
    assert_eq!(migrated.diagnostics().len(), 2);
    assert!(
        migrated
            .diagnostics()
            .iter()
            .all(|item| item.code == CfgCode::W004)
    );
    assert!(
        migrated
            .diagnostics()
            .windows(2)
            .all(|pair| pair[0] <= pair[1])
    );
}

#[test]
fn duplicate_schema1_keys_are_rejected_before_migration() {
    let source = br#"{
        "schema_version": 1,
        "schema_version": 1,
        "repositories": [],
        "agents": [],
        "selected_repository_index": null,
        "selected_agent_index": null
    }"#;

    let diagnostics = migrate_state(source)
        .err()
        .unwrap_or_else(|| panic!("duplicate schema-1 keys must fail"));

    assert_eq!(diagnostics.len(), 1);
    assert_eq!(diagnostics[0].code, CfgCode::E103);
}

#[test]
fn colliding_agent_legacy_identities_receive_source_order_ordinals() {
    let temp = tempfile::tempdir().value_or_panic("temporary repository root");
    let source = serde_json::to_vec(&json!({
        "schema_version": 1,
        "repositories": [{
            "id": "repository",
            "name": "Repository",
            "slug": "repository",
            "base_dir": temp.path(),
            "default_profile": "",
            "agent_ids": ["duplicate", "duplicate"]
        }],
        "agents": [
            {
                "id": "duplicate",
                "display_id": "A",
                "repository_id": "repository",
                "name": "First",
                "work_dir": temp.path().join("first")
            },
            {
                "id": "duplicate",
                "display_id": "B",
                "repository_id": "repository",
                "name": "Second",
                "work_dir": temp.path().join("second")
            }
        ],
        "selected_repository_index": null,
        "selected_agent_index": null
    }))
    .value_or_panic("collision fixture");

    let first = migrate_state(&source).value_or_panic("collision migration");
    let second = migrate_state(&source).value_or_panic("repeat collision migration");

    assert_ne!(first.state().agents[0].id, first.state().agents[1].id);
    assert_eq!(first.state().agents[0].id, second.state().agents[0].id);
    assert_eq!(first.state().agents[1].id, second.state().agents[1].id);
}

#[test]
fn remote_schema1_ids_and_hashes_match_fixed_vectors() {
    let source = serde_json::to_vec(&json!({
        "schema_version": 1,
        "repositories": [{
            "id": "remote",
            "name": "Remote",
            "slug": "remote",
            "base_dir": "/srv/project",
            "default_profile": "",
            "remote": {
                "enabled": true,
                "login_user": "dev",
                "host": "EXAMPLE.COM",
                "port": 2222,
                "run_as_user": "runner"
            },
            "agent_ids": ["agent"]
        }],
        "agents": [{
            "id": "agent",
            "display_id": "A",
            "repository_id": "remote",
            "name": "Agent",
            "description": "",
            "work_dir": "/srv/project/work",
            "profile": "",
            "mode_flags": [],
            "pass_continue": true,
            "sandbox_enabled": false,
            "sandbox_engine": "podman",
            "sandbox_flags": "",
            "status": "Queued",
            "runtime_binding": null
        }],
        "selected_repository_index": 0,
        "selected_agent_index": 0
    }))
    .value_or_panic("fixed-vector fixture");

    let migrated = migrate_state(&source).value_or_panic("fixed-vector migration");
    let repository = &migrated.state().repositories[0];
    let agent = &migrated.state().agents[0];

    assert_eq!(
        repository.id.as_str(),
        "repo.90c36b6f6cad7eb526f60ebdc4fbaac14ecf08aabfc9656626f09ff4bf8b5d30"
    );
    assert_eq!(
        agent.id.as_str(),
        "agent.f6c2c9886351227b08ccd5c2aee572a03589e7de5d2d7838cb3fa948abb62d25"
    );
    assert_eq!(
        agent.launch_signature.definition_hash.as_str(),
        "d9c86254b9b69126f482605301dd73ff1b2e81454f4a0ddb74c2dbc0ea79a313"
    );
    assert_eq!(
        agent.launch_signature.typed_value_hash.as_str(),
        "b265282e2d9552e775c7ecff34ce62c26fbee0244a7a8f18f65cea0e6fec033b"
    );
    assert_eq!(
        agent.launch_signature.target_fingerprint.as_str(),
        "4b7789f7ba8bfab6a1d9739e1a29c3b6da9d3e8b781c5cb8e0419ddb5bbc81bc"
    );
}

#[test]
fn unknown_schema1_root_values_remain_raw_json() {
    let source = serde_json::to_vec(&json!({
        "schema_version": 1,
        "repositories": [],
        "agents": [],
        "selected_repository_index": null,
        "selected_agent_index": null,
        "opaque": {"array": [1, null, true], "name": "value"}
    }))
    .value_or_panic("dormant fixture");

    let migrated = migrate_state(&source).value_or_panic("dormant migration");
    let raw = migrated
        .state()
        .dormant_records
        .first()
        .map(|record| &record.raw_value);

    assert_eq!(
        raw,
        Some(&json!({"array": [1, null, true], "name": "value"}))
    );
}

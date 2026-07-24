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
    serde_json::to_vec_pretty(&json!({
        "schema_version": 1,
        "repositories": [
            {
                "id": "local-a",
                "name": "Local A",
                "slug": "local-a",
                "base_dir": local_path,
                "default_profile": "review",
                "default_code_puppy_model": "sonnet",
                "default_code_puppy_version": "1.2.3",
                "github_repo": "acme/local-a",
                "github_issue_pr_repo": "upstream/local-a",
                "remote": {"enabled": false},
                "issue_base_prompt": "inspect",
                "default_agent_kind": "llxprt",
                "transient_agent_dir": "/tmp/transient",
                "default_code_puppy_yolo": true,
                "default_llxprt_mode_flags": ["--yolo", "--debug"],
                "transient_max_concurrent": 3,
                "default_llxprt_version": "nightly",
                "agent_ids": ["agent-a"],
                "future_repository_field": {"opaque": 7}
            },
            {
                "id": "local-b",
                "name": "Local B",
                "slug": "local-b",
                "base_dir": local_path,
                "default_profile": "",
                "default_agent_kind": "code_puppy",
                "agent_ids": ["agent-b"]
            },
            {
                "id": "remote-a",
                "name": "Remote A",
                "slug": "remote-a",
                "base_dir": "/srv/project/../project",
                "default_profile": "",
                "remote": {
                    "enabled": true,
                    "login_user": "dev",
                    "host": "EXAMPLE.COM",
                    "port": 2222,
                    "identity_file": "/home/dev/.ssh/id_ed25519",
                    "options": ["BatchMode=yes"],
                    "run_as_user": "runner",
                    "setup_env_default": true
                },
                "agent_ids": ["agent-remote"]
            }
        ],
        "agents": [
            {
                "id": "agent-a",
                "display_id": "A",
                "repository_id": "local-a",
                "shortcut_slot": 2,
                "name": "Alpha",
                "description": "primary",
                "work_dir": local_path.join("alpha"),
                "profile": "review",
                "code_puppy_model": "",
                "code_puppy_version": "",
                "code_puppy_yolo": null,
                "code_puppy_quick_resume": false,
                "mode_flags": ["--yolo"],
                "llxprt_debug": "trace",
                "pass_continue": true,
                "sandbox_enabled": true,
                "sandbox_engine": "podman",
                "sandbox_flags": "--network=none",
                "agent_kind": "llxprt",
                "llxprt_version": "nightly",
                "status": "Running",
                "runtime_binding": {
                    "session_name": "jefe-agent-a",
                    "launch_signature": {
                        "work_dir": local_path.join("alpha"),
                        "profile": "review",
                        "mode_flags": ["--yolo"],
                        "pass_continue": true,
                        "sandbox_enabled": true,
                        "sandbox_engine": "podman",
                        "sandbox_flags": "--network=none"
                    },
                    "attached": true,
                    "last_seen": 55,
                    "pid": 777,
                    "process_identity": {"pid": 777, "started_at": 1234},
                    "lifecycle_generation": 9
                },
                "origin": "persistent",
                "future_agent_field": ["opaque"]
            },
            {
                "id": "agent-b",
                "display_id": "B",
                "repository_id": "local-b",
                "name": "Beta",
                "description": "",
                "work_dir": local_path.join("beta"),
                "profile": "",
                "mode_flags": [],
                "pass_continue": false,
                "sandbox_enabled": false,
                "sandbox_engine": "podman",
                "sandbox_flags": "",
                "agent_kind": "codepuppy",
                "status": "Queued",
                "runtime_binding": null
            },
            {
                "id": "agent-remote",
                "display_id": "R",
                "repository_id": "remote-a",
                "name": "Remote",
                "description": "",
                "work_dir": "/srv/project/work",
                "profile": "",
                "mode_flags": [],
                "pass_continue": true,
                "sandbox_enabled": false,
                "sandbox_engine": "podman",
                "sandbox_flags": "",
                "status": "Paused",
                "runtime_binding": null
            }
        ],
        "selected_repository_index": 0,
        "selected_agent_index": 0,
        "hide_idle_repositories": true,
        "last_selected_agent_by_repo": [
            ["local-a", "agent-a"],
            ["remote-a", "agent-remote"]
        ],
        "pane_focus": "terminal",
        "terminal_focused": true,
        "user_preferences": {
            "by_repo": [["local-a", {
                "issue_search_query": "crash",
                "pr_search_query": "review",
                "issue_filter_field_index": 2,
                "pr_filter_field_index": 3,
                "last_merge_method": "Squash"
            }]]
        },
        "future_root_field": {"opaque": true}
    }))
    .value_or_panic("schema-1 fixture must serialize")
}

#[test]
fn schema1_migration_preserves_typed_product_state_and_dormant_unknowns() {
    let temp = tempfile::tempdir().value_or_panic("temporary repository root");
    let source = schema1_state(temp.path());

    let migrated = migrate_state(&source).value_or_panic("schema-1 state must migrate");
    let state = migrated.state();

    assert!(migrated.was_migrated());
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
    assert_eq!(state.agents[0].type_id, id("core.llxprt"));
    assert_eq!(state.agents[1].type_id, id("core.code-puppy"));
    assert_eq!(state.agents[2].type_id, id("core.llxprt"));
    assert_eq!(
        state.agents[0].runtime.session_id.as_deref(),
        Some("jefe-agent-a")
    );
    assert_eq!(state.agents[0].runtime.invocation_generation, 9);
    assert_eq!(
        state.agents[0].runtime.last_known,
        LastKnownRuntime::Unknown
    );
    assert_eq!(
        state.agents[0].values.get(&id("runtime-binding")),
        None,
        "PID and process evidence must not enter typed product values"
    );
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

//! Semantic selector validation tests (issue #269), extracted from
//! `persistence/tests.rs` so that file stays under the per-file line limit.
//!
//! All tests drive the real [`super::validate_state_selectors`] (re-exported
//! from [`super::selector_validation`]) and the real
//! [`super::FilePersistenceManager`], so no production logic is duplicated.

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

trait TestResultErrorExt<E> {
    fn error_or_panic(self, context: &str) -> E;
}

impl<E: std::fmt::Debug> TestResultErrorExt<E> for Result<(), E> {
    fn error_or_panic(self, context: &str) -> E {
        match self {
            Ok(()) => panic!("{context}: expected error"),
            Err(error) => error,
        }
    }
}

// ── Semantic selector validation on state load (issue #269) ──────────────
//
// A hand-edited state.json could carry an escaped NUL byte (\u0000) inside a
// syntactically valid JSON string. serde deserializes it into a Rust String
// containing a real NUL, which is structurally unrepresentable as a process
// argument. load_state must reject such state with a typed PersistenceError
// rather than loading it silently.

/// Helper: a syntactically valid JSON state string with a repo carrying a
/// clean (NUL-free) selector round-trips through the real
/// FilePersistenceManager without error.
#[test]
fn load_state_accepts_valid_repository_selector() {
    let json = serde_json::json!({
        "schema_version": 1,
        "repositories": [{
            "id": "repo-ok",
            "name": "OK Repo",
            "slug": "ok-repo",
            "base_dir": "/tmp/ok",
            "default_profile": "",
            "default_llxprt_version": "0.9.0",
            "agent_ids": []
        }],
        "agents": [],
        "selected_repository_index": null,
        "selected_agent_index": null
    });
    let state: State = serde_json::from_value(json).value_or_panic("valid state");
    validate_state_selectors(&state).value_or_panic("valid selector must pass");
}

/// A repository `default_llxprt_version` containing an embedded NUL must be
/// rejected by `validate_state_selectors` with a ParseError naming the repo.
#[test]
fn validate_state_rejects_nul_in_repository_selector() {
    // The json! macro value uses a Rust NUL escape (\0) so serde_json receives
    // a String containing a real 0x00 byte — exactly what a hand-edited file
    // carrying \u0000 would deserialize into.
    let json = serde_json::json!({
        "schema_version": 1,
        "repositories": [{
            "id": "repo-bad",
            "name": "Bad Repo",
            "slug": "bad-repo",
            "base_dir": "/tmp/bad",
            "default_profile": "",
            "default_llxprt_version": "0.9.0\0; rm -rf /",
            "agent_ids": []
        }],
        "agents": [],
        "selected_repository_index": null,
        "selected_agent_index": null
    });
    let state: State = serde_json::from_value(json).value_or_panic("JSON deserializes");

    let error = validate_state_selectors(&state).error_or_panic("NUL selector must fail");
    match error {
        PersistenceError::ParseError(msg) => {
            assert!(
                msg.contains("Bad Repo"),
                "error must name the repository: {msg}"
            );
            assert!(
                msg.contains("default_llxprt_version"),
                "error must name the field: {msg}"
            );
            assert!(
                msg.contains("NUL"),
                "error must mention the NUL byte: {msg}"
            );
        }
        other => panic!("expected ParseError, got {other:?}"),
    }
}

/// An agent `llxprt_version` containing an embedded NUL must be rejected by
/// `validate_state_selectors` with a ParseError naming the agent.
#[test]
fn validate_state_rejects_nul_in_agent_selector() {
    let json = serde_json::json!({
        "schema_version": 1,
        "repositories": [],
        "agents": [{
            "id": "agent-bad",
            "display_id": "agent-bad",
            "repository_id": "repo-x",
            "name": "Bad Agent",
            "description": "",
            "work_dir": "/tmp",
            "profile": "",
            "llxprt_version": "0.9.0\0",
            "mode_flags": [],
            "pass_continue": true,
            "status": "Queued"
        }],
        "selected_repository_index": null,
        "selected_agent_index": null
    });
    let state: State = serde_json::from_value(json).value_or_panic("JSON deserializes");

    let error = validate_state_selectors(&state).error_or_panic("NUL agent selector must fail");
    match error {
        PersistenceError::ParseError(msg) => {
            assert!(
                msg.contains("Bad Agent"),
                "error must name the agent: {msg}"
            );
            assert!(
                msg.contains("llxprt_version"),
                "error must name the field: {msg}"
            );
        }
        other => panic!("expected ParseError, got {other:?}"),
    }
}

/// A runtime binding `launch_signature.llxprt_version` containing an embedded
/// NUL must be rejected by `validate_state_selectors` with a ParseError
/// naming the agent and the runtime_binding location.
#[test]
fn validate_state_rejects_nul_in_runtime_binding_selector() {
    let json = serde_json::json!({
        "schema_version": 1,
        "repositories": [],
        "agents": [{
            "id": "agent-bind",
            "display_id": "agent-bind",
            "repository_id": "repo-x",
            "name": "Bound Agent",
            "description": "",
            "work_dir": "/tmp",
            "profile": "",
            "llxprt_version": "0.9.0",
            "mode_flags": [],
            "pass_continue": true,
            "status": "Running",
            "runtime_binding": {
                "session_name": "jefe-agent-bind",
                "launch_signature": {
                    "work_dir": "/tmp",
                    "profile": "",
                    "llxprt_version": "latest\0",
                    "mode_flags": [],
                    "pass_continue": true,
                    "sandbox_enabled": false,
                    "sandbox_engine": "podman",
                    "sandbox_flags": "--cpus=2 --memory=12288m --pids-limit=256"
                },
                "attached": false
            }
        }],
        "selected_repository_index": null,
        "selected_agent_index": null
    });
    let state: State = serde_json::from_value(json).value_or_panic("JSON deserializes");

    let error =
        validate_state_selectors(&state).error_or_panic("NUL runtime binding selector must fail");
    match error {
        PersistenceError::ParseError(msg) => {
            assert!(
                msg.contains("Bound Agent"),
                "error must name the agent: {msg}"
            );
            assert!(
                msg.contains("runtime_binding"),
                "error must name the runtime_binding location: {msg}"
            );
        }
        other => panic!("expected ParseError, got {other:?}"),
    }
}

/// Build an `Agent` with a valid runtime binding for the happy-path
/// validation test.
fn agent_with_valid_runtime_binding() -> crate::domain::Agent {
    use crate::domain::{
        Agent, AgentId, LaunchSignature, RemoteRepositorySettings, RepositoryId, RuntimeBinding,
        SandboxEngine,
    };

    Agent {
        id: AgentId("agent-ok".to_string()),
        display_id: "agent-ok".to_string(),
        repository_id: RepositoryId("repo-ok".to_string()),
        shortcut_slot: None,
        name: "OK Agent".to_string(),
        description: String::new(),
        work_dir: std::path::PathBuf::from("/tmp/ok"),
        profile: String::new(),
        code_puppy_model: String::new(),
        llxprt_version: "next".to_string(),
        code_puppy_yolo: None,
        code_puppy_quick_resume: false,
        mode_flags: Vec::new(),
        llxprt_debug: String::new(),
        pass_continue: true,
        sandbox_enabled: false,
        sandbox_engine: SandboxEngine::Podman,
        sandbox_flags: String::new(),
        agent_kind: crate::domain::AgentKind::Llxprt,
        status: crate::domain::AgentStatus::Running,
        runtime_binding: Some(RuntimeBinding {
            session_name: "jefe-agent-ok".to_string(),
            launch_signature: LaunchSignature {
                work_dir: std::path::PathBuf::from("/tmp/ok"),
                profile: String::new(),
                code_puppy_model: String::new(),
                llxprt_version: "0.9.0".to_string(),
                code_puppy_yolo: None,
                code_puppy_quick_resume: false,
                mode_flags: Vec::new(),
                llxprt_debug: String::new(),
                pass_continue: true,
                sandbox_enabled: false,
                sandbox_engine: SandboxEngine::Podman,
                sandbox_flags: String::new(),
                remote: RemoteRepositorySettings::default(),
                agent_kind: crate::domain::AgentKind::Llxprt,
            },
            attached: true,
            last_seen: None,
            pid: None,
        }),
    }
}

/// Build a `State` with valid selectors in all three locations (repo default,
/// agent, runtime binding) for the happy-path validation test.
fn state_with_all_valid_selectors() -> State {
    use crate::domain::{RemoteRepositorySettings, Repository, RepositoryId};

    let repo = Repository {
        id: RepositoryId("repo-ok".to_string()),
        name: "OK".to_string(),
        slug: "ok".to_string(),
        base_dir: std::path::PathBuf::from("/tmp/ok"),
        default_profile: String::new(),
        default_code_puppy_model: String::new(),
        default_llxprt_version: "0.9.0".to_string(),
        github_repo: String::new(),
        remote: RemoteRepositorySettings::default(),
        issue_base_prompt: String::new(),
        default_agent_kind: crate::domain::AgentKind::Llxprt,
        agent_ids: vec![],
    };
    State {
        schema_version: STATE_SCHEMA_VERSION,
        repositories: vec![repo],
        agents: vec![agent_with_valid_runtime_binding()],
        selected_repository_index: Some(0),
        selected_agent_index: Some(0),
        hide_idle_repositories: false,
        last_selected_agent_by_repo: vec![],
        pane_focus: String::new(),
        terminal_focused: false,
        user_preferences: crate::domain::UserPreferences::default(),
    }
}

/// A state with valid selectors in all three locations (repo default, agent,
/// runtime binding) must pass validation cleanly. This locks the happy path
/// alongside the rejection tests above.
#[test]
fn validate_state_accepts_all_valid_selectors() {
    let state = state_with_all_valid_selectors();
    validate_state_selectors(&state).value_or_panic("all-valid selectors must pass");
}

/// A legacy state.json with missing selector fields (backward compat) must
/// deserialize and pass validation: absent fields default to empty strings,
/// which are valid selectors.
#[test]
fn validate_state_accepts_backward_compatible_missing_selectors() {
    let legacy_json = r#"{
        "schema_version": 1,
        "repositories": [
            {
                "id": "legacy-repo",
                "name": "Legacy",
                "slug": "legacy",
                "base_dir": "/tmp/legacy",
                "default_profile": "",
                "agent_ids": []
            }
        ],
        "agents": [],
        "selected_repository_index": null,
        "selected_agent_index": null
    }"#;
    let state: State =
        serde_json::from_str(legacy_json).value_or_panic("legacy JSON should deserialize");
    validate_state_selectors(&state)
        .value_or_panic("missing selector fields must default to valid empty strings");
}

/// The real `load_state` on a state.json containing a NUL-escaped selector
/// must return a PersistenceError (not load the invalid state). This drives
/// the full file-based load path end-to-end. The raw file uses the JSON
/// escape \u0000 (interpreted by serde_json at parse time, not by rustc),
/// proving the persistence layer rejects NUL bytes that arrive from disk.
#[test]
fn load_state_rejects_nul_selector_in_file() {
    let temp = std::env::temp_dir().join("jefe_test_load_nul_selector");
    let _ = std::fs::remove_dir_all(&temp);
    std::fs::create_dir_all(&temp).value_or_panic("create temp dir");

    // A syntactically valid JSON string with an escaped NUL in the repo's
    // default_llxprt_version field. serde_json interprets \u0000 as a real
    // NUL byte during deserialization.
    let malicious_json = r#"{
        "schema_version": 1,
        "repositories": [
            {
                "id": "repo-nul",
                "name": "NUL Repo",
                "slug": "nul-repo",
                "base_dir": "/tmp/nul",
                "default_profile": "",
                "default_llxprt_version": "0.9.0\u0000",
                "agent_ids": []
            }
        ],
        "agents": [],
        "selected_repository_index": null,
        "selected_agent_index": null
    }"#;
    let state_path = temp.join("state.json");
    std::fs::write(&state_path, malicious_json).value_or_panic("write malicious state.json");

    let paths = PersistencePaths {
        settings_path: temp.join("settings.toml"),
        state_path,
    };
    let mgr = FilePersistenceManager::with_paths(paths);

    let error = match mgr.load_state() {
        Ok(state) => panic!("NUL selector must be rejected on load, got: {state:?}"),
        Err(error) => error,
    };
    match error {
        PersistenceError::ParseError(msg) => {
            assert!(
                msg.contains("NUL Repo"),
                "error must name the repository: {msg}"
            );
        }
        other => panic!("expected ParseError, got {other:?}"),
    }

    let _ = std::fs::remove_dir_all(&temp);
}

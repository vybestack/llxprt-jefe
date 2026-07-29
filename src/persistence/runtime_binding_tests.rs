//! Persistence contracts for multi-runtime restart metadata.

use super::*;
use crate::domain::canonical_values::{insert_json, typed_field};
use crate::domain::{
    Agent, AgentId, AgentStatus, AgentTypeId, LaunchSignatureV1, ProcessIdentity, Repository,
    RepositoryId, RuntimeBinding, TypedValue,
};

fn set_string(agent: &mut Agent, field: &str, value: &str) {
    insert_json(
        &mut agent.values,
        field,
        serde_json::Value::String(value.to_owned()),
    )
    .unwrap_or_else(|error| panic!("valid field {field}: {error}"));
}

fn set_bool(agent: &mut Agent, field: &str, value: bool) {
    insert_json(&mut agent.values, field, serde_json::Value::Bool(value))
        .unwrap_or_else(|error| panic!("valid field {field}: {error}"));
}

fn bound_runtime_agent(repository_id: &RepositoryId, index: u32, type_id: AgentTypeId) -> Agent {
    let id = AgentId(format!("agent-Ω-{index}"));
    let mut agent = Agent::new(
        id.clone(),
        repository_id.clone(),
        type_id.clone(),
        crate::domain::TypedMap::new(),
        format!("Agent Ω {index}"),
        std::path::PathBuf::from(format!(r"C:\work dirs\agent Ω {index}")),
    );
    agent.status = AgentStatus::Running;
    let definition = crate::domain::agent_definition::AgentDefinition::shipped()
        .into_iter()
        .find(|definition| definition.id == type_id)
        .unwrap_or_else(|| panic!("shipped type fixture must have a definition"));
    if definition
        .repository_fields
        .iter()
        .any(|field| field.id == "model")
    {
        set_string(&mut agent, "model", "model/Ω");
        set_string(&mut agent, "version_selector", "0.0.361-rc1");
        set_bool(&mut agent, "interactive", true);
    } else {
        set_string(&mut agent, "profile", &format!("profile-{index}"));
        set_string(
            &mut agent,
            "version_selector",
            "0.10.0-nightly.260712.21cb698b6",
        );
    }
    agent.runtime_binding = Some(RuntimeBinding {
        session_name: crate::runtime::RuntimeSession::session_name_for(&id),
        launch_signature: LaunchSignatureV1::default(),
        attached: index == 0,
        last_seen: Some(1_000 + u64::from(index)),
        pid: Some(10_000 + index),
        process_identity: Some(ProcessIdentity::new(
            10_000 + index,
            90_000 + u64::from(index),
        )),
        lifecycle_generation: 0,
        worker_identities: Vec::new(),
    });
    agent
}

fn assert_loaded_runtime_state(loaded: &State) {
    assert_eq!(loaded.repositories[0].name, "Repository Ω With Spaces");
    assert_eq!(
        loaded.agents[0].type_id,
        crate::domain::shipped_agent_type(3)
    );
    assert_eq!(
        loaded.agents[1].type_id,
        crate::domain::shipped_agent_type(1)
    );
    assert_eq!(
        typed_field(&loaded.agents[1].values, "interactive"),
        Some(&TypedValue::Bool(true))
    );
    assert_eq!(
        typed_field(&loaded.agents[1].values, "version_selector"),
        Some(&TypedValue::String("0.0.361-rc1".to_owned()))
    );
    assert_eq!(
        typed_field(&loaded.agents[0].values, "version_selector"),
        Some(&TypedValue::String(
            "0.10.0-nightly.260712.21cb698b6".to_owned()
        ))
    );
    assert!(loaded.agents.iter().all(|agent| {
        agent
            .runtime_binding
            .as_ref()
            .is_some_and(|binding| binding.launch_signature == LaunchSignatureV1::default())
    }));
}

#[test]
fn restart_roundtrip_preserves_unicode_multi_runtime_bindings() {
    let repository_id = RepositoryId("repo-Ω spaces".to_owned());
    let repository = Repository::new(
        repository_id.clone(),
        crate::domain::shipped_agent_type(3),
        crate::domain::TypedMap::new(),
        "Repository Ω With Spaces".to_owned(),
        "repository-omega".to_owned(),
        std::path::PathBuf::from(r"C:\work dirs\repository Ω"),
    );
    let state = State {
        repositories: vec![repository],
        agents: vec![
            bound_runtime_agent(&repository_id, 0, crate::domain::shipped_agent_type(3)),
            bound_runtime_agent(&repository_id, 1, crate::domain::shipped_agent_type(1)),
        ],
        ..State::default_with_version()
    };
    let expected_bindings = state
        .agents
        .iter()
        .map(|agent| agent.runtime_binding.clone())
        .collect::<Vec<_>>();
    let json = serde_json::to_vec(&state)
        .unwrap_or_else(|error| panic!("serialize runtime state: {error}"));
    let loaded: State = serde_json::from_slice(&json)
        .unwrap_or_else(|error| panic!("deserialize runtime state: {error}"));

    assert_loaded_runtime_state(&loaded);
    let loaded_bindings = loaded
        .agents
        .iter()
        .map(|agent| agent.runtime_binding.as_ref())
        .collect::<Vec<_>>();
    let expected_json = serde_json::to_value(&expected_bindings)
        .unwrap_or_else(|error| panic!("serialize expected bindings: {error}"));
    let loaded_json = serde_json::to_value(&loaded_bindings)
        .unwrap_or_else(|error| panic!("serialize loaded bindings: {error}"));
    assert_eq!(loaded_json, expected_json);
}

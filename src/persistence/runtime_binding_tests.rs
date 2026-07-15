//! Persistence contracts for multi-runtime restart metadata.

use super::*;
use crate::domain::{
    Agent, AgentId, AgentKind, AgentStatus, LaunchSignature, ProcessIdentity,
    RemoteRepositorySettings, Repository, RepositoryId, RuntimeBinding, SandboxEngine,
};

fn bound_runtime_agent(repository_id: &RepositoryId, index: u32, kind: AgentKind) -> Agent {
    let id = AgentId(format!("agent-Ω-{index}"));
    let work_dir = std::path::PathBuf::from(format!(r"C:\work dirs\agent Ω {index}"));
    let mut agent = Agent::new(
        id.clone(),
        repository_id.clone(),
        format!("Agent Ω {index}"),
        work_dir.clone(),
    );
    agent.agent_kind = kind;
    agent.status = AgentStatus::Running;
    agent.profile = format!("profile-{index}");
    agent.code_puppy_model = "model/Ω".to_owned();
    agent.code_puppy_quick_resume = kind == AgentKind::CodePuppy;
    agent.pass_continue = kind == AgentKind::Llxprt;
    if kind == AgentKind::Llxprt {
        agent.llxprt_version =
            crate::domain::LlxprtNpmPackageSelector::normalize("0.10.0-nightly.260712.21cb698b6");
    }
    agent.runtime_binding = Some(runtime_binding(&agent, &id, work_dir, index, kind));
    agent
}

fn runtime_binding(
    agent: &Agent,
    id: &AgentId,
    work_dir: std::path::PathBuf,
    index: u32,
    kind: AgentKind,
) -> RuntimeBinding {
    let signature = LaunchSignature {
        work_dir,
        profile: agent.profile.clone(),
        code_puppy_model: agent.code_puppy_model.clone(),
        code_puppy_yolo: Some(true),
        code_puppy_quick_resume: agent.code_puppy_quick_resume,
        mode_flags: vec!["--flag Ω".to_owned()],
        llxprt_debug: "runtime=trace".to_owned(),
        pass_continue: agent.pass_continue,
        sandbox_enabled: false,
        sandbox_engine: SandboxEngine::Podman,
        sandbox_flags: String::new(),
        remote: RemoteRepositorySettings::default(),
        agent_kind: kind,
        llxprt_version: agent.llxprt_version.clone(),
    };
    RuntimeBinding {
        session_name: crate::runtime::RuntimeSession::session_name_for(id),
        launch_signature: signature,
        attached: index == 0,
        last_seen: Some(1_000 + u64::from(index)),
        pid: Some(10_000 + index),
        process_identity: Some(ProcessIdentity::new(
            10_000 + index,
            90_000 + u64::from(index),
        )),
    }
}

#[test]
fn restart_roundtrip_preserves_unicode_multi_runtime_bindings() {
    let repository_id = RepositoryId("repo-Ω spaces".to_owned());
    let repository = Repository::new(
        repository_id.clone(),
        "Repository Ω With Spaces".to_owned(),
        "repository-omega".to_owned(),
        std::path::PathBuf::from(r"C:\work dirs\repository Ω"),
    );
    let state = State {
        repositories: vec![repository],
        agents: vec![
            bound_runtime_agent(&repository_id, 0, AgentKind::Llxprt),
            bound_runtime_agent(&repository_id, 1, AgentKind::CodePuppy),
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

    assert_eq!(loaded.repositories[0].name, "Repository Ω With Spaces");
    assert_eq!(loaded.agents[0].agent_kind, AgentKind::Llxprt);
    assert_eq!(loaded.agents[1].agent_kind, AgentKind::CodePuppy);
    assert!(loaded.agents[0].pass_continue);
    assert!(loaded.agents[1].code_puppy_quick_resume);
    assert_eq!(
        loaded.agents[0]
            .runtime_binding
            .as_ref()
            .and_then(|binding| binding.launch_signature.llxprt_version.as_ref())
            .map(crate::domain::LlxprtNpmPackageSelector::as_str),
        Some("0.10.0-nightly.260712.21cb698b6")
    );
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

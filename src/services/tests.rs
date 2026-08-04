use super::*;
use crate::domain::canonical_values::typed_field;
use crate::domain::{AgentStatus, RemoteRepositorySettings, Repository, RepositoryId, TypedValue};

fn local_repository() -> Repository {
    Repository::new(
        RepositoryId("repo-1".to_owned()),
        crate::domain::shipped_agent_type(3),
        crate::domain::TypedMap::new(),
        "Repo 1".to_owned(),
        "repo-1".to_owned(),
        std::path::PathBuf::from("/tmp/repo-1"),
    )
}

fn remote_repository() -> Repository {
    Repository {
        remote: RemoteRepositorySettings {
            enabled: true,
            login_user: "ubuntu".to_owned(),
            host: "example.com".to_owned(),
            run_as_user: "acoliver".to_owned(),
            setup_env_default: false,
            ..RemoteRepositorySettings::default()
        },
        ..local_repository()
    }
}

fn params<'a>(
    repository: &'a Repository,
    name: &'a str,
    work_dir: &'a str,
) -> CreateAgentParams<'a> {
    CreateAgentParams {
        repository,
        name,
        description: "",
        work_dir,
        profile: "",
        code_puppy_model: "",
        code_puppy_version: "",
        code_puppy_yolo: false,
        code_puppy_quick_resume: crate::domain::QuickResume::default(),
        agent_type_id: "core.llxprt",
        mode: "",
        llxprt_debug: "",
        llxprt_version: "",
        pass_continue: true,
        sandbox_enabled: false,
        sandbox_engine: "podman",
        sandbox_flags: "",
        shortcut_slot: None,
        next_display_index: 1,
    }
}

fn created(params: CreateAgentParams<'_>) -> Agent {
    let Some(agent) = create_agent(params) else {
        panic!("agent should be created");
    };
    agent
}

#[test]
fn create_agent_rejects_blank_name() {
    let repo = local_repository();
    assert!(create_agent(params(&repo, "   ", "/tmp/agent")).is_none());
}

#[test]
fn create_agent_rejects_blank_work_dir() {
    let repo = local_repository();
    assert!(create_agent(params(&repo, "Agent", "   \t ")).is_none());
}

#[test]
fn create_agent_sets_running_status() {
    let repo = local_repository();
    let agent = created(params(&repo, "Agent", "/tmp/agent"));
    assert_eq!(agent.status, AgentStatus::Running);
}

#[test]
fn create_agent_trims_name() {
    let repo = local_repository();
    let agent = created(params(&repo, "  Agent One  ", "/tmp/agent"));
    assert_eq!(agent.name, "Agent One");
}

#[test]
fn create_agent_normalizes_profile() {
    let repo = local_repository();

    let blank = created(CreateAgentParams {
        profile: "  ",
        ..params(&repo, "Agent", "/tmp/agent")
    });
    assert_eq!(
        typed_field(&blank.values, "profile"),
        Some(&TypedValue::String(String::new()))
    );

    let brackets = created(CreateAgentParams {
        profile: "[]",
        ..params(&repo, "Agent", "/tmp/agent")
    });
    assert_eq!(
        typed_field(&brackets.values, "profile"),
        Some(&TypedValue::String(String::new()))
    );

    let custom = created(CreateAgentParams {
        profile: "custom",
        ..params(&repo, "Agent", "/tmp/agent")
    });
    assert_eq!(
        typed_field(&custom.values, "profile"),
        Some(&TypedValue::String("custom".to_owned()))
    );
}

#[test]
fn create_agent_maps_declared_yolo_input_to_typed_values() {
    let repo = local_repository();
    let disabled = created(params(&repo, "Agent", "/tmp/agent"));
    assert_eq!(
        typed_field(&disabled.values, "yolo"),
        Some(&TypedValue::Bool(false))
    );

    let enabled = created(CreateAgentParams {
        mode: "--yolo",
        ..params(&repo, "Agent", "/tmp/agent")
    });
    assert_eq!(
        typed_field(&enabled.values, "yolo"),
        Some(&TypedValue::Bool(true))
    );
}

#[test]
fn create_agent_maps_enabled_sandbox_to_typed_values() {
    let repo = local_repository();
    let agent = created(CreateAgentParams {
        sandbox_enabled: true,
        sandbox_engine: "Docker",
        sandbox_flags: "  --network none  ",
        ..params(&repo, "Agent", "/tmp/agent")
    });
    assert_eq!(
        typed_field(&agent.values, "sandbox_enabled"),
        Some(&TypedValue::Bool(true))
    );
    assert_eq!(
        typed_field(&agent.values, "sandbox_engine"),
        Some(&TypedValue::String("docker".to_owned()))
    );
    assert_eq!(
        typed_field(&agent.values, "sandbox_flags"),
        Some(&TypedValue::String("--network none".to_owned()))
    );
}

#[test]
fn create_agent_clears_sandbox_configuration_when_disabled() {
    let repo = local_repository();
    let agent = created(CreateAgentParams {
        sandbox_engine: "Docker",
        sandbox_flags: "--network none",
        ..params(&repo, "Agent", "/tmp/agent")
    });
    assert_eq!(
        typed_field(&agent.values, "sandbox_enabled"),
        Some(&TypedValue::Bool(false))
    );
    assert_eq!(typed_field(&agent.values, "sandbox_engine"), None);
    assert_eq!(typed_field(&agent.values, "sandbox_flags"), None);
}
#[test]
fn create_agent_expands_tilde_for_local_repository() {
    let Some(home) = std::env::var_os("HOME") else {
        // No HOME set in this environment; tilde expansion is a no-op, which is
        // covered indirectly elsewhere. Skip the home-dependent assertion.
        return;
    };
    let home = home.to_string_lossy().into_owned();
    let repo = local_repository();
    let agent = created(params(&repo, "Agent", "~/work/agent"));
    assert_eq!(
        agent.work_dir,
        std::path::PathBuf::from(format!("{home}/work/agent"))
    );
}

#[test]
fn create_agent_keeps_work_dir_verbatim_for_remote_repository() {
    let repo = remote_repository();
    let agent = created(params(&repo, "Agent", "~/work/agent"));
    assert_eq!(agent.work_dir, std::path::PathBuf::from("~/work/agent"));
}
